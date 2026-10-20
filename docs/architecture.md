# 总体架构

> 适用范围：整个仓库（`src/`、`tests/`、`assets/`、`scripts/`、`.github/`）；对应版本：typewords-tui 0.1.0。

本文回答三个问题：程序由哪些部分组成、它们如何协作完成一轮「选词库 → 练习 → 结算 → 统计」、以及为什么这样设计。各模块的内部细节见 [domain.md](domain.md)、[app.md](app.md)、[ui.md](ui.md)、[persistence.md](persistence.md)。

## 1. 项目定位

typewords-tui 是上游 [TypeWords](https://github.com/zyronon/TypeWords)（Nuxt + Vue 的 Web / Tauri 桌面端）拆分出的**独立终端客户端**：一个单进程、无网络、数据全本地的 Rust 程序。

- **目标**：在终端里完成单词练习闭环——选词库、按 FSRS 排程生成今日任务、跟写/听写/默写/自测/复习/随机/自由七种流程、结算写入统计、断点续练；界面支持响应式布局、鼠标与自定义主题。
- **非目标**（v1 明确不做，详见 [upstream-parity.md](upstream-parity.md)）：文章背诵、发音/键音、云同步、备份导入导出、在线查词翻译、多语言界面、自定义流程与快捷键。
- **硬约束**：运行时不依赖 Node.js / WebView / 网络；词库资源随发布包分发；终端最小 60 × 20；Windows 是主要发布平台，代码本身跨平台（CI 在 Linux 与 Windows 上跑测试）。

## 2. 技术栈

| 依赖 | 版本 | 用途 |
| --- | --- | --- |
| Rust（edition 2024，MSRV 1.90） | — | 程序与测试语言；release 配置 `opt-level = 3`、`lto = "thin"`、`strip = true` |
| [ratatui](https://ratatui.rs/) | 0.30.2 | 终端 UI：布局、组件、缓冲渲染；事件层复用其转出的 `ratatui::crossterm` |
| （无） | — | FSRS-6 间隔重复算法由 `src/domain/fsrs.rs` 自实现（逐条移植上游 ts-fsrs 5.4.1），不引入外部 crate |
| serde / serde_json | 1 | 存档、词库、主题 JSON 的序列化；未知字段保留 |
| chrono | 0.4 | 本地时间、自然日、跨天时间片（带 serde） |
| rand | 0.10 | 可注入种子的随机源（`StdRng`）与洗牌 |
| clap | 4（derive） | 命令行参数与子命令 |
| anyhow / thiserror | 1 / 2 | 边界错误传播（`anyhow`）；领域错误类型（`thiserror` 为预留依赖） |
| dirs | 7 | 跨平台本地数据目录（Windows 为 `%LOCALAPPDATA%`） |
| unicode-width | 0.2 | 终端显示宽度（CJK 占 2 列），折行/截断/布局都依赖它 |
| tempfile（dev） | 3 | 测试用临时数据目录 |

代码中**没有** async 运行时、线程池、`unsafe`、网络请求或子进程调用：整个程序是单线程的「事件轮询 + 重绘」循环，数据是本地小体积 JSON。

## 3. 分层与依赖规则

```text
main.rs
  └─ lib.rs::run(cli)
       ├─ Paths::resolve(...)                 ← 资源目录 / 数据目录解析
       ├─ tw doctor | tw task | tw import-dict ← cli.rs（不进入 TUI）
       └─ app::run(paths, cli)
            ├─ App::bootstrap()               ← bootstrap.rs：store + domain + theme 装载
            └─ App::run() → event_loop
                 ├─ ui::render(&App)          ← 只读渲染
                 └─ keymap / mouse → Action → App::apply_action
                      └─ domain（typing / flow / session / scheduler / timer / statistics）
                           └─ store::FileStore ← JSON 存档
```

| 层 | 目录 | 职责 | 依赖约束 |
| --- | --- | --- | --- |
| 领域层 | `src/domain/` | 与终端无关的类型、词库、调度、流程、键入、会话、统计 | 只依赖自身与 `paths::Paths`；**不能**依赖 `app`/`ui`/`store`/`theme` |
| 存档层 | `src/store/` | 本地 JSON 读写、版本校验、损坏隔离、原子写 | 依赖 `domain` |
| 主题配置层 | `src/theme/` | 颜色/主题模型、内置主题、`<home>/themes/*.json` 加载与校验 | 不依赖 ratatui，也不依赖 `ui` |
| 应用层 | `src/app/` | `App` 状态机、Action、键鼠映射、事件循环、保存时机 | 依赖 `domain`/`store`/`bootstrap`/`cli`/`paths`，渲染入口调用 `ui` |
| 渲染层 | `src/ui/` | ratatui 布局与绘制、主题→颜色映射、鼠标命中区域 | 读 `app::App` 与 `domain` 数据，**不修改**状态 |
| 入口/装配 | `src/main.rs`、`src/lib.rs`、`src/cli.rs`、`src/paths.rs`、`src/bootstrap.rs` | 解析参数、解析路径、装载初始状态、分发子命令 | 最外层，可依赖所有层 |

有两条值得明确的实际约束：

1. **domain 不依赖终端**。因此 domain 可以在没有终端的情况下被单元测试和 CLI 命令（`tw task`）直接使用；这也是与 Web 端实现对齐时的主要隔离层。
2. **app 与 ui 在同一 crate 内互相引用**。`App::render` 调用 `ui::render`，`App::theme` 返回 `ui::Theme`；`ui` 的各页面直接读 `App` 字段。这是 ratatui 命令行程序的常见做法（省去中间视图模型），代价是两块耦合较紧；若要拆 crate，需要先引入只读的 ViewState。此项记录在「已知限制」中。

## 4. 代码地图

```text
src/
  main.rs / lib.rs      入口、模块声明、run() 分支、版本行
  cli.rs                参数、doctor / task / import-dict
  paths.rs              资源目录与数据目录解析、themes/log 路径
  bootstrap.rs          启动装载：读存档 + 词库 + 主题，确保当前学习词典可用
  domain/               领域逻辑（详见 domain.md）
  store/mod.rs          dict.json / setting.json / practice-word.json 的原子读写（详见 persistence.md）
  theme/                主题配置层：颜色模型、内置主题、用户主题加载（详见 themes.md）
  app/                  应用状态机与事件循环（详见 app.md）
  ui/                   ratatui 渲染与鼠标命中（详见 ui.md）
tests/                  集成测试（详见 development.md）
examples/               dump.rs（练习页转文本）、shot.rs（页面转 JSON 快照）
tools/                  render-snapshot.py：JSON 快照 → PNG
scripts/                sync-assets.mjs（同步上游词库）、package.ps1（本地打包）
assets/                 list/word.json + dicts/en/word/CET4_T.json（内置 CET-4，2607 词）
docs/                   本文档目录
.github/workflows/      ci.yml（fmt/clippy/test）、release.yml（tag v* 发版）
```

规模参考：`src/` 43 个 Rust 文件、约 1.2 万行；集成测试约 1.1 千行；内置词库总大小约 4.5 MB。

## 5. 进程生命周期

### 5.1 启动（TUI 模式）

序号自 `main.rs` 开始：

1. `Cli::parse()` 解析参数（`--assets`/`--home`/`--dict`/`--mode`/`--theme`/`--seed`/`--reset-practice`）。
2. `Paths::resolve()` 决定数据目录（`--home` > `TYPEWORDS_HOME` > 系统本地数据目录的 `typewords-tui`）与资源目录（`--assets` > `TYPEWORDS_ASSETS` > 可执行文件同级 `assets/` > 当前目录及祖先的 `assets/` 或 `public/`）。
3. `Command` 分支：`doctor` / `task` / `import-dict` 走 CLI 路径并直接结束；无子命令则进入 TUI。
4. `App::bootstrap()`：
   - `bootstrap::load()` 读取三份存档（`FileStore`）、加载词库目录（`Catalog`）、加载主题（`ThemeRegistry`），把损坏/版本过高的通知暂存为 notices；
   - 应用 `--mode` / `--theme` 覆盖；生成或使用 `--seed` 随机源；
   - notices 转成 Toast；主题不存在时追加回退提示；
   - `--reset-practice` 清空练习缓存，否则尝试从 `practice-word.json` 恢复上次会话；
   - `ensure_study_dict()` 确保当前学习词库已加载词表（必要时回退到 `cet4` 或第一本词库）；
   - 刷新今日统计与今日任务预览；
   - 词库加载失败则记录 `fatal`，此时 `app::run` 在进入终端前 `bail!`，不留下半初始化界面。
5. `App::run()`：`ratatui::init()` 进入备用屏幕、开启鼠标捕获、安装 panic hook（panic 时恢复终端与鼠标捕获），进入事件循环。
6. 退出循环后 `save_all()`：保存词典、设置与（未结算的）练习缓存；关闭鼠标捕获、`ratatui::restore()`。

### 5.2 事件循环

```text
loop {
    terminal.draw(render)          // 全量重绘当前帧
    on_tick(now_ms())              // Toast 过期、计时、键入定时、定时保存
    if quit { break }
    if poll(100ms) {
        drain ≤ 64 events:
            Key(press) → on_key
            Mouse      → on_mouse
            Resize     → 忽略（下一帧按新尺寸重排）
    }
}
```

- **按键**只处理 `KeyEventKind::Press`（忽略 Release/Repeat），避免 Windows 上重复触发。
- **一次 poll 最多处理 64 个事件**再重绘，防止鼠标移动/滚轮事件造成高频重绘。
- **没有后台线程**：计时、自动保存检查、Toast 过期都在 `on_tick` 中按时间戳推进；`on_tick` 接收 `now` 参数，测试可直接注入时间。
- **退出**只有两条路径：`q` 触发确认弹层后 `quit = true`，或事件循环返回错误；两者都会走保存与终端恢复。

### 5.3 非 TUI 子命令

| 命令 | 流程 |
| --- | --- |
| `tw doctor` | 解析路径 → 加载词库目录并统计每个词库的词数 → 加载存档并列出文件大小 → 加载主题并列出 id/名称/告警 → 打印。不进入 TUI、不修改数据。 |
| `tw task` | 启动装载时计算今日任务：新词数、到期复习数、随机补足数，并打印前 5 个新词/复习词。用于脚本化或排障。 |
| `tw import-dict FILE` | 校验并复制词库 JSON 到数据目录的 `dicts/en/word/`，把资源登记写进数据目录的 `list/word.json`，打印词库名与词数，并提示用 `--dict` 切换。 |

## 6. 状态管理与数据流

### 6.1 单一状态源与输入路径

`App` 是唯一的状态持有者，状态只在 app 层被修改。输入有两条路径：

- **非练习页 / 弹层 / 鼠标 / 集成测试**：`keymap.rs` 与 `ui/hit.rs` 产出 `Action`，统一由 `App::apply_action` 执行；
- **练习页键盘**：`App::on_practice_key` 直接调用键入状态机（`TypingEngine`）与会话方法（`PracticeSession`）。这里是输入频率最高、状态迁移最复杂的路径，不经过 Action 枚举中转。

此外 `App::bootstrap`（初始化装载）与 `App::on_tick`（计时/保存/Toast 过期）也直接更新状态。因此 `Action` 管线是**非练习输入、鼠标与测试的统一入口**，而不是全程序唯一入口；真正的约束是：domain 提供受控的迁移函数，ui 只读 `App`。

```text
非练习键位 ── keymap.rs（弹层优先 → 屏幕键位）──┐
鼠标 ──────── mouse.rs → ui::hit::action_at ──────┼─→ Action ─→ App::apply_action ─→ 状态变更
测试（非练习）─ 直接构造 Action / KeyEvent ────────┘

练习页键位 ── App::on_practice_key ─→ TypingEngine / PracticeSession（直接迁移）
启动 / 定时  ── App::bootstrap / App::on_tick ─→ 初始化、计时、定时保存、Toast
                                                              │
                                                              ▼
                                                    ui::render(&App) 只读绘制
```

这样做的收益：

- 非练习键位/鼠标/测试共享同一条状态迁移路径，行为一致、可测；
- 渲染层完全只读，不存在「画的时候顺手改状态」的隐蔽副作用；
- 新增非练习交互时只需在 `action.rs` 增加动作、在 `keymap.rs`/`hit.rs` 映射、在 `apply_action` 实现；练习页按键则在 `on_practice_key` 中接入。

代价是两套输入路径并存：新增练习页按键时要同时注意 `on_practice_key` 与帮助页文案；`Action` 枚举与 `apply_action` 的分支也会随非练习交互增长（目前 25 个动作）。

### 6.2 一轮练习的数据流

```text
BaseState(dict.json) ──┐
SettingState(setting.json) ──┼─→ create_study_task ─→ TaskWords（新词 + 到期复习 + 随机复习）
Catalog(assets) ───────┘                                      │
                                                              ▼
                    PracticeSession::new / begin_session_with_task
                                                              │
        ┌─────────────────────────────────────────────────────┤
        ▼                                                     ▼
  TypingEngine（按键状态机）                           Navigator（流程游标/相位）
        │ TypingEvent(Completed/Wrong/Notice…)                │ Notice(级别+文案)
        └──────────────→ App::handle_typing_events ←──────────┘
                              │
                              ▼
             PracticeState（计时/新词/复习/错误） + 系统词典写入（收藏/错词/已掌握）
                              │
                  ┌───────────┴────────────┐
                  ▼                        ▼
        PracticeSession 游标推进      practice-word.json（防抖保存）
                  │
                  ▼
        finish_session → flush_stat_to_store → dict.json 的 Dict.statistics
                  │
                  ▼
        SummaryData → 结算弹层 → 回主页并刷新统计/今日任务
```

关键点：

- **今日任务**在进入练习前一次性生成（`create_study_task`）；练习过程中不再重新取词。
- **练习进度**由 `PracticeFlowCursor` 表示（节点/步骤/循环/错词清空状态），是流程位置的唯一指针。
- **评级与卡片更新**在会话内完成；结算时把本轮的时长、词数、错误数聚合进词典的 `statistics`。
- **断点缓存**是会话快照（游标 + 任务词表 + 统计快照），恢复时要求词库与流程仍匹配。

### 6.3 渲染与命中共享几何

所有页面都遵循同一模式：`layout(area, ...)` 计算出一组 `Rect`，`render()` 与 `targets()`（鼠标命中区域）使用同一份布局结果。滚动位置也来自同一状态，因此「绘制的位置」与「可点击的位置」不会漂移；集成测试 `mouse_target_rects_match_rendered_text` 会检查这一点。详见 [ui.md](ui.md)。

## 7. 持久化与数据完整性

- 三份存档对应上游 IndexedDB 的三个 key：`dict.json`（v4）、`setting.json`（v23）、`practice-word.json`（v2），外层都是 `SaveEnvelope { val, version, updated_at }`。
- 写入采用**同目录临时文件 + fsync + rename**；解析失败的文件会被重命名为 `*.corrupt-<时间戳>.json` 并回退默认值；`dict.json` 与 `setting.json` 版本高于程序支持版本时都**只读不覆盖**；练习缓存版本不受支持时忽略（不移动文件），详见 [persistence.md](persistence.md)。
- 练习缓存在练习页内按 500ms 防抖保存、练习时长累计到 30 秒整数倍时触发保存、退出/切页时保存；结算后删除。恢复失败（词库不匹配、版本不符）时丢弃缓存并提示。
- 存档字段与上游同名，序列化使用 camelCase（`Card` 沿用 ts-fsrs 的 snake_case），因此可以做手工迁移，但**不读写浏览器 IndexedDB**。

完整格式与版本策略见 [persistence.md](persistence.md)。

## 8. 主题与渲染

- 颜色全部来自主题的 13 个 token（`bg/panel/element/border/fg/muted/dim/accent/accent_alt/warn/good/bad/selection`），`ui/` 中不出现硬编码颜色。
- `theme/` 只负责「主题是什么」（id、名称、base、颜色取值、校验），不依赖 ratatui；`ui/theme.rs` 把 token 映射成 ratatui 的 `Color`。
- 启动时加载内置主题 + `<home>/themes/*.json`；未知 id 回退深色并提示；修改自定义主题需重启进程。
- 响应式外壳：`< 100` 列切换为窄屏顶栏标签；`>= 120` 列侧栏 26 列，否则 24 列；小于 60 × 20 只渲染「终端太小」提示。

详见 [themes.md](themes.md) 与 [ui.md](ui.md)。

## 9. 关键设计决策

| 决策 | 理由 | 代价 |
| --- | --- | --- |
| 独立仓库、独立进程，不共享上游代码 | 终端 UI 与 Vue/Web 差异过大；Rust 单二进制、无 Node/WebView 依赖 | 功能对齐需人工维护，差异见 [upstream-parity.md](upstream-parity.md) |
| domain 与终端解耦 | 可被单元测试与 `tw task` 直接复用；与上游实现对照时边界清晰 | 需要在层间传递上下文结构（`TypingContext` 等） |
| 本地 JSON 存档（而非 SQLite） | 与上游字段同名、可读可手工修复、无数据库依赖 | 需要自己做原子写/版本/损坏隔离；无查询能力 |
| Action 单入口改状态 | 键盘、鼠标、测试行为一致；渲染纯函数 | 枚举与分支样板代码 |
| 单线程 + 时间戳驱动 | 无并发/死锁；本地小数据足够快 | 无法在后台做重 IO；长任务会阻塞界面 |
| 固定 7 个内置流程、固定键位 | v1 控制范围，换取流程行为可验证 | 不支持自定义流程与快捷键（路线图 P1/P2） |
| 主题 token 化、不硬编码颜色 | 三套内置主题 + 用户自定义主题无需改渲染代码 | 多一层 token→Color 映射 |
| 点阵大字（5×7，可 2×） | 终端里有辨识度的「大字单词」体验 | 字形表与放大逻辑增加渲染复杂度；非 ASCII 回退文本 |
| 不引入音频/网络 | 无系统音频依赖、无隐私与联网问题 | 听写无发音、无云同步/在线翻译 |
| 存档字段沿用上游命名 | 为将来数据互通留余地（手工迁移已可行） | 命名被上游约束；新增字段需兼容 |

## 10. 性能与资源

以下为本机（Windows、开发机）参考值，用于量级判断，不是性能承诺：

| 场景 | 实测 |
| --- | --- |
| release 二进制（strip + thin LTO） | 约 2.5 MB |
| 内置 CET-4 词库资源 | 约 4.5 MB（2607 词，含释义/例句/词源） |
| `tw doctor`（release，含读取并统计 4.5 MB 词库） | 约 50 ms |
| `tw doctor`（debug 构建） | 约 200 ms |
| 首帧渲染（debug 构建，含装载当前词库） | 约 130–200 ms |
| 空转 CPU | 每 100 ms 一次 `poll`，无事件时仅重绘 + 定时检查，占用极低 |

设计上的性能取舍：

- 练习期间所有单词（2607 词规模）与卡片常驻内存，换取逐词键入零 IO；词库超大时启动内存线性增长，但当前内置词库量级无压力。
- 事件轮询超时 100 ms 决定空转节奏；有输入时立即重绘，交互响应取决于终端与渲染耗时（毫秒级）。
- 每帧从设置解析一次主题、构造 `Theme`（13 个颜色的小结构体），开销可忽略；若将来出现渲染热点，可缓存主题或按脏区重绘。
- 练习缓存的 500 ms 防抖避免每次按键都写盘；30 秒周期保存限制意外退出时的进度损失。

## 11. 可移植性与终端行为

- 基于 ratatui/crossterm：Windows Terminal、iTerm2、kitty 等支持 ANSI/鼠标的终端可用；conhost 对 CJK 宽度与部分组合键支持较差，README 建议 Windows Terminal。
- 布局使用 `unicode-width` 计算显示宽度，CJK 按 2 列处理，折行/截断/命中区域一致。
- 深色/浅色主题输出 24-bit RGB，需要终端支持 truecolor（`COLORTERM=truecolor`）；不支持时可用纯 ANSI 的「高对比」主题。
- 程序启动时开启鼠标捕获，因此终端原生文本选择/右键菜单失效，需要 Shift+拖拽选择（个别终端用 Alt）。
- 安装 panic hook：panic 时关闭鼠标捕获并恢复终端，避免终端留在备用屏幕或鼠标捕获状态。
- CI 在 Linux 与 Windows 上运行测试；macOS 未在 CI 覆盖，源码未使用平台特定 API（数据目录由 `dirs` 处理）。

## 12. 安全与隐私

- **无网络**：依赖树中没有 HTTP/WebSocket 客户端，程序不发起任何网络请求；无遥测、无账号、无云同步。
- **纯本地数据**：存档是可读 JSON，位于用户数据目录；备份/迁移即复制文件。
- **输入校验**：词库、存档、主题都是 serde 反序列化；主题文件校验失败只跳过该文件并告警；存档损坏会被隔离而不是覆盖。
- **无子进程/动态加载**：不执行外部命令（scripts/ 下的 Node/PowerShell 只在维护者本机用于同步词库与打包，不随程序运行）。
- **许可证**：GPL-3.0；词库数据来自上游 TypeWords 仓库，遵循同一许可证，再分发时需保留 LICENSE。

## 13. 已知限制与技术债

按影响排序（功能层缺失见 [upstream-parity.md](upstream-parity.md)）：

| 项 | 说明 |
| --- | --- |
| app ↔ ui 耦合 | `App` 直接调用 `ui::render`，各页面直接读 `App`；未来拆视图层需要先引入只读 ViewState。 |
| 界面文案硬编码 | `domain/i18n.rs` 是一张中文映射表，键名对齐上游 i18n，但没有语言切换；新增文案需同时改代码。 |
| 设置项只建模一部分 | `SettingState` 建模 28 个具名字段，设置页覆盖其中 22 个配置项 + 2 个动作（24 行）；其余上游字段原样透传保存。`enable_short_term` / `learning_steps` / `relearning_steps` 仍不生效（学习步进固定 1/6/10 分钟），详见 [settings.md](settings.md)。 |
| 键位不可配置 | 固定键位；`Ctrl+P` 从上游的发音改为暂停计时。 |
| 练习缓存只支持 v2 | v1 缓存直接丢弃（上游有 v1→v2 升级）。 |
| （已消除） | FSRS 已改为逐条移植上游 ts-fsrs 5.4.1，同一张卡在两端得到相同的下次复习时间；`tests/fsrs_parity.rs` 用上游生成的黄金样本锁定。 |
| 无运行日志 | 程序不写日志文件；排障依赖 `tw doctor`、Toast、`--seed` 与复现步骤（panic 回溯走 stderr）。 |
| 无存档迁移框架 | 旧版本缺失字段靠 `#[serde(default)]` 兼容；不提供像 Web 那样的逐版本升级代码（当前版本号 4 / 23 / 2）。 |
| 未覆盖 macOS CI | 代码未用平台特定 API，但未在 macOS 上持续验证。 |
| 文档链接检查 | 依赖仓库测试校验（见 [development.md](development.md#10-文档维护)），新增文档后需同步索引。 |

## 14. 扩展点

| 想做的事 | 建议改动位置 |
| --- | --- |
| 新增练习流程 | `domain/flow/mod.rs` 的 `builtin_flow` / `get_flow_id_for_mode`；`domain/constants.rs` 的模式名；`ui/practice.rs` 的阶段展示；`tests/flow_parity.rs` |
| 新增统计维度 | `domain/types.rs` 的 `Statistics`；`domain/statistics.rs`；`ui/insights.rs`；主页/统计页渲染 |
| 新增设置项 | `domain/types.rs` 的 `SettingState` 字段与默认值；`app/mod.rs` 的 `SETTING_ROWS` 与 `EditTarget`；`app/action.rs` 的读写逻辑；`ui/settings.rs` 分区；`persistence.md` 的字段表 |
| 新增页面/弹层 | `app/mod.rs` 的 `Screen`；`app/keymap.rs`；`ui/mod.rs` 分发；新页面需 `layout()` + `render()` + `targets()`；`tests/ui_snapshots.rs` |
| 新增鼠标交互 | `ui/<page>.rs` 的 `targets()`；`ui/hit.rs` 命中顺序；`app/action.rs` |
| 新增存档字段 | `domain/types.rs`（保持 serde 命名约定）；`store/mod.rs` 版本号处理；`persistence.md`；`tests/store_roundtrip.rs` |
| 新增主题 token | `theme/mod.rs`（模型与校验）；`theme/builtin.rs`；`ui/theme.rs`（映射）；`docs/themes.md` 的 token 表 |
| 接入新词库 | 把 JSON 放进 `assets/dicts/en/word/` 并登记 `assets/list/word.json`，或用 `tw import-dict`；无需改代码 |
| 新增 CLI 子命令 | `cli.rs` 的 `Command` 与 `lib::run` 分发；`docs/usage.md` |

## 15. 文档地图

代码区域与权威文档的对应：

| 代码区域 | 权威文档 |
| --- | --- |
| `src/domain/` | [domain.md](domain.md) |
| `src/app/` | [app.md](app.md) |
| `src/ui/` | [ui.md](ui.md) |
| `src/store/`、三份存档、资源 JSON | [persistence.md](persistence.md) |
| `src/theme/`、`ui/theme.rs` | [themes.md](themes.md) |
| 交叉能力（分层、生命周期、主循环） | 本文 |
| 构建/测试/CI/发布 | [development.md](development.md) |
| 与上游功能差异 | [upstream-parity.md](upstream-parity.md) |

## 相关文档

- [文档索引](README.md)
- [领域逻辑](domain.md)
- [应用状态机](app.md)
- [渲染层](ui.md)
- [存档与数据格式](persistence.md)
- [开发指南](development.md)
