# 应用层（app）

应用层把领域层的状态机与终端输入输出接在一起：它持有唯一的 `App` 实例，把键盘、鼠标与定时器事件翻译成领域操作，决定何时读档与存档，并在每一帧把只读状态交给渲染层。流程推进、FSRS 调度、键入判定等算法在 `domain` 中实现，布局与像素在 `ui` 中实现；应用层负责「什么时候调用谁」。

> 适用范围：`src/app/` 的四个文件，以及启动装配相关的 `src/main.rs`、`src/lib.rs`、`src/cli.rs`、`src/paths.rs`、`src/bootstrap.rs`；对应版本：typewords-tui 0.1.0。

## 1. 职责与边界

### 1.1 文件清单

| 文件 | 内容 |
| --- | --- |
| `src/app/mod.rs` | `App` 结构体，`Screen`/`Toast`/`EditState`/`ConfirmState`/`SummaryData`/`SettingKind`/`SETTING_ROWS`；启动、事件循环、练习会话集成、设置动作、`on_tick`、`run`/`build_for_test` |
| `src/app/action.rs` | `Action` 枚举与 `App::apply_action`：非练习页、鼠标、弹层与测试共用的状态迁移入口 |
| `src/app/keymap.rs` | 弹层与各屏幕的键位到 `Action` 映射，不直接修改状态；练习页的键位改由 `src/domain/shortcut.rs` 的键位表驱动 |
| `src/app/mouse.rs` | 鼠标事件 → `ui::hit::action_at`/`wheel_action` → `Action` |
| `src/bootstrap.rs` | `Bootstrapped`（存档、词库目录、主题、`notices`、`readonly_dict`）与 `bootstrap::load` |
| `src/main.rs`、`src/lib.rs` | 入口：`Cli::parse` → `Paths::resolve` → 子命令分发 → `app::run` |
| `src/cli.rs`、`src/paths.rs` | 参数与 `doctor`/`task`/`import-dict`；数据目录/资源目录解析 |

### 1.2 与 domain / ui 的边界

```text
      crossterm 事件                App（唯一状态源）            只读
 Key ──on_key───────┐    ┌──────────────────────────┐    ┌───────────────┐
 Mouse ─on_mouse────┼───▶│ screen / session / typing│───▶│ ui::render    │
 now ───on_tick─────┘    │ stat / toasts / ...      │    │ (ratatui)     │
                         └────────────┬─────────────┘    └───────────────┘
                                      │ domain 公开 API
                         ┌────────────▼─────────────┐
                         │ PracticeSession /         │
                         │ TypingEngine / timer/flow │──▶ store::FileStore（JSON）
                         └───────────────────────────┘
```

- `domain` 不知道终端存在；`ui` 只读 `App`、不修改状态；唯一的反向接口是 `ui::hit` 根据命中区域产出 `Action`，由应用层执行。
- 状态变更入口有两类：**非练习页键位、弹层、鼠标与测试**走 `Action` + `App::apply_action`；**练习页键位**由 `on_practice_key` 直接调用与 `apply_action` 相同的 `App` 方法（练习键位大多要落入键入状态机，不适合建模成离散动作）。启动流程与 `on_tick` 也直接修改状态；「所有状态变更都经过 `apply_action`」只在键盘/鼠标/测试这条链路上成立。
- `app` 与 `ui` 同 crate 互相引用：`App::render` 调 `ui::render`，`App::theme` 返回 `ui::Theme`，页面直接读 `App` 公开字段。这是刻意取舍，代价见 [总体架构](architecture.md)。

## 2. App 状态

### 2.1 字段分组

| 分组 | 字段 | 说明 |
| --- | --- | --- |
| 启动装载 | `boot: Bootstrapped`、`store: FileStore` | 存档状态、词库目录、主题注册表、`readonly_dict` |
| 页面 | `screen`、`dict_cursor`、`settings_cursor`、`stats_scroll`、`help_scroll` | 当前页与三个列表/滚动光标 |
| 会话 | `session: Option<PracticeSession>`、`typing: TypingEngine`、`stat: PracticeState`、`identify_select` | 练习中才有 `session`；开始新一轮时 `typing`/`stat` 重建或重置 |
| 随机源 | `rng: Rng`、`seed: u64` | `StdRng`；`--seed` 固定后整条事件序列可复现 |
| 提示与弹层 | `toasts: Vec<Toast>`、`quit: bool`、`edit`、`confirm`、`summary` | 最多保留 4 条 Toast；`quit` 由退出确认置位；三类弹层正常交互中互斥 |
| 显示开关 | `show_toolbar`、`show_panel`、`show_meta` | 练习页 Ctrl+B / Ctrl+L / Ctrl+E 的状态（仅本次进程） |
| 统计缓存 | `task_preview: Option<(usize, usize, usize)>`、`today_spend`、`today_new`、`today_review`、`today_wrong` | 主页「今日任务」「统计」区块的预计算值 |
| 生命周期 | `fatal: Option<String>`、`last_activity`、`last_timer_tick`、`pending_practice_save` | 后三个私有：空闲判定、1 秒计时节拍、防抖保存截止时间 |

