# 开发指南

本文面向要改代码、跑测试或发版的人：环境与命令、分层约束、测试体系、调试与截图、词库资源、CI 与发布、文档维护。开工前先读 [总体架构](architecture.md)，目录与依赖规则见 [architecture.md · 代码地图](architecture.md#4-代码地图)。

## 1. 环境要求

| 项 | 要求 |
| --- | --- |
| Rust | **1.90+ stable**（`Cargo.toml` 的 `rust-version`），edition 2024 |
| 平台 | 发布覆盖 Windows x86_64、Linux x86_64（gnu + musl）、macOS arm64 与 x86_64；CI 测试矩阵仍为 Ubuntu + Windows |
| 终端 | Windows Terminal 推荐；最小 60 × 20；真彩色终端体验更好 |
| Node.js / Python | 仅维护者用：`scripts/sync-assets.mjs` 与 `tools/fsrs_golden.mjs` 需 Node，截图渲染需 Python + Pillow。构建、运行、`cargo test` 都不需要 |

## 2. 获取与常用命令

```bash
git clone https://github.com/StarTurambar/typewords-tui.git && cd typewords-tui

cargo run --locked                 # 开发运行，自动使用仓库内 assets/
cargo run --locked -- doctor       # 检查路径、词库、存档、主题
cargo run --locked -- task         # 只预览今日任务，不进界面
cargo test --locked                # 全部测试（单元 + 集成）
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo build --release --locked     # target/release/tw（Windows 为 tw.exe）
```

- 仓库提交了 `Cargo.lock`，CI 与发布都用 `--locked`；改依赖时一并更新锁文件。
- 开发运行不用复制 `assets/`：`paths::locate_assets` 依次尝试 `--assets`、`TYPEWORDS_ASSETS`、exe 同级 `assets/`、当前目录及最多 3 级祖先的 `assets/` 或 `public/`。隔离数据用 `--home target/tmp/home`（或 `TYPEWORDS_HOME`），复现随机问题用 `--seed`。

## 3. 目录与分层约定

硬约束（完整目录地图见 [architecture.md · 代码地图](architecture.md#4-代码地图)）：

- `src/domain/` 不依赖 `app` / `ui` / `store` / `theme`，也不引入终端相关 crate；`src/theme/` 不依赖 `app` / `ui` 与终端 crate；`tests/layering.rs` 守着这两条。
- 状态只在 app 层改：非练习键位与鼠标经 `Action` → `App::apply_action`，练习页键盘走 `App::on_practice_key`，启动与 `App::on_tick` 也直接改状态；`ui/` 只读 `App`。
- 每个页面的 `layout()` 同时服务 `render()` 与命中检测 `targets()`；新增可点击区域必须注册进 `targets()`，`tests/ui_matrix.rs` 的表驱动矩阵校验两者不漂移。
- 颜色只来自主题的 13 个 token；存档结构体普遍 `#[serde(default, rename_all = "camelCase")]`（`Card` 保持 ts-fsrs 的 snake_case），新增字段必须带 `default`。

## 4. 测试体系

全部测试通过是提交的最低要求。测试数量随代码演进，文档不写死——需要具体数字时跑 `cargo test --locked -- --list`。

### 4.1 单元测试（模块内 `#[cfg(test)]`）

单元测试写在被测模块的 `#[cfg(test)]` 块里，跟着代码走；完整清单以 `cargo test --locked -- --list` 为准，这里只记几处不显然的设计：

- `src/domain/fsrs.rs` 的参数夹取与高级 FSRS 页的权重区间同源，改一侧要连带改另一侧。
- `src/domain/shortcut.rs` 校验默认表覆盖全部动作、且与上游默认表逐条一致；加动作时这条会失败，属预期。
- `src/fs.rs` 的原子写有 Windows 专属的只读属性与占用重试用例，跨平台改动别只看 Unix 分支。
- `src/logging.rs` 的过期清理只删匹配 `tw-*.log` / `crash-*.txt` 的文件，未来时间戳不删。

### 4.2 集成测试（`tests/`）

`tests/` 下每个文件按用途命名，文件名即覆盖范围；完整清单以 `cargo test --locked -- --list` 为准。不显然的几处：

- `tests/common/mod.rs` 的 `Fixture` 生成临时 home 与最小词库，`cli_for` 给出与 fixture 同源的缺省 `Cli`（详见 4.3）。
- `tests/ui_matrix.rs` 是绘制 / 命中覆盖矩阵：每屏 × 多种终端尺寸（60×20 到 160×50）断言正文与命中矩形，各弹层同样按尺寸断言。
- `tests/layering.rs` 用 `syn` 解析源码守分层，不靠运行期约定。
- `tests/real_assets.rs` 依赖仓库内 `assets/`，必须从仓库根运行；`tests/fsrs_parity.rs` 对着 `tests/fixtures/fsrs_golden.json` 逐条比对。
- `tests/crash_report.rs` 与 `tests/diagnostics.rs` 涉及进程级全局状态（panic 钩子、日志），各自独立成二进制或串行执行。
- `tests/docs.rs` 守文档的形式一致性（相对链接、链接里的 `#锚点`、索引、版本、CHANGELOG 结构），不守语义——文档里的说法对不对仍要人看。

### 4.3 测试基础设施与写法

- `Fixture` 在临时目录里生成 12 词最小词库（另含 1 本书、2 篇文章）与数据目录，`Fixture::cli()` 默认 `--seed 42`。
- 用 `app::build_for_test(paths, cli)` 拿 `App`，`KeyEvent` 驱动 `on_key`、`MouseEvent` 驱动 `on_mouse`、显式 `on_tick(now)` 推进时间——不要 `sleep`；`build_for_test` 会静音 TTS，断言朗读内容时用 `audio::record_speech`。
- 断言落盘结果时读临时 home 下的 JSON 或用 `FileStore` 重新装载；新增 UI 优先用 `TestBackend` 渲染成文本断言子串，确需快照式断言时覆盖 60×20、窄屏 90 列、宽屏 120 列。
- 修 bug 先写能复现的测试再改代码；涉及随机洗牌固定 seed；测试名用行为句子（如 `practice_progress_persists_and_resumes`）。

## 5. 调试与可复现性

| 手段 | 用法 |
| --- | --- |
| 路径排查 | `tw doctor` 打印数据目录、资源目录、每本词库的文件与实际词数（空词条不计）、5 份存档大小、主题清单与告警；只读、不取锁 |
| 任务排查 | `tw task` 打印今日新词 / 到期复习 / 随机补足数量与前几个词，不进界面 |
| 清空断点 | `--reset-practice` 同时清 `practice-word.json` 与 `practice-article.json`；只探测可读性、不读内容，损坏的直接删（不隔离），读不出来的保留并提示「未清空」 |
| 练习页可视化 | `cargo run --locked --example dump -- <mode> [typed] [宽] [高] [big 或 plain]`，`mode` 取 `system` / `free` / `review` / `shuffle` / `identify` / `dictation` / `listen` |
| 任意页面快照 | `cargo run --locked --example shot -- <screen> [宽] [高] [theme] [--plain]`；screen 取第 6 节表内名称，另有 `edit` / `confirm` 两个弹层快照 |
| panic 定位 | panic 会自动写 `<日志目录>/crash-<时间戳>.txt`：panic 消息与位置、完整回溯、最近 40 行日志，**不需要** `RUST_BACKTRACE`；TUI 里的 panic 由 `src/app/mod.rs` 的 `install_panic_hook` 先恢复终端与鼠标捕获再转交 `crash` 钩子 |
| 日志 | 默认开启，`<home>/logs/tw-<日期>.log`；`--log FILE` 换路径、`--no-log` 关闭、`--log-level debug` 连 Toast 的成功 / 提示类与清理记录一起记；本次实际使用的随机种子也在里面（TUI 随机、`tw task` 固定 42），复现随机行为时从这里取 `--seed`；日志与崩溃报告同目录（用过 `--log` 就取它所在的目录），保留 14 / 30 天。写失败只警告一次，不阻断启动 |
| 手工检查存档 | 5 份存档都是可读 JSON，字段含义与版本策略见 [persistence.md](persistence.md) |

## 6. 截图与视觉检查

`examples/shot.rs` 用 ratatui `TestBackend` 渲染任意页面，输出带样式的 JSON；`tools/render-snapshot.py`（Python + Pillow）把 JSON 画成 PNG。字体自动探测，可用 `--font` / `--cjk-font` 指定，`--scale` 调像素倍率（默认 2）：

```bash
cargo run --quiet --example shot -- home 120 40 dark > target/tmp/home.json
python tools/render-snapshot.py target/tmp/home.json docs/screenshots/home-120x40-dark.png  # 省略输出路径时写到 target/screenshots/
cargo run --quiet --example shot -- practice 120 40 dark --plain                          # 只输出文本，直接在终端对比
```

`docs/screenshots/` 的 28 张图由下表组合生成（Windows、Cascadia Mono + 微软雅黑、`--scale 2`）。`shot` 对 `dict` / `dict-builtin` / `stats` / `settings` / `shortcuts` / `adv-fsrs` 走真实入口（数字键、`s`、设置行 `Enter`），底栏因此带「选中 X」焦点提示；`help` / `help-faq` / `help-about` 同样走真实入口（`?` + `→`），但帮助页底栏显示的是分页提示（`←→` 分页）。直接改 `app.screen` 会留下上一页的焦点身份，截图不要那样写。`help-about` 会把数据目录换成占位路径，避免截图带出本机路径。

| 文件 | 命令参数（screen, 宽, 高, theme） |
| --- | --- |
| `home-120x40-dark.png` | `home 120 40 dark`（启动首帧，默认选中「开始 / 继续学习」） |
| `home-focus-120x40-dark.png` | `home-focus 120 40 dark`（内容区选中「f 自由练习」：按一次 `→`） |
| `home-nav-focus-120x40-dark.png` | `home-nav-focus 120 40 dark`（侧栏选中「文章」：`←` 进侧栏，`↓` 到「文章」） |
| `home-90x30-dark.png` | `home 90 30 dark`（窄屏外壳） |
| `home-120x40-light.png` | `home 120 40 light` |
| `home-120x40-high-contrast.png` | `home 120 40 high-contrast` |
| `home-120x40-midnight.png` | `home 120 40 midnight` |
| `home-120x40-paper.png` | `home 120 40 paper` |
| `home-120x40-ikb.png` | `home 120 40 ikb` |
| `practice-120x40-dark.png` | `practice 120 40 dark` |
| `practice-buttons-120x40-dark.png` | `practice-buttons 120 40 dark`（按钮条按两次 `→` 选中「跳过」） |
| `practice-collect-120x40-dark.png` | `practice-collect 120 40 dark`（`Enter` 收藏当前词，保留 Toast） |
| `dict-120x40-dark.png` | `dict 120 40 dark`（三本内置词典在前，光标停在当前词库） |
| `dict-builtin-120x40-dark.png` | `dict-builtin 120 40 dark`（选中内置词典「收藏」，右侧详情与「w 查看词条」） |
| `stats-120x40-dark.png` | `stats 120 40 dark` |
| `settings-120x40-dark.png` | `settings 120 40 dark` |
| `adv-fsrs-120x40-dark.png` | `adv-fsrs 120 40 dark`（设置页 FSRS 分区的「高级 FSRS」行按 `Enter` 进入） |
| `shortcuts-120x40-dark.png` | `shortcuts 120 40 dark` |
| `wordtest-120x40-dark.png` | `wordtest 120 40 dark` |
| `words-120x40-dark.png` | `words 120 40 dark` |
| `articles-120x40-dark.png` | `articles 120 40 dark` |
| `articlepractice-120x40-dark.png` | `articlepractice 120 40 dark` |
| `flows-120x40-dark.png` | `flows 120 40 dark` |
| `help-120x40-dark.png` | `help 120 40 dark`（默认「快捷键」分页） |
| `help-faq-120x40-dark.png` | `help-faq 120 40 dark`（帮助页按一次 `→` 到「常见问题」） |
| `help-about-120x40-dark.png` | `help-about 120 40 dark`（按两次 `→` 到「关于与反馈」；数据目录为占位路径） |
| `summary-120x40-dark.png` | `summary 120 40 dark` |
| `summary-article-120x40-dark.png` | `summary-article 120 40 dark` |

- 截图依赖本机字体与 Pillow 版本，跨机器像素不会完全一致；同一台机器重跑时，除含日期的区域（主页「本周学习记录」、统计月历、结算「本周记录」）外应与仓库文件一致，可用 hash 对比。侧栏与底栏渲染 `v<CARGO_PKG_VERSION>`，改版本或改界面后要重新生成受影响截图，不要手工裁剪或缩放（Pillow 只是开发期依赖，运行 TUI 与 `cargo test` 都不需要 Python）。

## 7. 词库资源维护

- 运行期只读资源：`assets/list/word.json`（词库目录）、`assets/dicts/en/word/*.json`（词条）、`assets/list/article.json`（文章书目）、`assets/dicts/en/article/*.json`（文章）。当前内置 CET-4（2607 词，4,487,640 字节）与《新概念英语1》（5 篇）。
- 同步上游（需 Node）：`node scripts/sync-assets.mjs --source ../TypeWords`。脚本先删除整个 `assets/`，再从上游 `public/` 复制 `list/word.json`、`list/article.json`、`dicts/en/word/`、`dicts/en/article/`；源路径可用 `--source` / `TYPEWORDS_SOURCE`，输出目录可用 `--out`。
- 文章书目 `assets/list/article.json` 的 `length` 要与对应文章文件的篇数一致：运行时 `domain::article` 会用文件实际篇数校准，但数据本身仍应写对，`tests/real_assets.rs` 会校验。同步后跑 `cargo test`（尤其该测试），并在提交信息里写明上游来源与日期。
- 词库资源已提交进仓库，运行与测试不需要 Node；用户侧导入走 `tw import-dict`（写入用户数据目录），不要把用户数据提交进仓库。词库数据遵循上游的 GPL-3.0。
- FSRS 黄金样本：`tests/fixtures/fsrs_golden.json` 由上游 ts-fsrs 生成。上游升级或改动 FSRS 行为后，在上游仓库 `pnpm install`，再执行 `TYPEWORDS_UPSTREAM=<上游目录> node tools/fsrs_golden.mjs > tests/fixtures/fsrs_golden.json`；脚本只读上游 `node_modules/ts-fsrs`，生成后必须让 `cargo test --locked --test fsrs_parity` 通过。

## 8. CI 与发布

- **持续集成**（`.github/workflows/ci.yml`）：push 到 `main` 与任意 pull request 触发，旧任务自动取消；`fmt`（`cargo fmt --all --check`）、`clippy`（`--all-targets --locked -- -D warnings`，Ubuntu + Windows 矩阵——`#[cfg(windows)]` 代码在 Linux 上不参与编译，只在 Linux 跑 clippy 等于让它无门禁）、`test`（Ubuntu + Windows 矩阵，`cargo test --locked`）三个任务，依赖用 `Swatinem/rust-cache` 缓存。提交前本机至少跑这三条。
- **发布**（`.github/workflows/release.yml`）：推送 `v*` tag 触发，但**推 tag 不等于直接发版**：先 `guard`（tag 必须等于 `Cargo.toml` 的 `version`，且 `CHANGELOG.md` 里要有该版本的条目、`[Unreleased]` 已清空，不一致立刻失败，不浪费后面的测试与构建），再 `verify`（`cargo fmt --all --check`、`cargo clippy --all-targets --locked -- -D warnings`、`cargo test --locked`，矩阵 Ubuntu + Windows，命令与 `ci.yml` 逐条相同），两关全绿才进入 `package`；`package` 用 `needs: [verify-lint, verify-test]` 显式依赖测试作业，`release` 依赖 `package`，任何一项红灯都不会产出归档、更不会发布（`tests/docs.rs` 的 `release_runs_the_ci_suite_before_packaging` 守着这条依赖）。之所以在发布 workflow 里对 tag 提交重跑一遍、而不是等 `main` 上的 CI：tag 可以指向任意提交，只有对 tag 提交本身跑过测试才作数，也避免跨 workflow 的竞态。`package` 任务用 matrix 覆盖 5 个目标——`x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`、`x86_64-unknown-linux-musl`、`aarch64-apple-darwin`、`x86_64-apple-darwin`，每个目标在自己的 `runs-on` 上构建（Windows、Ubuntu、macOS；gnu 目标在 Ubuntu 22.04 上构建，构建基线即 glibc 2.35，更老的发行版建议改用 musl 静态包）。musl 目标直接用 rustc 自带的 self-contained musl 工具链（不要覆盖 `CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER`，否则会用系统 `musl-gcc` 混链，产出运行即崩的二进制），产物因此是静态链接、不依赖系统 glibc；构建后统一用 `tar -czf` 打成 `typewords-tui-vX.Y.Z-<target>.tar.gz`（内含可执行文件 + `assets/` + `docs/` + `README.md` + `LICENSE`），并为每个归档生成 `<归档名>.sha256`；随后把归档解压出来跑一次 `tw --version` 与 `tw doctor` 冒烟（顺带验证 `assets/` 与可执行文件同级；`x86_64-apple-darwin` 在 arm64 runner 上运行需要 Rosetta，跳过）。各个构建任务只 `upload-artifact`；独立的 `release` 任务下载全部 artifact 后断言「归档数 = matrix 条目数」并逐个核对 `.sha256`，再生成聚合 `SHA256SUMS`（`sha256sum -c` 自检），最后用 `gh release create` 一次发布。**不能每个构建任务各自 create release**：多个任务会并发竞争同一个 release（重复创建或互相覆盖上传），而聚合 `SHA256SUMS` 只有在全部归档到齐后才能生成，只能由汇总任务统一产出。发布说明只有一个来源：[CHANGELOG.md](../CHANGELOG.md)——`guard` 先用 `bash scripts/changelog.sh <version>` 确认该版本小节存在且有条目，并确认 `[Unreleased]` 已清空（否则说明这些改动没被写进任何版本），`release` 作业再用同一个脚本把正文交给 `gh release create --notes-file`，并把 `CHANGELOG.md` 底部该版本链接定义里的比对地址附成「完整变更」。`.github/release-notes/` 里 0.2.0–0.3.3 的原始公告已冻结为历史归档，新说明只写 `CHANGELOG.md`；脚本本地也能用（`bash scripts/changelog.sh 0.3.3`、`bash scripts/changelog.sh unreleased`）。workflow 顶层只给 `permis... (line truncated to 2000 chars)
- **发版清单**：① 改 `Cargo.toml` 的 `version`（包名与 `tw --version` 都取这里）、同步 [docs/README.md](README.md) 唯一的那行「对应版本」，并把 [CHANGELOG.md](../CHANGELOG.md) 的 `[Unreleased]` 整段移到新的 `## [x.y.z] - 日期` 小节（日期用该版本 tag 的提交日期，底部补一行 `compare` 链接定义、最新版本的比对终点是 `vX.Y.Z`；`[Unreleased]` 必须清空，否则 `guard` 拒绝发版）；② 重新生成 `docs/screenshots/`（侧栏与底栏显示版本号，见第 6 节）；③ fmt / clippy / test 全绿、`tests/docs.rs` 通过（发布 workflow 会在 tag 提交上重跑这三条，红灯直接拦在打包前）；④ 解压产物到空目录（Windows 为 `tw.exe`，Linux / macOS 为 `tw`），跑 `doctor` 与一次「开始练习 → 退出」冒烟，确认 `tw --version`、tar.gz 名与 sha256、`SHA256SUMS` 一致；本机可用 `pwsh -File scripts/package.ps1` 或 `bash scripts/package.sh` 打同名同结构的包核对命名与布局（`dist/` 下 `<归档名>.tar.gz` 与同名 `.sha256`；tar 会记录 mtime，本地 sha256 与 CI 不同）。

## 9. 代码风格与约定

- **格式与静态检查**：`rustfmt` 默认配置；clippy 全目标 `-D warnings`，不要用 `#[allow(...)]` 掩盖可修问题。
- **注释**：模块顶部 `//!` 说明职责；公共类型 / 函数写中文文档注释；复杂算法（FSRS 评级映射、流程游标、计时跨天拆分）写明不变量。
- **错误处理**：跨层边界用 `anyhow::Result`；启动期不可恢复错误在进入终端前 `bail!`，运行期可恢复错误转 Toast；不用 panic 作控制流。
- **状态与布局**：只在 app 层改状态；`layout()` 结果同时给 `render()` 与 `targets()`，新增可点击区域必须注册并在测试里校验。
- **提交信息**：conventional 前缀（`feat(tui):`、`fix(paths):`、`refactor(...):`、`docs:`、`chore:`），一个提交聚焦一件事，说明动机与影响面。
- **依赖**：新增前先确认必要性（保持轻量：无 HTTP 客户端、无音频 crate，发音走系统 TTS 与外部播放器），并一并更新 `Cargo.lock`。

## 10. 文档维护

文档以源码为唯一事实来源，同一事实只在一个文件展开：用户行为（按键、CLI、界面、排障）写 [usage.md](usage.md)，设置项写 [settings.md](settings.md)，键位写 [shortcuts.md](shortcuts.md)，主题写 [themes.md](themes.md)，存档与数据字段写 [persistence.md](persistence.md)，分层与扩展写 [architecture.md](architecture.md)（映射表见 [architecture.md · 文档地图](architecture.md#11-文档地图)），上游差异写 [upstream-parity.md](upstream-parity.md)，版本变更写 [CHANGELOG.md](../CHANGELOG.md)。

- 版本号只在 [docs/README.md](README.md) 写一次，其它文档不写「对应版本」，发版只改那一处（`CHANGELOG.md` 的版本标题是历史记录，不属于「对应版本」）。
- 用户可见改动（按键、CLI、设置项、存档字段、主题格式、行为变化）除了改对应文档，还要在根目录 [CHANGELOG.md](../CHANGELOG.md) 的 `## [Unreleased]` 下补一条；发版时整段移到版本标题下，GitHub Release 正文由它生成。
- `tests/docs.rs` 校验仓库根与 `docs/` 下所有 Markdown 的相对链接指向真实文件、链接里的 `#锚点` 命中目标文档的标题，要求 `docs/README.md` 登记每个文档，并校验 `CHANGELOG.md` 的格式与发布说明单源；新增、重命名文档或改标题后先改索引与链接。
- 引用源码只写路径与符号名，不写行号；改界面后按第 6 节重新生成截图，不要手工截图或缩放。

## 11. 常见开发问题

| 现象 | 处理 |
| --- | --- |
| `real_assets` 失败 | 该测试依赖仓库内 `assets/`：确认在仓库根运行，且没有用 `--assets` 覆盖资源目录 |
| 测试影响了真实存档 | 测试应使用 `Fixture` / 临时 `--home` 隔离；不要运行会写用户目录的示例 |
| 终端被打乱 | panic hook 会恢复终端；若被强制结束进程，`reset`（Unix）或重开终端 |
| 中文 / 边框错位或颜色不对 | 换 Windows Terminal 并确认真彩色；字体要等宽且覆盖 CJK 与制表符，或临时 `--theme high-contrast` |
| 鼠标无法选中文本 | 程序开启了鼠标捕获，按住 `Shift` 拖拽选择 |
| clippy 报 `needless_...` | 按提示修，不要全局 allow；CI 用 `-D warnings` |
| 首次构建很慢 | 依赖编译较慢属正常；`cargo build --release` 带 thin LTO 更慢，日常用 debug |

## 相关文档

[文档索引](README.md) · [总体架构](architecture.md) · [存档与数据格式](persistence.md) · [与上游对照](upstream-parity.md)
