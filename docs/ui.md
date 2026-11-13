# 渲染层（ui）

渲染层负责把只读的 `App` 状态画到 ratatui 的 `Frame` 上：布局、颜色、文本折行、大字点阵，以及「屏幕上的哪块区域对应哪个鼠标动作」。它不修改 `App`、不写存档、不推进会话；所有数据都由应用层准备好，所有鼠标交互都通过 `ui::hit` 的 `Target` 映射回应用层。

> 适用范围：`src/ui/` 全部文件，以及 `src/theme/` 与 `src/app/` 中与渲染相关的接口（`App::theme`、`App::render`、`ui::hit`）；对应版本：typewords-tui 0.2.0。

## 1. 职责与文件清单

| 文件 | 内容 |
| --- | --- |
| `src/ui/mod.rs` | 渲染入口 `render`：终端过小检查、背景、按 `Screen` 分发、DIM 遮罩、七种弹层的顺序 |
| `src/ui/theme.rs` | `Theme`（13 个 token 映射为 ratatui `Color`）与 `Theme::from_spec` |
| `src/ui/shell.rs` | 外壳：响应式侧栏/顶栏/底栏、`Nav`、`ShellLayout`/`SidebarLayout`、颜色提示文案 |
| `src/ui/home.rs` | 主页 dashboard、`primary_label` 与 `targets`（鼠标命中 + 键盘焦点的稳定身份） |
| `src/ui/focus.rs` | 所有套外壳页面的键盘焦点：焦点身份（`FocusId` / `NavId` / `ContentId` / `Page`）、支持页面与启用条件（`supports_focus` / `enabled`）、`Pane` 两栏与 `NavOrientation` 主轴、主页内容区视觉分带（`content_move`）、导航栏步进（`nav_move`）、底栏标签（`focused_label` / `page_label`）、导航栏与主页的整块高亮 `paint` |
| `src/ui/practice.rs` | 练习页（大字、音标、卡片、阶段条、词表侧栏）与 `PracticeLayout` |
| `src/ui/bigtext.rs` | 5×7 点阵字体、1×/2× 放大、遮罩占位、折行 |
| `src/ui/dict_picker.rs`、`stats.rs`、`settings.rs`、`help.rs`、`shortcuts.rs`、`word_test.rs`、`words.rs`、`articles.rs`、`flows.rs` | 词库页、统计页、设置页、帮助页、快捷键编辑页、单词测试页、词条管理页、文章背诵页、练习流程页 |
| `src/ui/overlays.rs` | 结算/输入/确认/跳组/收藏目标弹层的布局、绘制、命中 |
| `src/ui/widgets.rs` | 通用文本与小部件工具（宽计算、截断、折行、按钮、进度条、弹层边框） |
| `src/ui/target.rs`、`hit.rs` | `Target`（Rect + `Action` + `label` + 可选 `FocusId`）与命中检测 `targets`/`action_at`/`wheel_action` |
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
  │    WordTest / Words / Articles / ArticlePractice / Flows
  │    DictPicker / Stats / Settings / Shortcuts / Help → 对应 render
  ├─ modal_open = summary || edit || confirm || word_edit
  │            || mode_picker || group_picker || collect_picker
  │    └─ 为 buffer 中每个 cell 加 Modifier::DIM（终端无 alpha，用 DIM 模拟遮罩）
  └─ 依次渲染 summary → edit → confirm → word_edit
     → mode_picker → group_picker → collect_picker（Clear + panel 背景恢复高亮）
