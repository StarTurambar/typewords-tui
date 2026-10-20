# 开发指南

> 适用范围：构建、测试、调试、资源维护、CI 与发布流程；对应版本：typewords-tui 0.1.0。

本文面向要改代码或维护仓库的人。先读 [architecture.md](architecture.md) 了解分层与主循环，再按任务查阅 [domain.md](domain.md)、[app.md](app.md)、[ui.md](ui.md)、[persistence.md](persistence.md)。

## 1. 环境要求

| 项 | 要求 |
| --- | --- |
| Rust | **1.90+ stable**（`Cargo.toml` 的 `rust-version`）；edition 2024 |
| 平台 | Windows 为首要目标（默认 `x86_64-pc-windows-msvc`）；Linux/macOS 源码可用，CI 覆盖 Ubuntu 与 Windows |
| 终端（运行） | Windows Terminal 推荐；最小 60 × 20；真彩色终端体验更佳 |
| Node.js / Python | **仅维护者用**：`scripts/sync-assets.mjs` 需 Node；截图渲染需 Python + Pillow。构建、运行、测试都不需要 |
| 链接器 | Windows MSVC 工具链可直接 `cargo build`；仓库不含 C/C++ 依赖 |

## 2. 获取与常用命令

```bash
git clone https://github.com/StarTurambar/typewords-tui.git
cd typewords-tui

cargo run                         # 开发运行，自动使用仓库内 assets/
cargo run -- doctor               # 检查路径、词库、存档、主题
cargo run -- task                 # 只预览今日任务

cargo test --locked               # 全部测试（单元 + 集成）
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all --check

cargo build --release --locked    # target/release/tw(.exe)
```

说明：

- 仓库提交了 `Cargo.lock`，CI 与发布都使用 `--locked`；提交依赖变动时请一并更新锁文件。
- 开发运行不需要复制 `assets/`：`Paths::locate_assets` 会从当前目录向上查找 `assets/`（或上游风格的 `public/`）。
- 调试时想隔离数据，可用 `--home` 指向临时目录：`cargo run -- --home target/tmp/home`。

## 3. 目录与分层约定