访问器：`base()`、`settings()`、`theme()`、`theme_label()`、`study_progress()`。`theme()` 每次调用都会 `ThemeRegistry::resolve` 并构造 `ui::Theme`（13 个颜色的小结构体，开销可忽略）。

### 2.2 Screen

| 变体 | 对应页 | 侧栏 `Nav` | 数字快捷键 |
| --- | --- | --- | --- |
| `Home` | 主页 | `Nav::Home`（`Practice` 也映射到它） | 1 |
| `Practice` | 练习页（全屏，不套外壳） | — | 无（练习中数字属于键入/自测） |
| `WordTest` | 单词测试页（一题 4 选 1） | `Nav::Home` | 无（数字用于选答案） |
| `DictPicker` | 词库页 | `Nav::Dicts` | 2 |
| `Stats` | 统计页 | `Nav::Stats` | 3 |
| `Settings` | 设置页 | `Nav::Settings` | 4 |
| `Shortcuts` | 快捷键编辑页 | `Nav::Settings` | —（由设置页进入） |
| `Help` | 帮助页 | `Nav::Help` | 5 |

### 2.3 提示与弹层数据

- `Toast`：`message`、`level: NoticeLevel`（Success/Warning/Error/Info）、`until`（毫秒时间戳）。`App::toast` 取当前时间 + 3200 ms，超过 4 条丢最旧；`current_toast(now)` 从尾部找第一条未过期项。
- `ConfirmAction`：`Quit`、`ResetPractice`、`ClearAllData`；`ConfirmState` 为 `message` + `action`，文案统一由 `confirm_message(action)` 产出（退出 / 清空练习进度 / 清除所有数据各一条）。
- `SummaryData`：`spend_ms`、`accuracy`（`total == 0` 记 100）、`new_words`、`review`、`wrong`、`last_learn_index`、`length`、`complete`；后三项来自当前学习词典的总进度。
- `EditState`：`target: EditTarget` + `buffer: String`（ASCII 数字，最长 6 位）。`EditTarget` 有 `PerDay`、`Ratio`、`Repeat`、`RepeatCustom`、`WaitMs`、`SpaceCooldown`、`EasyLimit`、`GoodLimit`、`HardLimit`、`Retention` 十个取值；`PerDay` 没有设置行可打开，仅 `apply_number_value` 保留处理（每日词数由主页/词库页 `+`/`-` 修改）。

### 2.4 设置表与 `SettingKind`

`SETTING_ROWS` 是设置页唯一数据源，共 24 项，数组顺序即渲染顺序。

| `SettingKind` | `SETTING_ROWS` 中的 key（按出现顺序） |
| --- | --- |
| `Mode` / `Theme` / `SoundType` | `word_practice_mode` / `theme` / `sound_type` |
| `Bool` | `big_word`、`auto_random_review`、`ignore_simple_word`、`auto_next_word`、`input_wrong_clear`、`ignore_case`、`show_word_question`、`show_etymology`、`show_near_word`、`show_usage_tips` |
| `Number` | `word_review_ratio`→`Ratio`、`repeat_count`→`Repeat`、`repeat_custom_count`→`RepeatCustom`、`wait_time`→`WaitMs`、`space_cooldown`→`SpaceCooldown`、`fsrs_easy_limit`→`EasyLimit`、`fsrs_good_limit`→`GoodLimit`、`fsrs_hard_limit`→`HardLimit`、`request_retention`→`Retention` |
| `Action(ConfirmAction)` | `reset_practice_progress`→`ResetPractice`、`clear_all_data`→`ClearAllData` |

设置页按 key 归入「通用 / 音频 / 复习 / 键入 / 显示 / FSRS / 其他」七个分区（起点见 `src/ui/settings.rs` 的 `SECTIONS`），值格式化由渲染层完成，细节见 [设置项参考](settings.md)。

## 3. 启动流程

### 3.1 从进程入口到 UI

`src/main.rs` 里 `Cli::parse()` 解析参数后调用 `typewords_tui::run(cli)`；`src/lib.rs` 的 `run` 先 `Paths::resolve(cli.assets, cli.home)`（任一目录解析失败直接返回错误），再按 `cli.command` 分发：无子命令时 `app::run(paths, cli)` → `App::bootstrap` → 若 `fatal` 有值则 `anyhow::bail!` → `App::run` 进入终端。

| `Command` | 流程 | 进入 TUI |
| --- | --- | --- |
| `Doctor` | `cli::doctor`：打印版本、数据/资源目录、词库目录与每个词库的词数、存档文件大小、主题 id/名称/告警 | 否 |
| `Task` | `cli::task_preview`：装载存档与词库，用 `cli.seed`（缺省 42）构造随机源，打印今日任务的新词/复习/到期数与前 5 个词 | 否 |
| `ImportDict { file }` | `cli::import_dict`：校验并复制词库 JSON、登记到数据目录的 `list/word.json`，打印词库信息 | 否 |

### 3.2 `App::bootstrap` 的顺序