```

- 练习页在 `session` 为空时直接返回，画面上只有背景色；正常流程不会出现这个状态。
- 主题在每帧开头由 `App::theme()` 解析一次；13 色的小结构体让这层开销可忽略。
- 七种弹层都有独立布局；即使出现异常的多弹层叠加，渲染顺序也是 summary → edit → confirm → word_edit → mode_picker → group_picker → collect_picker（后画的在上）。
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

分工：`src/theme/` 的 `ThemeRegistry` 负责 id、`base` 继承、用户主题加载、`auto` 映射与「未知主题回退深色」告警；`ui/theme.rs` 只做类型转换，并有单测断言默认深色主题的 `bg = Rgb(0x0a, 0x0a, 0x0a)`、`accent = Rgb(0xfa, 0xb2, 0x83)`。

## 4. 响应式外壳

### 4.1 尺寸规则

`shell::layout(area, active)` 只根据 `area` 计算，`active: Nav` 参数实际不参与几何计算（只用于渲染高亮）。两处（渲染与鼠标命中）都调用它，因此按钮位置不会漂移。

| 条件 | 结果 |
| --- | --- |
| `area.width < 100` | 窄屏：无侧栏，`main = area`，顶栏显示 `Type Words` 与 6 个可点击标签 |
| `100 <= area.width < 120` | 宽屏：侧栏 24 列 |
| `area.width >= 120` | 宽屏：侧栏 26 列 |
| 主区行分配 | 顶栏 `main.y`、分隔线 `main.y+1`、正文区从 `main.y+3` 到 `footer.y` 前 |
| 正文内容区 | `body.x+2` 起、宽度减 4；即所有页面共享的左右各 2 列留白 |

`ShellLayout` 字段：`sidebar: Option<SidebarLayout>`、`area`（整屏区域，键盘焦点据此重算主页布局）、`main`、`topbar`、`rule`、`body`、`footer`、`content`、`nav_targets: Vec<(Screen, Rect)>`、`quit_target`、`narrow`。`SidebarLayout` 字段：`area`、`wordmark`、`wordmark_rows`、`subtitle`、`top_nav`、`bottom_rule`、`version`、`settings`、`quit`。

### 4.2 侧栏

- `TW` 大字标识：内宽 `width-4`，当 `内宽 >= 22 && 高度 >= 28` 时用 2× 点阵（7 行），否则 1×（4 行）；下面最多一行 `Type Words` 副标题（放得下才画）。
- 导航项 `Nav::MAIN`（单词 / 文章 / 词库 / 统计 / 帮助）每个占 **1 行**（命中区与绘制行一致），从副标题下 2 行开始；与底部保留区冲突时停止追加。早期命中区占 2 行，键盘选中时高亮会盖住相邻两项。
- 底部固定 5 行：分隔线 `height-5`、版本号 `height-4`、设置 `height-3`、退出 `height-2`，左侧有一条 1 列宽的分隔边（`Borders::RIGHT`）。导航栏的键盘焦点走的就是这批矩形：`shell.nav_targets`（`Nav::MAIN` + 设置）加上 `quit_target`（退出），顺序与绘制顺序一致；窄屏顶栏没有退出项。
- 选中项用 `selection` 铺背景、左侧 1 列 `accent`，标签用 `fg`/`accent_alt`，未选中用 `muted`/`dim`。

`Nav` 枚举：`Home`（单词，1）、`Articles`（文章，2）、`Dicts`（词库，3）、`Stats`（统计，4）、`Help`（帮助，5）、`Settings`（设置，6）。`Nav::ALL` 是 6 项（窄屏顶栏用，顺序即视觉顺序）；`Nav::MAIN` 是除 Settings 外的 5 项（宽屏侧栏的导航区用，Settings 固定在底部）。编号按**视觉顺序**排：设置虽然在侧栏最底部，编号却是 6，否则侧栏会显示「统计 4 / 帮助 5 / 设置 6」夹着底部的 6 号，和主导航区对不上。

`Nav::from_screen` 把 `Practice` 映射为 `Home`、`Articles` 与 `ArticlePractice` 映射为 `Articles`、`Flows` 映射为 `Home`（流程页是单词练习的附属页）。文章页必须高亮「文章」——早期映射成 `Home`，用户站在文章页却看到「单词」被选中。

### 4.3 顶栏与底栏

- 宽屏顶栏：`▎` + 当前页标题（bold）；右侧空间足够时再画 `模式 X  ·  主题名`（`practice_mode_name` + `theme_label`），放不下就省略。
- 窄屏顶栏：左侧 `Type Words`，右侧 6 个标签；当前页用 `element` 背景 + `fg`，其余 `muted`；键盘焦点落在这 6 个标签上时整块 `accent` 覆盖当前页底色。`narrow_nav_targets` 从 `Type Words` 右侧 2 列开始排布，宽度为标签显示宽度 + 2，放不下的标签不生成。
- 底栏左侧：有 Toast 时显示最新一条（Success/Warning/Error/Info 分别用 `good`/`warn`/`bad`/`accent_alt`）；无 Toast 时显示当前页快捷键提示（`hints_for(screen)`）。主页在提示区不足 52 列时改用 `HOME_COMPACT_HINTS`（enter 开始、f 自由、d 词库、? 帮助、q 退出）；任何页面有键盘焦点时优先显示 `shell::focus_hints`（「选中 X / ↑↓←→ 移动 / enter 触发或切换 / esc 取消或返回 / tab 换栏」；label 由 `ui::focus::focused_label` 按导航栏 / 主页按钮 / 页面光标给出）。右侧是版本号 `v0.2.0`（`CARGO_PKG_VERSION`）。
- 终端过小：`render_too_small` 用 `modal_block` 画一个居中提示，不渲染任何页面内容；`App::interactive()` 同一判定（`ui::MIN_WIDTH` / `MIN_HEIGHT`）为假，键盘与鼠标事件全部丢弃，放大窗口后自动恢复。

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
- 底部按钮：主按钮宽 `min(22, 左栏宽)`，文案取 `home::primary_label`（词典已学、已学完或有练习断点时「继续学习」，否则「开始学习」）——按钮、键盘焦点的 `Target.label` 与底栏「选中 X」共用它。主按钮常态是 `element` 底 + `accent` 文字 + 加粗，不再用 `accent` 底：否则选中主按钮与常态完全相同，看不到选中变化。进入主页时默认选中它（见 5.4），所以首帧就是整块 `accent` 高亮。`f 自由练习` 按钮宽 13、`element` 底 + `fg`，放得下才出现。
- 右栏「今日任务」：`◆`（`warn`）+ 标题；右端是「模式 X ▾」标签（`accent_alt`），点击打开模式选择弹层；每日目标行 `[-] N [+] `，右栏宽度 `< 24` 时用短文案「目标 [-] N [+]」；下面两块数字卡：新词、复习，复习卡在 `review == 0 && due == 0` 时补「无到期」小字。
- 目标按钮的命中矩形由**绘制用的同一行文本**算出：`goal_line_text` 生成文本，`goal_buttons` 在其中找 `[-]` 与 `[+]` 的字符位置，按显示宽度换算成列号。早期实现用固定偏移（`hero_right.x+9` / `+21`）估算，与实际排版差 2 列，所有终端尺寸下点 `+` 都没反应。
- 「模式 X ▾」标签的宽度按**当前模式名**算（`mode_label_text`），不能写死「学习」——写死会让「自由练习」「随机复习」「单词测试」等 6 个模式被截成「模式 随机…」，连下拉三角 `▾` 都被吃掉。

### 5.2 统计与本周记录

- 标题「统计」，有本周区块时右侧写「本周学习记录」；内容宽度 `>= 60` 时左 58%（`max 24`）放 3 块数字卡，右侧放本周 7 列；否则只放数字卡。
- 3 块数字卡：今日学习时长（`today_spend`）、总学习天数、总学习时长（`insights::totals`）；卡宽 `< 15` 时用短标签「今日时长 / 总天数 / 总时长」。
- 本周记录：周一至周日 7 列，今天用 `accent` 背景；有记录的日子日期用 `accent_alt` 并画 `●`，无记录用 `dim` 与 `·`；行高不足时省略圆点行。

### 5.3 我的词典与推荐

- 「我的词典」：标题行右侧「d 词库」；卡片数量 `max_tiles = (tiles.width / 14).clamp(3, 5)`。当前学习词典如果排在第 `max_tiles` 位之后，会与第一张卡交换，避免被系统词典挤出视野；卡片不足时追加「+ / d 管理」卡。
- 每张词库卡：名称（bold）、徽标（`内置` / `当前` / `词库`）与「N 词」；词数取 `effective_length()`。
- 「推荐」：从 `catalog.resources()` 过滤掉当前词典与 `book_list` 已拥有的 id/enName，按行显示 名称（截断 18 列）/ 简介（22 列）/ 词数；行数由区块高度决定；无推荐时显示「暂无推荐 · d 浏览词库」。
- 词典卡片的排布由 `dict_tile_layout` 统一计算，绘制（`render_dicts`）与命中检测（`layout`）共用它——两边各算一遍必然漂移，「+ / d 管理」卡就会点不到。`ordered_dicts` 负责展示顺序（当前词库优先）。
- `targets`：主按钮 `StartPractice`、自由按钮 `StartFreePractice`、目标 `AdjustDaily(±5)`、模式标签 `OpenModePicker`、「d 词库」「+ / d 管理」卡「d 更多」→ `Navigate(DictPicker)`；每个 target 都用 `Target::focusable` 带稳定身份 `FocusId::Content(ContentId::…)` 与中文 `label`，键盘焦点与鼠标命中同源。标题右侧文字本身是 `section_header` 画出来的，命中矩形由 layout 预留（`open_dicts`/`open_more`/`manage_tile`）。

### 5.4 键盘焦点（所有套外壳页面）

- **适用范围**：主页、文章、词库、统计、设置、词条管理、流程、快捷键、帮助九类页面共用同一套两栏焦点；练习页与文章跟打页是全屏页面，不画侧栏，不参与；单词测试页是专用答题页，键位由键位表单独处理（`ui::focus::supports_focus` 决定）。练习会话进行中（含练习里打开的整屏帮助）、有弹层、词条搜索框正在输入时，焦点导航整体让位。
- **焦点身份**：`App.focus: Option<ui::focus::FocusId>`。`FocusId::Nav(NavId)` 是导航栏条目；`FocusId::Content(ContentId)` 是主页内容区的 8 个固定元素；`FocusId::Page` 是其它页面的内容区——页面自己的光标（词库行、设置行、文章 / 书本行、词条行、流程行、快捷键行、日历日期、滚动位置）就是身份，由页面状态保存，不需要再存下标。`App.last_content_focus: Option<ContentId>` 只服务主页内容区。导航栏含宽屏底部「退出」（`NavId::Quit`），窄屏顶栏没有这一项。身份定义在 `src/ui/target.rs`，解析与移动规则在 `src/ui/focus.rs`，状态迁移在 `src/app/focus.rs`。
- **目标来源**：两栏都与鼠标命中同源。`ui::focus::nav_targets` 取 `shell::layout` 的 `nav_targets`（`Nav::MAIN` + 设置）再追加 `quit_target`（宽屏「退出」）；`ui::focus::content_targets` 取 `ui::home::targets(app, shell.content)` 的 8 项，每项都带 `ContentId` 与中文 `label`。其它页面的内容区不注册逐行身份：`ui::focus::page_selection_rect` 按页面自己的光标与 layout 直接解析当前选中项的矩形，所以高亮与滚动、分栏、窗口尺寸同源。
- **默认选中**：`App::set_screen` 是所有切屏路径的唯一入口（导航、返回、结算、退出练习……），切屏时清空旧焦点并按目标页调用 `App::focus_default`：主页选中 `ContentId::Start`（开始 / 继续学习）；其它套外壳页面选中 `FocusId::Page`（页面自己的内容区）；练习页 / 文章跟打页 / 单词测试页保持无焦点。因此进入任何标签页后方向键都立刻作用在页面内容上，底栏显示「选中 X」，再按 `←` / `Tab` 才进导航栏。
- **统一的键 × 栏 × 结果**（宽屏指 `>= 100` 列；到边停、不回绕；有弹层时方向键属于弹层）：

| 栏 | `↑` | `↓` | `←` | `→` | `Tab` | `Enter` | `Esc` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 宽屏侧栏（竖向） | 上一项（含「退出」） | 下一项 | 无动作 | 回内容区 | 回内容区 | 触发选中项；当前页入口只给提示 | 清空焦点（主页）/ 返回（其它页） |
| 窄屏顶栏（横向） | 无动作 | 回内容区 | 上一标签 | 下一标签 | 回内容区 | 同上 | 同上 |
| 主页内容区（按行带） | 上一带（带内水平中心最近者） | 下一带 | 同带左移；无左邻时进侧栏 | 同带右移；到边停 | 进侧栏 | 触发选中项 | 清空焦点 |
| 其它页面内容区 | 页面自己的上移（列表光标 / 统计滚动 / 帮助滚动） | 页面自己的下移 | 页面横向操作（文章页换栏、统计页前一天），没有横向操作或到左边缘时进侧栏 | 页面横向操作；到边停 | 进侧栏 | 触发页面选中项 | 返回上一层 |
| 无焦点（主页 Esc / 点空白后） | 进导航栏（落在当前页入口） | 落「开始 / 继续学习」 | 同 `↑` | 同 `↓` | 进导航栏 | 开始 / 继续学习（`StartPractice`） | 无动作 |

- **内容区按视觉行分带**（`ui::focus::bands` / `content_move`，仅主页）：目标先按 `(y, x)` 排序，纵向范围有重叠的算同一带，带内再按 `x` 排序；`↑` / `↓` 在带之间移动，落点是目标带内**水平中心**离当前元素最近的元素，`←` / `→` 在同一带内前后移动。因此下键不会向上跳、右键不会向左跳。120 × 40 的带序是：练习模式 → 每日目标 `-` / `+` → 开始 / 继续学习、自由练习 → 词库页 → 管理词典（整张卡自成一带）→ 更多词库；终端变矮时「推荐」「我的词典」区先收起，变窄时模式标签也可能先消失。
- **其它页面的内容区移动**（`App::move_selection_page`）：先让页面自己的光标动，动不了再按几何跨栏。词库 / 设置 / 词条 / 流程 / 快捷键：`↑`/`↓` 移动行光标，`←` 直接进侧栏（宽屏），窄屏在首行再按 `↑` 进顶栏；文章页：`↑`/`↓` 在当前栏内选，`←`/`→` 在书本栏与文章栏之间切换，在书本栏再按 `←` 进侧栏；统计页：`↑`/`↓` 滚动正文、`←`/`→` 在日历里选日期，到本月 1 号再按 `←` 进侧栏，`Enter` 与 `v` 一样切换「日历 / 按词典」视图；帮助页：`↑`/`↓` 滚动，`←` 进侧栏，`Enter` 返回上一层。`j` / `k` 是所有页面的 `↓` / `↑` 别名。
- **跨栏只发生在到边之后**：`App::move_focus` 先尝试栏内移动，返回 `None`（该方向没有目标）才判断是否换栏——宽屏内容区 `←`、窄屏内容区 `↑` 进导航栏，宽屏侧栏 `→`、窄屏顶栏 `↓` 回内容区；其余方向到边就停。回内容区时主页恢复 `last_content_focus`（仍存在时），否则落「开始 / 继续学习」；其它页面恢复页面自己的光标（页面状态本来就在）。进导航栏落当前页面对应的入口（例如词库页落「词库」），找不到则第一项。`Tab` / `Shift+Tab` 在任何套外壳页面都能直接换栏，给统计这类横向键被页面占用的页面留一条固定通路。
- **为什么不用固定顺序**：早期按「动作优先级」排序并回绕（到头绕回另一端），移动顺序与画面方向无关——`↓` 会从右下角跳回右上角，跨栏后还会清空选中。现在顺序由几何与页面光标决定、到边停。
- **失效与触发**：导航栏与主页内容区的 `Enter` 按当前 `viewport` 重新解析身份；解析不到（窗口缩放后元素消失、模式名变长把标签挤掉）只清空焦点并 Toast「选中项已随布局变化失效，请重新选择」，绝不回退成开始练习。`FocusId::Page` 不依赖几何，页面光标始终有效。焦点元素的 `Action` 就是鼠标点击同一个 `Target` 会执行的 `Action`；落在导航栏当前页入口上时给 Info Toast「已在「X」页」，避免「按了没反应」。其它页面 `Enter` 走页面自己的激活逻辑（词库设为当前、设置修改、文章开始跟打、词条编辑、快捷键改绑、流程启用、统计切视图、帮助返回）。
- **高亮**：`ui::focus::paint` 把当前焦点目标的整个矩形铺成 `accent` 底 + `bg` 文字 + 加粗，画在页面内容之上、弹层遮罩之下。导航栏、主页内容区、以及其它页面内容区的当前选中项（词库行 / 设置行 / 文章或书本行 / 词条行 / 流程行 / 快捷键行 / 日历选中日）都走这同一套高亮；焦点进导航栏时页面行让位给导航项，页面自己的行高亮 / 光标仍在底下可见。侧栏「当前页」用 `selection` 底 + 左侧 1 列 `accent` 条，与焦点高亮可以同屏共存。
- **底栏联动**：`ui::focus::focused_label` 返回 `String`：导航栏取页面名，主页取按钮名（`Target.label`），其它页面取页面当前选中项（词库名 / 设置项名 / 文章标题 / 词条 / 流程名 / 快捷键动作名 / 统计日期 / 「帮助」）。有焦点时 `shell::focus_hints` 把底栏换成「选中 {label}  ↑↓←→ 移动  enter 触发 / 切换  esc 取消 / 返回  tab 换栏」（`tab` 在末尾，底栏太窄时先被丢弃）；无焦点时恢复常规快捷键提示。开始按钮的 `label` 与按钮文案统一由 `home::primary_label` 决定（有断点缓存或已学词时都是「继续学习」）。
- **Esc 的语义**：主页 `Esc` 取消选中回到「无焦点」；其它页面 `Esc` 是返回上一层（与改造前一致），导航栏焦点上按 `Esc` 同样返回。
- **鼠标**：点击导航栏会把焦点同步到对应导航项；点击内容区会把焦点交给页面内容区（`FocusId::Page`）；主页点击可聚焦元素还会写 `last_content_focus`，点空白清空焦点。见第 11 节。
- **`App.viewport`**：按键时按 `App.viewport` 重新算布局（`on_key` 不经过渲染），而渲染用 `Frame::area()`；两者必须一致——事件循环每帧同步，测试与 `examples/shot` 用 `App::set_viewport` 显式设置。不一致时按到的目标与画面上的位置不同。

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
- `targets`：每个可见行一个 target——光标所在行是 `ActivateDict(index)`，其它行是 `SelectDict(index)`（第一次点击选中、再次点击激活）；按钮是 `ActivateDict(dict_cursor)`。键盘焦点在内容区时，光标行由 `ui::focus::page_selection_rect` 解析成整块 `accent` 高亮（与主页一致）；焦点在侧栏时恢复页面自己的 `selection` 底 + 左侧强调条。截图见 [词库页](screenshots/dict-120x40-dark.png)。

### 8.2 统计页（stats.rs）

- 6 块总览卡：累计时长、学习天数、新词、复习、错误、正确率；内容宽度 `>= 90` 时 6 列 × 1 行（卡高 3，带「今日 N」小字），否则 3 列 × 2 行（卡高 2，无小字行）。
- 「最近 14 天」：`insights::recent_days(base, 14)` 按日期倒序；每行「MM-DD 周x / 今天 + 相对最长时长的小进度条（最多 24 格）+ 时长 + 新 N 复 N + 错 N」；今天整行用 `selection` 背景。记录多于一屏时列表少画一行并给出 `↑`/`↓` 提示；行宽不足 28 列时跳过明细。
- `app.stats_scroll` 在渲染时被夹到 `total - visible`，不会越界。日历里每一天的整格（3 列日期 + 2 列记录点）都是一个鼠标 target（`StatsSelectDay`），页头的「v 切换」是 `StatsToggleView`；滚轮与 `PgUp` / `PgDn` 滚动正文。键盘焦点在内容区时 `↑`/`↓` 滚动、`←`/`→` 选日期、`Enter` 切换视图；月初再按 `←` 进侧栏。当天的高亮矩形直接复用 `StatsSelectDay` 的命中区，因此高亮与点得中的格子永远同一块（早期命中区从数字列末尾开始，高亮会向右歪一格）。截图见 [统计页](screenshots/stats-120x40-dark.png)。

### 8.3 设置页（settings.rs）

- 7 个分区标题（通用/音频/复习/键入/显示/FSRS/其他）+ 32 行设置项 = 39 行；分区标题是「▎ 标题 ────」样式的整行。`layout` 先保证光标行可见，再尽量把光标所在分区的标题一起滚入视口；顶部/底部溢出时在内容区最右列画 `↑`/`↓`。
- 每行「标签 + 右对齐当前值」：选中行 `selection` 背景 + 左侧 1 列 `accent`；值的颜色按类型区分（模式/主题/音标 `accent_alt`，布尔开 `good` 关 `muted`，数字 `fg`，「按 Enter 重置」`dim`）。数字值的格式化：每日词数「N 词」、比例裸数字、重复「N 次」、等待/冷却「N ms」、FSRS 阈值裸数字、留存率「N%」。
- 数字项按 `Enter` 后不就地编辑，而是打开 `ui/overlays.rs` 的输入弹层（见第 10 节）；设置页布局本身不变。
- `targets`：只给设置项行生成（分区标题不可点）；光标行 `ActivateSetting(index)`，其它行 `SelectSetting(index)`。截图见 [设置页](screenshots/settings-120x40-dark.png)。

### 8.4 帮助页（help.rs）

- 内容宽度 `>= 78` 且高度 `>= 20` 时两栏：左「练习」、右「导航/查看」，中间 1 列 `│` 分隔线 + 1 列留白；左栏过窄（`< 26`）时回退单栏。
- 单栏把两个分区顺序拼接并追加鼠标说明（「鼠标：侧栏/按钮/列表可点击，滚轮滚动；Shift+拖拽选择文本」按 3 条短行渲染），`help_scroll` 被夹到最大偏移，右侧画 `↑`/`↓`。
- 条目三种：分区标题、按键（`key_hint`：按键 `accent_alt` bold + 说明 `muted`）、说明（`dim`，按宽度折行并缩进 1 列）。按键内容以帮助页常量表为准，键位语义见 [应用层 · 输入路由](app.md#5-输入路由)。
- `targets`：只注册右侧的 `↑` / `↓` 滚动指示器；正文是只读文本，点击不产生动作。早期把整块内容区映射成 `HelpBack`，点空白、点窗口聚焦都会退出帮助页。键盘焦点在内容区时 `↑`/`↓` 滚动、（宽屏）`←` 进侧栏、`Tab` 换栏、`Enter` 返回；`Esc` 与其它任意键仍是返回。窄屏可滚动，截图见 [帮助页](screenshots/help-120x40-dark.png)。

### 8.5 文章背诵页与跟打页（articles.rs）

- 列表页（`render_articles`）套外壳（`Nav::Articles`）：左「书本」、右「文章」两栏（38% / 其余，右栏至少 20 列），行首 `▶` 为选中项；文章行显示标题、句数与断点标记「· 上次第 N 句」（`accent_alt`）。`targets` 给两个列表的可见行注册点击目标，滚轮映射为 `ArticleBookMove`。键盘焦点进页在内容区（左栏书本）：`↑`/`↓` 选书本、`→` 切到文章栏、`←` 从文章栏回书本栏（已在左栏再按 `←` 进侧栏），`Enter` 开始选中的文章；`Tab` 是导航栏 ↔ 内容区换栏。当前栏的选中行由 `ui::focus::page_selection_rect` 整块 `accent` 高亮，切栏时高亮跟着移动；另一个栏的 `▶` 只表示它自己的光标位置。
- 跟打页（`render_article_practice`）全屏、不套外壳，纵向分四段：状态行（标题 `accent` + 第 N/M 句 + 已完成 / 错误 / 用时；错误数把当前句已出现的错误也算进去）、「跟打」区块、「当前句译文」区块、两行提示。`article_lines` 把整篇文章按段落折行成同一页的行列表（段落之间空一行），`cursor_line` 找出光标所在行，视口据此滚动；`offset` 只在渲染期计算，不写 `App` 状态。
- 逐字符着色：非跟打字符（标点、符号与多余空白）在当前句用 `dim`、已完成句用 `muted`、未来句用 `fg`；词间空格是可选目标，光标会停在待输入的空格上，但直接打下一个字母也不算错；当前句里已输入且正确的字符 `good`，打错的字符 `bad` 并显示实际按下的字符——错误只会出现在字母 / 数字位置（省略空格时自动补成正确输入，不再有「空格处错字」与下划线样式，渲染里也不再有 `Modifier::UNDERLINED` 分支）；光标位置（第一个未输入字符）用 `selection` 背景 + BOLD；当前句首行行首是 `▶ `（`accent`）标记，其它行补两个空格对齐。`article_lines` 折行时行尾空格直接丢掉：空格可省略且总按正确输入显示，折掉不会藏起错误标记。
- 底部第一行平时提示「 字母、数字照打 · 空格可省略 · 标点不用打 · Backspace 退格」（`dim`；58 列，60 列最小终端下可完整显示）；本句出现过错误时换成「 红色 = 打错的字符 · Backspace 改正」（`warn`）。该页不注册鼠标目标，只用键盘。截图见 [文章跟打](screenshots/articlepractice-120x40-dark.png)。

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
| 结算 `summary` | 居中 `72 × (12 + extra)`，`extra = 2 + 错词 Top10 占用行数`（本周记录占 2 行；错词 Top10 为空时不占行，否则为 2 + min(10, 错词数)）；内区左右各缩 2、上下各缩 1；按钮行在内区最后一行 | 标题「结算」，内容按 `SummaryData.kind` 分两套：**单词**（`Word`，会话与单词测试）为 `今日任务完成` / `学习报告`、学习时长与正确率、新词/复习/错词、学习进度与「已完成」、细进度条、错词 Top10、本周记录，按钮「Enter 继续」（14 列）/「r 重学」（11 列）/「q 主页」（10 列）；**文章**（`Article`）为 `文章跟打完成` + 文章标题、学习时长与正确率、「完成 N / M 句」「错句 N」、细进度条、本周记录，按钮「Enter 返回列表」（14 列）/「r 重打本篇」（11 列）/「q 主页」（10 列） |
| 输入 `edit` | 居中 `46 × 5` | 标题「输入」；文案「修改 {设置名}（Enter 保存 · Esc 取消）」+「> {buffer}_」 |
| 确认 `confirm` | 居中 `56 × 7`；内区左右各缩 2、上下各缩 1 | 标题「确认」；消息（`fit_text`）、空行、「Enter/y 确认 · 其他键取消」；按钮「确认」「取消」（各 10 列） |

- `centered_rect` 先把宽高夹到 `area`，所以最小终端 60×20 下弹层不会越界（单测 `summary_layout_fits_min_terminal`、`confirm_buttons_are_inside_modal` 覆盖）。
- 结算弹层高度随内容自适应：`height = 12 + extra`，`summary_extra_lines` 在基础 12 行之外累计「本周记录（1 空行 + 1 行）」与「错词 Top10（1 空行 + 标题 + 每词 1 行）」；文章结算没有错词 Top10，`extra` 恒为 2。内区高度 = 高度 − 2，正文后留白、按钮画在内区最后一行；`render_summary` 压入错词 Top10 与本周记录前都检查剩余高度（`lines.len() + 2`），最小终端 60×20 下按钮不会被内容顶掉。
- 遮罩：`ui::render` 在弹层打开时给全 buffer 加 `Modifier::DIM`；弹层绘制前用 `Clear` + `theme.bg` 清掉弹层外扩 2 列/行的背景，再用 `theme.panel` 画自身，从而在无 alpha 的终端里「压暗背景、恢复前景」。
- 鼠标：`overlays::targets` 为确认弹层生成「确认/取消」、为结算弹层生成三个按钮，为模式 / 跳组 / 收藏 / 词条编辑弹层生成各自的行 target（输入弹层没有按钮 target）。
  `target_action` **复用 `targets` 的结果**做命中——两处各写一遍时，模式 / 收藏 / 词条编辑三个弹层只有 `targets` 一侧，鼠标点它们毫无反应。
  「点弹层外」的语义各弹层不同：跳组 / 确认 / 输入弹层点外面 = 取消；模式 / 收藏 / 词条编辑弹层点外面 = 无动作；结算弹层只响应三个按钮，点外面不动。
  结算截图见 [结算弹层](screenshots/summary-120x40-dark.png)（单词会话）与 [文章结算](screenshots/summary-article-120x40-dark.png)（文章跟打）。

## 11. 鼠标命中

- `Target { rect: Rect, action: Action, label: Option<&'static str>, focus: Option<FocusId> }`；`contains(column, row)` 使用半开区间 `[x, right) × [y, bottom)`。`label` 只给键盘焦点用（底栏「选中 X」），鼠标命中路径不读它：主页 target 用 `Target::labeled` 标注中文名，其它页面仍是 `Target::new`。`focus` 只给导航栏与主页内容区（`Target::focusable`）；其它页面的内容区焦点是 `FocusId::Page`，由页面光标表示，不需要逐行注册身份。
- `hit::targets` 的收集顺序（也是命中优先级）：
  1. 有弹层时只返回 `overlays::targets`；
  2. **独占整屏**的页面（`Practice`、`ArticlePractice`）不画侧栏，也不注册侧栏 target——早期实现按 shell 布局注册了导航，点屏幕上什么都没画的位置会突然跳回主页；
  3. 外壳：收集侧栏/顶栏导航 target，以及（宽屏）`quit_target`；
  4. 页面 target：`Home`、`DictPicker`、`Settings`、`Shortcuts`、`Words`、`Stats`、`Flows`、`Articles`、`WordTest` 的页面内按钮/列表；`Help` 只有右侧滚动指示器（正文不做点击，返回由键盘或导航负责）。
- `action_at` 用 `find` 返回第一个包含坐标的 target；因此导航优先于页面内按钮，页面内又按 `targets()` 的 push 顺序（例如主页：开始按钮 → 自由练习 → 目标 +/- → d 词库 → d 更多）。
  **重叠的 target 要靠注册顺序让位**：设置页 / 词库页的滚动指示器画在内容区最右一列，行 target 必须让出这一列，否则点 `↓` 会误触行 target。
- **键盘焦点与命中目标同源**：`ui::focus::nav_targets` 复用的就是 `shell::layout` 的导航矩形，主页内容区复用 `home::targets(app, shell.content)`——方向键选中的元素一定就是鼠标点得到的那一块。其它页面的内容区焦点（`FocusId::Page`）对应页面自己的光标，页面已经用同一份 `layout` 算出该行矩形，所以选中行和点击行也不会漂移。键盘顺序与 `action_at` 的命中优先级无关；焦点只决定「`Enter` 触发哪一项」。鼠标点击导航栏会同步导航焦点；点击其它页面内容区会把焦点交给页面内容区；主页点可聚焦元素还会记 `last_content_focus`，点空白清空焦点。切屏时由 `App::set_screen` 重置焦点。
- `wheel_action`：有弹层返回 `None`；词库页 → `DictMove(delta)`、设置页 → `SettingsMove(delta)`、统计页 → `StatsScroll(delta)`、帮助页 → `HelpScroll(delta)`、词条页 → `WordsMove(delta)`、快捷键页 → `ShortcutMove(delta)`、流程页 → `FlowMove(delta)`、文章页 → `ArticleBookMove(delta)`，其它页面 `None`；`delta` 由滚轮方向给出（上 `-1`、下 `1`）。
- 不变量：每个页面只写一份 `layout`/`layout_xxx`，`render` 与 `targets` 都从它取 `Rect`；滚动偏移来自 `App` 的同一字段。这样绘制位置与可点击位置不会漂移。
  **文本驱动的控件更进一步**：主页每日目标的 `[-]` / `[+]` 位置由「绘制用的同一行文本」算出来（`home::goal_line_text` + `goal_buttons`），不能各写一份偏移量——早期实现用固定偏移估算，与实际排版差 2 列，所有终端尺寸下点 `+` 都没反应。
- 相关测试：`tests/app_drive.rs` 用 `hit::targets` 找按钮再模拟点击；`tests/ui_snapshots.rs` 的 `mouse_target_rects_match_rendered_text` 校验命中行与渲染文本一致；`tests/interaction_fixes.rs` 覆盖三个弹层的鼠标命中、主页 `+/-` 的像素对齐、滚动指示器不被行 target 抢走等回归；弹层布局单测保证按钮在弹层内。

## 12. 渲染测试与新增页面检查清单

### 12.1 现有渲染测试

`tests/ui_snapshots.rs` 用 ratatui `TestBackend` 渲染 `App`，断言关键文本、颜色与不 panic（不是逐像素快照）：

| 测试 | 覆盖点 |
| --- | --- |
| `home_screen_renders` / `home_uses_theme_surfaces` / `home_compact_renders_at_min_size` | 主页文本、`bg`/`panel`/`accent_alt` 表面、60×20 紧凑布局 |
| `practice_screen_renders_big_word` / `masked_mode_renders_placeholder_blocks` / `typed_letters_are_colored` | 大字方块字符、默写占位符、正确输入用 `theme.good` 着色 |
| `all_screens_render_without_panic` / `cjk_text_wraps_within_width` | 4 个非主页屏幕在 80×24 与 120×40 下渲染、逐行不超出宽度 |
| `all_builtin_themes_render_home` / `tiny_terminal_shows_hint` / `overlay_screens_render` / `usage_tips_can_be_hidden` | 六套内置主题背景、`< 60 × 20` 提示、弹层文本、关闭「显示使用提示」后底部提示行消失 |
| `article_summary_renders_article_labels` | 文章结算弹层用文章口径（文章标题 / 完成 N / M 句 / 错句 / 返回列表按钮）而不是单词会话口径 |
| `mouse_target_rects_match_rendered_text` / `help_scrolls_at_min_size` | 命中矩形与渲染文本一致、帮助页在小终端可滚动 |

方向键焦点另有独立集成测试 `tests/focus_nav.rs`：启动 / 回主页默认选中主按钮（首帧高亮与底栏、状态页 / 词库页返回、练习 `Ctrl+C` 返回、结算关闭）、无焦点落点、内容区换带单调不反向、带内左右移动、跨栏后恢复上次选中、宽屏侧栏逐项（含「退出」）、窄屏顶栏横向与到边行为、窗口缩放导致焦点失效时 `Enter` 不触发动作、底栏文案、高亮矩形与 `focused_rect` 一致、最小终端忽略按键、鼠标点击同步焦点；以及每个标签页落地选中内容区、`Tab` / 换栏、宽屏 `←` 与窄屏 `↑` 跨栏、词库 / 设置 / 统计 / 帮助页各自的页面光标移动、搜索框聚焦时焦点导航让位、子页面共用同一套模型等。

`examples/shot.rs` 用同一套 `TestBackend` 输出带样式的 JSON（cell 的字符/前景/背景/修饰/宽度），再由 `tools/render-snapshot.py` 生成 PNG；`shot` 支持 `home / home-focus / home-nav-focus / practice / wordtest / words / articles / flows / articlepractice / dict / stats / settings / shortcuts / help / summary / summary-article / edit / confirm` 与自定义宽高、主题；`home` 是启动首帧（默认选中主按钮）的快照，`home-focus` 是内容区焦点选中「f 自由练习」后的快照（启动默认选中主按钮，按一次 `→`），`home-nav-focus` 是侧栏焦点选中「文章」后的快照（先 `←` 进侧栏、再 `↓` 到「文章」）。截图文件： [主页](screenshots/home-120x40-dark.png)、[主页内容区焦点](screenshots/home-focus-120x40-dark.png)、[主页导航栏焦点](screenshots/home-nav-focus-120x40-dark.png)、[主页 90×30](screenshots/home-90x30-dark.png)、[主页浅色](screenshots/home-120x40-light.png)、[主页高对比](screenshots/home-120x40-high-contrast.png)、[练习](screenshots/practice-120x40-dark.png)、[词库](screenshots/dict-120x40-dark.png)、[统计](screenshots/stats-120x40-dark.png)、[设置](screenshots/settings-120x40-dark.png)、[帮助](screenshots/help-120x40-dark.png)、[结算](screenshots/summary-120x40-dark.png)、[文章结算](screenshots/summary-article-120x40-dark.png)。生成命令与字体要求见 [开发指南](development.md)。

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
