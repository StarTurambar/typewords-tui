# TypeWords TUI —— 终端单词练习客户端

[![CI](https://github.com/StarTurambar/typewords-actions/workflows/ci.yml/badge.svg)](https://github.com/StarTurambar/typewords-actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/StarTurambar/typewords-tui)](https://github.com/StarTurambar/typewords-releases)

用 Rust + [ratatui](https://ratatui.rs/) 实现的 TypeWords 终端客户端：在终端里完成「选词库 → 智能学习 → 结算 → 统计」闭环，进度保存在本地 JSON，重启进程可继续上次练习。

这是从 [TypeWords](https://github.com/zyronon/TypeWords) 拆分出来的独立项目，只包含终端客户端（Rust），不包含 Web / Tauri 桌面端。词库资源随仓库分发；存档字段命名与上游保持一致，但**不读写浏览器的 IndexedDB**。

![主页](docs/screenshots/home-120x40-dark.png)

![练习](docs/screenshots/practice-120x40-dark.png)

## 安装

### 下载发布包（推荐）

从 [Releases](https://github.com/StarTurambar/typewords-releases) 下载 `typewords-tui-vX.Y.Z-x86_64-pc-windows-msvc.zip`，解压后：

```text
typewords-tui-vX.Y.Z-x86_64-pc-windows-msvc/
  tw.exe
  assets/
    list/word.json
    dicts/en/word/*.json
  README.md
  LICENSE
```

在终端里运行 `tw.exe` 即可，程序会自动使用与 `tw.exe` 同级的 `assets/`。

### 从源码构建

需要 Rust **1.90+**（见下方「环境要求」）。

```bash
git clone https://github.com/StarTurambar/typewords-tui.git
cd typewords-tui
cargo run                 # 开发运行，自动使用仓库内 assets/
cargo build --release --locked
```

本地发布包：`pwsh -File scripts/package.ps1`。

### 快速开始

```bash
tw doctor        # 检查路径、词库与存档
tw               # 启动界面
tw task          # 只预览今日任务，不进入界面
```
## v1 范围

已实现：

- **单词练习主线**：跟写（7 词小组 → 组内拼写）、听写、默写、自测、复习、随机复习、自由练习（7 个内置流程）
- **FSRS 调度**：评级阈值与 Web 一致（默认 0 / 3 / 6 次错误分档），到期复习按自然日挑选
- **错词清空**：阶段结束仍有错词时自动进入巩固循环
- **三本系统词典**：收藏（Enter）、错词（自动收录）、已掌握（反引号键），已掌握词下次练习自动排除
- **词库与进度管理**：词库切换、每日词数调整、分组进度、跳过阶段
- **统计**：结算弹层；主页展示今日学习时长 / 总学习天数 / 总学习时长与本周记录，统计页展示累计概览（新词、复习、错误、正确率）与近 14 天逐日记录
- **设置**：练习模式、主题、音标、大字开关、复习比例、重复次数、FSRS 阈值与留存率等 22 项，按「通用 / 复习 / 键入 / 显示 / FSRS / 其他」分组
- **主题**：内置深色 / 浅色 / 高对比三套，支持 `<home>/themes/*.json` 自定义主题与 `--theme` 覆盖
- **界面与鼠标**：响应式外壳（宽屏侧栏 / 窄屏顶栏标签）、可点击的侧栏 / 按钮 / 列表、滚轮滚动
- **断点续练**：练习缓存写入本地 JSON，重启进程恢复到同一词/同一阶段
- **工具命令**：`tw doctor` / `tw task` / `tw import-dict`

v1 明确不包含：

| 不包含 | 说明 |
| --- | --- |
| 发音 / 键盘音效 | 不引入音频依赖；听写阶段依靠音标与释义回忆 |
| 文章背诵 | 只读单词词库，`dicts/en/article` 不加载 |
| 云同步（Supabase）/ 在线翻译 | 无网络依赖 |
| 网页备份导入导出 | 存档字段与 Web 同名（`typing-word-dict` v4、`typing-word-setting` v23、`PracticeSaveWord` v2），后续可加互通 |
| 自定义流程编辑器 / 批量标记 | 固定 7 个内置流程 |
| 可配置快捷键 | v1 键位固定，帮助页（`?`）可查 |

> 完整的「还缺什么」与补齐顺序见下文 **与 Web / 桌面端的差距** 与 **补齐路线图**。

## 环境要求

- Rust **1.90+**（stable；Windows 上默认 `x86_64-pc-windows-msvc` 工具链可直接 `cargo build`）
- Windows 建议使用 **Windows Terminal**（conhost 对 CJK 宽度与部分组合键支持较差）
- 终端最小尺寸 **60 × 20**
- 深色 / 浅色主题按 24-bit RGB 着色，建议使用支持真彩色的终端；不支持时颜色会被终端近似甚至失真，可换「高对比」（纯 ANSI 颜色）或自定义 ANSI 颜色主题
- 鼠标交互依赖终端上报鼠标事件（Windows Terminal、iTerm2、kitty 等默认支持）；不支持的终端仍可纯键盘操作

纯 TUI 不依赖 WebView2；运行 TUI 与测试都不需要 Node.js（只有维护者同步词库、打包发布时才用到 Node/PowerShell）。

## 构建、测试与静态检查

```bash
# 开发运行（自动使用仓库内 assets/ 作为词库资源）
cargo run

# 测试 / 静态检查
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt --all --check

# 发布构建（产物：target/release/tw.exe）
cargo build --release --locked
```

发布时请把 `assets/` 与 `tw.exe` 放在同一目录，或通过 `--assets` 指定资源目录；本仓库自带 `assets/`（当前为 CET-4 词库）。
## 命令行

```
tw [--assets DIR] [--home DIR] [--dict DICT_ID] [--mode MODE] [--theme NAME] [--seed N] [--reset-practice]

tw doctor            # 打印解析到的路径、词库、存档与主题状态
tw task              # 预览今日任务（不启动界面）
tw import-dict FILE  # 把词库 JSON 导入数据目录并注册
```

| 参数 | 说明 |
| --- | --- |
| `--assets DIR` | 只读资源目录（需含 `list/word.json` 与 `dicts/`） |
| `--home DIR` | 数据目录（存档与导入词库），等价于环境变量 `TYPEWORDS_HOME` |
| `--dict ID` | 直接进入指定词库（id 或 enName，如 `cet4`） |
| `--mode MODE` | 覆盖练习模式：`system/free/review/shuffle/identify/dictation/listen` |
| `--theme NAME` | 启动时覆盖主题：`dark` / `light` / `high-contrast` / `auto` 或 `<home>/themes` 下的文件名 |
| `--seed N` | 固定随机种子（复现问题用；正式使用请忽略） |
| `--reset-practice` | 启动前清空练习缓存 |

资源目录查找顺序：`--assets` > `TYPEWORDS_ASSETS` > `<exe 目录>/assets` > 当前目录及其上 3 级祖先的 `assets/`（开发期），并兼容同层 `public/`（从上游仓库内运行时）。
数据目录默认 `%LOCALAPPDATA%\typewords-tui`（Windows），可用 `--home` 或 `TYPEWORDS_HOME` 覆盖。
`tw doctor` 会额外列出自定义主题目录、已加载的主题 id / 名称与加载告警。

## 数据位置

```
<home>/
  dict.json           # 词典 + FSRS 卡片 + 收藏/错词/已掌握 + 笔记（对应 Web typing-word-dict v4）
  setting.json        # 设置（对应 Web typing-word-setting v23）
  practice-word.json  # 断点练习缓存（对应 Web PracticeSaveWord v2）
  themes/*.json       # 自定义主题（启动时加载；新增或修改后需重启进程）
  dicts/en/word/*.json# import-dict 导入的词库
  list/word.json      # 用户词库目录（与内置目录合并）
```

写入策略：同目录临时文件 + fsync + rename；解析失败时把坏文件重命名为 `*.corrupt-<时间戳>.json` 并回退默认值；存档版本高于程序支持版本时**只读不覆盖**。

## 按键

练习页：

| 键 | 动作 |
| --- | --- |
| 字母 / 符号 / 空格 / 退格 | 输入 |
| 空格 | 确认 / 继续 |
| `Esc` | 显示完整单词（遮罩时计一次错误） |
| `Ctrl+Z` | 切换翻译显示 |
| `Ctrl+I` | 切换默写遮罩 |
| `Ctrl+P` | 暂停 / 恢复计时（v1 无音频，`Ctrl+P` 不再用于发音） |
| `Ctrl+R` | 随机默写当前词表 |
| `Ctrl+B` / `Ctrl+L` | 收起状态栏 / 词表侧栏 |
| `Ctrl+E` | 显示 / 隐藏例句与词源 |
| `Ctrl+C` | 保存并返回主页 |
| `Ctrl+Enter` | 立即保存练习进度与词典 |
| `Tab` | 跳过当前词（不计输入数） |
| 反引号键 | 标记 / 取消已掌握 |
| `Enter` | 收藏 / 取消收藏 |
| `Ctrl+→`（别名 `Ctrl+N`）/ `Ctrl+←` | 下一个词 / 上一个词 |
| `Shift+→`（别名 `Ctrl+J`） | 跳到下一阶段 |
| `1` / `2` / `3` | 自测：认识 / 不认识 / 已掌握 |
| `Alt+1`~`Alt+4` | 自测选择题 A~D |
| `?` | 帮助 |

其它页面：

- **主页**：`Enter` 开始 / 继续学习，`f` 自由练习（不改当前练习模式，本次以自由模式启动），`d` 词库，`s` 设置，`t` 统计，`m` 切换练习模式，`+` / `-` 调整每日词数（每次 5 词），`1` ~ `5` 切换页面（单词 / 词库 / 统计 / 设置 / 帮助），`?` 帮助，`q` 退出（需确认）。
- **词库页**：`↑` / `↓`（别名 `j` / `k`）选择，`Enter` 设为当前词库，`+` / `-` 调整每日词数，`Esc` / `q` 返回。
- **统计页**：`↑` / `↓`（别名 `j` / `k`）滚动，`PgUp` / `PgDn` 翻 5 行，`Esc` / `q` / `Enter` 返回。
- **设置页**：列表按「通用 / 复习 / 键入 / 显示 / FSRS / 其他」分区，`↑` / `↓`（别名 `j` / `k`）选择，`Enter` / `Space` 修改；数字项进入输入框后 `Enter` 保存、`Esc` 取消，`Esc` / `q` 保存并返回。
- **帮助页**：`↑` / `↓`（别名 `j` / `k`）、`PgUp` / `PgDn` 或滚轮滚动；其它键返回。
- **弹层**：确认弹层 `Enter` / `y` 确认、其它键取消；结算弹层 `Enter` / `Space` 继续、`r` 重学、`q` / `Esc` 回主页；输入弹层 `Enter` 保存、`Esc` 取消。

> 数字 `1`~`5` 只在非练习、无弹层时用于切页；练习中 `1`/`2`/`3` 属于自测按键，`Alt+1`~`Alt+4` 属于选择题。帮助页的「其它键返回」（滚动键除外）优先于数字切页。
> 组合键在部分终端会被吞（尤其 `Alt+数字`、`Ctrl+方向键`）。此时可用直接拼写单词代替选择题，或使用括号内的 ASCII 别名。

## 鼠标

TUI 启动时会开启终端鼠标捕获，支持：

- **点击导航**：宽屏侧栏的「单词 / 词库 / 统计 / 帮助 / 设置 / 退出」，窄屏顶栏的页面标签。
- **点击按钮**：主页的「开始 / 继续学习」「自由练习」、每日目标 `[-]` / `[+]`、词库页的「Enter 设为当前」，以及结算 / 确认弹层的按钮。
- **点击区块入口**：主页「我的词典」标题右侧的 `d 词库`、「推荐」标题右侧的 `d 更多`。
- **点击列表**：词库页与设置页第一次点击选中一行，再次点击已选中的行（或按 `Enter`）才会激活（词库页：设为当前词库并返回主页；设置页：修改该项）。
- **滚轮滚动**：词库页 / 设置页移动光标，统计页滚动最近 14 天记录，帮助页滚动快捷键列表；其它页面忽略。
- **弹层**：确认弹层点击「确认 / 取消」按钮，点击弹层外区域取消；输入弹层点击弹层外取消（内部点击无动作，用 `Enter` 保存 / `Esc` 取消）；结算弹层只响应三个按钮。
- 练习页以键盘为主，鼠标只在结算弹层里点按钮。

因为终端把鼠标事件交给程序，终端原生的拖拽选择 / 右键菜单会失效；**按住 `Shift` 拖拽**可绕过鼠标捕获选择文本（多数终端如此，个别终端用 `Alt` 或需在终端设置里关闭鼠标上报）。

## 界面

除练习页外，所有页面共用一套外壳；练习页保持全屏布局，只换用主题配色。

### 外壳

- **宽屏（≥ 100 列）**：左侧栏占 24 列（≥ 120 列时 26 列），自上而下是 `TW` 大字标识、`Type Words` 副标题、「单词 / 词库 / 统计 / 帮助」导航（右侧标出数字快捷键）、分隔线、版本号、「设置」、「退出」。
- **窄屏（< 100 列）**：不显示侧栏；顶栏左侧是 `Type Words`，右侧是可点击的「单词 / 词库 / 统计 / 设置 / 帮助」标签，当前页用 `element` 背景高亮。
- **顶栏**：宽屏时显示当前页标题（`▎` 前缀），右侧有空间时显示「模式 X · 主题名」。
- **底栏**：左侧显示 Toast（成功 / 警告 / 错误 / 提示分别用 `good` / `warn` / `bad` / `accent_alt`），没有 Toast 时显示当前页快捷键提示；右侧显示版本号。
- 窗口小于 **60 × 20** 时只渲染「终端太小」提示。

### 主页

主页是一个纵向 dashboard，从上到下：

1. **当前词库卡片**：词库名、预计完成日期（已学完显示「已学完 · 总复习阶段」）、进度条与百分比、「已学 N / M 词」；底部是主按钮「Enter 开始 / 继续学习 →」与「f 自由练习」。
2. **今日任务**（卡片右侧）：每日目标 `[-] N [+]`（每次 ±5 词）与「新词 / 复习」两块数字卡；没有到期复习时显示「无到期」。
3. **统计**：今日学习时长 / 总学习天数 / 总学习时长三块数字卡；右侧是「本周学习记录」——周一至周日，今天用 `accent` 高亮，有记录的日子显示 `●`，没有显示 `·`。
4. **我的词典**：3~5 块词库卡（`内置` / `当前` / `词库` 徽标 + 词数），当前词库优先展示；词库不够时补一块「+ d 管理」。
5. **推荐**：尚未加入的词库（名称 / 简介 / 词数），标题右侧「d 更多」进入词库页；没有推荐时显示「暂无推荐 · d 浏览词库」。

窗口高度不足时依次收起「推荐」「我的词典」两段（60×20 只保留词库卡与统计）；宽度不足时隐藏「本周学习记录」。

### 词库 / 统计 / 设置 / 帮助页

- **词库页**：左侧是全部词库列表（当前词库 `●`、光标 `▶`、进度条、词数、百分比，溢出时右侧显示 `↑` / `↓`），右侧是选中词库的详情（名称、id、分类、简介、标签、词数、进度、每日词数）与「Enter 设为当前」按钮；内容宽度不足 72 列时只显示列表。
- **统计页**：6 块总览卡（累计时长、学习天数、新词、复习、错误、正确率，其中新词 / 复习 / 错误带今日增量），下面是「最近 14 天」逐日条形记录（用时条 + 新 / 复 / 错）；记录多于一屏时可滚动。
- **设置页**：单列表格，按「通用 / 复习 / 键入 / 显示 / FSRS / 其他」分区，每行「名称 + 当前值」；上下溢出时右侧显示 `↑` / `↓`。
- **帮助页**：宽屏分「练习」「导航/查看」两栏，窄屏单栏并可滚动（右侧显示 `↑` / `↓`）；末尾说明鼠标操作，滚动键以外任意键返回。

### 练习页

练习页保持 v1 的全屏布局（不套用侧栏 / 顶栏 / 底栏），重设计只更换了配色与面板样式：

- **大字单词**：内置 5×7 点阵字体（A-Z、0-9 与常见标点），用 ▀ / ▄ / █ 半块字符拼成。
  已输入且正确的字母显示为 `good` 色，错误的为 `bad` 色，下一个待输入字母用 `accent` 色，其余为 `fg` 色；
  遮罩 / 默写模式下未输入的字母显示为 `dim` 色空心方框，不泄露字形。
  空间充足时自动改用 **2 倍像素**（像素级放大，边缘仍清晰），并按「本轮最长单词」决定整轮是否放大，
  避免同一轮里字号忽大忽小；窗口过窄或单词过长时整轮回退 1 倍。
  设置页的「大字单词」可关闭 2 倍放大（回退 1 倍）；用 `cargo run --example dump -- free '' 160 45 plain` 可对比 1 倍与 2 倍效果。
  超长单词会自动压缩字距，必要时拆成多行；点阵不支持的单词（如带重音符号）自动回退为普通文本。
- **信息卡片**：音标在大字上方居中；释义、例句（以及开启后的词源/词根）分别放在圆角长条卡片里
  （`panel` 背景 + `border` 边框 + `dim` 标题），按终端宽度折行，超出卡片高度的内容以「…」截断。
- **顶部状态行**：词库 · 阶段（如「新词 · 跟写」「小组巩固」）· 进度条 · 错词数（有错词时用 `bad`）· 计时（暂停时用 `warn`）。
- **底部阶段条**：每个节点一根小进度条 + 两行快捷键 + 临时提示（Toast）；`Ctrl+B` 可收起。
- `Ctrl+L` 展开右侧词表侧栏（`▶` 当前词、错词 `bad`、已排除词 `dim`）。
- 自测阶段：单词以实心方块遮挡，答案选项卡（A~D，Alt+1~4 选择）沿用同样的卡片样式；揭晓后选项按对错用 `good` / `bad` 着色。

调试时可用 `cargo run --example dump -- dictation ca` 把某个模式的界面直接打印成文本。

## 主题

TUI 的配色全部来自主题的 13 个颜色 token，渲染层不出现硬编码颜色。主题配置位于 `src/theme/`，与 ratatui 渲染无关。

### 内置主题

| id | 名称 | 说明 |
| --- | --- | --- |
| `dark` | 深色 | 默认主题；`auto` 等同深色 |
| `light` | 浅色 | 浅色背景 |
| `high-contrast` | 高对比 | 纯 ANSI 颜色（黑底白字 + 亮色强调），适合低色彩终端 |

- 设置页「主题」一行按 `auto` → 内置主题 → 自定义主题（按文件名排序）循环切换；`auto` 显示为「自动（深色）」。
- 启动参数 `--theme NAME` 可覆盖当前主题设置（与 `--mode` 一样，退出时会随设置保存）；未知名字会回退深色并弹 Toast 提示。
- `tw doctor` 会列出自定义主题数量、主题目录、全部主题 id 与名称，以及加载告警。

### 自定义主题

把 `*.json` 放到 `<home>/themes/`（Windows 默认 `%LOCALAPPDATA%\typewords-tui\themes`），启动时加载；**新增或修改后需重启进程生效**。

`<home>/themes/ocean.json` 示例：

```json
{
  "name": "Ocean",
  "base": "dark",
  "colors": {
    "bg": "#0b1020",
    "panel": "#121a30",
    "element": "#1a2440",
    "border": "#2c3a5e",
    "fg": "#d7e2ff",
    "muted": "#8fa0c8",
    "dim": "#6b7aa3",
    "accent": "#6fd3c7",
    "accent_alt": "#7aa2ff",
    "warn": "#e2b86b",
    "good": "#9ece6a",
    "bad": "#e06c75",
    "selection": "#243053"
  }
}
```

- 主题 id 取文件名并转为小写（上例为 `ocean`；`Ocean.json` 也会得到 id `ocean`）；`name` 可省略（默认使用文件名原文）。
- `base` 可省略，默认 `dark`；可填 `dark` / `light` / `high-contrast`，未覆盖的 token 继承 base 的颜色。
- `colors` 中的 13 个 token 都可省略，只写想改的即可。
- 颜色写法：`#rrggbb`、`#rgb`，或 ANSI 颜色名：`reset`/`default`、`black`、`darkgray`、`red`、`lightred`、`green`、`lightgreen`、`yellow`、`lightyellow`、`blue`、`lightblue`、`magenta`（别名 `purple`）、`lightmagenta`、`cyan`、`lightcyan`、`gray`/`grey`、`white`；`light*` 也可写成 `light_red`、`brightred` 等形式。
- 文件名需为 1~32 个字母、数字、下划线或连字符；主题 id 会自动转为小写（`Ocean.json` 与 `ocean.json` 视为同一个 id）；`auto` / `dark` / `light` / `high-contrast` 是保留名，不能作为文件名（不区分大小写）。
- 文件校验失败（JSON 语法、未知字段、未知 token、未知 base、非法颜色等）时只跳过该文件并在启动时弹告警，不影响其它主题；告警也可用 `tw doctor` 查看。

### 颜色 token

| token | 用途 |
| --- | --- |
| `bg` | 页面背景 |
| `panel` | 卡片 / 弹层 / 词库详情等面板背景 |
| `element` | 数字卡、次级按钮、当前标签等块背景 |
| `border` | 分隔线、卡片与弹层边框 |
| `fg` | 正文主色 |
| `muted` | 次要文字（副标题、说明、未选中项） |
| `dim` | 提示、辅助信息、已排除/禁用状态 |
| `accent` | 主强调色（当前页、主按钮、选中标记、当前待输入字母） |
| `accent_alt` | 次强调色（数字、进度条、按键提示） |
| `warn` | 警告 Toast、暂停计时、今日任务标记 |
| `good` | 正确、成功、已完成 |
| `bad` | 错误、错词、危险操作 |
| `selection` | 列表选中行 / 当前行背景 |

## 与 Web / 桌面端的差距

> 对照基线：上游 Web 端 `app/` 约 **3.1 万行、15 个页面**；本仓库 `src/` 约 **1.2 万行、6 个页面（主页 / 练习 / 词库 / 统计 / 设置 / 帮助），以及结算 / 输入 / 确认三种弹层**。
> TUI 已对齐「单词练习闭环」，下面按「完全缺失 / 未接线 / 行为差异 / 数据互通」列出差距。

> 下表「Web 端载体」列中的路径均指上游 TypeWords 仓库内的文件。

### 完全缺失

| 功能 | Web 端载体 | 说明 |
| --- | --- | --- |
| 文章背诵 | `pages/(articles)/*`、`components/article/*`、`core/hooks/article.ts` | 句子级跟打/默写、句音频、课后题；TUI 完全不读 `dicts/en/article` |
| 发音与键盘音效 | `core/hooks/sound.ts`、`components/setting/SoundSetting.vue` | 有道发音、TTS 兜底、5 套键音、音量/语速 |
| 云同步（用户自带 Supabase） | `core/utils/supabase.ts`、`useDataSyncPersistence.ts` | 多设备同步、冲突检测、hash 备份回滚、迁移弹窗；**没有官方账号体系**（登录/注册代码为遗留，无页面） |
| 备份导入导出 | `core/hooks/export.ts` | ZIP（data.json + mp3）/JSON 导入导出；TUI 无对应 UI |
| 单词测试（words-test） | `pages/(words)/words-test/[id].vue` | 随机/复习测试与判分；枚举值存在但无流程 |
| 词库与单词管理 | `pages/(words)/dict-list.vue`、`dict.vue` | 搜索、增删改、批量删除、自定义词典、Excel/JSON 导入、在线词库目录 |
| 单词查找 / 在线翻译 | `WordLookupPopover.vue`、`libs/translate/*` | 练习中查词、例句翻译 |
| 笔记 | `noteData` + TypeWord 编辑区 | TUI 只落盘，不显示也不可编辑 |
| 批量标记 | `WordMarkPickList.vue` | 一次标记一批词认识 / 不认识 / 已掌握 |
| 自定义练习流程 | `practice-flow-runtime.ts`（`PracticeFlowV2`） | TUI 固定 7 个内置流程 |
| 自定义快捷键 | `shortcutKeyMap`（45+ 动作） | TUI 键位写死 |
| 多语言 | `i18n/locales/*`（14 种） | TUI 文案为中文硬编码 |
| 统计维度 | `Statistics.vue`、主页日历 | 按词典拆分、学习天数、周记录；TUI 有主页本周记录与统计页概览 + 近 14 天，但不按词典拆分、无日历视图 |
| 练习内快捷设置 / 引导 | `SettingDialog.vue`、`PracticeOnboardingHost.vue` | TUI 只有全局设置页 + toast |
| 帮助 / 关于 / 文档 / 反馈页 | `pages/help.vue` 等 | TUI 有快捷键帮助页，无关于 / 文档 / 反馈页 |

### 计划内但尚未接线

- **跳组**：`create_task_from_group` 已实现，未接到界面（词库页加 `g` 键即可）。
- **上一组 / 下一组（重复本章 / 下一章）**：练习中 `Ctrl+Enter` 目前只做「立即保存」。
- **Shift+Enter 收藏到指定词典**：只能收藏到「收藏」。
- **笔记展示与编辑**。
- **`showUsageTips`**：设置项存在但不生效（提示常显）。
- **`Ctrl+M` 简洁模式**；`Ctrl+Q` 快捷切主题也未接线（主题目前只能在设置页或启动时用 `--theme` 切换）。
- **设置缺项**：`maximum_interval`、`enable_fuzz`、21 个可编辑的 FSRS `w` 权重。
- **`alwaysShowNote`** 等少量设置项只持久化、无 UI。

### 同名功能的行为差异

1. **无音频**：听写阶段改为「看音标/释义回忆」。
2. **FSRS 间隔近似**：评级映射与 Web 完全一致；间隔由 `fsrs` crate（FSRS-6，同 21 个默认权重）计算，但不复刻 ts-fsrs 的 learning steps：
   - 新卡 / 学习卡：Again → 1 分钟，Hard → 6 分钟，Good → 10 分钟；
   - 复习卡：走 FSRS 间隔（≥ 1 天，受「最大间隔」截断）；
   - 复习选取只看自然日，因此「今天哪些词到期」基本一致，但具体间隔天数可能不同。
3. **固定键位**：不支持改键；`Ctrl+P` 被复用为暂停计时。
4. **设置项**：只建模约 22 项，其余 40+ 字段原样透传保存但无 UI。
5. **断点缓存**：支持 v2；v1 直接丢弃（Web 有 v1→v2 升级）。
6. **统计口径**：主页有本周记录条、统计页有累计概览与最近 14 天；仍无按词典拆分与日历视图。
7. **交互**：支持鼠标点击 / 滚轮与主题化界面；无拖拽、无动画，键位仍固定；鼠标捕获下需 Shift+拖拽选择文本。

### TUI 独有

- **自定义主题**：`<home>/themes/*.json` + 内置三套主题；Web 侧用 CSS 变量主题，与 TUI 不共享主题文件。
- **鼠标操作**：侧栏 / 按钮 / 列表可点击、滚轮滚动；这是终端鼠标协议（需终端支持，Shift+拖拽选文本），Web 端本身即浏览器鼠标。

### 数据互通

- 存档**字段同名**（`dict` v4 / `setting` v23 / `PracticeSaveWord` v2），但 TUI 不读写浏览器 IndexedDB；当前从 Web 迁移只能手工处理 JSON。
- TUI 新增了 `tuiBigWord` 字段（Web 会忽略并原样保留，不会报错）。
- 没有 article cache 与本地 mp3 的对应存储。

## 补齐路线图

| 优先级 | 内容 | 预估 |
| --- | --- | --- |
| P0 | 跳组、上一组/下一组、笔记展示+编辑、`showUsageTips` 生效、`Ctrl+M` 简洁模式、收藏到指定词典、补 `maximum_interval`/`enable_fuzz` | 每项 0.5–1 天 |
| P1 | 词库/单词管理屏（搜索/删除/新增/自定义词典/批量标记）、单词测试模式、统计按词典与日历 | 3–5 天 |
| P2 | Web 备份 JSON 导入导出（再做 ZIP+mp3）、音频（有道 + 键音）、文章背诵、Supabase 同步 | 1–2 周/块 |

P0 完成即可明显接近网页版的日常手感；P2 的每一项都建议单独评审，避免一次性铺开。

## 代码结构

```
src/
  main.rs / lib.rs      # 入口与模块声明（lib + bin 便于集成测试）
  cli.rs                # 参数、doctor（含主题清单）、task、import-dict
  paths.rs              # 资源/数据目录解析（含 <home>/themes）
  bootstrap.rs          # 启动装载：读存档 + 词库 + 主题
  domain/               # 与终端无关的领域逻辑
    types.rs            # 数据模型（字段名与 Web 存档一致）
    catalog.rs          # 词库目录与词条文件
    study_task.rs       # 今日任务（新词 + 到期复习 + 随机复习）
    scheduler.rs        # FSRS 评级与间隔
    timer.rs            # 计时片段 / 跨天拆分 / 空闲暂停
    flow/               # 流程编排（7 个内置 flow、游标、推进器、阶段展示）
    typing.rs           # 键入状态机
    question.rs         # 自测选择题
    session.rs          # 会话控制器（初始化 / 恢复 / 结算 / 下一组）
    statistics.rs       # 结算写入词典统计
    i18n.rs             # 中文文案（键名对齐 Web i18n）
    rng.rs              # 可注入随机源
  store/mod.rs          # 本地 JSON 存档（原子写、版本校验、损坏隔离）
  theme/                # 主题配置层（不依赖 ratatui）
    mod.rs              # 颜色模型、BaseMode、ThemeRegistry、主题回退
    builtin.rs          # dark / light / high-contrast 三套内置主题
    load.rs             # <home>/themes/*.json 读取、base 继承、校验告警
  app/
    mod.rs              # 应用状态机与事件循环（键盘 / 鼠标 / 定时器）
    action.rs           # Action 枚举与 apply_action（唯一状态变更入口）
    keymap.rs           # 键位 → Action（含弹层键位）
    mouse.rs            # 鼠标事件 → ui::hit 命中检测 → Action
  ui/
    mod.rs              # 渲染分发 + 终端过小提示
    theme.rs            # 主题 token → ratatui 颜色
    shell.rs            # 外壳：侧栏 / 顶栏 / 底栏 / 响应式布局
    widgets.rs          # 通用小部件（按钮、进度条、折行、按键提示）
    target.rs           # 可点击目标（Rect + Action）
    hit.rs              # 命中检测：targets / action_at / wheel_action
    home.rs             # 主页 dashboard（当前词库 / 今日任务 / 统计 / 本周 / 词典 / 推荐）
    practice.rs         # 练习页（大字单词 + 释义/例句卡片 + 自测选项 + 阶段条）
    bigtext.rs          # 5×7 点阵字体与 1×/2× 放大
    dict_picker.rs      # 词库页（列表 + 详情）
    stats.rs            # 统计页（累计概览 + 最近 14 天）
    settings.rs         # 设置页（分区列表）
    help.rs             # 帮助页（快捷键 + 鼠标说明）
    insights.rs         # 主页 / 统计的纯数据聚合（总量、本周、预计完成）
    overlays.rs         # 结算 / 输入 / 确认弹层
tests/                  # 集成测试（流程、存档、应用驱动、端到端、真实词库、UI 快照）
examples/dump.rs        # 把练习页渲染成文本，便于调样式
examples/shot.rs        # 把任意页面渲染成带样式的 JSON（截图工具）
tools/render-snapshot.py # JSON 快照 → PNG（开发用）
```

各页面的 `layout()` 同时被 `render()` 与 `targets()` 使用，绘制与鼠标命中共享同一份几何数据，滚动后不会漂移。
键盘（`app/keymap.rs`）、鼠标（`app/mouse.rs` → `ui/hit.rs`）与测试都只产出 `app/action.rs` 的 `Action`，统一由 `App::apply_action` 修改状态；界面层只读 `App`，不直接改状态。

对应关系：Web 的 `usePracticeWordNavigator` → `domain/flow/navigator.rs`；`usePracticeWordTyping` → `domain/typing.rs`；`usePracticeWordSession` → `domain/session.rs`；`usePracticeFlowDisplayState` → `domain/flow/mod.rs` 的 `display_state`；`TypeWord.vue` 的视觉 → `ui/practice.rs`；Web 主页的信息层级 → `ui/home.rs`（预计完成日期对应 Web 的 `_getAccomplishDate` → `ui/insights.rs`）；`Statistics.vue` → `ui/stats.rs`。

## 测试

```bash
cargo test --locked
```

覆盖：

- `src/domain/typing.rs` 单元测试：五种练习类型的键入分支、大小写、全角标点、重复次数、自动下一词
- `src/ui/bigtext.rs` 单元测试：字形行数、1×/2× 放大、遮罩空心占位、超长拆行、非 ASCII 回退
- `src/theme/mod.rs` / `src/theme/load.rs` 单元测试：`#rgb` / `#rrggbb` / ANSI 颜色解析、`auto` 与缺失主题回退、base 继承、非法文件 / 保留 id / 未知 token 被跳过
- `src/ui/widgets.rs` 单元测试：按显示宽度截断与折行、底部按键提示不越界
- `src/ui/insights.rs` 单元测试：累计总量与正确率、本周从周一开始、预计完成日期
- `src/ui/overlays.rs` 单元测试：结算 / 确认弹层在最小终端内不越界
- `tests/flow_parity.rs`：7 个内置流程均可推进到结算；错词清空；相位与分组
- `tests/store_roundtrip.rs`：存档往返、未知字段保留、损坏隔离、版本过高不覆盖
- `tests/app_drive.rs`：合成按键驱动（开始、输入推进、跳过、持久化恢复、设置保存与持久化、错词收录、退出确认），以及自由练习不改设置、`+`/`-` 每日词数持久化、数字键切页、鼠标（主页开始、侧栏导航、滚轮、点击弹层外部取消、结算按钮）、自定义主题循环与渲染、`--theme` 覆盖
- `tests/end_to_end.rs`：完整会话到结算，写入统计与 FSRS 卡片
- `tests/real_assets.rs`：使用仓库内置 CET-4（2607 词）真实数据校验
- `tests/ui_snapshots.rs`：TestBackend 渲染（主页 / 练习大字 / 默写占位 / 各屏幕 / 终端过小 / 60×20 紧凑布局 / CJK 宽度不越界 / 正确输入着色 / 三套内置主题 / 主题面板与强调色 / 弹层 / 鼠标命中区域与渲染文本一致）

### 截图 / 视觉回归

```bash
# 1) 把任意页面渲染成带样式的 JSON（纯 Rust，无需 Python）
cargo run --quiet --example shot -- home 120 40 dark > home.json

# 2) JSON → PNG（需要 Python + Pillow）
python tools/render-snapshot.py home.json target/screenshots/home-120x40-dark.png
```

- `shot` 支持 `home / practice / dict / stats / settings / help / summary / edit / confirm`，参数为 `<页面> [宽] [高] [主题]`，默认 `120 × 40`、`dark`；加 `--plain` 可直接输出等宽纯文本。
- `render-snapshot.py` 省略输出路径时默认写到 `target/screenshots/<页面>-<宽>x<高>-<主题>.png`；字体自动探测，可用 `--font` / `--cjk-font` 指定，`--scale` 调整像素倍率。
- Pillow 只是开发期截图依赖（`pip install Pillow` 或使用自带 Pillow 的 Python）；运行 TUI 与 `cargo test` 都不需要 Python。

## 与上游 TypeWords 的关系

- 上游项目：[zyronon/TypeWords](https://github.com/zyronon/TypeWords)，用 Nuxt + Vue 实现 Web / Tauri 桌面端。
- 本仓库是**独立进程的终端客户端**：与上游共享词库资源格式和存档字段命名，但**不共享代码**，也不读写浏览器 IndexedDB。
- `assets/` 中的词库来自上游仓库的 `public/list/word.json` 与 `public/dicts/en/word/`；维护者可用 `node scripts/sync-assets.mjs --source <TypeWords 仓库路径>` 更新。
- 功能差距见上文「与 Web / 桌面端的差距」，补齐路线图见下节。

## 开发

```bash
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked

# 从上游 TypeWords 仓库同步词库（需要 Node）
node scripts/sync-assets.mjs --source ../TypeWords

# 本地打包 Windows release
pwsh -File scripts/package.ps1
```

CI 配置见 `.github/workflows/ci.yml`（fmt / clippy / Linux + Windows 测试）；推送 `v*` tag 会触发 `.github/workflows/release.yml` 生成 Windows 发布包。

## 许可证

本项目遵循 [GPL-3.0](LICENSE)。词库数据来自上游 TypeWords 仓库，遵循同一许可证。
## 排障

| 现象 | 处理 |
| --- | --- |
| 启动报「找不到词库资源目录」 | 确认 `assets/` 与 `tw.exe` 同级（或位于当前目录），或用 `--assets` 指向含 `list/word.json` 的目录 |
| 中文/边框错位 | 换用 Windows Terminal；确认终端字体支持中文与制表符 |
| 提示「存档已损坏，已隔离」 | 坏文件被重命名为 `*.corrupt-*.json`，可直接删除或用于排查 |
| 提示「存档版本过高」 | 存档来自更新版本，TUI 不会覆盖；请升级程序 |
| 进入练习提示「没有可学习的单词」 | 词库已学完或全部被标记为已掌握，换词库或取消已掌握标记 |
| 组合键无效 | 参考「按键」章节的 ASCII 别名 |
| 自定义主题没生效 / 启动提示主题告警 | 检查文件名（1~32 位字母、数字、`_`、`-`，不能用保留名）、JSON 是否有未知字段、颜色写法；用 `tw doctor` 查看已加载主题与告警；修改后需重启进程 |
| 颜色发灰、偏色或出现异常颜色 | 深色 / 浅色主题按 24-bit RGB 输出，需终端支持真彩色（确认 `COLORTERM=truecolor`）；否则可换「高对比」或自定义 ANSI 颜色主题 |
| 鼠标点击无反应 / 无法复制文本 | 终端需支持鼠标事件；TUI 会开启鼠标捕获，按住 `Shift` 拖拽选择文本（个别终端用 `Alt` 或需关闭鼠标上报） |
| `--theme` 指定后回退深色 | 主题名写错或自定义文件加载失败，启动时会提示并回退；用 `tw doctor` 核对主题 id |
