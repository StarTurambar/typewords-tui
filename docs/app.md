# 应用层（app）

应用层把领域层的状态机与终端输入输出接在一起：它持有唯一的 `App` 实例，把键盘、鼠标与定时器事件翻译成领域操作，决定何时读档与存档，并在每一帧把只读状态交给渲染层。流程推进、FSRS 调度、键入判定等算法在 `domain` 中实现，布局与像素在 `ui` 中实现；应用层负责「什么时候调用谁」。

> 适用范围：`src/app/` 的四个文件，以及启动装配相关的 `src/main.rs`、`src/lib.rs`、`src/cli.rs`、`src/paths.rs`、`src/bootstrap.rs`；对应版本：typewords-tui 0.2.0。

## 1. 职责与边界

### 1.1 文件清单

| 文件 | 内容 |
| --- | --- |
| `src/app/mod.rs` | `App` 结构体，`Screen`/`Toast`/`EditState`/`ConfirmState`/`SummaryData`/`SummaryKind`/`SettingKind`/`SETTING_ROWS`；启动、事件循环、练习会话集成、设置动作、`on_tick`、`run`/`build_for_test` |
| `src/app/action.rs` | `Action` 枚举与 `App::apply_action`：非练习页、鼠标、弹层与测试共用的状态迁移入口 |
| `src/app/buttons.rs` | 键入练习页底部的动作按钮：`TypingButton`、两页的按钮表（`PRACTICE_BUTTONS` / `ARTICLE_BUTTONS`）、列数 `columns`、键盘选中 `handle_typing_button_key` 与执行入口 `activate_typing_button` |
| `src/app/keymap.rs` | 弹层与各屏幕的键位到 `Action` 映射，不直接修改状态；练习页的键位改由 `src/domain/shortcut.rs` 的键位表驱动 |
| `src/app/mouse.rs` | 鼠标事件 → `ui::hit::action_at`/`wheel_action` → `Action` |
| `src/app/focus.rs` | 所有套外壳页面的键盘焦点：`FocusDir`、`set_screen`（切屏唯一入口）、`focus_default`（进入页面默认选中内容区）、`move_focus`/`switch_focus_pane`/`activate_focus`/`clear_focus`/`focus_home_default`；保存焦点身份（`FocusId`）与主页内容区上次选择（`last_content_focus`），两栏主轴、视觉分带与几何在 `ui::focus` |
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
| 键盘焦点 | `focus: Option<ui::focus::FocusId>`、`last_content_focus: Option<ui::target::ContentId>`、`viewport: Rect` | 焦点存稳定身份（导航栏 `NavId` / 主页内容区 `ContentId` / 其它页面 `Page`）。导航栏与主页内容区每次按键与 `Enter` 前按当前布局重新解析，失效即清空；`Page` 由页面自己的光标表示，不依赖几何。`last_content_focus` 供主页跨栏回内容区时恢复；`viewport` 是最近一次绘制区域，按键时按它算布局（见 5.4 与 6.1） |
| 会话 | `session: Option<PracticeSession>`、`typing: TypingEngine`、`stat: PracticeState`、`identify_select` | 练习中才有 `session`；开始新一轮时 `typing`/`stat` 重建或重置 |
| 随机源 | `rng: Rng`、`seed: u64` | `StdRng`；`--seed` 固定后整条事件序列可复现 |
| 提示与弹层 | `toasts: Vec<Toast>`、`quit: bool`、`edit`、`confirm`、`summary` | 最多保留 4 条 Toast；`quit` 由退出确认置位；三类弹层正常交互中互斥 |
| 显示开关 | `show_toolbar`、`show_panel`、`show_meta` | 练习页 Ctrl+B / Ctrl+L / Ctrl+E 的状态（仅本次进程） |
| 动作按钮 | `typing_button: Option<usize>` | 键入练习页底部动作按钮的选中下标（行优先，见 `app::buttons`）。`None` = 没有选中，方向键与 `Enter` 仍属于键入 / 键位表；`set_screen` 每次切屏清空它，工具栏关闭时 `typing_buttons()` 返回空切片，按键路径随即把选中清掉 |
| 统计缓存 | `task_preview: Option<(usize, usize, usize)>`、`today_spend`、`today_new`、`today_review`、`today_wrong` | 主页「今日任务」「统计」区块的预计算值 |
| 生命周期 | `fatal: Option<String>`、`last_activity`、`last_timer_tick`、`pending_practice_save` | 后三个私有：空闲判定、1 秒计时节拍、防抖保存截止时间 |

访问器：`base()`、`settings()`、`theme()`、`theme_label()`、`study_progress()`。`theme()` 每次调用都会 `ThemeRegistry::resolve` 并构造 `ui::Theme`（13 个颜色的小结构体，开销可忽略）。

### 2.2 Screen

| 变体 | 对应页 | 侧栏 `Nav` | 数字快捷键 |
| --- | --- | --- | --- |
| `Home` | 主页 | `Nav::Home`（`Practice` 也映射到它） | 1 |
| `Practice` | 练习页（全屏，不套外壳） | — | 无（练习中数字属于键入/自测） |
| `WordTest` | 单词测试页（一题 4 选 1） | `Nav::Home` | 无（数字用于选答案） |
| `Words` | 词条管理页（搜索 / 编辑 / 增删 / 批量标记） | `Nav::Dicts` | —（由词库页进入） |
| `Articles` | 文章背诵页（书本列表 + 文章列表） | `Nav::Articles` | 2（也可由主页 `a` 进入） |
| `ArticlePractice` | 文章跟打页（全屏，不套外壳） | — | 无 |
| `Flows` | 练习流程页（内置 + 自建流程） | `Nav::Home` | —（由主页 `Ctrl+F` 进入） |
| `DictPicker` | 词库页 | `Nav::Dicts` | 3 |
| `Stats` | 统计页 | `Nav::Stats` | 4 |
| `Settings` | 设置页 | `Nav::Settings` | 6 |
| `Shortcuts` | 快捷键编辑页 | `Nav::Settings` | —（由设置页进入） |
| `Help` | 帮助页 | `Nav::Help` | 5 |

### 2.3 提示与弹层数据

