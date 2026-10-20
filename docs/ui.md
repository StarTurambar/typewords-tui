# 渲染层（ui）

渲染层负责把只读的 `App` 状态画到 ratatui 的 `Frame` 上：布局、颜色、文本折行、大字点阵，以及「屏幕上的哪块区域对应哪个鼠标动作」。它不修改 `App`、不写存档、不推进会话；所有数据都由应用层准备好，所有鼠标交互都通过 `ui::hit` 的 `Target` 映射回应用层。

> 适用范围：`src/ui/` 全部文件，以及 `src/theme/` 与 `src/app/` 中与渲染相关的接口（`App::theme`、`App::render`、`ui::hit`）；对应版本：typewords-tui 0.1.0。

## 1. 职责与文件清单

| 文件 | 内容 |
| --- | --- |
| `src/ui/mod.rs` | 渲染入口 `render`：终端过小检查、背景、按 `Screen` 分发、DIM 遮罩、三种弹层的顺序 |
| `src/ui/theme.rs` | `Theme`（13 个 token 映射为 ratatui `Color`）与 `Theme::from_spec` |
| `src/ui/shell.rs` | 外壳：响应式侧栏/顶栏/底栏、`Nav`、`ShellLayout`/`SidebarLayout`、颜色提示文案 |
| `src/ui/home.rs` | 主页 dashboard 与 `targets` |
| `src/ui/practice.rs` | 练习页（大字、音标、卡片、阶段条、词表侧栏）与 `PracticeLayout` |
| `src/ui/bigtext.rs` | 5×7 点阵字体、1×/2× 放大、遮罩占位、折行 |
| `src/ui/dict_picker.rs`、`stats.rs`、`settings.rs`、`help.rs`、`shortcuts.rs`、`word_test.rs`、`words.rs`、`articles.rs`、`flows.rs` | 词库页、统计页、设置页、帮助页、快捷键编辑页、单词测试页、词条管理页、文章背诵页、练习流程页 |
| `src/ui/overlays.rs` | 结算/输入/确认/跳组/收藏目标弹层的布局、绘制、命中 |
| `src/ui/widgets.rs` | 通用文本与小部件工具（宽计算、截断、折行、按钮、进度条、弹层边框） |
| `src/ui/target.rs`、`hit.rs` | `Target`（Rect + `Action`）与命中检测 `targets`/`action_at`/`wheel_action` |
| `src/ui/insights.rs` | 主页/统计页使用的纯数据聚合（总量、正确率、本周、最近 N 天、预计完成日期） |

边界：`ui` 不修改 `App`；颜色只来自 `ui::Theme`，页面与部件中不出现硬编码的 RGB/命名色（`Color::Reset` 仅用于 `BigLetter` 的默认占位）。主题的「含义、加载、校验、回退」在 `src/theme/`，见 [主题系统](themes.md)；本层只做 token → `Color` 的转换。

## 2. 渲染管线

`App::render` 只是 `ui::render(self, frame)` 的转发；每帧流程固定：

```text
ui::render(app, frame)
  ├─ area = frame.area(); theme = app.theme()
  ├─ area.width < 60 || area.height < 20
  │    └─ shell::render_too_small（「终端太小，请放大窗口」/「最小尺寸 60 × 20」）并返回
  ├─ 用 Block(bg = theme.bg, fg = theme.fg) 铺满 area
  ├─ match app.screen
  │    Home → home::render_home      Practice → practice::render_practice
  │    DictPicker / Stats / Settings / Help → 对应 render
  ├─ modal_open = summary.is_some() || edit.is_some() || confirm.is_some()
  │    └─ 为 buffer 中每个 cell 加 Modifier::DIM（终端无 alpha，用 DIM 模拟遮罩）
  └─ 依次渲染 summary → edit → confirm（Clear + panel 背景恢复高亮）
```

- 练习页在 `session` 为空时直接返回，画面上只有背景色；正常流程不会出现这个状态。
- 主题在每帧开头由 `App::theme()` 解析一次；13 色的小结构体让这层开销可忽略。
- 三种弹层都有独立布局；即使出现异常的多弹层叠加，渲染顺序也是 summary → edit → confirm（后画的在上）。
- 渲染只读 `App` 的公开字段。没有任何 `render` 函数会写回状态或落盘。

## 3. 主题映射

`ui::theme::Theme` 的 13 个字段与 `theme::ThemeColors` 一一对应，`Theme::from_spec` 把它们转换成 ratatui 的颜色：