目录地图与依赖规则见 [architecture.md · 代码地图](architecture.md#4-代码地图)；需要记住的硬约束：

- `domain/` 不依赖 `app/`、`ui/`、`store/`、`theme/`，也不要引入终端相关 crate。
- 状态只在 app 层修改：非练习键位/鼠标/测试经 `Action` → `App::apply_action`，练习页键盘由 `on_practice_key` 直接驱动，启动与 `on_tick` 也会更新状态；`ui/` 只读 `App`，不要在里面改状态。
- 每个页面的 `layout()` 同时服务 `render()` 与鼠标 `targets()`，不要另写一套坐标。
- 存档字段命名保持与上游一致（camelCase；`Card` 用 ts-fsrs 的 snake_case），新增字段要能被旧文件缺省填充。

## 4. 测试体系

当前共 **97 个测试**：40 个单元测试 + 57 个集成测试。全部通过是提交的最低要求。

### 4.1 单元测试（模块内 `#[cfg(test)]`）

| 文件 | 数量 | 覆盖点 |
| --- | --- | --- |
| `src/domain/typing.rs` | 10 | 五种练习类型的键入分支、大小写、全角标点、重复次数、自动下一词、错误计数 |
| `src/ui/bigtext.rs` | 5 | 字形行数、1×/2× 放大、遮罩空心占位、超长拆行、非 ASCII 回退 |
| `src/theme/mod.rs` | 3 | 颜色解析（`#rgb`/`#rrggbb`/ANSI）、主题回退 |
| `src/theme/load.rs` | 4 | base 继承、非法文件跳过、保留 id、未知 token 告警 |
| `src/paths.rs` | 3 | 资源目录识别与优先级 |
| `src/ui/insights.rs` | 3 | 累计总量与正确率、本周从周一开始、预计完成日期 |
| `src/ui/widgets.rs` | 3 | 按显示宽度截断/折行、底栏提示不越界 |
| `src/ui/overlays.rs` | 2 | 结算/确认弹层在最小终端内不越界 |
| `src/ui/theme.rs` | 2 | 主题 token → ratatui Color 映射 |
| `src/domain/scheduler.rs` | 2 | `enable_fuzz` 的开关、最小间隔与 ±5% 抖动范围 |
| `src/domain/statistics.rs` | 2 | 跨天会话计数去重、single/旧记录兼容 |
| `src/app/mod.rs` | 1 | 30 秒周期保存只在时长真正推进时触发 |

### 4.2 集成测试（`tests/`）

| 文件 | 数量 | 覆盖点 |
| --- | --- | --- |
| `tests/app_drive.rs` | 23 | 用合成按键/鼠标驱动 `App`：开始练习与键入推进、Tab 跳过、断点恢复、设置保存、大字开关、错词收录、退出确认、自由练习不改设置、每日词数持久化、数字键切页、鼠标（主页开始/侧栏导航/滚轮/弹层内外点击/结算按钮）、自定义主题循环与渲染、`--theme` 覆盖；新增 setting 版本过高只读、自定义重复次数、跨天统计去重用例 |
| `tests/end_to_end.rs` | 4 | 完整会话到结算并写入统计与 FSRS 卡片；已掌握词下一轮被排除；只剩 1 个错词、小组只有 1 个词时巩固阶段不会卡住 |
| `tests/flow_parity.rs` | 3 | 7 个内置流程均可推进到结算；错词清空；相位与练习类型一致 |
| `tests/store_roundtrip.rs` | 7 | 存档往返、未知字段保留、损坏隔离、版本过高不覆盖、缺省设置、练习缓存删除、练习缓存版本不支持（不移动文件） |
| `tests/import_dict.rs` | 2 | 词库导入登记、重复导入不追加目录项、非 `.json` 拒绝 |
| `tests/real_assets.rs` | 2 | 使用仓库内置 CET-4 真实词库（2607 词）装载、建任务、保存并重载 |
| `tests/ui_snapshots.rs` | 14 | TestBackend 渲染：主页/练习大字/默写占位/全部屏幕/终端过小/60×20 紧凑布局/CJK 宽度/正确输入着色/三套主题/弹层/鼠标命中区域与渲染文本一致/关闭使用提示后提示行消失 |
| `tests/docs.rs` | 2 | 文档一致性：README 与 `docs/` 的相对链接必须指向存在的文件；`docs/README.md` 必须登记 `docs/` 下全部 Markdown |

### 4.3 测试基础设施与写法

- `tests/common/mod.rs` 提供 `Fixture`：在临时目录生成 12 个测试词的最小词库与数据目录，`Fixture::cli()` 默认带 `--seed 42`，保证随机行为可复现。
- 测试直接构造 `app::build_for_test(paths, cli)` 得到 `App`，用 `KeyEvent` 驱动 `on_key`、用 `MouseEvent` 驱动 `on_mouse`、用显式 `on_tick(now)` 推进时间——不要 `sleep`。
- 需要断言落盘结果时，读取临时 home 下的 JSON 文件或再用 `FileStore` 装载；不要依赖真实用户目录。
- 新增 UI 时优先复用 TestBackend 渲染成文本断言子串；确需新增快照式断言时，同时检查终端尺寸边界（60×20、窄屏 90 列、宽屏 120 列）。
- 修 bug 时先写能复现的测试，再改代码；涉及随机洗牌的逻辑固定 seed。

## 5. 调试与可复现性

| 手段 | 用法 |
| --- | --- |
| 路径排查 | `tw doctor` 打印数据目录、资源目录、每本词库的文件与词数、存档文件大小、主题清单与告警 |
| 任务排查 | `tw task` 打印今日新词/到期复习/随机补足数量与前几个词，不进入界面 |
| 数据隔离 | `--home <临时目录>`（或 `TYPEWORDS_HOME`）避免污染真实存档 |
| 资源隔离 | `--assets <目录>`（或 `TYPEWORDS_ASSETS`）指定测试词库 |
| 复现随机问题 | `--seed <N>` 固定随机源；报告问题时附上 seed 与操作步骤 |
| 清空练习状态 | `--reset-practice`，或删除 `<home>/practice-word.json` |
| 练习页可视化 | `cargo run --example dump -- <mode> [typed] [width] [height] [big 或 plain]` 把练习页渲染成等宽文本 |
| 任意页面快照 | `cargo run --example shot -- <screen> [w] [h] [theme] [--plain]`，screen 支持 `home/practice/dict/stats/settings/help/summary/edit/confirm` |
| panic 定位 | `RUST_BACKTRACE=1`；程序安装了 panic hook，会先恢复终端与鼠标捕获再打印回溯 |
| 日志 | 程序当前**没有**日志设施（不写日志文件）；排障依赖 `tw doctor`、Toast、`--seed` 与复现步骤 |
| 手工检查存档 | 存档是可读 JSON：`dict.json`、`setting.json`、`practice-word.json`，字段说明见 [persistence.md](persistence.md) |

## 6. 截图与视觉检查

截图链路：`examples/shot.rs` 用 ratatui TestBackend 渲染任意页面，输出带样式的 JSON；`tools/render-snapshot.py`（Python + Pillow）把 JSON 画成 PNG。字体自动探测，也可用 `--font` / `--cjk-font` 指定；`--scale` 调整像素倍率。

```bash
# 单张：JSON → PNG（省略输出路径时写到 target/screenshots/<页面>-<宽>x<高>-<主题>.png）
cargo run --quiet --example shot -- home 120 40 dark > target/tmp/home.json
python tools/render-snapshot.py target/tmp/home.json docs/screenshots/home-120x40-dark.png

# 不生成图片、直接在终端对比渲染结果
cargo run --quiet --example shot -- practice 120 40 dark --plain
```

仓库 `docs/screenshots/` 的 13 张图由以下组合生成（Windows、Cascadia Mono + 微软雅黑，`--scale 2`）：

| 文件 | 命令参数（screen, w, h, theme） |
| --- | --- |
| `home-120x40-dark.png` | `home 120 40 dark` |
| `home-90x30-dark.png` | `home 90 30 dark`（窄屏外壳） |
| `home-120x40-light.png` | `home 120 40 light` |
| `home-120x40-high-contrast.png` | `home 120 40 high-contrast` |
| `practice-120x40-dark.png` | `practice 120 40 dark` |
| `dict-120x40-dark.png` | `dict 120 40 dark` |
| `stats-120x40-dark.png` | `stats 120 40 dark` |
| `settings-120x40-dark.png` | `settings 120 40 dark` |
| `shortcuts-120x40-dark.png` | `shortcuts 120 40 dark` |
| `wordtest-120x40-dark.png` | `wordtest 120 40 dark` |
| `words-120x40-dark.png` | `words 120 40 dark` |
| `help-120x40-dark.png` | `help 120 40 dark` |
| `summary-120x40-dark.png` | `summary 120 40 dark` |

注意事项：

- 截图依赖本机字体与 Pillow 版本，不同机器像素不会完全一致；同一台机器上重新生成应与仓库文件一致（可用 hash 对比）。
- 修改界面后同步更新受影响的截图，并在 PR 中说明是纯样式变化还是布局变化。
- Pillow 只是开发期依赖：运行 TUI 与 `cargo test` 都不需要 Python。

## 7. 词库资源维护

- 运行期只读资源目录：`assets/list/word.json`（词库目录）+ `assets/dicts/en/word/*.json`（词条）。仓库当前内置 CET-4（2607 词，约 4.5 MB）。
- 同步上游词库（需要 Node）：

  ```bash
  node scripts/sync-assets.mjs --source ../TypeWords
  ```

  脚本会整体替换 `assets/list/word.json` 与 `assets/dicts/en/word/`，来源为上游仓库的 `public/`。默认源路径为 `../TypeWords`，也可用 `--source` / `TYPEWORDS_SOURCE` 指定。
- 词库资源已提交进仓库，**运行与测试不需要 Node**；同步后检查词库仍能被 `tw doctor` 与 `tests/real_assets.rs` 正确加载，并在提交信息中说明上游来源与日期。
- 用户侧导入词库走 `tw import-dict`（写入用户数据目录），不要把用户数据提交进仓库。
- 许可证：词库数据来自上游 TypeWords，遵循 GPL-3.0，与代码一起分发。

### 7.1 重新生成 FSRS 黄金样本

`tests/fixtures/fsrs_golden.json` 是 `tests/fsrs_parity.rs` 的比对基准，由上游 ts-fsrs 生成。上游升级或改动 FSRS 行为后需要重新生成：

```bash
# 上游仓库先装好依赖（在 ../TypeWords 或 TYPEWORDS_UPSTREAM 指向的目录里）
pnpm install
# 回到本仓库
node tools/fsrs_golden.mjs > tests/fixtures/fsrs_golden.json
```

脚本只读取上游的 `node_modules/ts-fsrs`，不修改上游仓库；上游目录可用 `TYPEWORDS_UPSTREAM` 环境变量指定。生成后必须让 `cargo test --locked --test fsrs_parity` 通过，否则说明 FSRS 实现与上游产生了偏差。

## 8. CI 与发布

### 8.1 持续集成（`.github/workflows/ci.yml`）

- 触发：push 到 `main`、任意 pull request；同一分支的旧任务会被取消。
- 任务：`fmt`（`cargo fmt --all --check`）、`clippy`（`-D warnings`）、`test`（Ubuntu + Windows 矩阵，`cargo test --locked`），使用 `Swatinem/rust-cache` 缓存依赖。
- 本机提交前至少跑：

  ```bash
  cargo fmt --all --check
  cargo clippy --all-targets --locked -- -D warnings
  cargo test --locked
  ```

### 8.2 发布（`.github/workflows/release.yml` + `scripts/package.ps1`）

推送 `v*` tag 触发 Windows 发布任务：`cargo build --release --locked` → 组装目录 → `Compress-Archive` → 生成 `.sha256` → `gh release create` 上传。本地可用 `pwsh -File scripts/package.ps1` 复现。

发布包内容：

```text
typewords-tui-vX.Y.Z-x86_64-pc-windows-msvc/
  tw.exe
  assets/            # 词库资源（必须与 exe 同级或通过 --assets 指定）
  docs/              # 本套文档（含截图），保证发布包内链接可用
  README.md
  LICENSE
```

发布检查清单：

1. 更新 `Cargo.toml` 的 `version`（发布包名与 `tw --version` 都取这里）。
2. `cargo test --locked`、clippy、fmt 全绿；如改过文档，检查文档相对链接与截图。
3. 打 tag `vX.Y.Z` 并推送；等待 release 工作流完成。
4. 解压产物到空目录，运行 `tw.exe doctor` 与 `tw.exe` 做一次开始练习/退出冒烟；确认 `tw --version`、zip 名、sha256 一致。
5. 确认 `LICENSE` 与 `README.md` 在包内；文档链接指向包内 `docs/`（相对链接）而非外部路径。

## 9. 代码风格与约定

- **格式/静态检查**：`rustfmt` 默认配置；clippy 全目标 `-D warnings`，不要用 `#[allow(...)]` 掩盖可修的问题。
- **注释**：模块顶部用 `//!` 说明职责；公共类型/函数写中文文档注释；复杂算法（FSRS 映射、流程游标、计时拆分）写明不变量。
- **错误处理**：跨层边界用 `anyhow::Result`；启动期不可恢复错误在进入终端前 `bail!`，运行期可恢复错误转成 Toast；不要把 panic 当作控制流。
- **serde**：结构体普遍 `#[serde(default, rename_all = "camelCase")]`；`Card` 保持 ts-fsrs 的 snake_case；新增可选字段要有 `default`，保证旧存档可读。
- **状态变更**：只在 `app/action.rs` 的 `apply_action`（及其调用的 `App` 方法）里改状态；渲染层只读。
- **布局与命中**：同一页面的 `layout()` 结果同时给 `render()` 和 `targets()`；新增可点击区域必须同时在 `targets()` 注册并补测试。
- **测试命名**：用行为句子（如 `practice_progress_persists_and_resumes`），避免 `test1`。
- **提交信息**：仓库沿用 conventional 前缀（`feat(tui):`、`fix(paths):`、`refactor(...):`、`chore:`），一个提交聚焦一件事，说明动机与影响面。
- **依赖**：新增依赖前先确认是否必要（TUI 应保持轻量、无网络/无系统音频）；更新 `Cargo.lock` 并说明原因。

## 10. 文档维护

- 文档索引与写作约定见 [docs/README.md](README.md)；代码区域到权威文档的映射见 [architecture.md · 文档地图](architecture.md#15-文档地图)。
- 用户可见行为（按键、CLI、设置项、存档字段、主题格式）改动时，同步更新 [usage.md](usage.md)、[settings.md](settings.md)、[persistence.md](persistence.md)、[themes.md](themes.md) 中受影响者。
- 内部结构（分层、流程、调度、渲染）改动时，同步更新 [architecture.md](architecture.md) 与对应模块文档。
- 文档中的相对链接与截图由 `tests/docs.rs` 校验（README 与 `docs/` 的 Markdown 相对链接必须指向存在的文件；`docs/README.md` 必须登记 `docs/` 下全部 Markdown）。新增文档后记得在本索引与 docs/README.md 登记。
- 修改截图后按第 6 节命令重新生成，不要手工裁剪或压缩。

## 11. 常见开发问题

| 现象 | 处理 |
| --- | --- |
| `cargo test` 中 `real_assets` 失败 | 该测试依赖仓库内 `assets/`；确认在仓库根运行，且没有用 `--assets` 覆盖目录 |
| 测试修改了真实存档 | 测试应使用 `Fixture`/临时 `--home`；不要直接运行会写用户目录的示例 |
| 终端被打乱 | 程序有 panic hook 会恢复终端；若被强制 kill，可 `reset`（Unix）或重开终端 |
| 界面中文/边框错位 | 使用 Windows Terminal；确认字体等宽且支持 CJK 与制表符 |
| 鼠标无法选中文本 | 程序开启鼠标捕获，按住 Shift 拖拽选择（个别终端用 Alt 或关闭鼠标上报） |
| 颜色不对 | 需要 truecolor 终端；或 `--theme high-contrast` 验证 |
| clippy 报 `needless_...` | 按提示修，不要全局 allow；CI 用 `-D warnings` |
| 构建很慢 | 首次编译依赖较慢属于正常；`cargo build --release` 启用 thin LTO 会更慢；日常开发用 debug |

## 相关文档

- [文档索引](README.md)
- [总体架构](architecture.md)
- [领域逻辑](domain.md)
- [应用状态机](app.md)
- [渲染层](ui.md)
- [存档与数据格式](persistence.md)