- `Toast`：`message`、`level: NoticeLevel`（Success/Warning/Error/Info）、`until`（毫秒时间戳）。`App::toast` 取当前时间 + 3200 ms，超过 4 条丢最旧；`current_toast(now)` 从尾部找第一条未过期项。
- `ConfirmAction`：`Quit`、`ResetPractice`、`ClearAllData`；`ConfirmState` 为 `message` + `action` + `focus: ConfirmFocus`（`Accept` / `Cancel`，默认 `Accept`），文案统一由 `confirm_message(action)` 产出（退出 / 清空练习进度 / 清除所有数据各一条）。方向键（含 `j` / `k` 别名）/ `Tab` 在「确认 / 取消」之间移动选中（到边停、不回绕），`Enter` / 空格执行当前选中的按钮。
- `SummaryData`：`kind: SummaryKind`（`Word`（默认）/ `Article { article_id, title }`）、`spend_ms`、`accuracy`（`total == 0` 记 100）、`new_words`、`review`、`wrong`、`last_learn_index`、`length`、`complete`、`focus: SummaryButton`（`Continue` / `Relearn` / `Home`，默认 `Continue`）。`Word` 是单词会话（`finish_session`）与单词测试（`finish_word_test`）的口径：`last_learn_index` / `length` / `complete` 来自当前学习词典的总进度，`wrong` 是本轮错词数；`Article` 是文章跟打（`finish_article_practice`）的口径：`last_learn_index` 是完成句数、`length` 是总句数、`wrong` 是错句数（出现过错误的句子数）、`review` 固定 0——**文章结算的指标是句数而不是词数**，没有「新词 / 复习」维度；`article_id` / `title` 供结算文案与「重打本篇」使用。`focus` 是结算按钮焦点：方向键（含 `j` / `k` 别名）/ `Tab` 在三个按钮之间移动（到边停、不回绕），`Enter` / 空格执行当前选中的按钮；每次结算都重新构造 `SummaryData`，焦点自然回到默认的「继续」。
- `EditState`：`target: EditTarget` + `buffer: String`（ASCII 数字，最长 6 位；自由文本项另有限制）。`EditTarget` 有 `PerDay`、`Ratio`、`Repeat`、`RepeatCustom`、`WaitMs`、`SpaceCooldown`、`EasyLimit`、`GoodLimit`、`HardLimit`、`Retention`、`Note`、`WordSoundSpeed`、`DictName` 十三个取值；`PerDay` 对应设置页「复习 → 每日词数」行（`daily_words`），保存时夹在 `1..=词表长度` 并写 `dict.json` 的当前学习词典，`Note` / `DictName` 是自由文本（单词笔记、新建词典的名字）。

### 2.4 设置表与 `SettingKind`

`SETTING_ROWS` 是设置页唯一数据源，共 32 行（30 个配置项 + 2 个动作项），数组顺序即渲染顺序。

| `SettingKind` | `SETTING_ROWS` 中的 key（按出现顺序） |
| --- | --- |
| `Mode` / `Theme` / `SoundType` / `AudioSource` | `word_practice_mode` / `theme` / `sound_type` / `tts_source` |
| `Bool` | `big_word`、`keyboard_sound`、`effect_sound`、`word_sound`、`article_sound`、`auto_random_review`、`ignore_simple_word`、`auto_next_word`、`input_wrong_clear`、`ignore_case`、`show_word_question`、`show_etymology`、`show_near_word`、`show_usage_tips` |
| `Number` | `daily_words`→`PerDay`、`word_review_ratio`→`Ratio`、`repeat_count`→`Repeat`、`repeat_custom_count`→`RepeatCustom`、`wait_time`→`WaitMs`、`space_cooldown`→`SpaceCooldown`、`fsrs_easy_limit`→`EasyLimit`、`fsrs_good_limit`→`GoodLimit`、`fsrs_hard_limit`→`HardLimit`、`request_retention`→`Retention` |
| `Shortcuts` / `Action(ConfirmAction)` | `shortcut_settings`（跳到快捷键编辑页）、`reset_practice_progress`→`ResetPractice`、`clear_all_data`→`ClearAllData` |

设置页按 key 归入「通用 / 音频 / 复习 / 键入 / 显示 / FSRS / 其他」七个分区（复习分区起点是 `daily_words`；起点见 `src/ui/settings.rs` 的 `SECTIONS`），值格式化由渲染层完成，细节见 [设置项参考](settings.md)。

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
5. `--reset-practice` 删除两份断点缓存并提示「已清空练习进度」；否则只调用 `refresh_article_resume()` 把文章断点读进内存（损坏/版本不支持时提示），**不切换页面**（见 3.3）。
6. `ensure_study_dict()`：当前词典缺词表时按 id 重新加载，找不到就用 `cet4` 或第一本词库；失败写 `fatal = "加载词库失败: ..."`。
7. `refresh_stats()` 与 `refresh_task_preview()` 计算主页缓存，返回 `App`。

### 3.3 恢复练习缓存与 fatal

启动时**不恢复会话、也不切换页面**：`App::bootstrap` 只调用 `refresh_article_resume()` 把 `practice-article.json` 读进内存（损坏/版本不支持时弹 Warning 并忽略），因此首个画面稳定落在 `Home`。早期实现会在启动时直接调 `try_restore_practice` / `try_restore_article_practice`：初始画面取决于磁盘上残留了哪个缓存；两份缓存同时存在时还会叠成「单词会话 + 文章练习」，按 `Esc` 后停在一张没有状态的空白跟打页。

单词练习的恢复逻辑抽在 `restore_practice_from_cache`（返回是否成功）：要求学习词典已有词表且 `cache_from_compact` 能还原，然后用缓存里的 `flow_id` 反查模式、新建 `PracticeSession`、把当前词典全部单词洗牌后交给 `apply_cache`。成功则 `screen = Practice`、重置键入；失败静默丢弃（不删文件，等待下一次练习覆盖）。

`try_resume_saved_practice`：主页按 `Enter`（启动后第一次，或同一次运行内退出练习页后）时，若有断点缓存就接着上次的练，而不是重新生成任务。对齐上游 `words.vue` 的 `systemPractice()`——它只在自由模式下 `resetCache=true`，其余模式把现存的 `practiceData` 传给练习页，练习页优先 `applyPracticeCache`。要求断点里的 flow 模式与当前模式一致，否则不恢复（流程阶段对不上）；自由练习显式开新一轮。

文章跟打：`refresh_article_resume` 在启动与 `open_articles` 时读取缓存；文章列表把缓存对应的那一行标成「上次第 N 句」，`start_article_practice` 发现选中文章的 id 与缓存一致时从记录的句子续打（下标会 clamp 到当前句数内），否则从头开始。缓存里的文章 id 在当前书目里找不到时直接忽略，不硬套。

状态不变量：`session` 与 `article_practice` 不会同时为 `Some`；`Screen::ArticlePractice` 只在 `article_practice` 存在时进入。`back()` 从跟打页返回时直接切到 `Screen::Articles`，不经过会被会话拦下的 `navigate_to`；渲染层在缺少状态时也会画出「没有进行中的文章练习 / Esc 返回」而不是空白页。

> **统计落盘时机**：上游的 `flushStatToStore` 只在**结算**与**重置缓存**两处调用，中途退出后继续练时统计留在缓存里不落盘。TUI 同此——`flush_practice_stats` 的调用点也相应调整。

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
- 每帧在 `on_tick` 之后、处理事件之前把 `viewport` 同步成 `terminal_area()`：键盘焦点要按布局算几何，而 `on_key` 不经过渲染，只能靠这个字段知道当前终端尺寸（测试与 `examples/shot` 用 `App::set_viewport` 显式设置）。
- `terminal_area()` 由 `crossterm::terminal::size()` 构造 `Rect`，失败回退 `80 × 24`；用于鼠标命中与 `viewport` 同步（焦点导航按它算布局），渲染仍以 `Frame::area()` 为准。退出路径只有两条：主页 `q` → 确认弹层 → `quit = true`，或 `event_loop` 返回错误；两者都走保存与终端恢复。

### 4.2 `on_tick(now)` 的四块逻辑