| `ThemeColor` | ratatui `Color` |
| --- | --- |
| `ThemeColor::Rgb(Rgb { r, g, b })` | `Color::Rgb(r, g, b)` |
| `ThemeColor::Ansi(AnsiColor::*)` | 同名 `Color`（`Reset`、`Black`、`DarkGray`、`Red`、`LightRed` … `White`） |

各 token 在界面中的用法（主题语义与自定义格式见 [主题系统](themes.md)）：

| token | 主要用途 |
| --- | --- |
| `bg` / `panel` / `element` | 页面背景 / 卡片与弹层背景 / 数字卡与次级按钮背景 |
| `border` | 分隔线、卡片与弹层边框 |
| `fg` / `muted` / `dim` | 正文 / 次要文字 / 提示与禁用态 |
| `accent` / `accent_alt` | 主强调（当前页、主按钮、待输入字母）/ 次强调（数字、进度、按键提示） |
| `warn` / `good` / `bad` | 警告 / 正确与成功 / 错误与危险 |
| `selection` | 列表选中行、统计页「今天」行 |

分工：`src/theme/` 的 `ThemeRegistry` 负责 id、`base` 继承、用户主题加载、`auto` 映射与「未知主题回退深色」告警；`ui/theme.rs` 只做类型转换，并有单测断言默认深色主题的 `bg = Rgb(0x17, 0x17, 0x20)`、`accent = Rgb(0xc9, 0x8e, 0xc7)`。

## 4. 响应式外壳

### 4.1 尺寸规则

`shell::layout(area, active)` 只根据 `area` 计算，`active: Nav` 参数实际不参与几何计算（只用于渲染高亮）。两处（渲染与鼠标命中）都调用它，因此按钮位置不会漂移。

| 条件 | 结果 |
| --- | --- |
| `area.width < 100` | 窄屏：无侧栏，`main = area`，顶栏显示 `Type Words` 与 5 个可点击标签 |
| `100 <= area.width < 120` | 宽屏：侧栏 24 列 |
| `area.width >= 120` | 宽屏：侧栏 26 列 |
| 主区行分配 | 顶栏 `main.y`、分隔线 `main.y+1`、正文区从 `main.y+3` 到 `footer.y` 前 |
| 正文内容区 | `body.x+2` 起、宽度减 4；即所有页面共享的左右各 2 列留白 |

`ShellLayout` 字段：`sidebar: Option<SidebarLayout>`、`main`、`topbar`、`rule`、`body`、`footer`、`content`、`nav_targets: Vec<(Screen, Rect)>`、`quit_target`、`narrow`。`SidebarLayout` 字段：`area`、`wordmark`、`wordmark_rows`、`subtitle`、`top_nav`、`bottom_rule`、`version`、`settings`、`quit`。

### 4.2 侧栏

- `TW` 大字标识：内宽 `width-4`，当 `内宽 >= 22 && 高度 >= 28` 时用 2× 点阵（7 行），否则 1×（4 行）；下面最多一行 `Type Words` 副标题（放得下才画）。
- 导航项 `Nav::MAIN`（单词 / 词库 / 统计 / 帮助）每个占 2 行，从副标题下 2 行开始；与底部保留区冲突时停止追加。
- 底部固定 5 行：分隔线 `height-5`、版本号 `height-4`、设置 `height-3`、退出 `height-2`，左侧有一条 1 列宽的分隔边（`Borders::RIGHT`）。
- 选中项用 `selection` 铺背景、左侧 1 列 `accent`，标签用 `fg`/`accent_alt`，未选中用 `muted`/`dim`。

`Nav` 枚举：`Home`（单词，1）、`Dicts`（词库，2）、`Stats`（统计，3）、`Settings`（设置，4）、`Help`（帮助，5）。`Nav::ALL` 是 5 项（窄屏顶栏用），`Nav::MAIN` 是除 Settings 外的 4 项（宽屏侧栏的导航区用，Settings 固定在底部）；`Nav::from_screen` 把 `Practice` 映射为 `Home`。

### 4.3 顶栏与底栏

