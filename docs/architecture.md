# 总体架构

本文是 typewords-tui 的跨层说明：程序由哪些部分组成、它们如何协作完成一轮「选词库 → 练习 → 结算 → 统计」，以及改动时应先看哪里。各子系统的细节以源码与测试为准；这里只记录稳定的契约、不变量与扩展入口，不逐函数走读。

> 读者：贡献者与维护者。用户操作见 [使用手册](usage.md)，设置见 [设置项参考](settings.md)，数据格式见 [存档与数据格式](persistence.md)，主题见 [主题系统](themes.md)，与上游的差异见 [与上游对照](upstream-parity.md)。

## 1. 项目定位与能力边界

typewords-tui 是上游 [TypeWords](https://github.com/zyronon/TypeWords)（Nuxt + Vue 的 Web / Tauri 桌面端）拆分出的独立终端客户端：单进程 Rust 程序，数据全部是本地 JSON，不共享上游代码，也不读写浏览器 IndexedDB。默认离线；唯一的联网能力是用户主动开启「有道发音」后下载 mp3。

已实现的能力（文档与代码必须按这个口径描述）：

- 10 种练习模式：学习、自由练习、自测、默写、听写、随机复习、复习、随机单词测试、单词测试、自定义流程。
- 7 个内置练习流程，以及基于内置流程副本编辑步骤的自定义流程。
- 文章背诵与逐句跟打、文章结算与断点续学。
- FSRS-6 调度（逐条对齐上游 ts-fsrs 5.4.1，黄金样本锁定）。
- 词条管理（搜索、编辑、增删、排序、批量标记、新建 / 复制自定义词典）。
- 42 个可改绑的快捷键动作、6 套内置主题与用户自定义主题。
- 系统 TTS / 终端响铃的音频实现，可选有道发音。
- 与网页版备份 JSON（BackupData v5）互通的 tw import-web / tw export-web，词库词条可经 tw import-dict / tw export-dict 往返。
- 默认开启的运行日志与 panic 崩溃报告（`<home>/logs/`），以及 Windows 双击启动时的早退保护。

非目标（不计划在 TUI 内实现）：Supabase 云同步、在线查词 / 翻译、多语言界面、Web / Tauri 界面、xlsx 与 ZIP / mp3 打包备份、跨设备冲突合并。

硬约束：

- 终端最小 60 × 20；宽度 < 100 切窄屏顶栏，>= 120 侧栏 26 列，否则 24 列。
- Windows 是主要发布平台；CI 在 Ubuntu 与 Windows 上跑测试，macOS 未持续验证。
- 运行时不依赖 Node.js / WebView / 网络；词库与文章资源随发布包分发。
- 发布包中 assets/ 必须与可执行文件同级，或用 --assets 指定。

## 2. 技术栈与运行约束

| 依赖 | 版本 | 用途 |
| --- | --- | --- |
| Rust（edition 2024，MSRV 1.90） | — | 程序与测试语言；release 使用 opt-level = 3、thin LTO、strip |
| ratatui | 0.30.2 | 终端布局、组件与渲染；事件层复用其转出的 crossterm |
| serde / serde_json | 1 | 存档、词库、主题 JSON；未知字段保留 |
| chrono | 0.4 | 本地自然日、跨天时间片（带 serde） |
| rand | 0.10 | 可注入种子的随机源与洗牌 |
| clap | 4（derive） | 命令行参数与子命令 |
| anyhow / thiserror | 1 / 2 | 跨层错误传播；thiserror 为预留的领域错误类型 |
| dirs | 7 | 跨平台本地数据目录 |
| unicode-width | 0.2 | CJK 显示宽度；折行、截断、命中共用 |
| tempfile（dev） | 3 | 测试临时目录 |

运行约束：

- 单线程、无 async 运行时、无线程、无 unsafe；时间、Toast 过期、定时保存都由 on_tick(now) 按时间戳推进。
- 子进程仅限音频：Windows 的常驻 PowerShell TTS、外部播放器（ffplay / mpv / vlc）。它们即发即忘或常驻复用，不阻塞事件循环。
- 依赖树中没有 HTTP / WebSocket / 音频解码库。可选的有道发音用系统 curl 下载 mp3，再交给外部播放器。
- 所有随机性来自注入的 Rng；--seed 固定后，今日任务、题目顺序与错词重练都可复现。
- 每帧全量重绘；事件轮询 100 ms，一次最多处理 64 个事件。

## 3. 分层与依赖规则

    main.rs（先装 crash 的 panic 钩子）-> lib.rs::run(cli)
      -> Paths::resolve_home          数据目录（先于资源目录：日志要尽早可用）
      -> logging / crash              初始化运行日志、记下崩溃报告上下文
      -> Paths::resolve_in            资源目录
      -> cli.rs                        doctor / task / import-dict / export-dict / import-web / export-web
      -> app::run(paths, cli)
           -> App::bootstrap           bootstrap.rs: store + domain + theme
           -> 事件循环: ui::render(&App)（只读）
                        keymap / mouse / buttons -> Action -> App::apply_action
                                      -> domain（typing / flow / navigator / session / scheduler / timer / statistics / article）
                                      -> store::FileStore -> JSON 存档

| 层 | 目录 | 职责 | 依赖约束 |
| --- | --- | --- | --- |
| domain | src/domain/ | 与终端无关的类型、词库、今日任务、FSRS、流程、键入、会话、文章、统计 | 不依赖 app / ui / store / theme，也不引入终端相关 crate；tests/layering.rs 守护 |
| store | src/store/ | 5 份 JSON 存档的读写、版本校验、损坏隔离 | 依赖 domain 与 fs |
| theme | src/theme/ | 主题模型、6 套内置主题、用户主题加载与校验 | 不依赖 app / ui，也不引入终端相关 crate；tests/layering.rs 守护 |
| fs | src/fs.rs | 原子写（唯一临时名 + fsync + rename）与数据目录锁 | 只依赖 std；domain 与 store 共用，避免两层成环 |
| app | src/app/ | App 状态机、Action、键鼠映射、焦点、事件循环、保存时机 | 依赖 domain / store / bootstrap / cli / paths；渲染入口调用 ui |
| ui | src/ui/ | ratatui 布局与绘制、token → Color、鼠标命中 | 只读 App 与 domain 数据，不修改状态 |
| audio | src/audio/ | 终端响铃、系统 TTS、有道下载与外部播放器 | 只依赖 domain 的设置与 logging；子进程不阻塞事件循环 |
| logging / crash | src/logging.rs、src/crash.rs | 运行日志（追加写、按启动日期分文件、保留清理）、panic 崩溃报告与 Windows 早退保护 | 只依赖 std + chrono + anyhow；不做业务判断，写日志失败不影响启动 |
| 入口/装配 | src/main.rs、lib.rs、cli.rs、paths.rs、bootstrap.rs、backup.rs | 解析参数与路径、装载初始状态、分派子命令、网页版备份适配 | 最外层，可依赖所有层 |

三条必须记住的实际约束：

1. domain 不依赖终端，因此可以被单元测试与 tw task 直接复用；与上游实现对照时，它也是主要隔离层。
2. app 与 ui 在同一 crate 内互相引用：App::render 调用 ui::render，App::theme 返回 ui::Theme，各页面直接读 App 字段。这是省去视图模型的刻意取舍，代价是两块耦合较紧；ui 的写权限必须保持在「只读 App、只通过 Target 产出 Action」。
3. logging / crash 是进程级全局状态（`static Mutex`），只在 main.rs 与 lib::run 里初始化；其它层只通过 `logging::info/warn/error/debug` 打点，不碰全局。业务层不因为日志失败而失败。

## 4. 代码地图

src/ 共 67 个 Rust 文件，按目录分组：

| 目录 | 文件与职责 |
| --- | --- |
| src/（9） | main.rs 入口（第一件事装 panic 钩子）；lib.rs 模块声明、run()、版本行；cli.rs 参数 / 6 个子命令；paths.rs 数据目录与资源目录解析；bootstrap.rs 启动装载；fs.rs 原子写与目录锁；backup.rs 网页版 BackupData v5 导入导出；logging.rs 运行日志；crash.rs 崩溃报告与早退保护 |
| src/app/（7） | mod.rs App、事件循环、on_tick、会话集成、SETTING_ROWS；action.rs Action 与 apply_action；buttons.rs 两个键入页的动作按钮；dicts.rs 词库页条目与词数唯一口径；focus.rs 焦点状态机；keymap.rs 非练习键位 → Action；mouse.rs 鼠标 → Action |
| src/audio/（4） | mod.rs 音频分发；bell.rs 终端响铃；tts.rs 系统 TTS（Windows 常驻 PowerShell）；player.rs 有道发音下载与外部播放器 |
| src/domain/（17） | mod.rs；types.rs 存档模型与整数枚举；constants.rs；catalog.rs 词库目录 / 词条加载 / import_dict（导出的读取与写回在 cli.rs，因为要读存档里的内置词典）；study_task.rs 今日任务；fsrs.rs FSRS-6 移植；scheduler.rs 评级与到期；typing.rs 键入状态机；question.rs 自测题；session.rs 练习会话；article.rs 文章模型 / 断句 / 跟打；shortcut.rs 42 动作键位表；wordbook.rs 词条管理；statistics.rs 统计落盘与聚合；timer.rs 计时；i18n.rs 中文文案；rng.rs 可注入随机源 |
| src/domain/flow/（3） | mod.rs 内置流程 / 游标 / 相位 / 展示；navigator.rs 推进器与快照恢复；store.rs 自建流程数据结构 |
| src/store/（1） | mod.rs 5 份存档的读写、信封、版本、隔离、原子写 |
| src/theme/（3） | mod.rs 主题模型与注册表；builtin.rs 6 套内置主题；load.rs 用户主题加载与校验 |
| src/ui/（23） | mod.rs 渲染入口与弹层顺序；theme.rs token → Color；shell.rs 响应式外壳；home.rs；practice.rs；word_test.rs；words.rs；articles.rs；flows.rs；dict_picker.rs；stats.rs；settings.rs；shortcuts.rs；adv_fsrs.rs 高级 FSRS 编辑页；help.rs；bigtext.rs 5x7 点阵；typing_buttons.rs；overlays.rs；widgets.rs；focus.rs 焦点几何 / 分带；insights.rs 统计聚合；target.rs Target / FocusId / ContentId；hit.rs 命中检测 |

测试与辅助：

- tests/：35 个集成测试文件 + common/mod.rs（Fixture：临时 home + 12 词 toy 词库，默认 --seed 42）。
- examples/：dump.rs（练习页转纯文本，覆盖 7 个内置流程模式）；shot.rs（任意页面渲染成 JSON 快照）。
- tools/render-snapshot.py（JSON → PNG，需 Pillow，仅维护者用）。
- scripts/sync-assets.mjs（同步上游词库 / 文章）、scripts/package.ps1 与 scripts/package.sh（本地打包，分别用于 Windows 与 Linux / macOS）、scripts/changelog.sh（从根目录 `CHANGELOG.md` 取某版本的发布说明，发布 workflow 与本地预览共用）。
- assets/：list/word.json、list/article.json、dicts/en/word/CET4_T.json（2607 词）、dicts/en/article/NCE_1.json（5 篇）。
- .github/workflows/ci.yml（fmt / clippy / Ubuntu + Windows 测试）、release.yml（`v*` tag 触发：先 `guard` 校验 tag、版本与 `CHANGELOG.md` 里该版本的小节、`verify` 跑与 ci.yml 同一套 fmt / clippy / 测试，全绿后才打五平台 tar.gz 并发布：Windows x86_64、Linux x86_64 gnu + musl、macOS arm64 与 x86_64；发布说明由 `scripts/changelog.sh` 从 `CHANGELOG.md` 提取）。

## 5. 进程生命周期

### 5.1 启动与子命令

1. main.rs 第一件事是 `crash::install_hook()`（早于 Cli::parse 与任何路径解析），再 Cli::parse()、调用 typewords_tui::run(cli)；run 返回错误时 `crash::report_fatal_error` 打印 `Error: <错误链>` 与日志路径，Windows 双击启动（无参数、交互式终端）再等一次回车，退出码 1。
2. lib.rs::run 先 Paths::resolve_home（数据目录：--home > TYPEWORDS_HOME > 系统本地数据目录），再 `logging::init` 初始化运行日志（--log / --no-log / --log-level；返回实际日志文件，不写日志时为 None）并 `crash::set_context` 记下数据目录与报告目录，随后记录平台、进程号、命令行、数据目录与日志文件。资源目录缺失这类最常见的启动失败因此也在日志里。
3. run_resolved 再 Paths::resolve_in（资源目录：--assets > TYPEWORDS_ASSETS > exe 同级 assets/ > 当前目录及最多 3 级祖先的 assets/ 或 public/），补一次 `crash::set_context`（资源目录 + 日志文件）并记录资源目录；只读入口（tw doctor / tw export-dict / tw export-web）直接分派，其余入口先取 <home>/.tw.lock 再分派，两条路径都记「执行: <子命令>」。
4. 按 cli.command 分派；无子命令才进入 TUI。返回时按结果记「进程退出: 正常」或「进程退出: 失败: <错误链>」。

| 命令 | 行为 | 进入 TUI |
| --- | --- | --- |
| tw doctor | 打印版本、数据 / 资源目录、每本词库的实际可练习词数与文件路径、5 份存档大小、主题 id / 名称 / 告警；只读，不取数据目录锁 | 否 |
| tw task | 装载存档与词库，用 --seed（缺省 42）生成今日任务，打印词库、进度、新词 / 复习 / 到期数与前 5 个词；取数据目录锁 | 否 |
| tw import-dict FILE | 校验词条数组 JSON，复制到 home/dicts/en/word/，登记 home/list/word.json；取锁 | 否 |
| tw export-dict [DICT_ID] [--out FILE] | 按 id / enName（缺省 --dict，再缺省当前学习词库）导出词条数组 JSON；内置词典从存档取词条，普通词库从词条文件补齐；只读，不取锁 | 否 |
| tw import-web FILE | 解析网页版 BackupData v5，按版本与结构校验后拆成 dict / setting / practice-article 等文件，覆盖前留 .bak，失败回滚；取锁 | 否 |
| tw export-web [--out FILE] | 把当前数据目录导出成网页版可导入的 BackupData v5；只读，不取锁 | 否 |

5. App::bootstrap 的顺序：
   - bootstrap::load 创建 FileStore、读 dict.json / setting.json、加载 Catalog 与 ThemeRegistry；损坏、版本过高、主题告警收进 notices。--dict 先匹配三本系统词典 id，否则按 id / enName 选择资源词库。
   - 应用 --mode（cli::parse_mode）与 --theme 覆盖；构造 Rng（--seed 或随机种子）。
   - notices 转 Warning Toast；主题回退时追加提示。
   - 正常启动读入两份断点缓存（主页「上次任务」、文章列表「上次第 N 句」与只读保护都靠它）；--reset-practice 只探测可读性（不读内容，免得把损坏的缓存先隔离成副本）再删除两份缓存，读不出来的不删、提示「练习进度未清空」。
   - ensure_study_dict：系统词典保持不动；资源词库缺词表时按 id 重载，找不到则回退 cet4 或第一本；装载后按实际词数校正 length 与进度，丢弃不匹配的旧断点。
   - refresh_stats 与 refresh_task_preview 计算主页缓存。
   - 词库加载失败记 fatal；app::run 在进入终端前 bail!，不留下半初始化界面。
6. App::run：先在 crate::crash 的钩子外面包一层 `install_panic_hook`（panic 时先关鼠标捕获、`ratatui::restore()`，再转交 crash 钩子写报告与等回车），记录「进入 TUI」，`ratatui::init()` 进入备用屏幕并开启鼠标捕获，进入事件循环；退出后关闭捕获、恢复终端，并记「TUI 已退出，终端已恢复」或「TUI 异常退出」。
7. 事件循环退出后 save_all()：保存词典、设置与未结算的练习 / 文章缓存；关闭鼠标捕获并恢复终端。

panic 钩子的组合顺序不能变：crash 的钩子在 main 里最先安装（只在第一次调用时安装，保留原钩子并在写报告后转交），app 的钩子装在它外面，只负责恢复终端再转交。报告自身 panic 不会递归（`crash::REPORTING` 只放行第一次）。`set_context` 分两次调用——先数据目录、资源目录解析成功后再补齐——因此连「找不到 assets/」的崩溃报告也能落到数据目录。

### 5.2 事件循环

    loop {
        terminal.draw(|frame| app.render(frame))
        app.on_tick(now_ms())
        if app.quit { break }
        if event::poll(100ms) {
            最多消费 64 个事件：
                Key(Press) -> app.on_key
                Mouse       -> app.on_mouse(m, terminal_area())
                Resize 与其它 -> 忽略（下一帧按新尺寸重排）
        }
    }
    app.save_all()

- 只处理 KeyEventKind::Press，忽略 Release / Repeat，避免 Windows 上重复触发。
- 每帧把 viewport 同步成 terminal_area()。键盘焦点要按布局算几何，而 on_key 不经过渲染，只能靠这个字段知道终端尺寸；测试与 examples/shot 用 App::set_viewport 显式设置。
- 退出只有两条路径：主页 q → 确认弹层 → quit = true，或事件循环返回错误；两者都走保存与终端恢复。
- 没有后台线程。计时、自动保存检查、Toast 过期都在 on_tick 中按时间戳推进。

### 5.3 on_tick 的四块逻辑

| 顺序 | 逻辑 | 细节 |
| --- | --- | --- |
| 1 | Toast 过期 | 所有 Toast 的 until <= now 时清空；渲染侧只显示未过期项 |
| 2 | 练习计时 | 单词练习：screen == Practice 且有 session 时每满 1000 ms 调 timer::tick（空闲 180 秒自动暂停），本次确实累计时长且跨过 30 秒整数倍时调度防抖保存。文章跟打：screen == ArticlePractice 且有 article_practice 时同样按 1000 ms 累加 spend；暂停时只推进 last_timer_tick，不累计。单词测试页也单独累计 spend |
| 3 | 键入定时 | 练习页构造 TypingContext 并调 typing.on_tick，处理自动下一词、重复练习、错误清空等定时器 |
| 4 | 防抖保存 | pending_practice_save / pending_article_save 到期时立即落盘 |

## 6. 状态、输入与数据流

### 6.1 单一状态源与两条输入路径

App 是唯一的状态持有者。输入分两条路径：

- 非练习页 / 弹层 / 鼠标 / 集成测试：keymap.rs 与 ui/hit.rs 产出 Action，统一由 App::apply_action 执行；Action 目前有 103 个变体。
- 练习页键盘（以及文章跟打页键盘）：App::on_practice_key / on_article_practice_key 直接调用键入状态机与会话方法，不经过 Action。这里是输入频率最高、状态迁移最复杂的路径。

此外 App::bootstrap 与 App::on_tick 也直接更新状态。因此更准确的约束是：domain 提供受控迁移函数，app 是唯一修改状态的地方，ui 只读。

   非练习键位 -> keymap.rs（弹层优先 -> 屏幕键位）─┐
   鼠标       -> mouse.rs -> ui::hit::action_at ────┼─> Action -> App::apply_action -> 状态
   测试       -> 直接构造 Action / KeyEvent ─────────┘

   练习页键位 -> App::on_practice_key -> TypingEngine / PracticeSession
   启动/定时  -> App::bootstrap / App::on_tick -> 初始化、计时、保存、Toast
                                       ui::render(&App) 只读绘制

收益：键盘、鼠标、测试共享同一条迁移路径；渲染层没有隐藏副作用；新增非练习交互只需加 Action、映射与实现。代价是两套输入路径并存，新增练习页按键时要同时照顾 on_practice_key、按钮表与帮助页文案。

### 6.2 屏幕与弹层

13 个屏幕（app::Screen）：

| 变体 | 页面 | 外壳 / 入口 |
| --- | --- | --- |
| Home | 主页 | 侧栏「单词」；数字 1 |
| Practice | 练习页 | 全屏；Enter / f / 结算继续 |
| WordTest | 单词测试 | 专用答题页；不套外壳 |
| Words | 词条管理 | 词库页 w |
| Articles | 文章列表 | 侧栏「文章」；数字 2 或主页 a |
| ArticlePractice | 文章跟打 | 全屏 |
| Flows | 练习流程 | 主页 Ctrl+F |
| DictPicker | 词库页 | 侧栏「词库」；数字 3 |
| Stats | 统计页 | 侧栏「统计」；数字 4 |
| Settings | 设置页 | 侧栏「设置」；数字 6 |
| Shortcuts | 快捷键编辑页 | 设置页「快捷键」 |
| AdvFsrs | 高级 FSRS 页 | 设置页「FSRS → 高级 FSRS」 |
| Help | 帮助页（快捷键 / 常见问题 / 关于与反馈三个分页） | 数字 5 或 ? |

7 种弹层：summary、edit、confirm、word_edit、mode_picker、group_picker、collect_picker。正常交互中互斥；渲染顺序固定为 summary → edit → confirm → word_edit → mode_picker → group_picker → collect_picker（后画的在上），打开时给全 buffer 加 DIM 模拟遮罩。

不变量：

- 练习中禁止切页：navigate_to 在有 session 时直接返回；练习页只能通过 Ctrl+C 或完成结算离开。
- session 与 article_practice 不会同时为 Some；ArticlePractice 只在 article_practice 存在时进入。
- 所有切屏走 App::set_screen：清空焦点并按页面类型设置默认焦点（主页选中主按钮，其它套外壳页面选中内容区，全屏练习页无焦点）。
- Back 的语义：先关弹层，再按屏返回；设置页先保存设置，词库 / 统计回主页，帮助页按是否有会话回练习页或主页。
- 键盘焦点存稳定身份（导航栏 NavId、主页 ContentId、其它页面 Page），不存坐标；导航栏与主页每次按键前按当前布局重新解析，元素消失即清空并提示，不会误触发动作。

### 6.3 一轮练习的数据流

    BaseState(dict.json) ──┐
    SettingState ──────────┼─> create_study_task -> TaskWords（新词 + 到期复习 + 随机复习）
    Catalog(assets) ───────┘                         │
                                                     v
                                     PracticeSession::new / begin_session_with_task
                                                     │
                       ┌─────────────────────────────┴──────────────┐
                       v                                            v
                 TypingEngine（按键状态机）                  Navigator（流程游标 / 相位）
                       │ TypingEvent(Completed/Wrong/Notice)        │ Notice
                       └────────────> App::handle_typing_events <───┘
                                             │
                                             v
                         PracticeState（计时 / 新词 / 复习 / 错误）+ 系统词典写入
                                             │
                              ┌──────────────┴──────────────┐
                              v                             v
                     会话游标推进                    practice-word.json（防抖）
                              │
                              v
                finish_session -> flush_stat_to_store -> dict.json 的 statistics
                              │
                              v
                SummaryData -> 结算弹层 -> 主页并刷新统计 / 今日任务

关键点：

- 今日任务在进入练习前一次性生成；练习过程中不再重新取词。
- PracticeFlowCursor 是流程位置的唯一指针；细节见 7.1。
- FSRS 评级与卡片更新在会话内完成；结算时把时长、词数、错误数聚合进词典 statistics。
- 断点缓存是会话快照（flow_id + 游标 + 任务词表 + 统计快照）；恢复要求词库与流程仍匹配。
- 中途退出不落统计：只有结算（settle）或丢弃缓存时的兜底（flush_practice_stats）会写 statistics，与上游一致。

### 6.4 时间与确定性

- 时间参数统一是毫秒时间戳；on_tick(now) 让测试可以注入任意时间推进计时、Toast 与防抖保存，测试不要 sleep。
- 到期判断、统计分天、摘要日期都基于 chrono::Local 的自然日；FSRS 内部的 elapsed_days 用 UTC 日期差（对齐 ts-fsrs）。
- 随机性全部来自注入的 Rng；--seed 固定后任务与题目可复现。App::bootstrap、begin_session_with_task、refresh_task_preview 都会消耗随机数，因此同一路径与种子下结果一致，中间多一次预览会改变后续抽签。

## 7. 子系统契约

这一节只列每个子系统必须保持的契约与入口符号；实现细节见对应源码的 //! 注释与测试。

### 7.1 domain（src/domain/）

数据模型与序列化（types.rs）：

| 结构 | 命名 | 特殊处理 |
| --- | --- | --- |
| 大多数结构 | camelCase + serde(default) | 缺字段回退默认值 |
| Card | snake_case | 对齐 ts-fsrs；due / last_review 为 RFC3339 UTC |
| FsrsParameters | snake_case | 与 ts-fsrs 参数同名 |
| SettingState | camelCase | tuiBigWord / _ignoreWatch 显式 rename；未知字段进 extra |
| Dict / DictResource | camelCase | enName / en_name 双向兼容；未知字段进 extra |
| PracticeFlowCursor | camelCase | loop_range 序列化为 loop |
| PracticeCacheCompact | camelCase | 词表以单词字符串保存 |

常量（constants.rs）：GROUP_SIZE = 7、CURRENT_FLOW_VERSION = 6、三本系统词典 id（wordCollect / wordWrong / wordKnown）、SIMPLE_WORDS（42 个）、DEFAULT_FSRS_W（21 个）、DICT_STATE_VERSION = 4、SETTING_VERSION = 23、PRACTICE_CACHE_VERSION = 2。

词库目录（catalog.rs）：

- Catalog 合并 assets/list/word.json 与 home/list/word.json；同 id 时用户目录覆盖资源目录；词条文件先找 home 再找 assets。
- load_words 用 Word::is_usable 过滤空词条；界面、tw doctor、tw task 的词数都必须走同一条加载路径，不能自己解析 JSON 再 len()。
- 词数唯一口径是有效词数（实际可练习词条）；目录登记的 length 只在尚未加载词表时兜底，加载后会被校正。

今日任务（study_task.rs）：

- 从 last_learn_index 向后扫描，跳过忽略集合（已掌握 + 可选的 SIMPLE_WORDS），取 per_day 个新词。
- 学完判定只看词表本身（effective_length），不看目录登记 length；最后一个词也必须能学到，学完后 complete 置位。
- 忽略集合只有一份口径：`BaseState::ignore_words_set(ignore_simple_word)`（全小写）。任务生成与结算推进（session.rs 的 advance_dict_progress）都用它并按 `Word::lower` 比较；两边不一致时，词库尾部只剩首字母大写的忽略词（如 CET-4 的 Christ）会让 complete 永不置位。
- 到期复习按 Local 自然日判断（due <= 今天），按 due 升序取 per_day * ratio 个再洗牌；ratio 为 0 时，学完 / 到末尾仍按至少 1 处理。
- 无到期且开启随机填充时，从已学范围排除忽略词、本轮新词与已抽中的词，随机补足。
- create_shuffle_task 用于随机复习：range 上界在尚未学时取整本词表，否则取 last_learn_index。

FSRS（fsrs.rs + scheduler.rs）：

- Fsrs 是算法本体：逐条移植 ts-fsrs 5.4.1 的算法层、状态机、学习步进、fuzz 区间与随机源，同一张卡在两端得到相同间隔；tests/fsrs_parity.rs 用黄金样本锁定。
- Scheduler 只做两件事：按错误次数映射评级、按自然日判断到期。
- 评级阈值默认 0 / 3 / 6，按 Easy → Good → Hard 顺序匹配；自测「认识」或选择题答对写入的显式 Good 优先于错误次数。
- 卡片状态与步进、公式、fuzz 细节见 fsrs.rs 的文档注释；不要在别处复制一份公式。

流程与推进（flow/mod.rs + flow/navigator.rs）：

- 内置流程 7 个，id 与模式对应：system、free、identifyOnly、dictationOnly、listenOnly、shuffle、review；10 种模式见 usage.md 与 domain/constants.rs。
- 两种单词测试与 Custom 没有内置流程；get_flow_id_for_mode 对它们兜底 system，只为保证会话总能拿到可跑配置。单词测试实际直接进入 WordTest，不执行流程。
- PracticeFlowCursor 是唯一位置指针：node_index、step_index、in_wrong_word_clear、loop_range、end_action_index。持久化只存 flow_id + 游标 + 工作词 key，不存节点/步骤副本。
- 所有「当前练什么」都经 resolve_phase_by_cursor；display_state 只读游标派生展示数据。
- 同一 Node 内步骤共享 node_working_words；跨 Node 才按新词源重新解析（Current 节点沿用工作词表）。
- WordLoop 每完成 group_size 个词回放子步骤（学习流程是跟写后回放拼写）；最后一组不足一组时补齐回放。
- on_end 队列：WrongWordClear（有错词则洗牌重练）、NavigateNextStep、NavigateComplete、CollectWrongWords、GenerateReport。错词清空会过滤忽略集合与 exclude_words，重练再次出错的词继续进入 wrong_words。
- 恢复快照时逐项校验 flow_id、下标、in_wrong_word_clear 与 end_action_index 的一致性、loop_range 与 WordLoop 的匹配；不合法就重置到 system 初始游标。
- 分组导航已接通：上一组 / 下一组 / 重学本组 / 默写本组 / 跳组都通过 session 的组重建路径工作。

键入（typing.rs）：

- TypingEngine 消费 KeyInput（Char / Space / Backspace），产出 TypingEvent（Completed、Wrong、Notice 等）；应用层只负责把事件转成状态与 Toast。
- 跟写、拼写、听写、默写、自测的显隐默认由 session 的 DisplayPolicy 决定；Esc 显示答案在遮罩或非跟写阶段记一次错误（同一词同一阶段只计一次）。
- 重复次数只对跟写生效；自动下一词只对跟写与拼写生效；错误清空、空格冷却、切词等待都由设置驱动。
- 键入只接收不带 Ctrl / Alt 的按键；带修饰键的组合留给快捷键。

会话（session.rs）：

- PracticeSession 持有 Navigator、PracticeData、TaskWords、整本词典乱序词表（仅自测题用）、DisplayPolicy、mode / complete / last_word_key。
- 初始化 / 恢复 / 推进 / 结算 / 组重建都通过会话方法；应用层不直接改游标。
- 结算（settle）是 FSRS 卡、statistics、词典进度的唯一写入点；练习过程中只有错词本、收藏、已掌握会立即变化。
- 断点缓存 cache_to_compact / cache_from_compact 只保留可恢复所需的最小状态；恢复失败静默丢弃，不删除文件，等待下一次覆盖。

计时与统计（timer.rs + statistics.rs）：

- timer 以 1 秒节拍累加片段；180 秒无操作自动暂停；跨天时按本地自然日拆分片段。
- flush_stat_to_store 在结算时把 PracticeState 写入 dict.statistics；跨天会话按 start / middle / end 角色拆分，计数只归开始日，时长按天累加。
- daily_totals 遍历全部词典的 statistics，按本地日期聚合；计数只取 single / start / 缺失角色的记录，避免跨天重复。
- 中途退出时统计留在缓存；开新一轮或清空进度时用 flush_practice_stats 兜底，stats_recorded 保证同一轮只记一次。

文章（article.rs）：

- parse_sentence 先归一化智能引号，再依次匹配「货币 + 数字」「数字」「带点缩写」「单词」「单个符号」，与上游 hooks/article.ts 一致。
- gen_sections 把正文按段落与句子展开，并与译文按空行对齐。
- 跟打目标是字母、数字与词间空格；标点与多余空白不是目标。词间空格可省略：打下一个字母时自动补成正确输入，错误只会落在字母 / 数字位置。
- 断点缓存记录文章 id、句子下标与用时；恢复时下标 clamp 到当前句数，文章已不在列表时忽略。文章 0 句不显示断点标记。
- 书本篇数只认文章文件里的实际篇数；文章文件读不到时显示「篇数未知」，不拿目录登记值充数。

快捷键（shortcut.rs）：

- ShortcutAction 42 个，与上游 ShortcutKey 一一对应；ShortcutMap 是动作 → 键位的数据表，存档字段 shortcutKeyMap。
- 同一个键位可绑定多个动作，由当前场景决定取哪个；空串表示解除绑定。
- 终端兼容别名不落盘：Ctrl+N = 下一个词、Ctrl+J = 下一阶段；别名参与冲突检查。
- 领域层用 BaseKey / ShortcutKey 表示键位，不依赖终端 crate；app 层负责把 KeyEvent 转成 ShortcutKey。

词条管理（wordbook.rs）：filter_words（搜索）、mark_know / mark_mastered / mark_unknown、sort_words（原序 / 正序 / 倒序 / 随机）、create_custom_dict；系统词典只改 dict.json，资源词库把词表写回对应词条文件并同步用户目录 length。

其他：i18n::tr 是唯一文案入口（硬编码简体中文，键名对齐上游）；rng 是唯一的随机源工厂；question 用释义相似度 + 随机扰动生成 4 选 1。

### 7.2 store / fs / paths

- 5 份存档：dict.json（BaseState，v4）、setting.json（SettingState，v23）、practice-word.json（PracticeCacheCompact，v2）、practice-article.json（ArticleCacheEnvelope，v1）、flows.json（FlowStore，无版本）。
- 前 4 份（dict / setting / practice-word / practice-article）外层是 SaveEnvelope { val, version, updated_at }；flows.json 是裸 FlowStore，无信封、无版本号。写入用 serde_json 紧凑格式。
- 原子写：同目录唯一临时文件 .<文件名>.tw-<pid>-<n>.tmp（create_new，撞名换序号）→ write_all → sync_all → rename；失败清理临时文件。原子性只覆盖单份文件，多份文件之间可能来自不同时刻。
- 数据目录锁：写入口在启动时对 <home>/.tw.lock 取独占锁；锁随进程退出释放，锁文件常驻不删。doctor / export-dict / export-web 是只读入口，不取锁。
- 读取状态：Missing（默认值）、Loaded、Corrupt（隔离为 *.corrupt-<时间戳>.json）、Unreadable（IO 读失败：不隔离、不改写、不删除，本次运行只读）、VersionTooNew（dict / setting 只读不覆盖）、UnsupportedVersion（练习缓存忽略，不移动文件）。
- dict / setting 版本高于程序支持版本时置 readonly_dict / readonly_settings，本次运行不写回；练习缓存只接受 v2。
- dict / setting 为 Unreadable 时进入终端前 bail!（用默认值继续跑会在退出时覆盖好档）；两份断点缓存与 flows.json 为 Unreadable 时本次运行只读：保存 / 删除入口跳过（清空练习进度按实际结果提示），启动校正进度时也不会删除它。
- 字段与备份细节见 persistence.md。

### 7.3 theme

- ThemeSpec = id + name + base + 13 个颜色 token；ThemeColor 支持 RGB 或 ANSI。
- 6 套内置：dark、light、high-contrast、midnight、paper、ikb；DEFAULT_THEME_ID = dark。
- auto 不做终端背景探测，始终解析为 dark；未知 id 回退 dark 并产生告警。
- 用户主题放在 <home>/themes/*.json；加载失败只跳过该文件并记 warning，tw doctor 会列出告警。
- theme/ 层不依赖 ratatui；ui/theme.rs 负责 token → ratatui Color。渲染层不得出现硬编码颜色。
- 细节见 themes.md。

### 7.4 app

- App 状态分组：启动装载（boot、store）、页面（screen、各光标）、焦点（focus、last_content_focus、viewport）、会话（session、typing、stat、identify_select）、随机源（rng、seed）、提示与弹层（toasts、quit、edit、confirm、summary）、显示开关（show_toolbar / show_panel / show_meta）、动作按钮（typing_button）、统计缓存（task_preview、today_*）、生命周期（fatal、last_activity、last_timer_tick、pending_practice_save）。
- 设置页由 SETTING_ROWS 单一数据源驱动：33 行（31 配置 + 2 动作），7 个分区；SettingKind 决定提交行为（Mode 打开模式选择弹层、Theme 循环、Bool 切换、Number 打开输入弹层、Action 打开确认、Shortcuts 打开快捷键编辑页、AdvancedFsrs 打开高级 FSRS 页、AudioSource 切换）。高级 FSRS 页的行由 `AdvFsrsRow` 枚举，权重区间统一取 `domain::fsrs::weight_bounds`，修改经 `App::activate_adv_fsrs` / `apply_weight_text` 落盘。
- 保存时机：会话开始 / 推进 / 错误 / 自测 -> 500 ms 防抖保存练习缓存；30 秒整点 -> 防抖保存；Ctrl+C -> 立即保存缓存 + 词典；结算 -> 保存词典并删除练习缓存；设置修改 -> 立即保存设置（每日词数是词典字段）；退出 / 事件循环结束 -> save_all；窗口缩放不保存。保存失败转 Error Toast，不 panic。
- 清除所有数据：readonly 时拒绝；清缓存、恢复 BaseState / SettingState 默认、重新选学习词典、保存并刷新统计与任务；不可撤销。高级 FSRS 页的「恢复默认」只覆盖 `fsrsParameters`（含 `learning_steps` / `relearning_steps`），不动其它设置与学习记录。
- 细节：SETTING_ROWS 与 action.rs 是权威；按键语义见 usage.md / shortcuts.md。

### 7.5 ui

- 渲染入口 ui::render：终端过小检查 -> 铺背景 -> 按 Screen 分发 -> 弹层遮罩 -> 按固定顺序画弹层。
- 每个页面都遵循同一模式：layout(area, ...) 计算 Rect，render() 与 targets() 共用同一份布局；滚动位置也来自 App 状态，因此绘制位置与可点击位置不会漂移。tests/ui_matrix.rs 的表驱动矩阵守护这一点：13 个页面 × {60×20, 80×24, 100×30, 120×40, 160×50} 与 7 种弹层 × {60×20, 100×30, 120×40}，逐条断言目标矩形在画面内、且矩形范围内确实画着文字（逐列读缓冲区，CJK 续接列按空格处理）。tests/ui_snapshots.rs 另有主页两个目标的语义断言（命中行含按钮文案）。
- 外壳：宽屏侧栏、窄屏顶栏、底栏 Toast / 快捷键提示 / 版本号；布局函数与命中函数共用 ShellLayout。
- 键盘焦点：FocusId 三态（Nav / Content / Page）；宽屏两栏（导航栏 ↔ 内容区），窄屏顶栏横向导航；主页内容区按视觉行带移动；高亮整块使用 accent 底。
- 大字：5x7 点阵，每两个纵向像素合并进一个终端单元；空间足够时 2 倍放大；遮罩用空心方框；非 ASCII 回退普通文本。
- 通用部件 widgets：display_width / fit_text / wrap_text / footer_hints / progress bar / centered_rect / modal_block；所有宽度计算必须走 unicode-width，不用 chars().count()。
- 新增页面检查单：Screen 变体、ui::render 与 ui/hit 分支、shell 的 Nav（若进侧栏）、layout + render + targets、ui_snapshots 快照、焦点支持（若套外壳）。

### 7.6 audio / backup / bootstrap / cli

- audio：键音与对错提示音走终端 BEL；单词 / 文章朗读走系统 TTS（Windows 常驻 PowerShell + System.Speech；macOS say；Linux espeak-ng / spd-say）；可选有道发音用系统 curl 下载 mp3 后交外部播放器。被安全软件拦截时本轮静默放弃，不重试。
- 有道发音的下载 / 播放脚本从环境变量取路径与 URL（`TYPEWORDS_AUDIO_PATH` / `TYPEWORDS_AUDIO_URL`），**不追加在 `-Command` 之后**：Windows PowerShell 会把那里的参数拼进命令文本再解析一次（`$args` 恒为空，URL 里的 `&` 直接 ParserError），`tts` 的音色参数踩过同一个坑。启动前先查解释器、curl 与播放器是否在 PATH 里，缺任何一个或进程起不来都降级回系统 TTS，并写一行运行日志说明缺的是什么。
- backup：import-web 解析 BackupData v5，按版本与结构逐项校验；覆盖前复制 .bak，多文件写入失败时回滚；export-web 生成网页版可导入的 JSON。文章断点按来源分流：TUI 导出的带 tuiPracticeData 可往返，网页版自己的三级游标因缺文章 id 而丢弃。
- bootstrap：FileStore + Catalog + ThemeRegistry 装配；readonly 标记；notices；ensure_study_dict；normalize_study_progress。
- cli：参数与 6 个子命令的入口；parse_mode 接受英文模式名与中文别名；doctor_to / export_dict_to 便于测试断言输出；export-dict 的导出目标解析（内置词典走存档、普通词库走词条文件）也在这里，因为 domain 不能依赖 store。

### 7.7 logging / crash

- logging 只依赖 std + chrono + anyhow：Level（Error < Warn < Info < Debug；parse 大小写不敏感、warning 等同 warn）、Options、resolve_location（显式 --log FILE 用该文件与它的父目录，相对路径视为当前目录；否则 <home>/logs/tw-<本地日期>.log）、init（建目录并试写，失败只警告一次并返回 None；随后按修改时间清理 14 天前的 tw-*.log 与 30 天前的 crash-*.txt）。
- 日志是进程级全局 `static SINK: Mutex<Option<Sink>>`，每行单独 open + append + close——崩溃现场最需要的最后几行不会留在用户态缓冲区；一行是「时间戳 级别 消息」，级别左对齐占 5 列。给崩溃报告附现场用 recent_lines，只回看文件末尾 256 KB，不整份读入内存。
- 保留策略是纯函数 prune_matching（前缀 + 后缀 + 时长；修改时间读不到或时间戳在未来时保留），日志与崩溃报告共用。
- crash：set_context 分两次写入路径（先数据目录，资源目录解析成功后再补资源目录 / 日志文件 / 报告目录）；install_hook 只在第一次调用时安装，保留原钩子并在写报告后转交；报告自身 panic 不会递归（REPORTING 只放行第一次）；报告正文由纯函数 build_report 生成，便于单测。
- panic 报告落在「日志所在目录」的 crash-<YYYYMMDD-HHMMSS>.txt（同一秒冲突加序号，绝不覆盖），含路径、panic 消息与位置、Backtrace::force_capture() 全量回溯（不需要 RUST_BACKTRACE）、最近 40 行日志；路径同时写进日志并打印到 stderr。--no-log 只关日志，报告仍写 <home>/logs/。写报告前会再清理一次 30 天前的报告。
- Windows 早退保护：should_pause 是纯函数（Windows + 无命令行参数 + stdin 与 stderr 都是终端），TW_NO_PAUSE 或 CI 环境变量跳过；set_enter_wait_enabled 供测试进程关掉等待。

## 8. 关键设计决策

| 决策 | 理由 | 代价 |
| --- | --- | --- |
| 独立仓库、独立进程，不共享上游代码 | 终端 UI 与 Vue/Web 差异过大；Rust 单二进制、无 Node/WebView | 功能对齐需人工维护 |
| domain 与终端解耦 | 可单测、可被 tw task 直接复用；与上游对照边界清晰 | 层间要传 TypingContext 等上下文 |
| Action 作为非练习输入的统一入口 | 键盘、鼠标、测试行为一致；渲染纯只读 | 枚举与分支样板代码 |
| 单线程 + 时间戳驱动 | 无并发 / 死锁；本地小数据足够快 | 长任务会阻塞界面 |
| 7 个内置流程 + 可编辑自定义流程与键位 | 行为可验证，同时给用户编排空间 | 需要额外校验与持久化 |
| 主题 token 化、不硬编码颜色 | 6 套内置 + 用户主题无需改渲染 | 多一层 token → Color 映射 |
| 点阵大字（5x7，可 2 倍） | 终端里有辨识度的大字体验 | 字形表与放大逻辑增加复杂度；非 ASCII 回退文本 |
| 不打包音频、默认离线 | 无音频资源与 HTTP 依赖，隐私与离线可用 | 键音退化为 BEL；发音依赖系统 TTS；有道发音需联网与外部播放器 |

## 9. 扩展指南

| 想做的事 | 改动位置 |
| --- | --- |
| 新增非练习按键 / Action | action.rs 加变体并在 apply_action 实现；keymap.rs 映射；需要点击就在对应 targets() 加 Target；需要焦点就在 ui/target.rs 加 ContentId 或 shell::nav_targets 注册；补 tests/app_drive.rs、focus_nav.rs、ui_snapshots.rs |
| 新增页面 | Screen 加变体；ui::render 与 ui/hit 分支；侧栏页面扩展 ui/shell.rs 的 Nav；页面实现 layout + render + targets；补渲染快照与焦点测试 |
| 新增设置项 | domain/types.rs 的 SettingState 字段与默认值 -> app/mod.rs 的 SETTING_ROWS、EditTarget、number/bool 读写 -> ui/settings.rs 分区与格式化 -> domain/i18n.rs 文案 -> docs/settings.md -> 测试。普通新增字段走默认值 / extra，无需升版本 |
| 新增内置流程 / 模式 | types.rs 的 WordPracticeMode（编号与上游一致）-> constants.rs 的 PRACTICE_MODE_NAMES -> flow/mod.rs 的 builtin_flow / get_flow_id_for_mode / BUILTIN_FLOW_MODES -> app/mod.rs 的 practice_modes -> cli.rs 的 parse_mode -> i18n -> tests/flow_parity.rs |
| 新增存档字段 | domain/types.rs（保持 serde 约定）-> store/mod.rs 版本处理 -> docs/persistence.md -> tests/store_roundtrip.rs。旧存档必须能靠 serde(default) 读取 |
| 新增统计维度 | types.rs 的 Statistics / PracticeState -> statistics.rs 的 flush / DayTotals -> app/mod.rs 的 refresh_stats 与 SummaryData -> ui/insights.rs / stats.rs -> end_to_end 或存储往返测试 |
| 新增主题 token | theme/mod.rs（模型与校验）-> theme/builtin.rs（6 套色值）-> ui/theme.rs（映射）-> docs/themes.md 的 token 表 -> ui_snapshots |
| 新增 CLI 子命令 | cli.rs 的 Command 与 lib::run 分发；写入口注意取 <home>/.tw.lock，只读入口登记进 lib::is_read_only（tests/data_dir_lock.rs 守着）；docs/usage.md |
| 新增词库资源 | 把 JSON 放进 assets/dicts/en/word/ 并登记 assets/list/word.json，或用 tw import-dict；无需改代码 |

## 10. 已知限制与技术债

| 项 | 说明 |
| --- | --- |
| app 与 ui 耦合 | App 直接调用 ui::render，页面直接读 App；未来拆视图层要先引入只读 ViewState |
| 文案硬编码 | i18n.rs 是一张简体中文 match 表，没有运行时语言切换 |
| 设置项只建模一部分 | SettingState 建模 43 个具名字段，未建模的上游字段经 extra 原样保存 |
| 练习缓存只支持 v2 | 旧版本缓存直接忽略（不移动文件），不提供迁移 |
| 无存档迁移框架 | 旧版本缺字段靠 serde(default)，版本号高于支持版本时只读 |
| 日志是纯文本 | 只有「时间戳 级别 消息」单行文本，没有结构化字段（JSON）与字段级检索；级别过滤在写入端，没有远程上报、轮转压缩或按大小切分 |
| 日志文件按启动日期命名 | 文件名在 init 时确定，跨零点的长会话继续写同一个文件；保留清理只在启动时（写崩溃报告时另清一次报告） |
| 回溯可能不完整 | release 构建带 strip，崩溃报告里的符号名可能只剩地址；要读带符号的回溯就用 debug 构建复现 |
| 崩溃报告只覆盖 panic | 启动期 anyhow 致命错误只进日志与 stderr（report_fatal_error），不生成 crash-*.txt；被强制结束等非 panic 退出没有任何报告 |

## 11. 文档地图

| 代码区域 / 主题 | 权威文档 |
| --- | --- |
| 用户操作、CLI、界面、练习流程、排障 | [usage.md](usage.md) |
| 设置页 33 行、高级 FSRS 页、FSRS 参数、透传字段 | [settings.md](settings.md) |
| 42 个快捷键动作、改绑、TUI 专有键 | [shortcuts.md](shortcuts.md) |
| 6 套主题、13 token、自定义主题格式 | [themes.md](themes.md) |
| 5 份存档、资源 JSON、版本策略、Web 互通 | [persistence.md](persistence.md) |
| 构建、测试、调试、截图、CI、发布 | [development.md](development.md) |
| 与上游功能矩阵、行为差异、同步方法 | [upstream-parity.md](upstream-parity.md) |
| 各版本新增 / 变更 / 移除 / 修复、发布日期 | [CHANGELOG.md](../CHANGELOG.md) |
| 分层、生命周期、状态与输入、子系统契约、扩展 | 本文 |