| 顺序 | 逻辑 | 细节 |
| --- | --- | --- |
| 1 | Toast 过期 | 所有 Toast 都满足 `until <= now` 时清空 `toasts`；渲染侧用 `current_toast(now)` 过滤未过期项 |
| 2 | 练习计时 | 单词练习：`screen == Practice` 且有 `session`；距 `last_timer_tick` 满 1000 ms 时先记下 `spend`，再 `timer::tick(...)`（累加 1000 ms，空闲 180 秒自动暂停）；若本次 tick 确实增加了时长且新值跨过 30 秒整数倍，则 `schedule_practice_save()`。文章跟打：`screen == ArticlePractice` 且有 `article_practice`，同样按 1000 ms 推进 `spend`（30 秒整点排一次 `schedule_article_save()`）；`article_practice.paused`（`Ctrl+,`）为真时只把 `last_timer_tick` 推到 `now`、不累加，恢复后的第一次 tick 不会把暂停的整段算进用时 |
| 3 | 键入定时 | 同一条件下构造 `TypingContext` 并调 `typing.on_tick(&ctx, now)`，事件交给 `handle_typing_events`（自动下一词、重复练习、错误清除等定时器） |
| 4 | 防抖保存 | `pending_practice_save` 到期（`now >= deadline`）时 `save_practice_now()` |

第 2 块的保存条件比较 tick 前后的 `spend`：暂停/空闲时 `spend` 不变，不会重复调度；只有真正累加并跨过 30 秒整数倍时才请求防抖保存。

## 5. 输入路由

### 5.1 `on_key` 的分发顺序

1. 更新 `last_activity = now_ms()`（供空闲暂停使用）。
2. 若 `edit`/`confirm`/`summary` 任一存在：调 `keymap::overlay_action`，有返回值就 `apply_action`，然后**直接返回**——弹层是模态的，未识别的键被吞掉；优先级 `edit` → `confirm` → `summary`。
3. `screen == Practice` 时进入 `on_practice_key`（练习页键盘不经过 `Action`）。
4. `screen == ArticlePractice` 时进入 `on_article_practice_key`——跟打页与练习页同一套分流：先查键位表（只认两页共用的 `PlayWordPronunciation` / `ToggleShowTranslate` / `ToggleTheme`），再走 TUI 专有键位（`Ctrl+C` / `Ctrl+,` / `?` / `Esc`），剩下的字符才进 `article_typing_key`。跟打键入同样不经过 `Action`。

两个键入页的 `on_*_key` 第一步都是 `App::handle_typing_button_key`（见 5.4）：`←`/`→`/`↑`/`↓` 选中 / 移动底部动作按钮、`Enter` 执行选中的按钮，返回 `true` 时按键到此为止；返回 `false` 时按键继续原来的分流，并且按钮选中已被清掉（继续打字或按快捷键之后 `Enter` 回到「收藏」等旧语义）。
5. 其它屏幕调 `keymap::screen_action`，返回 `Action` 后交给 `apply_action`。

`screen_action` 先处理练习中打开的整屏帮助页（保持「滚动之外任意键返回」），再判断数字切页：`session.is_none()`、当前页属于 `Home` / `Articles` / `DictPicker` / `Stats` / `Settings` / `Help`、无修饰键、按 `1`~`6` 时直接产生 `Navigate`：`1` 单词、`2` 文章、`3` 词库、`4` 统计、`5` 帮助、`6` 设置。之后先过统一的焦点键（`focus_action`：`↑`/`↓`/`←`/`→` 与 `j`/`k` → `FocusMove`、`Tab`/`Shift+Tab` → `SwitchFocusPane`、`Enter` → `ActivateFocus`，只在 `ui::focus::enabled` 为真时生效），各页面分支只保留自己的特有键位。

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
| `Ctrl+P` | 用系统 TTS（或设置里选的有道发音）朗读当前词；换词时也会自动朗读（见 [设置项参考](settings.md) 的音频一节） |
| `Ctrl+R` | 随机默写当前词表：打乱当前节点词表、设为遮罩、从第一个词开始，并重置键入 |
| `Ctrl+B` / `Ctrl+L` / `Ctrl+E` | 切换工具栏 / 右侧词表 / 例句与词源 |
| `Ctrl+C` | 保存练习与词典，退出练习回主页 |
| `Ctrl+N` 或 `Ctrl+→` / `Ctrl+←` | 下一个词（`advance_word(false)`）/ 上一个词 |
| `Ctrl+J` 或 `Shift+→` | 跳到下一阶段（`skip_step`） |

`Ctrl+J` 是 `Shift+→` 的 ASCII 别名；部分终端会吞掉 `Ctrl+方向键` 或 `Alt+数字`，帮助页也提示了这一点。

收藏走 `toggle_current_word_collect`：切换 `boot.base` 的收藏词典后返回一条 Toast（「已收藏「word」」/「已取消收藏「word」」），由 `handle_practice_shortcut` 弹出。收藏表存在 `dict.json`，练习页只在离开 / 结算 / 退出时落盘（与其它练习内标记一致），但界面当场就能确认——单词上方的「★ 已收藏」徽标与收藏按钮的星标由渲染层读同一个 `PracticeSession::is_collect`。`Shift+Enter` 的「收藏到指定词典」同样按「加入 / 移出」给出不同提示（`select_collect_dict`）。