- 宽屏顶栏：`▎` + 当前页标题（bold）；右侧空间足够时再画 `模式 X  ·  主题名`（`practice_mode_name` + `theme_label`），放不下就省略。
- 窄屏顶栏：左侧 `Type Words`，右侧 5 个标签；当前页用 `element` 背景 + `fg`，其余 `muted`。`narrow_nav_targets` 从 `Type Words` 右侧 2 列开始排布，宽度为标签显示宽度 + 2，放不下的标签不生成。
- 底栏左侧：有 Toast 时显示最新一条（Success/Warning/Error/Info 分别用 `good`/`warn`/`bad`/`accent_alt`）；无 Toast 时显示当前页快捷键提示（`hints_for(screen)`）。主页在提示区不足 52 列时改用 `HOME_COMPACT_HINTS`（enter 开始、f 自由、d 词库、? 帮助、q 退出）。右侧是版本号 `v0.1.0`（`CARGO_PKG_VERSION`）。
- 终端过小：`render_too_small` 用 `modal_block` 画一个居中提示，不渲染任何页面内容。

## 5. 主页

主页是纵向 dashboard，方块之间留 1 行间隔。`home::layout(body)` 先按正文高度选一档行高，再决定后两段是否显示：

| 正文高度 `body.height` | hero | stats | dicts | recs |
| --- | --- | --- | --- | --- |
| `>= 34` | 11 | 7 | 7 | 6 |
| `>= 28` | 10 | 6 | 6 | 5 |
| `>= 23` | 9 | 5 | 5 | 4 |
| 其余 | 8 | 5 | 4 | 3 |

显示条件：`show_dicts = height >= hero+1+stats+1+dicts`；`show_recs = show_dicts && height >= 上述条件再加 1+recs`；最后用 `Constraint::Min(0)` 收尾。窗口高度不足时先收起「推荐」、再收起「我的词典」（60×20 只剩 hero 与 stats）。宽屏截图见 [主页 120×40](screenshots/home-120x40-dark.png)，紧凑布局见 [主页 90×30](screenshots/home-90x30-dark.png)，配色见 [浅色](screenshots/home-120x40-light.png) / [高对比](screenshots/home-120x40-high-contrast.png)。

### 5.1 hero 卡（当前词库）

- 外框 `panel` 背景、左侧 1 列 `accent` 竖条，中间 1 列 `border` 分隔左右两栏（58% / 1 列 / `Min(18)`）。
- 左栏：词库名（bold）、预计完成日期（`insights::estimated_finish`；已学完为 `已学完 · 总复习阶段`；未选词库为「按 d 选择词库」）、细线进度条 + 百分比（`thin_bar_spans`，完成用 `good` 否则 `accent_alt`）、「已学 N / M 词」。
- 底部按钮：主按钮宽 `min(22, 左栏宽)`，文案按 `last_learn_index > 0 || complete` 选择「Enter 继续学习 →」或「Enter 开始学习 →」；`f 自由练习` 按钮宽 13，放得下才出现。
- 右栏「今日任务」：`◆`（`warn`）+ 标题；每日目标行 `[-] N [+] `，右栏宽度 `< 24` 时用短文案「目标 [-] N [+]」；下面两块数字卡：新词、复习，复习卡在 `review == 0 && due == 0` 时补「无到期」小字。
- 目标按钮的命中矩形与实际绘制位置一致：普通布局在 `hero_right.x+9` 与 `+21`（各 3 列宽），短布局在 `+5` 与 `+14`。

### 5.2 统计与本周记录

- 标题「统计」，有本周区块时右侧写「本周学习记录」；内容宽度 `>= 60` 时左 58%（`max 24`）放 3 块数字卡，右侧放本周 7 列；否则只放数字卡。
- 3 块数字卡：今日学习时长（`today_spend`）、总学习天数、总学习时长（`insights::totals`）；卡宽 `< 15` 时用短标签「今日时长 / 总天数 / 总时长」。
- 本周记录：周一至周日 7 列，今天用 `accent` 背景；有记录的日子日期用 `accent_alt` 并画 `●`，无记录用 `dim` 与 `·`；行高不足时省略圆点行。

### 5.3 我的词典与推荐

- 「我的词典」：标题行右侧「d 词库」；卡片数量 `max_tiles = (tiles.width / 14).clamp(3, 5)`。当前学习词典如果排在第 `max_tiles` 位之后，会与第一张卡交换，避免被系统词典挤出视野；卡片不足时追加「+ / d 管理」卡。
- 每张词库卡：名称（bold）、徽标（`内置` / `当前` / `词库`）与「N 词」；词数取 `effective_length()`。
- 「推荐」：从 `catalog.resources()` 过滤掉当前词典与 `book_list` 已拥有的 id/enName，按行显示 名称（截断 18 列）/ 简介（22 列）/ 词数；行数由区块高度决定；无推荐时显示「暂无推荐 · d 浏览词库」。
- `targets`：主按钮 `StartPractice`、自由按钮 `StartFreePractice`、目标 `AdjustDaily(±5)`、「d 词库」「d 更多」→ `Navigate(DictPicker)`。标题右侧文字本身是 `section_header` 画出来的，命中矩形由 layout 预留（`open_dicts`/`open_more`）。