1. `bootstrap::load(&paths, cli.dict.as_deref())`：创建 `FileStore`（确保数据目录存在），读取 `dict.json`/`setting.json`，加载 `Catalog` 与 `ThemeRegistry`；损坏、版本过高、主题加载失败等通知收进 `notices`。传 `--dict` 时调用 `select_dict`，否则 `ensure_study_dict`。
2. 应用 CLI 覆盖：`--mode` 经 `cli::parse_mode` 写 `settings.word_practice_mode`（未知模式返回错误）；`--theme` 直接覆盖 `settings.theme` 字符串。
3. 随机种子取 `cli.seed`，缺省 `rng::random_seed()`；构造 `rng::seeded(seed)`；组装 `App`（`screen = Home`、无会话、空 Toast、工具栏显示、词表面板与例句开关的默认值）。
4. `notices` 逐条转 `Warning` Toast；再次 `ThemeRegistry::resolve`，发生回退时追加 Warning「主题 X 不存在，已回退到深色」。
5. `--reset-practice` 删除 `practice-word.json` 并提示「已清空练习进度」，否则 `try_restore_practice()`（见 3.3）。
6. `ensure_study_dict()`：当前词典缺词表时按 id 重新加载，找不到就用 `cet4` 或第一本词库；失败写 `fatal = "加载词库失败: ..."`。
7. `refresh_stats()` 与 `refresh_task_preview()` 计算主页缓存，返回 `App`。

### 3.3 恢复练习缓存与 fatal

`try_restore_practice`：读 `practice-word.json`，有损坏/版本不受支持的通知则弹 Warning；无缓存静默返回；有缓存时要求学习词典已有词表且 `cache_from_compact` 能还原，然后用缓存里的 `flow_id` 反查模式、新建 `PracticeSession`、把当前词典全部单词洗牌后交给 `apply_cache`。成功则 `screen = Practice`、重置键入、提示「已恢复上次练习进度」；失败静默丢弃（不删文件，等待下一次练习覆盖）。

错误分两级：`bootstrap::load` 自身的错误从 `App::bootstrap` 返回，`app::run` 不会初始化终端，进程以错误退出；`fatal` 只用于 `ensure_study_dict` 失败——`App::bootstrap` 仍返回 `Ok`，`app::run` 在 `ratatui::init` 之前 `bail!`，不会留下半初始化界面。

## 4. 事件循环与 tick

### 4.1 `App::run`

1. 安装 panic hook：panic 时先关闭鼠标捕获、`ratatui::restore()`，再调原 hook，避免终端停在备用屏幕/鼠标捕获状态。
2. `ratatui::init()` 进入备用屏幕，`EnableMouseCapture` 开启鼠标上报；调用 `event_loop`；结束后 `DisableMouseCapture` 并 `ratatui::restore()`。
3. 事件循环退出时执行 `save_all()`（见第 9 节）。

```text
loop {
    terminal.draw(|frame| app.render(frame))   // 先重绘当前帧
    app.on_tick(now_ms())                       // Toast/计时/键入定时/防抖保存
    if app.quit { break }
    if event::poll(100ms) {
        最多消费 64 个事件（每次 read 后 poll(0) 续读）:
            Key(Press) → app.on_key；Mouse → app.on_mouse(m, terminal_area())
            Resize 与其它事件 → 忽略（下一帧按新尺寸重排）
    }
}
app.save_all()
```

- 先 draw 再 tick：tick 造成的变化在下一帧可见；一帧内的按键事件在 draw 之后处理。
- 一次 `poll` 最多消费 64 个事件再重绘，防止鼠标事件造成高频重绘；`poll(0)` 只用于判断队列是否还有事件。
- 只处理 `KeyEventKind::Press`，忽略 Release/Repeat，避免 Windows 上重复触发。
- `terminal_area()` 由 `crossterm::terminal::size()` 构造 `Rect`，失败回退 `80 × 24`；只用于鼠标命中，渲染以 `Frame::area()` 为准。退出路径只有两条：主页 `q` → 确认弹层 → `quit = true`，或 `event_loop` 返回错误；两者都走保存与终端恢复。

### 4.2 `on_tick(now)` 的四块逻辑

| 顺序 | 逻辑 | 细节 |
| --- | --- | --- |
| 1 | Toast 过期 | 所有 Toast 都满足 `until <= now` 时清空 `toasts`；渲染侧用 `current_toast(now)` 过滤未过期项 |
| 2 | 练习计时 | 仅当 `screen == Practice` 且有 `session`；距 `last_timer_tick` 满 1000 ms 时先记下 `spend`，再 `timer::tick(...)`（累加 1000 ms，空闲 180 秒自动暂停）；若本次 tick 确实增加了时长且新值跨过 30 秒整数倍，则 `schedule_practice_save()` |
| 3 | 键入定时 | 同一条件下构造 `TypingContext` 并调 `typing.on_tick(&ctx, now)`，事件交给 `handle_typing_events`（自动下一词、重复练习、错误清除等定时器） |
| 4 | 防抖保存 | `pending_practice_save` 到期（`now >= deadline`）时 `save_practice_now()` |

第 2 块的保存条件比较 tick 前后的 `spend`：暂停/空闲时 `spend` 不变，不会重复调度；只有真正累加并跨过 30 秒整数倍时才请求防抖保存。

## 5. 输入路由

### 5.1 `on_key` 的分发顺序

