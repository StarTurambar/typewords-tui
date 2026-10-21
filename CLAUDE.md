# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概览

typewords-tui 是从上游 [TypeWords](https://github.com/zyronon/TypeWords)（Nuxt/Vue Web + Tauri 桌面端）拆出的**独立 Rust 终端客户端**：单进程、无网络、无 Node/WebView，数据全是本地 JSON。与上游共享词库资源格式与存档字段命名，**不共享代码**，也不读写浏览器 IndexedDB。

`docs/` 是本仓库的权威文档（架构、领域、应用状态机、渲染、存档、主题、开发指南、上游对照）。动手前先读对应模块文档；文档与代码不一致时以代码为准，但要同步修文档。

## 常用命令

```bash
cargo run --locked                    # 开发运行（自动从当前目录向上查找仓库内 assets/）
cargo run --locked -- doctor          # 打印数据目录/资源目录/词库/存档/主题状态
cargo run --locked -- task            # 只预览今日任务，不进 TUI
cargo run --locked -- --home target/tmp/home --seed 42   # 隔离数据目录 + 固定随机种子

cargo test --locked                   # 全部测试（40 单测 + 55 集成）
cargo test --locked --test app_drive practice_progress_persists_and_resumes  # 单个集成测试
cargo test --locked --lib domain::typing                                     # 单个模块的单测

cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo build --release --locked        # 产物：target/release/tw（Windows 为 tw.exe）
```

提交前这三条必须全绿（CI 跑 fmt / clippy / Ubuntu+Windows 测试）。

调试与可视化工具：

```bash
cargo run --example dump -- <mode> [typed] [width] [height] [big|plain]  # 练习页 → 等宽文本
cargo run --example shot -- <screen> [w] [h] [theme] [--plain]           # 任意页面 → JSON/文本快照
# screen: home/practice/dict/stats/settings/help/summary/edit/confirm
python tools/render-snapshot.py <snapshot.json> docs/screenshots/<name>.png  # JSON → PNG（需 Pillow）
node scripts/sync-assets.mjs --source ../TypeWords   # 同步上游词库（需 Node）
pwsh -File scripts/package.ps1                       # 本地复现发布包
```

后三条只在维护者本机需要；构建、运行、测试都不依赖 Node 或 Python。

## 架构

```text
main.rs → lib.rs::run(cli)
  ├─ Paths::resolve()            资源目录 / 数据目录解析
  ├─ cli.rs                      doctor / task / import-dict（不进 TUI，直接结束）
  └─ app::run(paths, cli)
       ├─ App::bootstrap()       store + domain + theme 装载，失败则在进入终端前 bail!
       └─ 事件循环：ui::render(&App)（只读）+ keymap/mouse → Action → App::apply_action
                                    └─ domain → store::FileStore → JSON 存档
```

| 层 | 目录 | 硬约束 |
| --- | --- | --- |
| domain | `src/domain/` | **不能**依赖 `app`/`ui`/`store`/`theme`，也不引入终端相关 crate（因此可被单测与 `tw task` 直接复用）；`tests/layering.rs` 守着这条 |
| store | `src/store/` | 依赖 domain 与 `fs`；原子写 + 版本校验 + 损坏隔离 |
| theme | `src/theme/` | 不依赖 ratatui，也不依赖 `ui` |
| fs | `src/fs.rs` | 只依赖 `std` 的原子写原语；`domain` 与 `store` 都用它，避免两层成环 |
| app | `src/app/` | 唯一的状态修改者，渲染入口调用 `ui` |
| ui | `src/ui/` | 只读 `App`，**不得**修改状态 |

必须保持的不变量：

- **状态只在 app 层改**。非练习键位 / 鼠标 / 集成测试经 `Action` → `App::apply_action`；练习页键盘走 `App::on_practice_key` → `TypingEngine`/`PracticeSession`（最高频路径，不经过 Action）；启动与 `App::on_tick` 也直接改状态。新增非练习交互 = `action.rs` 加动作 + `keymap.rs`/`hit.rs` 映射 + `apply_action` 实现。
- **同一份布局同时服务绘制与命中**。每个页面的 `layout()` 结果同时给 `render()` 和 `targets()`；新增可点击区域必须在 `targets()` 注册，`tests/ui_snapshots.rs` 的 `mouse_target_rects_match_rendered_text` 会校验二者不漂移。
- **颜色只来自主题的 13 个 token**（`bg/panel/element/border/fg/muted/dim/accent/accent_alt/warn/good/bad/selection`），`ui/` 中不出现硬编码颜色。
- **单线程 + 时间戳驱动**：无 async 运行时、无线程、无 `unsafe`、无子进程、无网络。计时、Toast 过期、定时保存都在 `on_tick(now)` 中按时间戳推进——测试直接注入 `now`，**不要 sleep**。
- 终端下限 60 × 20；`< 100` 列切窄屏顶栏，`>= 120` 列侧栏 26 列。CJK 宽度一律用 `unicode-width` 计算，折行/截断/命中区域共用同一套宽度。

## 数据与持久化

数据目录：Windows 为 `%LOCALAPPDATA%\typewords-tui`，可被 `--home` / `TYPEWORDS_HOME` 覆盖。三份存档对应上游 IndexedDB 的 key，外层都是 `SaveEnvelope { val, version, updated_at }`：

| 文件 | 版本 | 内容 |
| --- | --- | --- |
| `dict.json` | 4 | 词典 + FSRS 卡片 + 收藏/错词/已掌握 + `statistics` |
| `setting.json` | 23 | 设置（`SettingState` 建模 28 个具名字段，其余上游字段原样透传保存） |
| `practice-word.json` | 2 | 断点练习缓存（游标 + 任务词表 + 统计快照），结算后删除 |

写入是「同目录临时文件 + fsync + rename」；解析失败隔离为 `*.corrupt-<时间戳>.json`；`dict.json`/`setting.json` 版本高于程序支持版本时**只读不覆盖**，练习缓存版本不受支持时忽略（不移动文件）。完整字段与版本策略见 `docs/persistence.md`。

serde 约定：结构体普遍 `#[serde(default, rename_all = "camelCase")]`；`Card` 保持 ts-fsrs 的 snake_case；新增可选字段必须带 `default`，保证旧存档可读。字段命名沿用上游，便于手工迁移。

## 测试

- 集成测试用 `tests/common/mod.rs` 的 `Fixture`（临时目录 + 12 词最小词库），`Fixture::cli()` 默认 `--seed 42`。
- 驱动方式：`app::build_for_test(paths, cli)` 拿到 `App`，用 `KeyEvent`/`MouseEvent` 走 `on_key`/`on_mouse`，用显式 `on_tick(now)` 推进时间。
- 断言落盘结果时读临时 home 下的 JSON，或再用 `FileStore` 装载；不要依赖真实用户目录。
- `tests/real_assets.rs` 依赖仓库内 `assets/`，必须从仓库根运行，且不要用 `--assets` 覆盖资源目录。
- 修 bug 先写能复现的测试；涉及随机洗牌的逻辑固定 seed。测试名用行为句子（如 `practice_progress_persists_and_resumes`），不用 `test1`。

## 文档维护

- 代码区域 → 权威文档：`src/domain/` → `docs/domain.md`；`src/app/` → `docs/app.md`；`src/ui/` → `docs/ui.md`；`src/store/` 与三份存档 → `docs/persistence.md`；`src/theme/` → `docs/themes.md`；构建/测试/CI/发布 → `docs/development.md`；上游差异 → `docs/upstream-parity.md`。
- `cargo test` 会跑 `tests/docs.rs`：README 与 `docs/` 下所有 Markdown 的相对链接必须指向真实文件，且 **`docs/README.md` 必须登记 `docs/` 下每一个 Markdown**。新增文档后忘记登记索引会直接测试失败。
- 用户可见行为（按键、CLI、设置项、存档字段、主题格式）改动时，同步更新 `docs/usage.md`、`docs/settings.md`、`docs/persistence.md`、`docs/themes.md` 中受影响者；改界面后按 `docs/development.md` 重新生成 `docs/screenshots/`，不要手工截图或缩放。
- 文档引用源码只写路径与符号名，不写行号（避免重构后失效）。

## 上游与资源

- 上游 `zyronon/TypeWords` 的本地解包快照可能位于 `target/upstream/`（被 `.gitignore` 忽略，全新克隆没有），做功能对齐时可参考其 `app/**`；`docs/upstream-parity.md` 记录了对照基线与「已对齐 / 部分 / 行为不同 / 缺失 / TUI 独有」的判定结论，结论只以源码为准。
- `assets/` 是运行期只读资源：`list/word.json`（词库目录）+ `dicts/en/word/*.json`（内置 CET-4，2607 词，约 4.5 MB）。发布包中 `assets/` 必须与 `tw.exe` 同级，否则用 `--assets` 指定。
- 新增词库无需改代码：把 JSON 放进 `assets/dicts/en/word/` 并登记 `assets/list/word.json`，或走 `tw import-dict`（写入用户数据目录）。

## 约定

- 提交信息用 conventional 前缀（`feat(tui):`、`fix(paths):`、`refactor(...):`、`docs:`、`chore:`），一个提交聚焦一件事，说明动机与影响面。
- 模块顶部用 `//!` 说明职责；公共类型/函数写中文文档注释；复杂算法（FSRS 评级映射、流程游标、计时跨天拆分）写明不变量。
- 错误处理：跨层边界用 `anyhow::Result`；启动期不可恢复错误在进入终端前 `bail!`，运行期可恢复错误转 Toast；不用 panic 作控制流。
- clippy 全目标 `-D warnings`，不要用 `#[allow(...)]` 掩盖可修问题。
- 保持 TUI 轻量：新增依赖前先确认必要性（无网络、无系统音频、无 async 运行时），并一并更新 `Cargo.lock`。
- 发布：改 `Cargo.toml` 的 `version` → 推 `vX.Y.Z` tag 触发 Windows 发布包（`tw.exe` + `assets/` + `docs/` + README + LICENSE + sha256）。