## 6. 练习页

练习页是唯一不套外壳的页面，全屏布局（不画侧栏/顶栏/底栏），`practice::render_practice` 在无 `session` 时直接返回。

### 6.1 布局层次

```text
┌─ 顶部状态行（1 行）：词库 · 阶段 · 进度条 16 · index/total · 错词 N · 计时 ─┐
│                                                                          │
│  [Ctrl+L] 右侧词表 28 列 │  音标                                      │
│                          │  大字单词（点阵，最多 7 行/2×）              │
│                          │  提示（空格继续 / 开始拼写…）                │
│                          │  可选：上/下一个词                           │
│                          │  ┌ 释义 ────────────────────────────────┐   │
│                          │  ┌ 例句 ────────────────────────────────┐   │
│                          │  ┌ 词源 ────────────────────────────────┐   │
│                                                                          │
└─ 底部阶段条（6 行；Ctrl+B 收起为 1 行 Toast）────────────────────────────┘
```

- 垂直约束：`[1, Min(10), 6]`（显示阶段条）或 `[1, Min(10), 1]`（收起，只画 Toast）。
- `Ctrl+L` 打开词表后，正文改为水平 `[Min(30), 28]`，右侧 28 列渲染当前节点词表。
- 正文字宽 `inner_width = body.width - 4`；卡片与大字都按它折行/放大。

### 6.2 顶部状态行

词库名（`accent` bold）→ `· 阶段`（`display_state` 的 `status`）→ 16 格实心进度条（`█`/`░`，`good`/`dim`）→ `index/total` → `错词 N`（有错词用 `bad`，否则 `dim`）→ 计时（暂停显示「已暂停」并用 `warn`）。`index` 取 `data.index + 1` 并夹在 `total` 内。

### 6.3 大字单词的显隐与颜色

显隐由 `session.view_state()` 返回的 `PracticeViewState`（`practice_type`、`is_word_masked`、`is_show_translate`、`reveal_all`）与 `session.display.show_word_result` 共同决定；`word_lines` 逐字符生成 `BigLetter`（字符检查基于 `app.typing.input`）：

| 状态 | 颜色 |
| --- | --- |
| 已输入且匹配（`ignore_case` 生效） | `good` |
| 已输入但不匹配 | `bad` |
| 待输入且当前处于遮罩/默写/自测未揭示 | `dim`（空心方框占位） |
| 下一个待输入字符 | `accent` |
| 其余未输入字符 | `fg` |

自测阶段尚未揭示时（`Identify && !show_word_result && !reveal_all`）整词按遮罩处理；`view.is_word_masked` 为真时同理。点阵不支持该词（非 ASCII、重音字母等）时回退为普通文本：逐字符输出、已输入字符加粗、遮罩位输出下划线、末尾补空格。

大字是否 2× 由「本轮最长单词」统一决定，避免同一轮字号忽大忽小：`longest = max(session.data.words 的字符数)`、`max_letters = (inner_width + 2) / 12`、`use_big = tui_big_word && longest > 0 && longest <= max_letters && body.height >= 20`；`tui_big_word` 关闭时仍用 1× 点阵（不是关掉大字字体）。随后 `word_height = (big.len() + 5).min(body.height - 6).max(6)`，卡片高度 `min(卡片需求, body.height - word_height - 1).max(3)`，两者在剩余空间里垂直居中（`Flex::Center`）。

大字上方的音标取 `word.phonetic(sound_type == "uk")`（英式取 `phonetic0`，美式取 `phonetic1`）；`show_near_word` 打开且未遮罩时，下方追加「← 上一个词   下一个词 →」。

### 6.4 信息卡片

`card_specs` 决定显示哪些卡片与行数上限；卡片是圆角边框（`panel` 背景 + `border` + `dim` 标题），内容按宽度折行，超出容量的最后一行以 `…` 结尾。