1. 更新 `last_activity = now_ms()`（供空闲暂停使用）。
2. 若 `edit`/`confirm`/`summary` 任一存在：调 `keymap::overlay_action`，有返回值就 `apply_action`，然后**直接返回**——弹层是模态的，未识别的键被吞掉；优先级 `edit` → `confirm` → `summary`。
3. `screen == Practice` 时进入 `on_practice_key`（练习页键盘不经过 `Action`）。
4. 其它屏幕调 `keymap::screen_action`，返回 `Action` 后交给 `apply_action`。

`screen_action` 先处理 Help（「任意键返回」优先），再判断数字切页：`session.is_none()`、无修饰键、按 `1`~`5` 时直接产生 `Navigate(Home/DictPicker/Stats/Settings/Help)`。

### 5.2 练习页键位（`on_practice_key`）

| 键 | 行为 |
| --- | --- |
| 字母、数字、符号、空格、退格 | 经 `key_to_input` 进入 `handle_typing_key`；Ctrl/Alt 组合不进入键入 |
| `Esc` | `session.show_word`：显示完整单词；遮罩或非跟写流程下会记一次错误 |
| `Tab` | `skip_word` |
| `Enter` / `Ctrl+Enter` | 收藏/取消收藏 / 重学本组（`RepeatChapter`，按组重建会话） |
| `?` | 帮助页（`help_scroll = 0`）；会话保留，返回时回练习页 |
| 反引号 | 标记/取消已掌握，然后重置键入并推进到下一个词 |
| `Alt+1`~`Alt+4` | 自测选择题 A~D（`identify_choose(0..3)`） |
| `1`/`2`/`3`（自测阶段，非 Ctrl） | 认识 / 不认识 / 已掌握 |
| `Ctrl+Z` / `Ctrl+I` | 切换翻译 / 切换默写遮罩 |
| `Ctrl+,` | 暂停 / 恢复计时（带 Toast）；TUI 专有键位，上游的 `Ctrl+P` 是「播放单词发音」 |
| `Ctrl+P` | 用系统 TTS（或设置里选的有道发音）朗读当前词 |
| `Ctrl+R` | 随机默写当前词表：打乱当前节点词表、设为遮罩、从第一个词开始，并重置键入 |
| `Ctrl+B` / `Ctrl+L` / `Ctrl+E` | 切换工具栏 / 右侧词表 / 例句与词源 |
| `Ctrl+C` | 保存练习与词典，退出练习回主页 |
| `Ctrl+N` 或 `Ctrl+→` / `Ctrl+←` | 下一个词（`advance_word(false)`）/ 上一个词 |
| `Ctrl+J` 或 `Shift+→` | 跳到下一阶段（`skip_step`） |

`Ctrl+J` 是 `Shift+→` 的 ASCII 别名；部分终端会吞掉 `Ctrl+方向键` 或 `Alt+数字`，帮助页也提示了这一点。

### 5.3 其它屏幕键位（`screen_action`）

| 屏幕 | 键 → `Action` |
| --- | --- |
| 主页 | `Enter`→`StartPractice`；`f`→`StartFreePractice`；`d`/`s`/`t`→`Navigate`；`m`→`OpenModePicker`；`?`→Help；`q`→`QuitRequest`；`+`/`=`→`AdjustDaily(5)`，`-`→`AdjustDaily(-5)` |
| 词库页 | `Esc`/`q`→`Back`；`↑`/`k`→`DictMove(-1)`，`↓`/`j`→`DictMove(1)`；`Enter`→`ActivateDict(cursor)`；`+`/`-`→`AdjustDaily(±5)` |
| 统计页 | `Esc`/`q`/`Enter`→`Back`；`↑`/`k`→`StatsScroll(-1)`，`↓`/`j`→`StatsScroll(1)`；`PgUp`/`PgDn`→`StatsScroll(∓5)` |
| 设置页 | `Esc`/`q`→`Back`；`↑`/`k`→`SettingsMove(-1)`，`↓`/`j`→`SettingsMove(1)`；`Enter`/`Space`→`ActivateSetting(cursor)` |
| 帮助页 | `↑`/`k`、`↓`/`j`、`PgUp`/`PgDn` 滚动；其余任意键→`HelpBack` |

无会话的非练习页上，无修饰的 `1`~`5` 切到对应页面。

### 5.4 弹层键位

| 弹层 | 键 → `Action` |
| --- | --- |
| 输入 `edit` | `Esc`→`EditCancel`；`Enter`→`EditSave`；`Backspace`→`EditBackspace`；ASCII 数字→`EditChar`；其它忽略 |
| 确认 `confirm` | `y`/`Y`/`Enter`→`ConfirmAccept`；**其它任意键**→`ConfirmCancel` |
| 结算 `summary` | `Enter`/`Space`→`SummaryContinue`；`r`→`SummaryRelearn`；`q`/`Esc`→`SummaryHome`；其它忽略 |

### 5.5 键入映射与鼠标

`key_to_input` 把终端按键压成 `KeyInput`：`Backspace`→`Backspace`、`Char(' ')`→`Space`、其它 `Char(c)`→`Char(c)`，其余返回 `None`。带 `Control` 或 `Alt` 修饰的按键一律返回 `None`，所以组合键不会误输入字符；`Shift` 不排除，大写字母与符号可正常键入。