`Ctrl+P`（朗读）、`Ctrl+Z`（翻译）、`Ctrl+Q`（主题）、`Ctrl+,`（暂停计时）、`Ctrl+C`（保存并离开）、`?`（帮助）与[文章跟打页](#53-文章跟打页键位on_article_practice_key)**同一个键**做同一件事：前三个从键位表取（改绑后两页一起变），后三个是 TUI 专有键位。两页唯一的键位差异是 `Esc`——练习页的它被上游绑成「显示单词」，跟打页没有遮罩，`Esc` 保持全站通用的「返回上一层」。

### 5.3 文章跟打页键位（`on_article_practice_key`）

跟打页和练习页一样不经过 `screen_action`：`App::on_key` 交给 `App::on_article_practice_key`，它按「键位表 → TUI 专有键位 → 跟打输入」三段分流。键位表里只认两页共用的三个动作；其它动作（跳过 / 收藏 / 自测 / 逐句播放……）在跟打页没有对应物，一律让位给输入——`1`~`3`、空格、`Enter` 仍然是跟打内容或空操作，不会被键位表吞掉。

| 键 | 行为 |
| --- | --- |
| 字母、数字 | 记到当前句的光标位置（`SentenceTyping::input`）；打错也照样前进，错误标记留在打错的位置 |
| 空格 | 词与词之间的空格是**可选**目标：光标会停在待输入的空格上，按空格记为正确输入；不打空格、直接打下一个单词的首字母时自动补成正确输入（见下一行）。字母位置上的多余空格被忽略 |
| 空格位置上按了别的字母 / 数字 | 视为空格已正确输入：先补上空格，再把这个字符记到下一个目标位置——打对则不计错，打错则错字落在下一个单词的首字母位置（红色显示实际按下的字符，不带下划线），后面的字母不会错位。空格位置永远不会出现错字，因此「一句话没按空格、字母全打对」不计入错误句 |
| 标点、符号 | 不是跟打内容（`is_typing_char` 为假）：忽略，不计错也不前进 |
| `Backspace` | `SentenceTyping::backspace` 删除最后输入的字符；本句的 `wrong` 标记不因此清除 |
| `Ctrl+P`（`PlayWordPronunciation`） | `speak_current_sentence` 用系统 TTS 朗读当前句；改绑后新键位同样生效 |
| `Ctrl+Z`（`ToggleShowTranslate`） | 切换当前句译文面板（关掉后把它占的 4 行还给正文）；与练习页的「切换翻译」是同一个键、同一张键位表 |
| `Ctrl+Q`（`ToggleTheme`） | 切换主题；与练习页同一个键、同一张键位表 |
| `Ctrl+,` | 暂停 / 恢复计时：暂停只停止累加 `spend`，状态行显示「已暂停」 |
| `Ctrl+C` | `back()`：保存文章断点并返回文章列表（与练习页的「保存并返回主页」同义） |
| `?` | 整屏帮助页（`help_scroll = 0`）；返回时回到跟打页，跟打进度不变 |
| `Esc` | `back()` 保存文章断点并返回文章列表；跟打页没有遮罩，`Esc` 保持全站通用的「返回」语义 |
| 其它（含未映射的 `Ctrl` / `Alt` 组合） | 忽略 |

### 5.4 底部动作按钮（`app::buttons`）

两个键入页各有一张按钮表，按钮与键位表里的动作一一对应，执行时走与键盘**同一条**处理路径：

| 页面 | 按钮（行优先，两行） | 列数 |
| --- | --- | --- |
| 练习页 `PRACTICE_BUTTONS` | 显示 / 跳过 / 掌握 / 收藏 / 翻译、默写 / 暂停 / 重学 / 换组 / 返回 | `10.div_ceil(2) = 5` |
| 跟打页 `ARTICLE_BUTTONS` | 朗读 / 翻译 / 主题、暂停 / 帮助 / 返回 | `6.div_ceil(2) = 3` |

- `TypingButton::shortcut()` 给出对应的 `ShortcutAction`（显示→`ShowWord`、跳过→`Ignore`、掌握→`ToggleSimple`、收藏→`ToggleCollect`、翻译→`ToggleShowTranslate`、默写→`ToggleDictation`、重学→`RepeatChapter`、换组→`NextChapter`、朗读→`PlayWordPronunciation`、主题→`ToggleTheme`）；`Pause` / `Help` / `Return` 没有键位表条目，`fixed_key()` 返回 `ui::TUI_PAUSE_KEY` / `TUI_HELP_KEY` / `TUI_EXIT_KEY`。
- `handle_typing_button_key` 只处理两件事：有选中时的 `Enter` 执行按钮（消费按键），以及「其它按键先清空选中再继续分发」。`Enter` 没有选中时不被消费——练习页的 `Enter` 仍是「收藏」。
- **方向键在按键流水线末尾处理**：`on_practice_key` / `on_article_practice_key` 在「键位表 → TUI 键位 → 键入引擎」都没消费该键之后，才用 `buttons::plain_arrow_direction` 判断它是不是裸方向键，是就交给 `move_typing_button`。因此用户把动作绑到 `←`/`→`/`↑`/`↓` 时**键位表优先**，按钮条不会抢走显式绑定；没有绑定时方向键才是按钮条的选中键。
- `move_typing_button`：无选中时任一方向键落在第 0 个按钮；有选中时 `←`/`→` 在本行内移动、`↑`/`↓` 跨行并保持列，到边停、不回绕（列数由 `buttons::columns` 统一给出，绘制与移动不会各算一套）。
- `activate_typing_button(index)`：键盘 `Enter` 与鼠标点击 `Action::TypingButton(index)` 共用。先把选中落到该下标，再按当前页分派——练习页走 `handle_practice_shortcut` / `toggle_practice_pause` / `open_typing_help` / `save_and_leave_practice`，跟打页走 `handle_article_shortcut` / `toggle_article_pause` / `open_typing_help` / `back()`；换页的分支由 `set_screen` 顺手清空选中。
- `open_typing_help`、`toggle_practice_pause`、`save_and_leave_practice` 都是从原来的 `?` / `Ctrl+,` / `Ctrl+C` 分支里抽出来的同一个函数：按钮与快捷键不会各写一份实现。
- 键盘与鼠标共用同一个入口，因此「点哪个按钮就执行哪个」「点完方向键从它继续」不需要额外状态；按钮条不参与 `ui::focus` 的两栏焦点模型（练习页 / 跟打页本来就不在 `supports_focus` 里）。

输入长度达到当前句 `typing_target`（字母、数字与词间空格按顺序排成的字符序列，见 `Sentence::typing_cells`；词间空格可省略，省略时 `SentenceTyping::input` 自动补成正确输入，不会因此记错）的字符数就调用 `finish_article_sentence`：本句出现过错误时 `wrong_sentences += 1`，然后推进句子下标、排一次防抖保存并朗读下一句；`advance_over_empty_article_sentences` 会跳过没有跟打内容的纯标点 / 空白句（按已完成计），跳过之后到篇尾则直接结算。结算走 `finish_article_practice`，按文章口径生成 `SummaryKind::Article`（完成句数 / 总句数 / 错句数 / 文章标题），去向是文章列表 / 重打本篇 / 主页，见 7.5。

### 5.4 其它屏幕键位（`screen_action`）

所有套外壳页面（`ui::focus::supports_focus`）共用一组焦点键，由 `focus_action` 在页面分支之前统一处理：`↑`/`↓`/`←`/`→`（`j`/`k` 是上下别名）→ `FocusMove`，`Tab`/`Shift+Tab` → `SwitchFocusPane`，`Enter` → `ActivateFocus`。练习会话进行中、有弹层、词条搜索框获得焦点时 `ui::focus::enabled` 为假，这组键交给原页面逻辑。

| 屏幕 | 特有键 → `Action`（焦点键除外） |
| --- | --- |
| 主页 | `Esc`→`ClearFocus`；`f`→`StartFreePractice`；`d`/`s`/`t`→`Navigate`；`a`→`OpenArticles`；`Ctrl+F`→`OpenFlows`；`m`→`OpenModePicker`；`?`→Help；`q`→`QuitRequest`；`+`/`=`→`AdjustDaily(5)`，`-`→`AdjustDaily(-5)` |
| 词库页 | `Esc`/`q`→`Back`；`+`/`-`→`AdjustDaily(±5)`；`w`→`OpenSelectedWords`；`a`→`OpenArticles`；`n`→`WordsNewDict` |
| 统计页 | `Esc`/`q`→`Back`；`PgUp`/`PgDn`→`StatsScroll(∓5)`；`,`/`.`→`StatsMonthMove(∓1)`；`v`→`StatsToggleView` |
| 设置页 | `Esc`/`q`→`Back`；`Space`→`SelectActivate` |
| 流程页 | `Esc`→`Back`（编辑中→`FlowEdit`）；`]`→`FlowNextNode`；`n`→`FlowDuplicate`；`e`→`FlowEdit`；`Delete`→`FlowDelete`；编辑中 `a`/`d`/`s`/`g`→加步骤 / 删步骤 / 乱序 / 组内巩固 |
| 文章页 | `Esc`/`q`→`Back` |
| 词条页 | 搜索框聚焦时输入搜索词，`Esc` 退出搜索；否则 `Esc`/`q`→`Back`、`PgUp`/`PgDn` 翻 10 行、`Home`/`End` 到首/末、`Space`→勾选当前词、`/`→搜索、`a`/`d`/`n`/`o`/`c` 与 `1`/`2`/`3` 是词条操作（见第 8 节与使用手册） |
| 快捷键页 | `Esc`/`q`→`Back`；`PgUp`/`PgDn`、`Home`/`End`；`Space`→`SelectActivate`；`Delete` 清除绑定；`r`/`R` 恢复当前 / 全部默认 |
| 帮助页 | 练习 / 跟打中打开的整屏帮助：`↑`/`↓` 滚动、`PgUp`/`PgDn` 翻页、其余任意键→`HelpBack`（`help_back` 按 `session` / `article_practice` 回到对应练习页）；正常帮助页：`↑`/`↓`/`←`/`→`/`Tab`/`Enter` 走焦点键，`PgUp`/`PgDn` 滚动，其它键→`HelpBack` |
| 单词测试页 | 不参与焦点导航：`Esc`/`q`→`WordTestExit`，其余走键位表（`ChooseA`~`ChooseD` / `WordTestingNext` / `PlayWordPronunciation`） |

无会话的非练习页上，无修饰的 `1`~`6` 切到对应页面（1 单词 / 2 文章 / 3 词库 / 4 统计 / 5 帮助 / 6 设置）。

### 5.5 弹层键位

| 弹层 | 键 → `Action` |
| --- | --- |
| 输入 `edit` | `Esc`→`EditCancel`；`Enter`→`EditSave`；`Backspace`→`EditBackspace`；ASCII 数字→`EditChar`；其它忽略 |
| 确认 `confirm` | `←`/`↑`/`k`/`Shift+Tab`→`ConfirmMove(-1)`、`→`/`↓`/`j`/`Tab`→`ConfirmMove(1)`：在「确认 / 取消」之间移动按钮焦点（到边停、不回绕）；`Enter`/`Space`→`ConfirmActivate`：执行当前选中的按钮；`y`/`Y`→`ConfirmAccept`；`n`/`N`/`Esc`→`ConfirmCancel`；**其它任意键**仍→`ConfirmCancel` |
| 结算 `summary` | `←`/`↑`/`k`/`Shift+Tab`→`SummaryMove(-1)`、`→`/`↓`/`j`/`Tab`→`SummaryMove(1)`：在三个按钮之间移动按钮焦点（到边停、不回绕）；`Enter`/`Space`→`SummaryActivate`：执行当前选中的按钮；`r`→`SummaryRelearn`；`q`/`Esc`→`SummaryHome`；其它忽略。同一组键的含义由 `SummaryData.kind` 决定：单词结算是「继续 / 重学 / 主页」，文章结算是「返回文章列表 / 重打本篇 / 主页」（见 7.5） |

两个弹层都有按钮焦点：默认选中第一个按钮（「确认」/「继续」），所以弹层刚出现时的外观与旧版一致；`Enter` 执行的是**当前选中的按钮**，字母键（`y` / `r` / `q` 等）仍是固定去向的快捷键（`j` / `k` 例外：它们是 ↑ / ↓ 的全站别名，在弹层里同样移动按钮焦点）。

### 5.6 键入映射与鼠标

`key_to_input` 把终端按键压成 `KeyInput`：`Backspace`→`Backspace`、`Char(' ')`→`Space`、其它 `Char(c)`→`Char(c)`，其余返回 `None`。带 `Control` 或 `Alt` 修饰的按键一律返回 `None`，所以组合键不会误输入字符；`Shift` 不排除，大写字母与符号可正常键入。

`App::on_mouse` 先更新 `last_activity`，然后左键按下 → `ui::hit::action_at` 命中则 `apply_action`；滚轮上/下 → `wheel_action(..., -1/+1)`；其它鼠标事件忽略。命中区域与优先级见 [渲染层 · 鼠标命中](ui.md#11-鼠标命中)。

### 5.7 `Action` 全表

`Action` 的 98 个变体覆盖除练习页键入外的全部交互，`apply_action` 是唯一消费点：

| `Action` | `apply_action` 语义 |
| --- | --- |
| `Navigate(Screen)` | `navigate_to`：有会话时直接忽略；Home 刷新统计/任务预览，DictPicker/Settings 重置光标，Stats/Help 重置滚动；`Practice` 无动作。`Articles` 转交 `open_articles`——文章资源是懒加载的，光切 `screen` 会得到一个右边空空的文章页。离开 `WordTest` 时清掉 `word_test` 状态——否则切回来会显示上一轮的题目与分数 |
| `Back` | 依次关闭 confirm/edit/summary；否则按屏返回 Home（Settings 先保存设置；Help 走 `help_back`） |
| `QuitRequest` | 打开「确定退出 Type Words 吗？」确认弹层 |
| `StartPractice` / `StartFreePractice` | 按当前设置模式 / 以 `Free` 模式启动练习 |
| `AdjustDaily(delta)` | 每日词数以显示值 `Dict::per_day()` 为基准 `±delta`，再夹在 `1..=词表长度`；变化时保存词典、刷新任务预览并 Toast |
| `FocusMove(direction)` | 所有套外壳页面的方向键：`move_focus` 先按当前布局解析身份（导航栏 / 主页内容区失效就视为无焦点），再按所站栏的主轴移动——导航栏交给 `ui::focus::nav_move`（按 `NavOrientation` 取竖向 / 横向）；主页内容区交给 `ui::focus::content_move`（`↑`/`↓` 换视觉行带、`←`/`→` 带内左右）；其它页面的内容区交给 `move_selection_page`（先让页面光标动，动不了再按几何跨栏）。栏内到边才跨栏；完全无焦点时 `↑`/`←` 进导航栏、`↓`/`→` 落内容区。`set_screen` / `focus_default` 在进入页面时先选中内容区（主页是主按钮），所以正常打开界面后方向键从这个可见选中项开始移动 |
| `SwitchFocusPane` | `Tab` / `Shift+Tab`：`switch_focus_pane` 在导航栏与内容区之间换栏；方向键到边也能跨栏，这个键是固定通路（统计页横向键被页面占用时尤其有用） |
| `ActivateFocus` | `Enter`：`activate_focus` 按当前焦点触发。导航栏：解析目标并执行其 `Action`，落在当前页入口上只给 Info Toast「已在「X」页」，身份失效（缩放后元素消失）时只清空焦点并提示，不回退。主页内容区：执行焦点元素的 `Action`。其它页面：`activate_page_selection` 调页面自己的激活逻辑（词库设为当前、设置修改、文章开始跟打、词条编辑、快捷键改绑、流程启用、统计切换视图、帮助返回）。主页无焦点时回退 `StartPractice` |
| `ClearFocus` | 主页 `Esc`：`clear_focus` 清空 `focus` 与 `last_content_focus` |
| `OpenModePicker` / `ModeMove(delta)` / `ModeSelect` / `ModeCancel` | 模式选择弹层：打开弹层、移动光标、确认、取消；确认后按所选模式启动练习 |
| `OpenArticles` | 打开文章页：载入书本与当前书本的文章（见 6.3）。主页 `a` 键用它；侧栏 / 数字键走 `Navigate(Articles)`，两者最终落到同一个方法 |
| `DictMove(delta)` | 词库光标移动并夹在有效范围 |
| `SelectDict(i)` / `SelectSetting(i)` | 只移动光标（越界忽略） |
| `ActivateDict(i)` | 切换当前学习词库：保存词典、回主页、刷新任务与 Toast |
| `SettingsMove(delta)` | 设置光标移动并夹在 `0..SETTING_ROWS.len()-1` |
| `ActivateSetting(i)` | 设置光标置 `i` 并执行该项（见第 8 节） |
| `StatsScroll(delta)` | 统计页滚动，上限为「最近 14 天」记录数减一 |
| `SummaryMove(delta)` | 结算弹层按钮焦点在「继续 / 重学 / 主页」之间移动（`delta < 0` 向左 / 上、`> 0` 向右 / 下；`←`/`↑`/`k` 传 -1，`→`/`↓`/`j` 传 +1），到边停、不回绕 |
| `SummaryActivate` | 结算弹层 `Enter` / 空格：执行当前选中的按钮，与鼠标点它等价 |
| `SummaryContinue` / `SummaryRelearn` / `SummaryHome` | 结算弹层三种去向的固定快捷键，含义随 `SummaryData.kind` 不同（见 7.5） |
| `ConfirmMove(delta)` | 确认弹层按钮焦点在「确认 / 取消」之间移动（`delta < 0` 向确认、`> 0` 向取消；`←`/`↑`/`k` 传 -1，`→`/`↓`/`j` 传 +1），到边停、不回绕 |
| `ConfirmActivate` | 确认弹层 `Enter` / 空格：执行当前选中的按钮——选中「取消」时只关闭弹层，不会误执行危险动作 |
| `ConfirmAccept` / `ConfirmCancel` | 执行 / 关闭确认的固定快捷键（`y` / `n` / `Esc`）；`ConfirmAccept` 处理 Quit、ResetPractice 与 ClearAllData |
| `EditSave` / `EditCancel` | 解析输入并写回 / 关闭输入弹层 |
| `EditChar(c)` / `EditBackspace` | 追加 ASCII 数字（最多 6 位）/ 删除一位 |
| `HelpScroll(delta)` | 帮助页滚动，`max(0)` 后上限 80 |
| `HelpBack` | 有会话回练习页，否则回主页 |

## 6. 屏幕与弹层状态机

### 6.1 规则

- **练习中禁止切页**：`navigate_to` 在有 `session` 时直接返回。练习页只能通过 `Ctrl+C` 或完成结算离开（帮助页只是临时覆盖，返回后仍在练习）；鼠标侧栏导航在练习期间也不会生成 target。
- **弹层模态且互斥**：`summary` 只在练习完成时产生；`edit` 只从设置页数字项产生；`confirm` 只从主页 `q` 或设置页「重置进度」产生。正常交互中三者互斥；若出现异常叠加，键位层按 `edit` → `confirm` → `summary` 处理，渲染层按 `summary` → `edit` → `confirm` 绘制（后画者在上）。
- **Back 语义**：先尝试关闭弹层（正常流程里弹层已被键位层拦下）；再按屏返回：设置页先 `save_settings_now` 回主页，词库页/统计页回主页，帮助页按是否有会话回练习页或主页，主页/练习页无动作。
- **键盘焦点（所有套外壳页面）**：状态是 `App.focus: Option<ui::focus::FocusId>` + `App.last_content_focus: Option<ContentId>`。`FocusId` 三态：`Nav(NavId)` 导航栏、`Content(ContentId)` 主页内容区、`Page` 其它页面内容区（页面光标就是身份）。导航栏目标 = `shell::layout` 的 `nav_targets`（单词 / 文章 / 词库 / 统计 / 帮助 / 设置）加宽屏底部「退出」（`NavId::Quit`）；主页内容区目标 = `ui::home::targets` 的 8 项（`ContentId::Start` / `Free` / `DictPage` / `ManageDict` / `MoreDicts` / `DailyMinus` / `DailyPlus` / `Mode`）。导航栏与主页内容区每次按键 / `Enter` 前按当前 `viewport` 重新解析，布局变化后元素消失就视为失效；`Page` 由页面状态保存，不依赖几何。`App::set_screen` 是所有切屏的唯一入口：切屏时 `clear_focus` + `focus_default`，主页选中主按钮、其它套外壳页面选中 `Page`、练习页 / 文章跟打页 / 单词测试页保持无焦点。跨栏时宽屏内容区 `←`、窄屏内容区 `↑` 进导航栏，宽屏侧栏 `→`、窄屏顶栏 `↓` 回内容区，`Tab`/`Shift+Tab` 随处可用；主页回内容区用 `last_content_focus` 恢复上次选择。`Esc` 在主页清空焦点、其它页面返回上一层。鼠标点击导航栏 / 内容区会同步焦点，见 [渲染层 · 键盘焦点](ui.md#54-键盘焦点)。
- **通用选择**：`move_selection` / `switch_selection_pane` / `activate_selection` 把列表 / 卡片光标的语义集中在一处，`SelectUp` / `SelectDown` / `SelectActivate` / `SelectLeftPane` / `SelectRightPane` 是它们的 `Action`；`move_selection_page` / `activate_page_selection` 是焦点层对它们的补充（统计切视图、帮助返回）。各页面（词库 / 统计 / 设置 / 词条 / 流程 / 快捷键 / 文章）只负责把自己的光标接进来；文章页用 `article_pane_left` 区分书本栏与文章栏。
- **帮助页**：从练习页按 `?` 进入时 `session` 仍在，数字切页不生效；除滚动键外任意键都返回练习页。
- **退出与结算后**：只有主页 `q`（或宽屏侧栏「退出」：可点击，也可用方向键选中后按 `Enter`）请求退出；其它非练习页面的 `q` 是返回（练习页的 `q` 是普通输入）。结算后会话与缓存都已清理，继续/重学用当前设置里的练习模式开启新一轮（自由练习结算后继续，回到设置中的常规模式，而不是继续自由模式）。

### 6.2 状态流转

```text
                ┌── q / 侧栏退出 ─▶ confirm(Quit) ── Enter/空格 执行选中 ──▶ 保存并退出
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
                  ←/→ 选择 · Enter/空格 确认（继续 / 重学 / 主页）
```

### 6.3 文章页的进入路径

文章页有多个入口：主页按 `a`、侧栏 / 顶栏点「文章」、数字键 `2`。**它们必须走同一个方法**——`open_articles`。文章资源是懒加载的（书本在 `assets/list/article.json`、文章在 `assets/dicts/en/article/*.json`），`App` 启动时这两个列表都是空的；`open_articles` 负责载入书本、重置两个光标，再调 `load_selected_book` 载入当前书本的文章。

`open_articles` 还会刷新 `article_resume`：有跟打断点时，缓存对应的那篇文章会标出「上次第 N 句」，选中它按 `Enter` 从断点句子继续；选其他文章则从头开始。启动时不会自动进入跟打页——跟打的恢复只发生在这个显式入口。

> 早期只有主页 `a` 那条路径（`Action::OpenArticles` → `open_articles`）会载入资源，侧栏与数字键走 `navigate_to`——那条路径只切了 `screen`，进去是个右栏空空的页面。同一个页面的两个入口给出不同结果，是最难察觉的一类缺陷。

`navigate_to(Screen::Articles)` 因此转交给 `open_articles`；`Action::OpenArticles` 保留给主页的 `a` 键，两者最终落到同一处。

跟打页的状态是 `ArticlePracticeState`：`sentences` 是 `gen_sections` 之后按段落展开的扁平句子列表，`section_starts` 记录每个段落首句在其中的下标（渲染分段用；跟打仍按扁平顺序逐句推进），`index` 是当前句，`typing: SentenceTyping` 记录实际按下的字符与实际错误，另有 `finished`、`wrong_sentences`、`spend`。跟打目标由 `Sentence::typing_cells` 决定：字母、数字与词间空格是目标字符，标点、符号与多余空白不是（`typing_target` 即这些目标按顺序排成的字符序列）。词间空格是可选目标：按空格算正确输入，不打空格直接打下一个字母时自动补成正确输入，所以错误只会落在字母 / 数字位置。`start_article_practice` 构造它：命中同一篇文章的断点缓存时把下标 clamp 到当前句数内续打，否则从头开始；落点句子没有跟打内容（纯标点 / 空白）时自动向后跳到最近的有内容句子并按已完成计，整篇都没有可跟打内容时只弹 Warning、不进入跟打页。`finish_article_sentence` 在每句输满目标长度后推进，`advance_over_empty_article_sentences` 负责跳过没有内容的句子，全部完成后 `finish_article_practice` 写统计、给结算并删除断点缓存——这里的结算是文章口径的 `SummaryKind::Article`（完成句数 / 总句数 / 错句数 / 文章标题），不是单词会话那份「新词 / 复习 / 学习进度」，去向是文章列表 / 重打本篇 / 主页，详见 7.5。

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

`finish_session`（只在 `session.complete` 且尚无 `summary` 时执行）：`session.settle(&mut base, &settings, &mut stat, now)` 写回 FSRS 卡片与词典统计；由 `stat` 计算 `SummaryData`（`spend_ms = stat.spend`，`accuracy = round((total - wrong) / total × 100)`，`total == 0` 记 100，`new_words`/`review` 取会话统计，进度三项取学习词典）；`save_dict_now()` → 删除 `practice-word.json`（`store.remove_practice`，不动文章跟打缓存）→ `refresh_stats()` → `refresh_task_preview()`。

两个分支的结算弹层都有按钮焦点（默认选中第一个按钮，单词是「继续」、文章是「返回列表」）：`←`/`→`（或 `↑`/`↓`、`j`/`k`、`Tab`）移动选中，`Enter` / 空格执行选中项；`r` / `q` 仍是固定快捷键。下面两张表里的去向既能用方向键选中后 `Enter` 触发，也能用表中标注的字母键直接触发。

| 结算去向（单词会话 / 单词测试） | 方法 | 结果 |
| --- | --- | --- |
| 继续（默认选中，`Enter` / 空格） | `continue_after_summary` | `create_next_task` 生成下一轮；任务为空则提示并回主页；否则以当前设置模式开启新会话 |
| 重学（`r`，或选中后 `Enter`） | `relearn_after_summary` | `create_repeat_task` 生成重复任务，以当前设置模式开启新会话 |
| 主页（`q` / `Esc`，或选中后 `Enter`） | `summary_home` | 清空会话与结算、清两个待保存标记（缓存文件已由结算路径删掉）、回主页并刷新统计/任务 |

`leave_practice`（`Ctrl+C` 或结算继续失败时）在无结算的情况下先保存练习与词典，再清空会话、回主页、刷新缓存。结算弹层出现后练习缓存已被删除，因此「回主页」不再产生缓存文件。

**文章跟打分支**：文章跟打没有练习会话，结算由 `finish_article_practice` 直接生成，`kind = SummaryKind::Article { article_id, title }`，指标按文章口径填写（见 2.3）：

- `last_learn_index` = 完成句数、`length` = 总句数、`wrong` = 错句数（本句出现过错误即计一句）、`complete` = 完成句数是否达到总句数；`accuracy = (完成句数 - 错句数) / 完成句数`，0 句记 100。
- `new_words` / `review` 记 0、`wrong_top` 为空——文章没有「新词 / 复习 / 错词」与句级错词 Top10；`week_days` 必须在把本轮 statistics 写进当前词典**之后**再从该词典取，否则结算里的「本周记录」永远是空的。
- 统计照旧写进当前词典的 `statistics`（`title` 为文章标题，`total` / `review` 都是完成句数），`save_dict_now()` 后回主页并作为弹层背景，同时删除 `practice-article.json` 断点缓存。

| 结算去向（文章跟打） | 方法 | 结果 |
| --- | --- | --- |
| 返回列表（默认选中，`Enter` / 空格） | `continue_after_summary` | 清结算后 `return_to_articles()`：回到文章列表并保留书本 / 文章光标；列表为空时补一次 `open_articles()` |
| 重打本篇（`r`，或选中后 `Enter`） | `relearn_after_summary` | `restart_article(article_id)`：按 id 在文章列表里找回同一篇从头开始（先清断点标记）；文章已不在列表时回列表并 Toast 提示 |
| 主页（`q` / `Esc`，或选中后 `Enter`） | `summary_home` | 与单词结算相同：清空会话与结算、清两个待保存标记、回主页并刷新统计/任务（不删缓存文件——本轮缓存已由 `finish_article_practice` 删除） |

文章结算没有会话，这两个分支在 `kind == Article` 时提前返回，不再落到「无 session 就 `leave_practice()` 回主页」的兜底路径——早期实现三个按钮都走兜底，用户点「继续」看到的却是主页。

**中途退出的学习时长不能丢**：统计有两条写入路径——会话正常结束（`settle` → `flush_stat_to_store`）与断点缓存被丢弃时的兜底（`flush_practice_stats`）。后者在**开新一轮练习**与**「清空当前练习进度」**时调用，对齐上游 `words.vue` 的 `resetCacheData`（先 `flushStatToStore` 再清缓存）。`stats_recorded` 保证同一轮只记一次。

## 8. 设置与主题交互

### 8.1 `activate_setting` 的分派

| `SettingKind` | 行为 |
| --- | --- |
| `Mode` | `cycle_practice_mode`：按「学习 → 自由练习 → 复习 → 随机复习 → 自测 → 默写 → 听写 → 学习」循环，保存设置并刷新任务预览 |
| `Theme` | `cycle_theme`：按 `ThemeRegistry::cycle_ids`（`auto` + 内置 + 自定义）循环，保存设置 |
| `SoundType` | `us` ⇄ `uk`，影响练习页音标取哪个字段，保存设置 |
| `Bool` | `toggle_setting_bool(key)` 翻转对应字段，保存设置 |
| `Number(target)` | 打开 `EditState`，`buffer` 预填当前值的十进制字符串（尚未落盘；`PerDay` 预填显示值 `Dict::per_day()`） |
| `Action(action)` | 打开确认弹层，文案取 `confirm_message(action)`：`ResetPractice` 为「确定清空当前练习进度吗？」，`ClearAllData` 为「该操作将会清除所有数据，确认继续？」 |

分派结束后统一执行 `save_settings_now()` 与 `refresh_task_preview()`；`Bool`/`SoundType` 靠这一步落盘。

### 8.2 布尔项映射

`auto_random_review`→`auto_add_random_review_when_no_due`、`show_etymology`→`show_etymology_and_rel_words`、`big_word`→`tui_big_word`；其余 7 个 key（`ignore_simple_word`、`auto_next_word`、`input_wrong_clear`、`ignore_case`、`show_word_question`、`show_near_word`、`show_usage_tips`）与字段同名。`show_usage_tips` 控制练习页底部「提示：…」行是否显示（Toast 仍正常显示）。

### 8.3 数字项的夹取范围

设置页显示的数字值来自 `App::setting_number_value(target)`（比例、阈值、毫秒数直接取值，`Retention` 转成百分数，`PerDay` 取夹取后的 `Dict::per_day()`）；`EditSave` 解析输入（空串或非法值按 0），`apply_number_value` 按目标夹取后写回，并刷新任务预览：

| `EditTarget` | 字段 | 范围 |
| --- | --- | --- |
| `PerDay` | 学习词典 `per_day_study_number`（设置页 `daily_words` 行） | `1..=词表长度`（至少 1） |
| `Ratio` | `word_review_ratio` | 0 ~ 10 |
| `Repeat` | `repeat_count` | 1 ~ 100；100 表示改用 `repeat_custom_count`（下一行可编辑） |
| `RepeatCustom` | `repeat_custom_count` | 1 ~ 100；仅当 `repeat_count == 100` 时参与实际重复次数 |
| `WaitMs` / `SpaceCooldown` | `wait_time_for_change_word` / `space_cooldown_time` | 0 ~ 10000（毫秒） |
| `EasyLimit` / `GoodLimit` / `HardLimit` | `fsrs_easy_limit` 等 | 0 ~ 20 |
| `Retention` | `fsrs_parameters.request_retention` | 50 ~ 99（输入百分数，存为 0.50 ~ 0.99） |

每日词数在 `SETTING_ROWS` 中（`daily_words`，复习分区首行）：`Enter` 打开输入弹层，`apply_number_value(PerDay)` 夹在 `1..=词表长度` 后写入学习词典并保存 `dict.json`。主页与词库页的 `+`/`-` 通过 `adjust_daily_target(±5)` 修改同一字段，基准是显示值 `Dict::per_day()`（未完成词库按词表长度夹取），结果再夹在 `1..=词表长度`；变化时保存词典、刷新任务预览并 Toast「每日词数：N」。

## 9. 持久化时机

### 9.1 保存函数

| 函数 | 行为 |
| --- | --- |
| `save_dict_now` | `store.save_dict(&boot.base)`；`readonly_dict` 为真（存档版本过高）时直接返回 |
| `save_settings_now` | `store.save_settings(&boot.settings)`；`readonly_settings` 为真（设置存档版本过高）时直接返回 |
| `save_practice_now` | 无会话时返回；`session.build_cache()` 后把当前 `stat` 写进 `stat_store_data`，`cache_to_compact` 后写 `practice-word.json`，并清除 `pending_practice_save` |
| `schedule_practice_save` | 只设 `pending_practice_save = now + 500ms`，落盘由 `on_tick` 完成 |
| `save_article_practice_now` | 无文章跟打会话时删除 `practice-article.json`；否则把句子进度与用时写进 `practice_data` 后落盘，并清除 `pending_article_save` |
| `schedule_article_save` | 只设 `pending_article_save = now + 500ms`，落盘由 `on_tick` 完成；每完成一句与每 30 秒整点各调一次 |
| `save_all` | 依次保存词典、设置；若仍有未结算会话则保存练习缓存；文章跟打进行中也保存文章缓存 |
| `clear_practice_cache` | 删除 `practice-word.json` 并清除防抖 deadline（不动文章缓存） |
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
| `finish_article_practice` | 立即保存词典（统计写进当前词典），删除文章断点缓存 |
| 结算继续 / 重学 | 新会话立即保存练习缓存 |
| 主页/词库页 `+`/`-`、切换词库、设置页每日词数（`daily_words`，`PerDay`） | 立即保存词典 |
| 设置页其余项的修改、退出设置页 | 立即保存设置（「每日词数」行改的是词典，见上一行） |
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
- 可读状态包括 `screen`、`focus`、`viewport`、`session`（`data.index`、`current_word()`、`complete`、`mode` 等）、`stat`、`summary`、`edit`、`confirm`、`toasts`、`boot`、光标与滚动字段；私有字段 `last_timer_tick`/`pending_practice_save` 只能通过时间推进间接触发。
- 键盘焦点导航按 `viewport` 算几何：测试与 `examples/shot` 先用 `App::set_viewport(Rect)` 把区域设成与渲染一致，再按方向键，否则焦点会按默认 80 × 24 布局算出来；进入页面要经过真实入口（数字键 / 侧栏 / 快捷键），才会触发 `set_screen` 的默认选中。
- 测试辅助：`tests/common/mod.rs` 的 `Fixture` 在临时目录生成 12 词的 `toy` 词库与独立 `home`；`app::aggregate_stats` 把 `BaseState` 的逐日统计聚合成 `(day, spend, new, review, wrong)`。渲染测试用 `TestBackend` + `app.render(frame)`，驱动方式与真实事件循环一致（见 [渲染层](ui.md) 与 [开发指南](development.md)）。

## 11. 扩展指南

1. **新增按键 / Action**：`action.rs` 加变体并在 `apply_action` 实现；非练习页映射到 `keymap.rs` 的 `screen_action`/`overlay_action`，练习页在 `on_practice_key` 加分支；需要点击就在对应 `targets()` 加 `Target::new(rect, action)`；要进键盘焦点则在 `ui::target.rs` 的 `ContentId` 加取值，用 `Target::focusable(rect, action, label, FocusId::Content(id))` 注册（导航栏项加到 `shell::layout` 的 `nav_targets` / `quit_target` 即会被 `ui::focus` 采纳；新页面要参与焦点导航，就在 `ui::focus::supports_focus` 加 screen，并在 `move_selection_page` / `activate_page_selection` / `page_label` 里接上页面光标）；补 `tests/app_drive.rs`、`tests/focus_nav.rs` 与 `tests/ui_snapshots.rs`。
2. **新增页面**：`Screen` 加变体，`ui::render` 与 `ui/hit.rs` 补分支；需要侧栏时扩展 `ui/shell.rs` 的 `Nav`（`from_screen`/`screen`/`label`/`key`/`ALL`/`MAIN`）；在 `screen_action` 注册键位；页面实现 `layout()` + `render()` + `targets()` 并登记渲染快照。
3. **新增设置项**：`domain/types.rs` 的 `SettingState` 加字段与默认值 → `SETTING_ROWS` 加行（数字项加 `EditTarget` 并同步 `setting_number_value`/`apply_number_value` 与夹取范围，布尔项同步 `toggle_setting_bool`）→ `ui/settings.rs` 的 `SECTIONS` 与格式化 → `domain/i18n.rs` 文案 → [设置项参考](settings.md) 与测试。
4. **新增弹层**：`App` 加 `Option<...>` 字段与打开它的 `Action`；`keymap::overlay_action` 按优先级插入键位并同步 `on_key` 的模态判断；`ui/mod.rs` 的 `modal_open` 与渲染顺序、`ui/overlays.rs` 的 `layout`/`render`/`targets`/`target_action` 同步；补最小终端布局单测与渲染快照。

## 相关文档

- [文档索引](README.md)
- [渲染层](ui.md)
- [领域逻辑](domain.md)
- [存档与数据格式](persistence.md)
- [总体架构](architecture.md)