| 卡片 | 显示条件 | 内容与上限 |
| --- | --- | --- |
| 释义 | `view.is_show_translate && !word.trans.is_empty()` | 所有 `pos + cn` 用「；」拼接，最多 4 行 |
| 例句 | `show_meta && !word.sentences.is_empty()` | 取前 2 句，英文 + 中文各一行（中文用 `dim`），最多 4 行 |
| 词源 | `show_meta && settings.show_etymology_and_rel_words` | 第一条词源「t：d」+ 词根（`dim`），最多 3 行 |

自测未揭示时改为「选择答案」卡：`show_word_question` 开启时逐行列出 A~D 选项（`pos+cn` 前两条拼接，选中后追加正确单词）；选中的选项在揭示后用 `good`/`bad` 着色，未选中用 `fg`。关闭该设置时显示一行提示「1 我认识    2 不认识    3 已掌握    或直接拼写」。

### 6.5 底部阶段条与词表

- 阶段条：每个 `FlowProgressStage` 渲染「阶段名 + 8 格进度条（`bar_spans`）+ 百分比」；进行中用 `accent`、已完成用 `good`、其它用 `dim`。下面两行固定快捷键提示；区域高度 `>= 6` 时再追加 Toast 行；整体包在标题「阶段」的卡片里。
- `Ctrl+B` 收起后，底部只保留 1 行 `shell::toast_line`（有 Toast 显示 Toast；否则按「显示使用提示」设置决定是否显示「提示：Ctrl+C 退出练习并保存；? 查看快捷键」，关闭时该行为空）。
- `Ctrl+L` 词表：`▶` 当前词（`accent`）、错词（`bad`）、已排除词（`dim`）、其它（`fg`），每行「序号. 单词」。
- `Ctrl+E` 只影响 `show_meta`，即例句与词源两块卡片。
- 练习页本身不产生鼠标 target：页面上点击无反应；`Ctrl+L`/`Ctrl+B`/`Ctrl+E` 是键位状态，不落盘。结算弹层打开后由弹层提供按钮 target。
- 截图见 [练习页 120×40](screenshots/practice-120x40-dark.png)。

## 7. 大字点阵（bigtext.rs）

### 7.1 字形与编码

- 每个字形是 `5 × 7` 的 `#`/`.` 点阵（`GLYPH_WIDTH = 5`、`GLYPH_HEIGHT = 7`），覆盖 `A-Z`、`0-9` 与 `- ' . , / ? : ! ( )` 和空格；输入先 `to_ascii_uppercase()`，所以小写单词可用。没有字形的字符（非 ASCII、重音字母等）让 `render_word` 返回 `None`，由调用方回退普通文本。
- 两个纵向像素合并进一个终端单元：

| 上像素 | 下像素 | 字符 |
| --- | --- | --- |
| 实 | 实 | `█` |
| 实 | 空 | `▀` |
| 空 | 实 | `▄` |
| 空 | 空 | 空格 |

### 7.2 字号、压缩与拆行

- 1×：7 行像素压成 4 行终端文本（`ceil(7/2)`），字母间距 1 列；2×：像素级放大后 14 行压成 7 行，字母间距 2 列。单词期望宽度 = `n × 5 × scale + (n-1) × spacing`。
- 选择顺序：先尝试 2×（需同时满足宽度与 `max_height`），否则 1×（`render_word` 始终允许 1×，否则极小窗口无法显示）；1× 仍超宽时先把字距压到 0，再按每行 `max(max_width / 6, 1)` 个字母拆成多行。
- 遮罩：`BigLetter.masked` 为真时用空心方框占位（`PLACEHOLDER`），只表达「这里有一个字母」，不泄露字形；`BigLetter.color` 控制该字母颜色。
- 内部先由 `push_chunk_rows` 生成 `Line`，每个字母是一个带颜色的 `Span`；放大是像素级的 `(row*2)/scale` 映射，所以 2× 边缘仍是整块半块字符。
- 使用方：练习页大字、侧栏 `TW` 标识。单测覆盖 4 行行数、2× 放大、遮罩空心、非 ASCII 回退、长词拆行。

### 7.3 调试与截图

- `cargo run --example dump -- <mode> [typed] [width] [height] [big|plain]`：把练习页渲染成纯文本打印，`mode` 支持 7 种练习模式（如 `cargo run --example dump -- dictation ca 120 40`），最后一个参数 `plain` 强制 1×，便于对比 1×/2× 与键入着色。
- 带样式的截图由 `examples/shot.rs` 输出 JSON、再由 `tools/render-snapshot.py` 转 PNG；生成/更新方法见 [开发指南](development.md)。