`App::on_mouse` 先更新 `last_activity`，然后左键按下 → `ui::hit::action_at` 命中则 `apply_action`；滚轮上/下 → `wheel_action(..., -1/+1)`；其它鼠标事件忽略。命中区域与优先级见 [渲染层 · 鼠标命中](ui.md#11-鼠标命中)。

### 5.6 `Action` 全表

`Action` 的 86 个变体覆盖除练习页键入外的全部交互，`apply_action` 是唯一消费点：

| `Action` | `apply_action` 语义 |
| --- | --- |
| `Navigate(Screen)` | `navigate_to`：有会话时直接忽略；Home 刷新统计/任务预览，DictPicker/Settings 重置光标，Stats/Help 重置滚动；`Practice` 无动作 |
| `Back` | 依次关闭 confirm/edit/summary；否则按屏返回 Home（Settings 先保存设置；Help 走 `help_back`） |
| `QuitRequest` | 打开「确定退出 Type Words 吗？」确认弹层 |
| `StartPractice` / `StartFreePractice` | 按当前设置模式 / 以 `Free` 模式启动练习 |
| `AdjustDaily(delta)` | 每日词数 `±delta` 并夹在 `1..=词表长度`；变化时保存词典、刷新任务预览并 Toast |
| `OpenModePicker` / `ModeMove(delta)` / `ModeSelect` / `ModeCancel` | 模式选择弹层：打开弹层、移动光标、确认、取消；确认后按所选模式启动练习 |
| `DictMove(delta)` | 词库光标移动并夹在有效范围 |
| `SelectDict(i)` / `SelectSetting(i)` | 只移动光标（越界忽略） |
| `ActivateDict(i)` | 切换当前学习词库：保存词典、回主页、刷新任务与 Toast |
| `SettingsMove(delta)` | 设置光标移动并夹在 `0..SETTING_ROWS.len()-1` |
| `ActivateSetting(i)` | 设置光标置 `i` 并执行该项（见第 8 节） |
| `StatsScroll(delta)` | 统计页滚动，上限为「最近 14 天」记录数减一 |
| `SummaryContinue` / `SummaryRelearn` / `SummaryHome` | 结算弹层三种去向（见 7.5） |
| `ConfirmAccept` / `ConfirmCancel` | 执行/关闭确认；`ConfirmAccept` 处理 Quit、ResetPractice 与 ClearAllData |
| `EditSave` / `EditCancel` | 解析输入并写回 / 关闭输入弹层 |
| `EditChar(c)` / `EditBackspace` | 追加 ASCII 数字（最多 6 位）/ 删除一位 |
| `HelpScroll(delta)` | 帮助页滚动，`max(0)` 后上限 80 |
| `HelpBack` | 有会话回练习页，否则回主页 |

## 6. 屏幕与弹层状态机

### 6.1 规则

- **练习中禁止切页**：`navigate_to` 在有 `session` 时直接返回。练习页只能通过 `Ctrl+C` 或完成结算离开（帮助页只是临时覆盖，返回后仍在练习）；鼠标侧栏导航在练习期间也不会生成 target。
- **弹层模态且互斥**：`summary` 只在练习完成时产生；`edit` 只从设置页数字项产生；`confirm` 只从主页 `q` 或设置页「重置进度」产生。正常交互中三者互斥；若出现异常叠加，键位层按 `edit` → `confirm` → `summary` 处理，渲染层按 `summary` → `edit` → `confirm` 绘制（后画者在上）。
- **Back 语义**：先尝试关闭弹层（正常流程里弹层已被键位层拦下）；再按屏返回：设置页先 `save_settings_now` 回主页，词库页/统计页回主页，帮助页按是否有会话回练习页或主页，主页/练习页无动作。
- **帮助页**：从练习页按 `?` 进入时 `session` 仍在，数字切页不生效；除滚动键外任意键都返回练习页。
- **退出与结算后**：只有主页 `q`（或宽屏侧栏「退出」）请求退出；其它非练习页面的 `q` 是返回（练习页的 `q` 是普通输入）。结算后会话与缓存都已清理，继续/重学用当前设置里的练习模式开启新一轮（自由练习结算后继续，回到设置中的常规模式，而不是继续自由模式）。

### 6.2 状态流转

```text
                ┌── q / 侧栏退出 ─▶ confirm(Quit) ── Enter/y ──▶ 保存并退出
                │
 Home ◀─────────┼── Esc/q/Back ── Stats / Settings / DictPicker
  │  ▲          └── ? ───────────▶ Help ── 任意键 ──▶ 回到来源页
  │  │
  │  └── Ctrl+C / 结算按钮 ────── Practice
  │                                 ▲
  └── Enter / f ────────────────────┘
                                    │ 一轮任务完成
                                    ▼
                             summary 结算弹层
                        Enter 继续 · r 重学 · q/Esc 主页
```

## 7. 练习会话集成

### 7.1 启动一轮练习

| 入口 | 任务来源 | 模式 |
| --- | --- | --- |
| `start_practice` | `create_study_task(...).task_words` | `settings.word_practice_mode` |
| `start_free_practice` | 同上 | 强制 `Free`，**不修改设置** |
| `start_practice_with_mode(mode)` | `Shuffle` 用 `create_shuffle_task`，其它用 `create_study_task` | 参数 `mode` |
| `continue_after_summary` | `session.create_next_task(..., was_complete)` | `settings.word_practice_mode` |
| `relearn_after_summary` | `session.create_repeat_task` | `settings.word_practice_mode` |

前三个入口先检查学习词典（否则 Toast `no_assets`）与任务是否为空（否则 `no_words_to_study`），再调用 `begin_session_with_task`：

1. 复制当前词典全部单词到 `all_words` 并用 `rng` 洗牌（自测干扰项与自由练习随机用）。
2. `PracticeSession::new(mode)`、挂上 `all_words`；`stat = PracticeState::default()`；`initialize_task(task, rng, &mut stat, now)` 初始化流程游标、词表与计时。
3. 置 `session`、`screen = Practice`、清空 `summary`/`identify_select`、新建 `TypingEngine`；`last_activity` 与 `last_timer_tick` 置为当前时间。
4. `after_advance()` 同步显示策略、生成自测题、必要时结算、安排防抖保存；`save_practice_now()` 立即落盘；`refresh_stats()`。

### 7.2 键入上下文与事件

`typing_context_parts` 取当前词、`view_state()` 的 `practice_type`/`is_word_masked`、`session.display.show_word_result`；无会话或当前词为空时返回 `None`，键入被忽略。`typing_settings` 委托 `PracticeSession::typing_settings`，把设置投影成 `TypingSettings`（`ignore_case`、`repeat_count`（由 `effective_repeat_count()` 得到）、`wait_ms`、`space_cooldown_ms`、`auto_next_word`、`input_wrong_clear`）。

`handle_typing_key` 把 `KeyInput` 交给 `typing.handle_key(key, &ctx, now)`，逐条处理返回的事件：

| `TypingEvent` | 应用层处理 |
| --- | --- |
| `Completed` | `advance_word(true)`：`is_typing = true` 让会话把本次推进计入 `input_word_number`（跳过、阶段推进等传 `false`） |
| `Wrong` | `session.on_type_wrong` 记错并写系统错词本；安排防抖保存 |
| `ShowWordResult(bool)` | 写回 `session.display.show_word_result`（默写/听写/自测揭示） |
| `Notice(DeleteReinput / SpaceContinue)` | 以 `Info` Toast 显示对应中文提示 |
| `ResetForRepeat` | `typing.reset()`（重复练习重新开始输入） |

### 7.3 推进与 `after_advance`

| 方法 | 会话调用 | 额外动作 |
| --- | --- | --- |
| `advance_word(is_typing)` | `session.next(base, stat, rng, is_typing)` | 返回的 `Notice` 逐条转 Toast，然后 `after_advance` |
| `prev_word` / `skip_step` / `skip_word` | `session.prev` / `session.skip_step` / `session.skip` | 重置键入 + `after_advance` |
| `random_write`（Ctrl+R） | `session.random_write(rng)` | 重置键入 + `after_advance` |

`after_advance` 的固定动作：`session.refresh()` 同步显隐策略（阶段变化清空遮罩/翻译覆盖与揭示状态），单词或阶段变化、或会话置位 `reset_word` 时 `typing.reset()`（进入组内巩固/错词巩固、切换步骤时当前单词可能不变，键入状态必须一并丢弃，否则当前词无法完成）；`session.update_question(rng)` 生成自测题；清空 `identify_select`；若 `session.complete` 则 `finish_session`；最后 `schedule_practice_save`。

### 7.4 自测操作

| 方法 | 前置 | 行为 |
| --- | --- | --- |
| `identify_know` / `identify_unknown` | 阶段为 `Identify` | 揭示单词；前者 `on_word_know` + `mark_correct`，后者记一次错误；不立即推进（空格继续） |
| `identify_mastered` | 同上 | `on_word_mastered`，重置键入并 `advance_word(false)` |
| `identify_choose(index)` | 同上 | 选项高亮；正确走 `on_word_know`+`mark_correct`，错误走 `on_type_wrong`；不立即推进 |

四个方法都只在 `practice_type() == Identify` 时生效，除 `mastered` 外都会安排防抖保存。

### 7.5 结算与离开

`finish_session`（只在 `session.complete` 且尚无 `summary` 时执行）：`session.settle(&mut base, &settings, &mut stat, now)` 写回 FSRS 卡片与词典统计；由 `stat` 计算 `SummaryData`（`spend_ms = stat.spend`，`accuracy = round((total - wrong) / total × 100)`，`total == 0` 记 100，`new_words`/`review` 取会话统计，进度三项取学习词典）；`save_dict_now()` → `clear_practice_cache()` → `refresh_stats()` → `refresh_task_preview()`。

| 结算去向 | 方法 | 结果 |
| --- | --- | --- |
| 继续 | `continue_after_summary` | `create_next_task` 生成下一轮；任务为空则提示并回主页；否则以当前设置模式开启新会话 |
| 重学 | `relearn_after_summary` | `create_repeat_task` 生成重复任务，以当前设置模式开启新会话 |
| 主页 | `summary_home` | 清空会话与结算、清练习缓存、回主页并刷新统计/任务 |

`leave_practice`（`Ctrl+C` 或结算继续失败时）在无结算的情况下先保存练习与词典，再清空会话、回主页、刷新缓存。结算弹层出现后练习缓存已被删除，因此「回主页」不再产生缓存文件。

## 8. 设置与主题交互

### 8.1 `activate_setting` 的分派

| `SettingKind` | 行为 |
| --- | --- |
| `Mode` | `cycle_practice_mode`：按「学习 → 自由练习 → 复习 → 随机复习 → 自测 → 默写 → 听写 → 学习」循环，保存设置并刷新任务预览 |
| `Theme` | `cycle_theme`：按 `ThemeRegistry::cycle_ids`（`auto` + 内置 + 自定义）循环，保存设置 |
| `SoundType` | `us` ⇄ `uk`，影响练习页音标取哪个字段，保存设置 |
| `Bool` | `toggle_setting_bool(key)` 翻转对应字段，保存设置 |
| `Number(target)` | 打开 `EditState`，`buffer` 预填当前值的十进制字符串（尚未落盘） |
| `Action(action)` | 打开确认弹层，文案取 `confirm_message(action)`：`ResetPractice` 为「确定清空当前练习进度吗？」，`ClearAllData` 为「该操作将会清除所有数据，确认继续？」 |

分派结束后统一执行 `save_settings_now()` 与 `refresh_task_preview()`；`Bool`/`SoundType` 靠这一步落盘。

### 8.2 布尔项映射

`auto_random_review`→`auto_add_random_review_when_no_due`、`show_etymology`→`show_etymology_and_rel_words`、`big_word`→`tui_big_word`；其余 7 个 key（`ignore_simple_word`、`auto_next_word`、`input_wrong_clear`、`ignore_case`、`show_word_question`、`show_near_word`、`show_usage_tips`）与字段同名。`show_usage_tips` 控制练习页底部「提示：…」行是否显示（Toast 仍正常显示）。

### 8.3 数字项的夹取范围

设置页显示的数字值来自 `App::setting_number_value(target)`（比例、阈值、毫秒数直接取值，`Retention` 转成百分数）；`EditSave` 解析输入（空串或非法值按 0），`apply_number_value` 按目标夹取后写回，并刷新任务预览：

| `EditTarget` | 字段 | 范围 |
| --- | --- | --- |
| `PerDay` | 学习词典 `per_day_study_number` | `1..=词表长度`（至少 1） |
| `Ratio` | `word_review_ratio` | 0 ~ 10 |
| `Repeat` | `repeat_count` | 1 ~ 100；100 表示改用 `repeat_custom_count`（下一行可编辑） |
| `RepeatCustom` | `repeat_custom_count` | 1 ~ 100；仅当 `repeat_count == 100` 时参与实际重复次数 |
| `WaitMs` / `SpaceCooldown` | `wait_time_for_change_word` / `space_cooldown_time` | 0 ~ 10000（毫秒） |
| `EasyLimit` / `GoodLimit` / `HardLimit` | `fsrs_easy_limit` 等 | 0 ~ 20 |
| `Retention` | `fsrs_parameters.request_retention` | 50 ~ 99（输入百分数，存为 0.50 ~ 0.99） |

每日词数不在 `SETTING_ROWS` 中；主页与词库页的 `+`/`-` 通过 `adjust_daily_target(±5)` 修改同一字段，范围 `1..=词表长度`，变化时保存词典、刷新任务预览并 Toast「每日词数：N」。

## 9. 持久化时机

### 9.1 保存函数

| 函数 | 行为 |
| --- | --- |
| `save_dict_now` | `store.save_dict(&boot.base)`；`readonly_dict` 为真（存档版本过高）时直接返回 |
| `save_settings_now` | `store.save_settings(&boot.settings)`；`readonly_settings` 为真（设置存档版本过高）时直接返回 |
| `save_practice_now` | 无会话时返回；`session.build_cache()` 后把当前 `stat` 写进 `stat_store_data`，`cache_to_compact` 后写 `practice-word.json`，并清除 `pending_practice_save` |
| `schedule_practice_save` | 只设 `pending_practice_save = now + 500ms`，落盘由 `on_tick` 完成 |
| `save_all` | 依次保存词典、设置；若仍有未结算会话则保存练习缓存 |
| `clear_practice_cache` | 删除 `practice-word.json` 并清除防抖 deadline |
| `clear_all_data` | 见 9.3；把两份存档恢复默认并删掉练习缓存 |

保存失败不会 panic：统一转成 `Error` Toast（如「保存词典失败: ...」）。

### 9.2 触发时机

| 触发 | 保存动作 |
| --- | --- |
| `begin_session_with_task` | 立即保存练习缓存 |
| 每轮单词推进、键入错误、自测操作 | 安排 500 ms 防抖保存 |
| 本次 tick 累计了学习时长且跨过 30 秒整数倍（`on_tick`） | 安排 500 ms 防抖保存 |
| 防抖到期 | 保存练习缓存并清除 deadline |
| `Ctrl+C` | 立即保存练习缓存 + 词典，随后离开练习 |
| `Ctrl+Enter`（重学本组） | 新会话建立时立即保存练习缓存 |
| `finish_session` | 立即保存词典，删除练习缓存 |
| 结算继续 / 重学 | 新会话立即保存练习缓存 |
| 主页/词库页 `+`/`-`、切换词库、`PerDay` 数字项 | 立即保存词典 |
| 设置页任意修改、退出设置页 | 立即保存设置 |
| 确认退出 / 事件循环结束 | `save_all` |
| 窗口缩放 | 不保存 |

`readonly_dict` / `readonly_settings` 分别阻止词典与设置写回，两者都来自「存档版本过高」；练习缓存不受影响，仍可按需保存/删除。

### 9.3 `clear_all_data`

设置页「清除所有数据」（`ConfirmAction::ClearAllData`）走 `App::clear_all_data`，对齐上游 `dataSyncPersistence.clear()`——上游把 base store 与 setting store 一起 `setState(getDefault*())` 并清空练习缓存，TUI 照此实现：

1. `readonly_dict` 或 `readonly_settings` 为真时**直接返回**，只弹 `Error` Toast「存档版本高于当前程序，只读模式无法清除」；否则会出现界面报成功、重启后数据原样回来的假象。
2. `clear_practice_cache()`，并把会话 / 键入 / `stat` / 结算 / 自测选择 / 统计滚动 / 词库光标全部复位。
3. `boot.base = BaseState::default()` 且 `load = true`（上游 `clear()` 同样先置 `load`），`boot.settings = SettingState::default()`。
4. `ensure_study_dict()`：默认状态的 `study_index = -1`，按全新安装的路径重新选一本词典（有 `cet4` 用它，否则第一本词库），失败写 `fatal`。
5. `save_dict_now()` + `save_settings_now()` + `refresh_stats()` + `refresh_task_preview()`；Toast 取 i18n 的 `clear_success`（「清除成功」）。

清完仍停留在设置页。`tests/end_to_end.rs` 的 `clear_all_data_resets_dict_settings_and_cache` 覆盖了「练完一轮 → 清除 → 内存与落盘都回到默认」。

## 10. 确定性与测试接口

- `app::build_for_test(paths, cli)` 等价于 `App::bootstrap`，不创建终端、不安装 panic hook，可直接读写 `App` 公开字段。
- 随机性由 `seed` 决定：`--seed N` 或测试里 `Cli.seed` 固定 `StdRng`；`App::bootstrap`、`begin_session_with_task`、`refresh_task_preview`（内部调用 `create_study_task`）都会消耗随机数，因此**同一路径与种子**下任务与自测题可复现，中间多一次预览刷新会改变后续抽签结果。
- 时间完全由参数驱动：`on_tick(now)` 接受毫秒时间戳，测试可注入任意 `now` 推进计时、Toast 过期与防抖保存；`on_key`/`on_mouse` 直接吃 crossterm 事件。
- 可读状态包括 `screen`、`session`（`data.index`、`current_word()`、`complete`、`mode` 等）、`stat`、`summary`、`edit`、`confirm`、`toasts`、`boot`、光标与滚动字段；私有字段 `last_timer_tick`/`pending_practice_save` 只能通过时间推进间接触发。
- 测试辅助：`tests/common/mod.rs` 的 `Fixture` 在临时目录生成 12 词的 `toy` 词库与独立 `home`；`app::aggregate_stats` 把 `BaseState` 的逐日统计聚合成 `(day, spend, new, review, wrong)`。渲染测试用 `TestBackend` + `app.render(frame)`，驱动方式与真实事件循环一致（见 [渲染层](ui.md) 与 [开发指南](development.md)）。

## 11. 扩展指南

1. **新增按键 / Action**：`action.rs` 加变体并在 `apply_action` 实现；非练习页映射到 `keymap.rs` 的 `screen_action`/`overlay_action`，练习页在 `on_practice_key` 加分支；需要点击就在对应 `targets()` 加 `Target(rect, Action)`；补 `tests/app_drive.rs` 与 `tests/ui_snapshots.rs`。
2. **新增页面**：`Screen` 加变体，`ui::render` 与 `ui/hit.rs` 补分支；需要侧栏时扩展 `ui/shell.rs` 的 `Nav`（`from_screen`/`screen`/`label`/`key`/`ALL`/`MAIN`）；在 `screen_action` 注册键位；页面实现 `layout()` + `render()` + `targets()` 并登记渲染快照。
3. **新增设置项**：`domain/types.rs` 的 `SettingState` 加字段与默认值 → `SETTING_ROWS` 加行（数字项加 `EditTarget` 并同步 `setting_number_value`/`apply_number_value` 与夹取范围，布尔项同步 `toggle_setting_bool`）→ `ui/settings.rs` 的 `SECTIONS` 与格式化 → `domain/i18n.rs` 文案 → [设置项参考](settings.md) 与测试。
4. **新增弹层**：`App` 加 `Option<...>` 字段与打开它的 `Action`；`keymap::overlay_action` 按优先级插入键位并同步 `on_key` 的模态判断；`ui/mod.rs` 的 `modal_open` 与渲染顺序、`ui/overlays.rs` 的 `layout`/`render`/`targets`/`target_action` 同步；补最小终端布局单测与渲染快照。

## 相关文档

- [文档索引](README.md)
- [渲染层](ui.md)
- [领域逻辑](domain.md)
- [存档与数据格式](persistence.md)
- [总体架构](architecture.md)
