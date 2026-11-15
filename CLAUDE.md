# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概览

typewords-tui 是从上游 [TypeWords](https://github.com/zyronon/TypeWords)（Nuxt/Vue Web + Tauri 桌面端）拆出的**独立 Rust 终端客户端**：单进程、默认离线、无 Node/WebView，数据全是本地 JSON。与上游共享词库资源格式与存档字段命名，**不共享代码**，也不读写浏览器 IndexedDB。唯一的联网能力是用户主动开启「有道发音」后下载 mp3；子进程只用于音频（系统 TTS / 外部播放器）。

`docs/` 是本仓库的权威文档（架构、使用、设置、快捷键、主题、存档、开发指南、上游对照）。动手前先读对应文档；文档与代码不一致时以代码为准，但要同步修文档。

## 常用命令

    cargo run --locked                    # 开发运行（自动从当前目录向上查找仓库内 assets/）
    cargo run --locked -- doctor          # 打印数据目录/资源目录/词库/存档/主题状态
    cargo run --locked -- task            # 只预览今日任务，不进 TUI
    cargo run --locked -- --home target/tmp/home --seed 42   # 隔离数据目录 + 固定随机种子

    cargo test --locked                   # 全部测试（数量以 -- --list 为准）
    cargo test --locked --test app_drive practice_progress_persists_and_resumes  # 单个集成测试
    cargo test --locked --lib domain::typing                                     # 单个模块的单测

    cargo clippy --all-targets --locked -- -D warnings
    cargo fmt --all --check
    cargo build --release --locked        # 产物：target/release/tw（Windows 为 tw.exe）

提交前这三条必须全绿（CI 跑 fmt / clippy / Ubuntu+Windows 测试）。

## 架构

    main.rs（先装 crash 的 panic 钩子）→ lib.rs::run(cli)
      ├─ Paths::resolve_home()       数据目录解析（先于资源目录：日志要尽早可用）
      ├─ logging::init() / crash::set_context()   运行日志 + 崩溃报告上下文
      ├─ Paths::resolve_in()         资源目录解析
      ├─ cli.rs                      doctor / task / import-dict / import-web / export-web（不进 TUI）
      └─ app::run(paths, cli)
           ├─ App::bootstrap()       store + domain + theme 装载，失败则在进入终端前 bail!
           └─ 事件循环：ui::render(&App)（只读）+ keymap/mouse/buttons → Action → App::apply_action
                                        └─ domain → store::FileStore → JSON 存档

| 层 | 目录 | 硬约束 |
| --- | --- | --- |
| domain | `src/domain/` | **不能**依赖 `app`/`ui`/`store`/`theme`，也不引入终端相关 crate（因此可被单测与 `tw task` 直接复用）；`tests/layering.rs` 守着这条 |
| store | `src/store/` | 依赖 domain 与 fs；5 份 JSON 存档的原子写 + 版本校验 + 损坏隔离 |
| theme | `src/theme/` | 不依赖 ratatui，也不依赖 ui |
| fs | `src/fs.rs` | 只依赖 std 的原子写原语（唯一临时名）与数据目录锁（`<home>/.tw.lock`）；domain 与 store 都用它，避免两层成环 |
| app | `src/app/` | 唯一的状态修改者，渲染入口调用 ui |
| ui | `src/ui/` | 只读 `App`，**不得**修改状态 |
| audio | `src/audio/` | 终端响铃、系统 TTS、有道下载 + 外部播放器；子进程不阻塞事件循环 |
| logging / crash | `src/logging.rs`、`src/crash.rs` | 运行日志（`<home>/logs/tw-<日期>.log`，追加写、保留 14 天）与 panic 崩溃报告（`crash-*.txt`，保留 30 天、无需 `RUST_BACKTRACE`）；只依赖 std + chrono + anyhow，日志写失败不影响启动 |
| 入口/装配 | `main.rs`/`lib.rs`/`cli.rs`/`paths.rs`/`bootstrap.rs`/`backup.rs` | 参数、路径、装载、子命令、网页版备份适配 |

必须保持的不变量：

- **状态只在 app 层改**。非练习键位 / 鼠标 / 集成测试经 `Action` → `App::apply_action`；练习页键盘走 `App::on_practice_key` → `TypingEngine`/`PracticeSession`（最高频路径，不经过 Action）；启动与 `App::on_tick` 也直接改状态。新增非练习交互 = `action.rs` 加动作 + `keymap.rs`/`hit.rs` 映射 + `apply_action` 实现。
- **同一份布局同时服务绘制与命中**。每个页面的 `layout()` 结果同时给 `render()` 和 `targets()`；新增可点击区域必须在 `targets()` 注册，`tests/ui_snapshots.rs` 的 `mouse_target_rects_match_rendered_text` 会校验二者不漂移。
- **颜色只来自主题的 13 个 token**（`bg/panel/element/border/fg/muted/dim/accent/accent_alt/warn/good/bad/selection`），`ui/` 中不出现硬编码颜色。
- **单线程 + 时间戳驱动**：无 async 运行时、无线程、无 `unsafe`；除音频子进程外不启动子进程，默认不联网（可选有道发音除外）。计时、Toast 过期、定时保存都在 `on_tick(now)` 中按时间戳推进——测试直接注入 `now`，**不要 sleep**。
- 终端下限 60 × 20；`< 100` 列切窄屏顶栏，`>= 120` 列侧栏 26 列。CJK 宽度一律用 `unicode-width` 计算，折行 / 截断 / 命中区域共用同一套宽度。

## 数据与持久化

数据目录：Windows 为 `%LOCALAPPDATA%\typewords-tui`，可被 `--home` / `TYPEWORDS_HOME` 覆盖。四份带版本的存档外层是 `SaveEnvelope { val, version, updated_at }`；`flows.json` 是裸 `FlowStore`，无信封、无版本号：

| 文件 | 版本 | 内容 |
| --- | --- | --- |
| `dict.json` | 4 | 词典 + FSRS 卡片 + 收藏/错词/已掌握 + `statistics` |
| `setting.json` | 23 | 设置（`SettingState` 建模 43 个具名字段，其余上游字段原样透传保存） |
| `practice-word.json` | 2 | 断点单词练习缓存（游标 + 任务词表 + 统计快照），结算后删除 |
| `practice-article.json` | 1 | 文章跟打断点缓存（文章 id + 句子进度 + 用时），结算后删除 |
| `flows.json` | 无 | 自建练习流程（`{ activeId, flows }`） |

另有 `<home>/logs/`：运行日志 `tw-<日期>.log` 与崩溃报告 `crash-<时间戳>.txt`，程序自动创建，**不属于存档**（无信封 / 版本号，删了不影响进度，备份时可留可删）。

写入是「同目录唯一临时文件（`.<文件名>.tw-<pid>-<n>.tmp`）+ fsync + rename」；写入口启动时对 `<home>/.tw.lock` 取排他锁（`Paths::lock_home`，只读的 `tw doctor` / `tw export-web` 不取锁）；解析失败隔离为 `*.corrupt-<时间戳>.json`；`dict.json`/`setting.json` 版本高于程序支持版本时**只读不覆盖**，练习缓存版本不受支持时忽略（不移动文件）。完整字段与版本策略见 `docs/persistence.md`。

serde 约定：结构体普遍 `#[serde(default, rename_all = "camelCase")]`；`Card` 保持 ts-fsrs 的 snake_case；新增可选字段必须带 `default`，保证旧存档可读。字段命名沿用上游，便于手工迁移。

## 测试

- 集成测试用 `tests/common/mod.rs` 的 `Fixture`（临时目录 + 12 词最小词库），`Fixture::cli()` 默认 `--seed 42`。
- 驱动方式：`app::build_for_test(paths, cli)` 拿到 `App`，用 `KeyEvent`/`MouseEvent` 走 `on_key`/`on_mouse`，用显式 `on_tick(now)` 推进时间。
- 断言落盘结果时读临时 home 下的 JSON，或再用 `FileStore` 装载；不要依赖真实用户目录。
- `tests/real_assets.rs` 依赖仓库内 `assets/`，必须从仓库根运行，且不要用 `--assets` 覆盖资源目录。
- 修 bug 先写能复现的测试；涉及随机洗牌的逻辑固定 seed。测试名用行为句子（如 `practice_progress_persists_and_resumes`），不用 `test1`。

## 文档维护

- 代码区域 → 权威文档：整体分层/生命周期/状态与输入/子系统契约/扩展 → `docs/architecture.md`；用户操作与 CLI → `docs/usage.md`；设置 → `docs/settings.md`；键位 → `docs/shortcuts.md`；主题 → `docs/themes.md`；存档与资源 → `docs/persistence.md`；构建/测试/CI/发布 → `docs/development.md`；上游差异 → `docs/upstream-parity.md`。
- `cargo test` 会跑 `tests/docs.rs`：README、`CHANGELOG.md` 与 `docs/` 下所有 Markdown 的相对链接必须指向真实文件，`docs/README.md` 必须登记 `docs/` 下每一个 Markdown，文档版本必须与 `Cargo.toml` 一致，文档里引用的 `src/...rs` 路径必须存在；`CHANGELOG.md` 必须遵循 Keep a Changelog（六分类、日期、版本倒序、链接定义、最新版本与 `Cargo.toml` 一致、最早的 0.1.0 不缺失），且发布说明只从它提取。新增文档后忘记登记索引会直接测试失败。
- 用户可见行为（按键、CLI、设置项、存档字段、主题格式）改动时，同步更新 `docs/usage.md`、`docs/settings.md`、`docs/shortcuts.md`、`docs/persistence.md`、`docs/themes.md` 中受影响者；改界面后按 `docs/development.md` 重新生成 `docs/screenshots/`，不要手工截图或缩放。
- 用户可见改动同时记进根目录 `CHANGELOG.md` 的 `## [Unreleased]`（Keep a Changelog 六分类：新增 / 变更 / 弃用 / 移除 / 修复 / 安全）；发版时移到版本标题下，发布说明由它生成。
- 文档引用源码只写路径与符号名，不写行号（避免重构后失效）。

## 上游与资源

- 上游 `zyronon/TypeWords` 的本地解包快照可能位于 `target/upstream/`（被 `.gitignore` 忽略，全新克隆没有），做功能对齐时可参考其 `app/**`；`docs/upstream-parity.md` 记录了对照基线与「已对齐 / 部分 / 行为不同 / 缺失 / TUI 独有」的判定结论，结论只以源码为准。
- `assets/` 是运行期只读资源：`list/word.json`（词库目录）+ `dicts/en/word/*.json`（内置 CET-4，2607 词，约 4.5 MB）+ `list/article.json` + `dicts/en/article/*.json`（《新概念英语1》，5 篇）。发布包中 `assets/` 必须与可执行文件同级（Windows 为 `tw.exe`，Linux / macOS 为 `tw`），否则用 `--assets` 指定。
- 新增词库无需改代码：把 JSON 放进 `assets/dicts/en/word/` 并登记 `assets/list/word.json`，或走 `tw import-dict`（写入用户数据目录）。

## 约定

- 提交信息用 conventional 前缀（`feat(tui):`、`fix(paths):`、`refactor(...):`、`docs:`、`chore:`），一个提交聚焦一件事，说明动机与影响面。
- 错误处理：跨层边界用 `anyhow::Result`；启动期不可恢复错误在进入终端前 `bail!`，运行期可恢复错误转 Toast；不用 panic 作控制流。
- clippy 全目标 `-D warnings`，不要用 `#[allow(...)]` 掩盖可修问题。
- 保持 TUI 轻量：新增依赖前先确认必要性（不引入 HTTP 客户端或音频库；TTS / 播放器走系统命令），并一并更新 `Cargo.lock`。
- 发布：改 `Cargo.toml` 的 `version`，把 `CHANGELOG.md` 的 `[Unreleased]` 条目整段移到新的 `## [X.Y.Z] - 日期` 小节（`[Unreleased]` 必须清空）→ 推送 `vX.Y.Z` tag 触发发布。推 tag 不会直接发版：`release.yml` 先 `guard` 校验 tag、版本与 `CHANGELOG.md` 里该版本的条目、再 `verify` 跑与 CI 同一套 fmt / clippy / Ubuntu+Windows 测试，全绿后才进五平台矩阵打包（Windows x86_64、Linux x86_64 gnu / musl、macOS Apple Silicon / Intel）；每个目标产出 `typewords-tui-vX.Y.Z-<target>.tar.gz`（可执行文件 + `assets/` + `docs/` + README + LICENSE）与同名 `.sha256`，Release 另附聚合 `SHA256SUMS`，旧的 `.zip` 不再产出。GitHub Release 正文由 `scripts/changelog.sh` 从 `CHANGELOG.md` 提取——发布说明只有这一来源，`.github/release-notes/` 已冻结为历史归档。