## 8. 其它页面

### 8.1 词库页（dict_picker.rs）

- 内容宽度 `>= DETAIL_MIN_WIDTH(72)` 时左右分栏：列表占 58%，之后 1 列分隔线，右侧详情；否则只画列表。
- 列表从标题下空一行开始；`scroll_offset = cursor.min(total-1).saturating_sub(visible-1)`，保证光标在可视区内。行首标记：当前词库 `●`（`accent`）、选中 `▶`（`accent_alt`）；名称后可选显示 id（`name_space >= 18 && inner_width >= 40`），右侧是 6/4 格细进度条、`N 词`、百分比；列表被裁切时最右列画 `↑`/`↓`。
- 详情面板：名称（当前带 `●`）、id · 分类、简介折行、标签、词数、进度条、每日词数说明（当前/已拥有/未拥有三种文案），底部「Enter 设为当前」按钮（内宽 `>= 12` 且内高 `>= 2` 才画）。
- `targets`：每个可见行一个 target——光标所在行是 `ActivateDict(index)`，其它行是 `SelectDict(index)`（第一次点击选中、再次点击激活）；按钮是 `ActivateDict(dict_cursor)`。截图见 [词库页](screenshots/dict-120x40-dark.png)。

### 8.2 统计页（stats.rs）

- 6 块总览卡：累计时长、学习天数、新词、复习、错误、正确率；内容宽度 `>= 90` 时 6 列 × 1 行（卡高 3，带「今日 N」小字），否则 3 列 × 2 行（卡高 2，无小字行）。
- 「最近 14 天」：`insights::recent_days(base, 14)` 按日期倒序；每行「MM-DD 周x / 今天 + 相对最长时长的小进度条（最多 24 格）+ 时长 + 新 N 复 N + 错 N」；今天整行用 `selection` 背景。记录多于一屏时列表少画一行并给出 `↑`/`↓` 提示；行宽不足 28 列时跳过明细。
- `app.stats_scroll` 在渲染时被夹到 `total - visible`，不会越界；本页不产生鼠标 target，只吃滚轮。截图见 [统计页](screenshots/stats-120x40-dark.png)。

### 8.3 设置页（settings.rs）

- 6 个分区标题（通用/复习/键入/显示/FSRS/其他）+ 24 行设置项 = 30 行；分区标题是「▎ 标题 ────」样式的整行。`layout` 先保证光标行可见，再尽量把光标所在分区的标题一起滚入视口；顶部/底部溢出时在内容区最右列画 `↑`/`↓`。
- 每行「标签 + 右对齐当前值」：选中行 `selection` 背景 + 左侧 1 列 `accent`；值的颜色按类型区分（模式/主题/音标 `accent_alt`，布尔开 `good` 关 `muted`，数字 `fg`，「按 Enter 重置」`dim`）。数字值的格式化：比例裸数字、重复「N 次」、等待/冷却「N ms」、FSRS 阈值裸数字、留存率「N%」。
- 数字项按 `Enter` 后不就地编辑，而是打开 `ui/overlays.rs` 的输入弹层（见第 10 节）；设置页布局本身不变。
- `targets`：只给设置项行生成（分区标题不可点）；光标行 `ActivateSetting(index)`，其它行 `SelectSetting(index)`。截图见 [设置页](screenshots/settings-120x40-dark.png)。

### 8.4 帮助页（help.rs）

