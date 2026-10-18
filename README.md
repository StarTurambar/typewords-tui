# TypeWords TUI —— 终端单词练习客户端

[![CI](https://github.com/StarTurambar/typewords-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/StarTurambar/typewords-tui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/StarTurambar/typewords-tui)](https://github.com/StarTurambar/typewords-tui/releases)

用 Rust + [ratatui](https://ratatui.rs/) 实现的 TypeWords 终端客户端：在终端里完成「选词库 → 智能学习 → 结算 → 统计」闭环，进度保存在本地 JSON，重启进程可继续上次练习。

这是从 [TypeWords](https://github.com/zyronon/TypeWords) 拆分出来的独立项目，只包含终端客户端（Rust），不包含 Web / Tauri 桌面端。它与上游共享词库资源格式和存档字段命名，但**不共享代码**，也**不读写浏览器的 IndexedDB**；运行时不需要 Node.js、WebView 或网络。

![主页](docs/screenshots/home-120x40-dark.png)

![练习](docs/screenshots/practice-120x40-dark.png)

## 功能概览

- **单词练习主线**：跟写（7 词一组，组内拼写巩固）、听写、默写、自测、复习、随机复习、自由练习共 7 个内置流程；阶段间自动处理错词巩固。
- **FSRS 调度**：评级阈值与上游 Web 端一致（默认按错误次数分档），到期复习按本地自然日挑选；间隔由 `fsrs` crate（FSRS-6）计算。
- **三本系统词典**：收藏、错词（自动收录）、已掌握；已掌握的单词在后续练习中自动排除。
- **词库与进度管理**：内置 CET-4（2607 词），支持切换词库、每日词数调整、学习进度展示、跳过阶段；跟写阶段按 7 词一组推进。
- **统计**：结算弹层、主页今日/累计概览与本周记录、统计页累计概览与最近 14 天逐日记录。
- **设置与主题**：设置页 23 行（22 个配置项 + 1 个清空进度动作，覆盖练习模式、复习比例、重复次数与自定义次数、FSRS 阈值等）；内置深色/浅色/高对比三套主题，支持 `<home>/themes/*.json` 自定义主题。
- **终端交互**：响应式外壳（宽屏侧栏 / 窄屏顶栏）、可点击的导航/按钮/列表、滚轮滚动；60 × 20 起可用。
- **断点续练**：练习游标与统计快照写入本地 JSON，重启恢复到同一词、同一阶段。
- **工具命令**：`tw doctor` / `tw task` / `tw import-dict`，便于排障、脚本化预览与导入词库。

v1 明确**不包含**：发音与键盘音效、文章背诵、云同步（Supabase）、备份导入导出、在线查词/翻译、自定义练习流程、可配置快捷键、多语言界面。完整清单、行为差异与补齐顺序见 **[与上游对照](docs/upstream-parity.md)**。

## 快速开始

### 下载发布包（推荐）

从 [Releases](https://github.com/StarTurambar/typewords-tui/releases) 下载 `typewords-tui-vX.Y.Z-x86_64-pc-windows-msvc.zip`，解压后目录结构：

```text
typewords-tui-vX.Y.Z-x86_64-pc-windows-msvc/
  tw.exe
  assets/
    list/word.json
    dicts/en/word/*.json
  docs/                # 本套文档（含截图）
  README.md
  LICENSE
```

在终端里运行 `tw.exe` 即可，程序会自动使用与 `tw.exe` 同级的 `assets/`。

### 从源码构建

需要 Rust **1.90+**（stable）。仓库自带 `assets/`，开发运行无需额外准备：

```bash
git clone https://github.com/StarTurambar/typewords-tui.git
cd typewords-tui
cargo run --locked          # 开发运行
cargo build --release --locked   # 产物：target/release/tw（Windows 为 tw.exe）
```

### 第一次运行

```bash
tw doctor   # 检查路径、词库、存档与主题
tw          # 进入界面：Enter 开始学习，f 自由练习，? 查看帮助
tw task     # 只预览今日任务，不进入界面
```

终端建议使用 **Windows Terminal**；最小尺寸 60 × 20；深色/浅色主题需要真彩色终端（不支持时可换「高对比」主题）。终端兼容性、组合键问题与鼠标说明见 [使用手册](docs/usage.md)。

## 常用命令

| 命令 | 说明 |
| --- | --- |
| `tw` | 启动界面 |
| `tw doctor` | 打印解析到的数据目录、资源目录、词库、存档与主题状态 |
| `tw task` | 预览今日任务（新词 / 到期复习 / 随机补足） |
| `tw import-dict FILE` | 把词库 JSON 导入数据目录并注册 |
| `tw --assets DIR` | 指定只读资源目录（需含 `list/word.json`） |
| `tw --home DIR` | 指定数据目录（等价于环境变量 `TYPEWORDS_HOME`） |
| `tw --dict ID` | 启动后直接进入指定词库（id 或 enName，如 `cet4`） |
| `tw --mode MODE` | 覆盖练习模式：`system/free/review/shuffle/identify/dictation/listen` |
| `tw --theme NAME` | 覆盖主题：`dark/light/high-contrast/auto` 或自定义主题文件名 |
| `tw --seed N` | 固定随机种子（复现问题用） |
| `tw --reset-practice` | 启动前清空练习缓存 |

完整的参数说明、目录查找顺序与示例见 [使用手册 · 命令行参考](docs/usage.md#命令行参考)。

## 界面与操作速查

六个页面：**单词**（主页 dashboard）、**词库**、**统计**、**设置**、**帮助**、**练习**，以及结算 / 输入 / 确认三种弹层。宽屏（≥ 100 列）用左侧栏导航，窄屏用顶栏标签；数字 `1`~`5` 可直接切页（练习中与弹层打开时除外）。

| 场景 | 按键 |
| --- | --- |
| 主页 | `Enter` 开始/继续学习、`f` 自由练习、`d` 词库、`s` 设置、`t` 统计、`m` 切换练习模式、`+`/`-` 每日词数 ±5、`?` 帮助、`q` 退出（需确认） |
| 练习页 | 字母/空格/退格输入；`空格` 确认/继续；`Esc` 显示完整单词；`Tab` 跳过当前词；`Enter` 收藏；反引号标记已掌握；`Ctrl+Z` 翻译、`Ctrl+I` 默写遮罩、`Ctrl+P` 暂停计时、`Ctrl+L` 词表侧栏、`Ctrl+E` 例句/词源、`Ctrl+C` 保存并返回；自测用 `1`/`2`/`3`，选择题用 `Alt+1`~`Alt+4` |
| 词库 / 统计 / 设置 / 帮助 | `↑`/`↓`（或 `j`/`k`）移动，`Enter` 激活，`Esc`/`q` 返回；设置页 `Enter`/`Space` 修改 |
| 弹层 | 确认：`Enter`/`y`；结算：`Enter`/`Space` 继续、`r` 重学、`q`/`Esc` 回主页；输入：`Enter` 保存、`Esc` 取消 |

- **鼠标**：侧栏、按钮、列表、弹层可点击，滚轮滚动；程序开启鼠标捕获，按住 **Shift 拖拽**可选择文本。
- **主题**：设置页切换，或启动时 `--theme`；自定义主题放到 `<home>/themes/*.json`，格式见 [主题系统](docs/themes.md)。

完整按键表（含 ASCII 别名与组合键注意事项）见 [使用手册 · 按键参考](docs/usage.md#按键参考)；各页面布局与鼠标细节见 [使用手册 · 界面导览](docs/usage.md#界面导览)。

## 数据位置

默认数据目录：Windows 为 `%LOCALAPPDATA%\typewords-tui`，其他平台由 `dirs` 决定；可用 `--home` 或 `TYPEWORDS_HOME` 覆盖。

```text
<home>/
  dict.json           # 词典 + FSRS 卡片 + 收藏/错词/已掌握（对应 Web 存档 v4）
  setting.json        # 设置（对应 Web 存档 v23）
  practice-word.json  # 断点练习缓存（对应 Web 存档 v2）
  themes/*.json       # 自定义主题（启动时加载）
  dicts/、list/        # 通过 import-dict 导入的词库
```

写入采用临时文件 + fsync + rename；解析失败的存档会被隔离为 `*.corrupt-<时间戳>.json`。版本高于程序支持的 `dict.json` 与 `setting.json` 都只读不覆盖；练习缓存版本不受支持时忽略（见 [存档与数据格式](docs/persistence.md)）。备份、迁移与字段含义见同一文档。

## 文档

完整文档在 [`docs/`](docs/README.md)，按读者和主题组织：

| 文档 | 内容 |
| --- | --- |
| [使用手册](docs/usage.md) | 安装、终端要求、CLI、界面导览、练习流程、按键、鼠标、FAQ 与排障 |
| [设置项参考](docs/settings.md) | 设置页 23 行的默认值、取值范围、作用；FSRS 参数与无 UI 的透传字段 |
| [主题系统](docs/themes.md) | 13 个颜色 token、内置主题、自定义主题格式与校验规则 |
| [存档与数据格式](docs/persistence.md) | 数据目录、三份存档与资源 JSON、版本策略、与 Web 数据互通 |
| [总体架构](docs/architecture.md) | 技术栈、分层与依赖约束、启动/事件循环、数据流、设计决策、技术债 |
| [领域逻辑](docs/domain.md) | 数据模型、今日任务、FSRS、计时、键入状态机、7 个流程、会话与统计 |
| [应用状态机](docs/app.md) | App 状态、输入路由、Action 管线、屏幕/弹层状态机、保存时机 |
| [渲染层](docs/ui.md) | 渲染管线、响应式外壳、各页面、点阵大字、鼠标命中 |
| [开发指南](docs/development.md) | 构建/测试/静态检查、测试体系、调试、截图、词库同步、CI 与发布 |
| [与上游对照](docs/upstream-parity.md) | 与 Web/桌面端的功能矩阵、行为差异、数据互通、路线图 |

## 与上游 TypeWords 的关系

- 上游项目：[zyronon/TypeWords](https://github.com/zyronon/TypeWords)，Nuxt + Vue 的 Web / Tauri 桌面端。
- 本仓库是**独立进程的终端客户端**：与上游共享词库资源格式和存档字段命名，但不共享代码，也不读写浏览器 IndexedDB。
- `assets/` 中的词库来自上游仓库的 `public/list/word.json` 与 `public/dicts/en/word/`；维护者可用 `node scripts/sync-assets.mjs --source <TypeWords 仓库路径>` 更新。
- 当前功能覆盖范围、与上游的具体差异（FSRS 间隔、设置项、统计口径等）以及补齐路线图，见 **[与上游对照](docs/upstream-parity.md)**。

## 开发

```bash
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

CI 覆盖 fmt / clippy / Ubuntu + Windows 测试；推送 `v*` tag 触发 Windows 发布包。详细的测试体系、截图工具、词库同步与发布检查清单见 [开发指南](docs/development.md)。

## 排障

| 现象 | 处理 |
| --- | --- |
| 启动报「找不到词库资源目录」 | 确认 `assets/` 与可执行文件同级，或用 `--assets` 指向含 `list/word.json` 的目录 |
| 中文/边框错位 | 换用 Windows Terminal，确认字体支持中文与制表符 |
| 提示「存档已损坏，已隔离」 | 坏文件已重命名为 `*.corrupt-*.json`，可删除或用 `tw doctor` 排查 |
| 提示「存档版本过高」 | `dict.json` / `setting.json` 来自更新版本时本次只读、不会覆盖；请备份并升级程序 |

更多现象、原因与处理见 [使用手册 · 常见问题与排障](docs/usage.md#常见问题与排障)。

## 贡献与许可证

- 参与开发前请读 [开发指南](docs/development.md)；文档纠错直接提 PR 即可。
- 本项目遵循 [GPL-3.0](LICENSE)。词库数据来自上游 TypeWords 仓库，遵循同一许可证。
