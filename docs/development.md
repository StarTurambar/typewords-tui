# 开发指南

本文面向要改代码、跑测试或发版的人：环境与命令、分层约束、测试体系、调试与截图、词库资源、CI 与发布、文档维护。开工前先读 [总体架构](architecture.md)，目录与依赖规则见 [architecture.md · 代码地图](architecture.md#4-代码地图)。

## 1. 环境要求

| 项 | 要求 |
| --- | --- |
| Rust | **1.90+ stable**（`Cargo.toml` 的 `rust-version`），edition 2024 |
| 平台 | Windows 为首要目标（`x86_64-pc-windows-msvc`），MSVC 工具链可直接 `cargo build`；Linux / macOS 源码可用，CI 覆盖 Ubuntu 与 Windows |
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

- `src/domain/` 不依赖 `app` / `ui` / `store` / `theme`，也不引入终端相关 crate；`tests/layering.rs` 守着这条。
- 状态只在 app 层改：非练习键位与鼠标经 `Action` → `App::apply_action`，练习页键盘走 `App::on_practice_key`，启动与 `App::on_tick` 也直接改状态；`ui/` 只读 `App`。
- 每个页面的 `layout()` 同时服务 `render()` 与命中检测 `targets()`；新增可点击区域必须注册进 `targets()`，`tests/ui_snapshots.rs` 校验两者不漂移。
- 颜色只来自主题的 13 个 token；存档结构体普遍 `#[serde(default, rename_all = "camelCase")]`（`Card` 保持 ts-fsrs 的 snake_case），新增字段必须带 `default`。

## 4. 测试体系

全部测试通过是提交的最低要求。测试数量随代码演进，文档不写死——需要具体数字时跑 `cargo test --locked -- --list`。

### 4.1 单元测试（模块内 `#[cfg(test)]`）

25 个源文件带单元测试：

| 文件 | 覆盖点 |
| --- | --- |
| `src/domain/article.rs` | 断句（数字 / 货币 / 缩写 / 智能引号）、句子与译文对齐、跟打目标与可省略的词间空格、错字落位与退格、`load_books` 以文章文件实际篇数为准 |
| `src/domain/fsrs.rs` | FSRS-6：新卡与学习步进、毕业、重学、Hard/Good/Easy 间隔排序、fuzz 确定性与默认关闭、参数夹取 |
| `src/domain/typing.rs` | 五种练习类型的键入分支、大小写、全角标点、重复次数、自动下一词、错误计数 |
| `src/domain/session.rs` | 除默写外每种模式换词自动朗读、结算按词表实际词数判完成、推进到最后一词时把词典置为完成 |
| `src/domain/study_task.rs` | 取新词与学完判定按词表实际词数、最后一个词仍作为新词取、剩余词全被忽略时转向复习 |
| `src/domain/shortcut.rs` | 键位往返、默认表覆盖全部动作、与上游默认表一致、冲突拒绝、一键反查多动作、方向键往返 |
| `src/domain/scheduler.rs` | `enable_fuzz` 开关、最小间隔与 ±5% 抖动范围 |
| `src/domain/statistics.rs` | 跨天会话计数去重、single / 旧记录兼容 |
| `src/domain/types.rs` | 词典进度口径：有效词数是唯一分母、`last_learn_index` 夹取、`percent_of` 四舍五入与 0 分母、`normalize_progress` 装载校正 |
| `src/domain/flow/mod.rs` | 7 个内置流程模式（`BUILTIN_FLOW_MODES`）与 `builtin_flow` 一一对应、两种单词测试与自定义流程不在内置列表且兜底 `system` |
| `src/app/mod.rs` | 30 秒周期保存只在时长真正推进时触发 |
| `src/cli.rs` | `tw task` 的「N 词」与进度行都用有效词数（`dict_progress_lines`） |
| `src/fs.rs` | 原子写与目录锁：临时名唯一且带 PID、不复用残留的旧固定名文件、rename 失败清理、并发写只留完整内容；`DirLock` 互斥到守卫释放并记录 PID |
| `src/paths.rs` | 资源目录识别与优先级、`lock_home` 先创建数据目录再取锁 |
| `src/audio/tts.rs` | 系统 TTS：语速映射、逐行写 stdin、各平台命令构造、常驻进程复用与音色切换 |
| `src/audio/player.rs` | 仅整词可发音、缺少播放程序时报错、脚本参数拼接 |
| `src/theme/mod.rs` | 颜色解析（`#rgb` / `#rrggbb` / ANSI）、主题回退 |
| `src/theme/load.rs` | base 继承、非法文件跳过、保留 id、未知 token 告警 |
| `src/ui/bigtext.rs` | 字形行数、1×/2× 放大、遮罩空心占位、超长拆行、非 ASCII 回退 |
| `src/ui/focus.rs` | 视觉分带（同行合并、整卡自成一带）、上下换带取水平中心最近者 |
| `src/ui/insights.rs` | 累计总量与正确率、本周从周一开始、预计完成日期 |
| `src/ui/overlays.rs` | 结算 / 确认弹层在最小终端内不越界、按钮行与方向键提示在 60×20 放得下 |
| `src/ui/theme.rs` | 主题 token → ratatui `Color` 映射 |
| `src/ui/typing_buttons.rs` | 按钮条不越界不重叠、60 列下提示行不被裁掉 |
| `src/ui/widgets.rs` | 按显示宽度截断 / 折行、底栏提示不越界 |

### 4.2 集成测试（`tests/`）

27 个测试文件，另有 `tests/common/mod.rs` 的 `Fixture`。

| 文件 | 覆盖点 |
| --- | --- |
| `tests/app_drive.rs` | 合成按键 / 鼠标驱动 `App`：开始练习与键入推进、`Tab` 跳过、断点恢复、设置保存、大字开关、错词收录、退出确认、自由练习不改设置、每日词数持久化、数字键切页、鼠标（主页开始 / 侧栏 / 滚轮 / 弹层 / 结算按钮）、自定义主题与 `--theme`、setting 版本过高只读、跨天统计去重 |
| `tests/focus_nav.rs` | 所有标签页键盘焦点：启动默认选中主按钮、内容区换带与带内左右、每页落地内容区、`Tab` / 方向键换栏、各页光标移动、弹层与搜索框让位、resize 后焦点失效不触发动作、底栏文案与高亮矩形、最小终端忽略按键、鼠标点击同步焦点 |
| `tests/interaction_fixes.rs` | 已修交互缺陷的回归：三个弹层的鼠标命中、主页 `+` / `-` 的像素对齐、滚动指示器不被行 target 抢走、命中矩形与渲染一致 |
| `tests/overlay_keys.rs` | 弹层按钮焦点：方向键 / `Tab` / `j` / `k` 移动选中、两端停住、`Enter` 执行选中按钮（焦点在「取消」时只关闭）、`y` / `n` / `Esc` / `r` / `q` 旧快捷键保留、选中项高亮 |
| `tests/ui_snapshots.rs` | `TestBackend` 渲染：全部屏幕、终端过小、60×20 紧凑布局、CJK 宽度、正确输入着色、6 套内置主题、弹层、鼠标命中区域与渲染文本一致、设置页动作项文案与配色 |
| `tests/articles.rs` | 书本 / 文章列表、断句与整篇跟打（空格可省略、错字落位、结算口径）、「N 篇」等于实际篇数、滚轮按指针所在栏分流、底栏提示与方向键语义一致 |
| `tests/resume_cache.rs` | 启动固定停在主页、单词与文章断点续练、两份缓存不叠加、结算只清本轮缓存、资源更新后旧断点按当前句数 clamp、0 句文章不显示断点 |
| `tests/words_manage.rs` | 词条管理页：搜索、编辑、批量标记与自定义词典 |
| `tests/dict_word_counts.rs` | 词数只有一个来源：删除 / 编辑词条后词库页、主页磁贴与重启后同数；未打开过的词库用登记值；自建词库同步用户目录 `length`，内置资源目录不被改写 |
| `tests/system_dicts.rs` | 内置词典（收藏 / 错词 / 已掌握）：词库页排最前、词数取实际词条、`w` 打开词条页、删词只改 `dict.json`、`Enter` 设为当前并落盘 `studyIndex`、重启保持、`--dict` 接受内置 id、已掌握不建会话 |
| `tests/shell_nav.rs` | 宽屏侧栏与窄屏顶栏都列出文章、编号连续、当前页高亮、「+ / d 管理」卡与模式标签可点、帮助页列出文章与流程键 |
| `tests/flows.rs` | 自定义流程：新建副本、编辑步骤（含步骤光标 `h` / `l`）、启用内置流程并持久化、底栏提示、`●` 只标「下一轮实际生效」的流程、两种单词测试模式没有任何 `●` |
| `tests/store_roundtrip.rs` | 存档往返、未知字段保留、损坏隔离、版本过高不覆盖、缺省设置、练习缓存删除与版本不支持、`tw doctor` 列出全部存档 |
| `tests/end_to_end.rs` | 完整会话到结算并写入统计与 FSRS 卡片；已掌握词下一轮被排除；错词巩固不会卡住；目录登记词数偏大时学完仍置 `complete`；旧存档进度越界装载校正；只剩最后一个词时仍学完 |
| `tests/audio.rs` | 朗读触发：换词自动朗读、单词测试切题朗读与 `Ctrl+P` 重播（用 `audio::record_speech` 断言） |
| `tests/backup_roundtrip.rs` | 网页版备份 `BackupData` v5 导入导出：版本与结构校验、拒绝损坏载荷、覆盖前 `.bak` 备份与失败回滚、文章断点分流 |
| `tests/flow_parity.rs` | 7 个内置流程均可推进到结算、错词清空、相位与练习类型一致 |
| `tests/home_resume.rs` | 主页断点呈现：「上次任务」标题与「继续学习」主按钮 |
| `tests/import_dict.rs` | 词库导入登记、重复导入不追加目录项、非 `.json` 拒绝 |
| `tests/cli_doctor.rs` | `tw doctor` 的词数等于 `Catalog::load_words` 实际加载词数（空词条不计）；词库缺失或解析失败时逐本标注原因且不中断 |
| `tests/layering.rs` | `src/domain/` 不依赖 `app` / `ui` / `store` / `theme` |
| `tests/real_assets.rs` | 用仓库内置 CET-4（2607 词）装载、建任务、保存重载；内置文章书目 `length` 与文章文件篇数一致 |
| `tests/typing_keys.rs` | 两个键入练习页的键位统一：朗读 / 翻译 / 主题共用键位表，暂停、保存离开与帮助是两页共用 TUI 键位，键位表不抢跟打输入 |
| `tests/fsrs_parity.rs` | 与上游 ts-fsrs 黄金样本（`tests/fixtures/fsrs_golden.json`）逐条比对 |
| `tests/data_dir_lock.rs` | 数据目录互斥：同进程与跨进程（`CARGO_BIN_EXE_tw` 真实子进程）第二个写入口被拒且 stderr 给出提示、只读的 `tw doctor` / `tw export-web` 不受影响、启动创建 `<home>/.tw.lock` 退出释放 |
| `tests/progress_consistency.rs` | 进度口径：百分比与「N / M 词」同源（主页 / 词库页 / 结算 / 单词测试结算 / 学完判定），元数据 `length` 偏大时不漂移；「继续」不二次推进；每日 1 词也能学完 |
| `tests/docs.rs` | 文档一致性：README 与 `docs/` 的相对链接必须指向存在的文件；`docs/README.md` 必须登记 `docs/` 下全部 Markdown |

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
| 清空断点 | `--reset-practice` 同时清 `practice-word.json` 与 `practice-article.json` |
| 练习页可视化 | `cargo run --locked --example dump -- <mode> [typed] [宽] [高] [big 或 plain]`，`mode` 取 `system` / `free` / `review` / `shuffle` / `identify` / `dictation` / `listen` |
| 任意页面快照 | `cargo run --locked --example shot -- <screen> [宽] [高] [theme] [--plain]`；screen 取第 6 节表内名称，另有 `edit` / `confirm` 两个弹层快照 |
| panic 定位 | `src/app/mod.rs` 的 `install_panic_hook` 先恢复终端与鼠标捕获再打印；配合 `RUST_BACKTRACE=1` 看回溯 |
| 日志 | 没有日志设施（不写日志文件）；排障依赖 `tw doctor`、Toast、`--seed` 与复现步骤 |
| 手工检查存档 | 5 份存档都是可读 JSON，字段含义与版本策略见 [persistence.md](persistence.md) |

## 6. 截图与视觉检查

`examples/shot.rs` 用 ratatui `TestBackend` 渲染任意页面，输出带样式的 JSON；`tools/render-snapshot.py`（Python + Pillow）把 JSON 画成 PNG。字体自动探测，可用 `--font` / `--cjk-font` 指定，`--scale` 调像素倍率（默认 2）：

```bash
cargo run --quiet --example shot -- home 120 40 dark > target/tmp/home.json
python tools/render-snapshot.py target/tmp/home.json docs/screenshots/home-120x40-dark.png  # 省略输出路径时写到 target/screenshots/
cargo run --quiet --example shot -- practice 120 40 dark --plain                          # 只输出文本，直接在终端对比
```

`docs/screenshots/` 的 25 张图由下表组合生成（Windows、Cascadia Mono + 微软雅黑、`--scale 2`）。`shot` 对 `dict` / `dict-builtin` / `stats` / `settings` / `shortcuts` / `help` 走真实入口（数字键、`s`、设置行 `Enter`、`?`），底栏因此带「选中 X」焦点提示；直接改 `app.screen` 会留下上一页的焦点身份，截图不要那样写。

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
| `shortcuts-120x40-dark.png` | `shortcuts 120 40 dark` |
| `wordtest-120x40-dark.png` | `wordtest 120 40 dark` |
| `words-120x40-dark.png` | `words 120 40 dark` |
| `articles-120x40-dark.png` | `articles 120 40 dark` |
| `articlepractice-120x40-dark.png` | `articlepractice 120 40 dark` |
| `flows-120x40-dark.png` | `flows 120 40 dark` |
| `help-120x40-dark.png` | `help 120 40 dark` |
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

- **持续集成**（`.github/workflows/ci.yml`）：push 到 `main` 与任意 pull request 触发，旧任务自动取消；`fmt`（`cargo fmt --all --check`）、`clippy`（`--all-targets --locked -- -D warnings`）、`test`（Ubuntu + Windows 矩阵，`cargo test --locked`）三个任务，依赖用 `Swatinem/rust-cache` 缓存。提交前本机至少跑这三条。
- **发布**（`.github/workflows/release.yml`）：推送 `v*` tag 触发 Windows 任务——`cargo build --release --locked` → 组装 `tw.exe` + `assets/` + `docs/` + `README.md` + `LICENSE` → `Compress-Archive` → 生成 `.sha256` → `gh release create`。发布说明优先读 `.github/release-notes/<tag>.md`，没有则退回 `--generate-notes`。本地复现同一产物用 `pwsh -File scripts/package.ps1`。
- **发版清单**：① 改 `Cargo.toml` 的 `version`（包名与 `tw --version` 都取这里）并同步 [docs/README.md](README.md) 唯一的那行「对应版本」；② 重新生成 `docs/screenshots/`（侧栏与底栏显示版本号，见第 6 节）；③ fmt / clippy / test 全绿、`tests/docs.rs` 通过；④ 解压产物到空目录，跑 `tw.exe doctor` 与一次「开始练习 → 退出」冒烟，确认 `tw --version`、zip 名、sha256 一致。

## 9. 代码风格与约定

- **格式与静态检查**：`rustfmt` 默认配置；clippy 全目标 `-D warnings`，不要用 `#[allow(...)]` 掩盖可修问题。
- **注释**：模块顶部 `//!` 说明职责；公共类型 / 函数写中文文档注释；复杂算法（FSRS 评级映射、流程游标、计时跨天拆分）写明不变量。
- **错误处理**：跨层边界用 `anyhow::Result`；启动期不可恢复错误在进入终端前 `bail!`，运行期可恢复错误转 Toast；不用 panic 作控制流。
- **状态与布局**：只在 app 层改状态；`layout()` 结果同时给 `render()` 与 `targets()`，新增可点击区域必须注册并在测试里校验。
- **提交信息**：conventional 前缀（`feat(tui):`、`fix(paths):`、`refactor(...):`、`docs:`、`chore:`），一个提交聚焦一件事，说明动机与影响面。
- **依赖**：新增前先确认必要性（保持轻量：无 HTTP 客户端、无音频 crate，发音走系统 TTS 与外部播放器），并一并更新 `Cargo.lock`。

## 10. 文档维护

文档以源码为唯一事实来源，同一事实只在一个文件展开：用户行为（按键、CLI、界面、排障）写 [usage.md](usage.md)，设置项写 [settings.md](settings.md)，键位写 [shortcuts.md](shortcuts.md)，主题写 [themes.md](themes.md)，存档与数据字段写 [persistence.md](persistence.md)，分层与扩展写 [architecture.md](architecture.md)（映射表见 [architecture.md · 文档地图](architecture.md#11-文档地图)），上游差异写 [upstream-parity.md](upstream-parity.md)。

- 版本号只在 [docs/README.md](README.md) 写一次，其它文档不写「对应版本」，发版只改那一处。
- `tests/docs.rs` 校验 README 与 `docs/` 下所有 Markdown 的相对链接指向真实文件，并要求 `docs/README.md` 登记每个文档；新增或重命名文档后先改索引。
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