- 内容宽度 `>= 78` 且高度 `>= 20` 时两栏：左「练习」、右「导航/查看」，中间 1 列 `│` 分隔线 + 1 列留白；左栏过窄（`< 26`）时回退单栏。
- 单栏把两个分区顺序拼接并追加鼠标说明（「鼠标：侧栏/按钮/列表可点击，滚轮滚动；Shift+拖拽选择文本」按 3 条短行渲染），`help_scroll` 被夹到最大偏移，右侧画 `↑`/`↓`。
- 条目三种：分区标题、按键（`key_hint`：按键 `accent_alt` bold + 说明 `muted`）、说明（`dim`，按宽度折行并缩进 1 列）。按键内容以帮助页常量表为准，键位语义见 [应用层 · 输入路由](app.md#5-输入路由)。
- `targets`：整块内容区 → `HelpBack`，因此点击页面任意处都能返回。窄屏可滚动，截图见 [帮助页](screenshots/help-120x40-dark.png)。

## 9. 通用部件（widgets.rs）

| API | 作用 |
| --- | --- |
| `display_width(text)` | 基于 `unicode-width` 的显示宽度（CJK 占 2 列） |
| `fit_text(text, max)` | 按显示宽度截断，末位补 `…`；把换行替换为空格；不会截出半个 CJK |
| `wrap_text(text, width)` | 按显示宽度折行，保留原文换行；返回至少一行 |
| `rule(width, theme)` / `section_header(title, right, width, theme)` | 分隔线 / 左标题 + 右操作文案（中间补空格） |
| `key_hint(key, label, theme)` / `footer_hints(hints, width, theme)` | 单条按键提示 / 底栏提示序列；超宽时从尾部丢弃并以 `…` 结束 |
| `bar_spans(percent, width, filled, empty)` / `thin_bar_spans(...)` | 实心条（`█`/`░`）/ 细线进度条（`━`/`─`） |
| `centered_rect(width, height, area)` | 居中矩形，尺寸先夹到 `area`，再按剩余空间左右/上下居中 |
| `modal_block(title, theme)` / `paint_rect(frame, rect, color)` / `draw_button(...)` | 弹层圆角边框 / 纯色块 / 居中按钮 |

约定：所有文本宽度计算都走 `display_width`，不要用 `chars().count()`（CJK 与全角符号会错位）；新页面优先复用 `section_header`、`footer_hints`、`modal_block`、`centered_rect`，避免各页各写一套截断逻辑。`fit_text`/`wrap_text`/`footer_hints` 有单测覆盖 0 宽、CJK、不越界等边界。

## 10. 弹层（overlays.rs）

| 弹层 | 布局 | 内容与按钮 |
| --- | --- | --- |
| 结算 `summary` | 居中 `64 × 14`；内区左右各缩 2、上下各缩 1；按钮行在内区最后一行 | 标题「结算」，`今日任务完成` / `学习报告`、学习时长与正确率、新词/复习/错词、学习进度、细进度条；按钮「Enter 继续」（14 列）、「r 重学」（11 列）、「q 主页」（10 列） |
| 输入 `edit` | 居中 `46 × 5` | 标题「输入」；文案「修改 {设置名}（Enter 保存 · Esc 取消）」+「> {buffer}_」 |
| 确认 `confirm` | 居中 `56 × 7`；内区左右各缩 2、上下各缩 1 | 标题「确认」；消息（`fit_text`）、空行、「Enter/y 确认 · 其他键取消」；按钮「确认」「取消」（各 10 列） |

- `centered_rect` 先把宽高夹到 `area`，所以最小终端 60×20 下弹层不会越界（单测 `summary_layout_fits_min_terminal`、`confirm_buttons_are_inside_modal` 覆盖）。
- 遮罩：`ui::render` 在弹层打开时给全 buffer 加 `Modifier::DIM`；弹层绘制前用 `Clear` + `theme.bg` 清掉弹层外扩 2 列/行的背景，再用 `theme.panel` 画自身，从而在无 alpha 的终端里「压暗背景、恢复前景」。
- 鼠标：`overlays::targets` 为确认弹层生成「确认/取消」、为结算弹层生成三个按钮（输入弹层没有按钮 target）；`target_action` 的额外规则：确认弹层点击按钮外但在框内返回 `None`（不取消）、点击框外返回 `ConfirmCancel`；输入弹层点击框外返回 `EditCancel`、框内返回 `None`；结算弹层只响应三个按钮，框外不动。结算截图见 [结算弹层](screenshots/summary-120x40-dark.png)。

## 11. 鼠标命中

- `Target { rect: Rect, action: Action }`；`contains(column, row)` 使用半开区间 `[x, right) × [y, bottom)`。
- `hit::targets` 的收集顺序（也是命中优先级）：
  1. 有弹层时只返回 `overlays::targets`；
  2. 外壳：`session == None` 时收集侧栏/顶栏导航 target，以及（宽屏）`quit_target`；
  3. 页面 target：`Home`、`DictPicker`、`Settings` 的页面内按钮/列表；`Help` 整块内容区 → `HelpBack`；`Practice` 与 `Stats` 没有页面 target。
- `action_at` 用 `find` 返回第一个包含坐标的 target；因此导航优先于页面内按钮，页面内又按 `targets()` 的 push 顺序（例如主页：开始按钮 → 自由练习 → 目标 +/- → d 词库 → d 更多）。
- `wheel_action`：有弹层返回 `None`；词库页 → `DictMove(delta)`、设置页 → `SettingsMove(delta)`、统计页 → `StatsScroll(delta)`、帮助页 → `HelpScroll(delta)`、其它页面 `None`；`delta` 由滚轮方向给出（上 `-1`、下 `1`）。
- 不变量：每个页面只写一份 `layout`/`layout_xxx`，`render` 与 `targets` 都从它取 `Rect`；滚动偏移来自 `App` 的同一字段。这样绘制位置与可点击位置不会漂移。相关测试：`tests/app_drive.rs` 用 `hit::targets` 找按钮再模拟点击；`tests/ui_snapshots.rs` 的 `mouse_target_rects_match_rendered_text` 校验命中行与渲染文本一致；弹层布局单测保证按钮在弹层内。

## 12. 渲染测试与新增页面检查清单

### 12.1 现有渲染测试

`tests/ui_snapshots.rs` 用 ratatui `TestBackend` 渲染 `App`，断言关键文本、颜色与不 panic（不是逐像素快照）：

| 测试 | 覆盖点 |
| --- | --- |
| `home_screen_renders` / `home_uses_theme_surfaces` / `home_compact_renders_at_min_size` | 主页文本、`bg`/`panel`/`accent_alt` 表面、60×20 紧凑布局 |
| `practice_screen_renders_big_word` / `masked_mode_renders_placeholder_blocks` / `typed_letters_are_colored` | 大字方块字符、默写占位符、正确输入用 `theme.good` 着色 |
| `all_screens_render_without_panic` / `cjk_text_wraps_within_width` | 4 个非主页屏幕在 80×24 与 120×40 下渲染、逐行不超出宽度 |
| `all_builtin_themes_render_home` / `tiny_terminal_shows_hint` / `overlay_screens_render` / `usage_tips_can_be_hidden` | 三套内置主题背景、`< 60 × 20` 提示、三种弹层文本、关闭「显示使用提示」后底部提示行消失 |
| `mouse_target_rects_match_rendered_text` / `help_scrolls_at_min_size` | 命中矩形与渲染文本一致、帮助页在小终端可滚动 |

`examples/shot.rs` 用同一套 `TestBackend` 输出带样式的 JSON（cell 的字符/前景/背景/修饰/宽度），再由 `tools/render-snapshot.py` 生成 PNG；`shot` 支持 `home / practice / dict / stats / settings / help / summary / edit / confirm` 与自定义宽高、主题。截图文件： [主页](screenshots/home-120x40-dark.png)、[主页 90×30](screenshots/home-90x30-dark.png)、[主页浅色](screenshots/home-120x40-light.png)、[主页高对比](screenshots/home-120x40-high-contrast.png)、[练习](screenshots/practice-120x40-dark.png)、[词库](screenshots/dict-120x40-dark.png)、[统计](screenshots/stats-120x40-dark.png)、[设置](screenshots/settings-120x40-dark.png)、[帮助](screenshots/help-120x40-dark.png)、[结算](screenshots/summary-120x40-dark.png)。生成命令与字体要求见 [开发指南](development.md)。

### 12.2 新增/修改页面的检查清单

1. 写一个只依赖 `area` 与 `App` 的 `layout` 结构（或 `layout_xxx` 函数），让 `render` 与 `targets` 共用；所有坐标都从它来。
2. `render` 只读 `App` 与 `theme`，取色只用 `Theme` 字段，文本宽度只用 `display_width`/fit/wrap。
3. 在 `ui/mod.rs` 的 `Screen` 分发与 `ui/hit.rs` 的 `targets`/`wheel_action` 里注册；可点击控件补 `Target`。
4. 在 `tests/ui_snapshots.rs` 增加关键文本/颜色断言（至少 80×24 与 120×40 各一次），涉及鼠标时补 `Target` 命中测试；在 `tests/app_drive.rs` 补行为测试（如果新增了 `Action`）。
5. 用 `cargo run --example shot -- <页面> 120 40 dark` 重新生成截图并放入 `docs/screenshots/`（命名 `<页面>-<宽>x<高>-<主题>.png`）。
6. 更新 [应用层 · 扩展指南](app.md#11-扩展指南)、本文件相关章节与 [文档索引](README.md)。

## 相关文档

- [文档索引](README.md)
- [应用层](app.md)
- [主题系统](themes.md)
- [开发指南](development.md)
- [总体架构](architecture.md)
