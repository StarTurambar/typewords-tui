# 主题系统

typewords-tui 的配色全部来自主题：渲染层不出现硬编码颜色，所有前景/背景都从主题的 13 个颜色 token 取值。程序内置深色、浅色、高对比三套主题，也支持用户把 `*.json` 放进数据目录的 `themes/` 自定义主题。本文说明主题模型、内置主题、选择与优先级、自定义主题格式、颜色语法、token 语义、内部实现与常见问题。

> 适用范围：`src/theme/`（主题配置层）、`src/ui/theme.rs`（渲染映射）、`<home>/themes/*.json`、`--theme` 参数与设置页主题行、`tw doctor` 的主题输出；对应版本：typewords-tui 0.1.0。

## 1. 主题模型与分层

一个主题由以下部分组成（`src/theme/mod.rs` 的 `ThemeSpec`）：

| 组成 | 说明 |
| --- | --- |
| `id` | 主题标识：内置主题为 `dark`/`light`/`high-contrast`，自定义主题为文件名转小写 |
| `name` | 显示名，如「深色」「高对比」或自定义的 `Ocean`；为空时回退 id |
| `base` | 基础模式 `BaseMode`：`Dark`/`Light`/`HighContrast`，决定未覆盖 token 的继承来源 |
| `colors` | 13 个颜色 token（见第 6 节） |

主题相关代码分成两层，互不依赖：

- **`src/theme/`（配置层）**：只回答「主题是什么」——颜色值模型（`ThemeColor`：RGB 或 ANSI）、`ThemeSpec`、`ThemeRegistry`、内置主题表、用户主题文件加载与校验。它**不依赖 ratatui**，可单独测试。
- **`src/ui/theme.rs`（渲染层）**：把 `ThemeSpec` 的 13 个 token 转成 ratatui 的 `Color`（`ui::Theme`），供各页面绘制使用。

### 1.1 `auto` 的语义

设置里的 `theme` 默认值是 `"auto"`。`auto` 不做终端背景检测，**始终解析为深色（`dark`）**；它只是「跟随程序默认」的标记，设置页显示为「自动（深色）」。`--theme auto` 与不传 `--theme` 的默认效果相同。

## 2. 内置主题

| id | 名称 | 风格 | 适用场景 |
| --- | --- | --- | --- |
| `dark` | 深色 | 深灰蓝底 + 粉紫/青色强调 | 默认主题；深色终端、夜间使用 |
| `light` | 浅色 | 白/浅灰底 + 深紫/深青强调 | 浅色终端、明亮环境 |
| `high-contrast` | 高对比 | 纯黑底 + 纯白文字 + ANSI 亮色强调 | 不支持真彩色（24-bit RGB）的终端、需要最大对比度的场景 |

内置主题的完整色板（`src/theme/builtin.rs` 的 `builtin_specs`）：

| token | dark | light | high-contrast |
| --- | --- | --- | --- |
| `bg` | `#171720` | `#f7f7f9` | `black` |
| `panel` | `#1e1e2b` | `#ffffff` | `black` |
| `element` | `#262637` | `#ececf2` | `black` |
| `border` | `#34344a` | `#d9dae3` | `white` |
| `fg` | `#d7d9e6` | `#21222b` | `white` |
| `muted` | `#8a8ca3` | `#4f5262` | `gray` |
| `dim` | `#6d7189` | `#656a7a` | `gray` |
| `accent` | `#c98ec7` | `#93418a` | `lightmagenta` |
| `accent_alt` | `#7fd3d8` | `#176f75` | `lightcyan` |
| `warn` | `#e2b86b` | `#8a6100` | `lightyellow` |
| `good` | `#9ece6a` | `#2f7d4f` | `lightgreen` |
| `bad` | `#e06c75` | `#b23a48` | `lightred` |
| `selection` | `#2a2b40` | `#e9e7f1` | `black` |

![深色主题主页](screenshots/home-120x40-dark.png)

![浅色主题主页](screenshots/home-120x40-light.png)

![高对比主题主页](screenshots/home-120x40-high-contrast.png)

高对比主题有两个刻意设计：选中行的 `selection` 与 `bg` 同为黑色，选中态靠左侧 `accent` 强调条 + 粗体文字区分；块面（`panel`/`element`）也都用纯黑，避免在 ANSI 调色板里出现低对比度背景。

## 3. 选择与优先级

主题的实际来源只有一处：`SettingState::theme`（`setting.json` 的 `theme` 字段）。`App::theme()` 在每帧渲染时调用 `ThemeRegistry::resolve(settings.theme)` 并构造 `ui::Theme`，因此设置页切换主题后**下一帧立即生效**；只有主题**文件**的增删改需要重启进程（见第 4 节）。

| 来源 | 行为 |
| --- | --- |
| `settings.theme` | 基础值，默认 `"auto"`；设置页「主题」行循环切换并立即写回 `setting.json` |
| `--theme NAME` | 启动时覆盖 `settings.theme`（`App::bootstrap`）；正常退出时 `save_all` 会把该值写回设置，等价于修改了设置 |
| 未知/缺失 | `resolve` 回退深色；启动时弹 Warning「主题 <名字> 不存在，已回退到深色」，`settings.theme` 原值保留不自动修正 |

设置页的循环顺序由 `ThemeRegistry::cycle_ids` 决定：

```text
auto → dark → light → high-contrast → 自定义主题（按文件名/路径排序）→ 回到 auto
```

两个字符串匹配细节：

- `resolve` 会先 `trim` 再转小写，所以 `--theme LIGHT`、`--theme Ocean` 都能解析到对应主题；
- 但 `cycle_theme` 用**精确字符串**在当前列表中查找位置，找不到时从下标 0（`auto`）开始——因此 `--theme LIGHT` 或拼错的名字在设置页按 Enter 会先跳到 `dark`。

设置页/顶栏显示的是 `ThemeRegistry::label` 的结果：`auto` → 「自动（深色）」；已知主题 → 显示名（name 为空则 id）；未知主题 → 「<id>（缺失，回退深色）」。

`tw doctor` 会列出主题目录、自定义主题数量、全部主题 id 与显示名，以及加载告警：

```text
主题（1 个自定义）：
  目录: C:\Users\<用户>\AppData\Local\typewords-tui\themes
  - dark             深色
  - light            浅色
  - high-contrast    高对比
  - ocean            Ocean
  ! C:\Users\...\themes\bad.json: JSON 解析失败: ...
```

主题文件加载失败不会阻止启动：每个失败文件产生一条告警，逐条转成启动时的 Warning Toast（约 3.2 秒、最多同时 4 条），并保存在注册表的 `warnings()` 里供 `tw doctor` 展示。

## 4. 自定义主题

### 4.1 目录与加载时机

- 位置：`<home>/themes/*.json`（`<home>` 解析见 [存档与数据格式](persistence.md) 第 1 节）。
- 该目录**不会自动创建**；不存在时视为没有自定义主题，不产生告警。
- 只加载目录第一层的 `*.json`（扩展名大小写不敏感），子目录不递归。
- 加载发生在启动装载阶段（`bootstrap::load` → `ThemeRegistry::load`），运行中新增/修改/删除文件都不生效，**需重启进程**。

### 4.2 JSON 结构

```json
{
  "name": "Ocean",
  "base": "dark",
  "colors": {
    "bg": "#0b1020",
    "accent": "#6fd3c7"
  }
}
```

| 字段 | 必需 | 说明 |
| --- | --- | --- |
| `name` | 否 | 显示名；缺省或空白时使用**文件名原文**（保留大小写） |
| `base` | 否 | `dark`（缺省）/ `light` / `high-contrast`；也接受 `default`、`high_contrast`、`contrast` |
| `colors` | 否 | 13 个 token 的任意子集，键名见第 6 节 |

- 未写出的 token 继承 `base` 对应内置主题的颜色；`base` 只影响继承来源，不会限制你覆盖哪些 token。
- 顶层与 `colors` 内都启用了 `deny_unknown_fields`：多写一个字段（如 `"colour"`、`"bgColor"`）会让该文件整体校验失败，而不是静默忽略。
- 主题 id 由文件名决定、统一转小写：`Ocean.json` 的 id 是 `ocean`，`name` 仍保留 `Ocean`。不要放仅大小写不同的多个文件（Windows 文件系统本身也不允许）。

### 4.3 文件名规则

| 规则 | 说明 |
| --- | --- |
| 长度 1–32 | 按文件名（不含 `.json`）计；空格、点、括号等不在字符集内的字符会被拒绝 |
| 字符集 | 只允许 ASCII 字母、数字、`_`、`-` |
| 大小写 | id 会转小写；`name` 保留原始大小写 |
| 保留名 | `auto`、`dark`、`light`、`high-contrast` 不能作为文件名（比较前已转小写，`Dark.json` 同样被拒绝） |

### 4.4 校验失败的行为

单个文件校验失败只**跳过该文件**并在启动时告警，其他主题与内置主题不受影响。告警格式为 `<文件路径>: <原因>`，原因包括：

| 情形 | 告警原因（前缀 `<路径>: `） |
| --- | --- |
| 文件名长度/字符不合法 | `主题文件名需为 1~32 个字符` / `主题 id 只能包含字母、数字、下划线和连字符` |
| 使用保留名 | ``主题 id `dark` 为内置保留名，请换一个文件名`` |
| JSON 语法错误、未知字段、未知 token | `JSON 解析失败: <serde 错误>` |
| `base` 取值未知 | `未知主题 base: <值>（可用 dark / light / high-contrast）` |
| 颜色写法非法 | `colors.<token>: <颜色错误>` |
| 文件读取失败 | `读取失败: <io 错误>` |
| 文件名不是有效 UTF-8 | `文件名不是有效 UTF-8` |
| 主题目录读取失败（非「不存在」） | 单独一条 `读取主题目录失败 <目录>: <错误>` |

### 4.5 完整示例：`ocean.json`

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

### 4.6 两个简短变体

只覆盖少量 token 也是合法主题：

```json
{ "name": "Paper", "base": "light", "colors": { "bg": "#fbfaf7", "fg": "#1f1c18", "selection": "#efe9dd" } }
```

```json
{ "name": "Amber ANSI", "base": "high-contrast", "colors": { "accent": "yellow", "accent_alt": "lightyellow", "border": "gray" } }
```

后者演示了在 `base` 为高对比时继续用 ANSI 颜色名覆盖强调色——整个主题仍然只依赖终端的 16 色调色板。

## 5. 颜色语法

每个 token 的取值是字符串，支持两种写法：

### 5.1 RGB 十六进制

| 写法 | 示例 | 说明 |
| --- | --- | --- |
| `#rgb` | `#abc` | 每位重复一次，等价于 `#aabbcc` |
| `#rrggbb` | `#171720` | 标准 6 位十六进制，大小写均可 |

### 5.2 ANSI 颜色名

| 规范名 | 可写别名（大小写不敏感，`-` 与 `_` 等价） |
| --- | --- |
| `reset` | `default` |
| `black` | — |
| `darkgray` | `dark_gray`、`darkgrey`、`dark_grey` |
| `red` | — |
| `lightred` | `light_red`、`brightred`、`bright_red` |
| `green` | — |
| `lightgreen` | `light_green`、`brightgreen`、`bright_green` |
| `yellow` | — |
| `lightyellow` | `light_yellow`、`brightyellow`、`bright_yellow` |
| `blue` | — |
| `lightblue` | `light_blue`、`brightblue`、`bright_blue` |
| `magenta` | `purple` |
| `lightmagenta` | `light_magenta`、`brightmagenta`、`bright_magenta` |
| `cyan` | — |
| `lightcyan` | `light_cyan`、`brightcyan`、`bright_cyan` |
| `gray` | `grey` |
| `white` | — |

解析时先 `trim`，ANSI 名再转小写并把 `-` 换成 `_` 后查表；`#` 开头的才按十六进制处理。错误信息：

| 情形 | 错误文本 |
| --- | --- |
| 空字符串 | `颜色不能为空` |
| `#` 后不是 3/6 位 | `无效的十六进制颜色: #...（需要 #rgb 或 #rrggbb）` |
| `#` 后含非十六进制字符 | `无效的十六进制颜色: #...` |
| 未知 ANSI 名 | `未知颜色: <值>（可用 #rrggbb / #rgb 或 ANSI 颜色名）`；`<值>` 是转小写、`-` 换成 `_` 后的形式 |

### 5.3 为什么高对比主题用 ANSI

RGB 颜色需要终端支持 24-bit 真彩色。在不支持真彩色、或终端把 RGB 近似到有限色板的场景里，深色/浅色主题的颜色会被量化甚至失真；ANSI 颜色名则映射到终端自身的 16 色板，任何终端都能显示，并且会跟随用户的终端配色方案（保证可读性）。所以内置高对比主题只使用 ANSI 颜色名，自定义主题也可以按 token 混用 RGB 与 ANSI。

## 6. 13 个颜色 token 语义

渲染层不硬编码颜色：所有页面从 `ui::Theme` 取下面 13 个值。

| token | 语义 | 典型使用位置 |
| --- | --- | --- |
| `bg` | 页面背景 | 整屏背景填充（`src/ui/mod.rs` 的 `render`）、弹层下方遮罩 |
| `panel` | 卡片/面板背景 | 主页词库卡、词库详情、结算/输入/确认弹层（`src/ui/home.rs`、`src/ui/overlays.rs`） |
| `element` | 次级块面 | 数字卡、次级按钮、窄屏当前页标签（`src/ui/home.rs`、`src/ui/shell.rs`） |
| `border` | 边框与分隔线 | 练习信息卡边框、设置分区横线、帮助分隔线（`src/ui/practice.rs`、`src/ui/settings.rs`） |
| `fg` | 正文与标题主色 | 标题加粗、普通文本 |
| `muted` | 次要文字 | 侧栏未选中导航、副标题、说明文字 |
| `dim` | 提示/弱化信息 | 版本号、底部快捷键提示、遮罩下的空心方框、已排除词 |
| `accent` | 主强调色 | 当前页左侧条、顶栏 `▎`、主按钮背景、选中行左条、大字「下一个待输入字母」 |
| `accent_alt` | 次强调色 | 数字与数值、进度条、设置值、滚动指示 `↑`/`↓`、Info Toast |
| `warn` | 警告 | 警告 Toast、计时暂停、今日任务标记 |
| `good` | 正确/成功 | 正确字母、成功 Toast、已完成/开启状态 |
| `bad` | 错误/危险 | 错误字母、错词数、错误 Toast、结算里的错词数 |
| `selection` | 选中行背景 | 侧栏当前项、列表选中行、统计页今日条 |

自定义配色的实用建议：

- 保证 `fg`/`bg` 对比度足够（正文建议 ≥ 4.5:1）；`muted`/`dim` 也不要压得太暗，否则辅助信息在低亮度屏幕上不可读。
- `selection` 要能与 `panel`/`element` 区分，否则列表选中行看不出高亮；高对比主题是反面例子（`selection` 与 `bg` 相同，靠 `accent` 条区分）。
- `accent` 与 `accent_alt` 用于强调与数字，通常选与 `bg` 明度差较大的两种色相；`good`/`warn`/`bad` 保持语义稳定，不要互换。
- 只在 token 层改色，不要期待渲染层对颜色做自动校正；RGB 主题在不支持真彩色的终端上会失真，可参考 5.3 改用 ANSI。

## 7. 内部实现

```text
builtin_specs()（内置 3 套） / load_user_themes(dir)（用户 *.json） / --theme NAME（覆盖 settings.theme）
                                   |
                                   v
                              ThemeRegistry
                                   |  resolve(settings.theme)
                                   v
                        ThemeSpec（id、name、base、13 个颜色 token）
                                   |  ui::Theme::from_spec（src/ui/theme.rs）
                                   v
                              ratatui Color（各页面渲染）
```

### 7.1 配置层

- `builtin_specs() -> Vec<ThemeSpec>`：返回 3 个内置主题，`DEFAULT_THEME_ID = "dark"`；颜色用 `const fn rgb(u32)` / `ansi(AnsiColor)` 构造。代码里没有名为 `BuiltinSpec` 的类型，内置主题与自定义主题共用 `ThemeSpec`。
- `ThemeSpec::label()`：`name.trim()` 非空则用 name，否则用 id。
- `ThemeRegistry`：`BTreeMap<id, ThemeSpec>` + 有序 `order` + `warnings`。内置主题先按 `dark/light/high-contrast` 入表，自定义主题按文件名（路径）排序追加。

| 方法 | 行为 |
| --- | --- |
| `builtin()` | 只有内置主题的注册表 |
| `load(dir)` | 内置 + `load_user_themes(dir)`；失败文件只记 warning |
| `resolve(id)` | trim+小写；`""`/`auto` → `dark`；命中返回 `(spec, false)`；未命中返回 `(dark, true)`（第二项表示发生了回退） |
| `get(id)` / `contains(id)` | 大小写不敏感查找；`contains` 额外接受 `auto` |
| `cycle_ids()` | `["auto"] + order`，设置页循环用 |
| `label(id)` | `auto` →「自动（深色）」；未知 →「<id>（缺失，回退深色）」 |
| `custom_count()` / `is_custom(id)` | 统计/判断自定义主题 |
| `ids()` / `warnings()` | `tw doctor` 展示用 |

### 7.2 `load_user_themes` 校验流程

1. `read_dir(dir)`：`NotFound` 直接返回空（无告警）；其它错误记一条 `读取主题目录失败`。
2. 过滤出扩展名为 `.json`（大小写不敏感）的普通文件，按路径排序。
3. 对每个文件执行 `parse_theme_file`：
   - 取文件 stem，校验 UTF-8、1–32 字符、字符集合法；
   - id 转小写后检查保留名；
   - 读取文件内容，用 `ThemeFile`/`PartialColors`（均 `deny_unknown_fields`）反序列化；
   - `base` 解析（缺省 `Dark`），从内置主题里找同 `BaseMode` 的色板作为继承来源；
   - 合并 `colors`：缺省 token 继承 base，写出的 token 逐个 `ThemeColor::parse`；
   - `name` trim 后为空则回退文件名原文；
   - 得到 `ThemeSpec`。任一步出错只把 `"<路径>: <原因>"` 加入 warnings。

### 7.3 颜色与渲染映射

- `ThemeColor::parse` 是唯一颜色入口：返回 `ThemeColor::Rgb(Rgb)` 或 `ThemeColor::Ansi(AnsiColor)`；`parse_hex` 处理 `#rgb` 的逐位复制展开与 `#rrggbb`。
- `BaseMode::parse`：`""`/`dark`/`default` → Dark，`light` → Light，`high-contrast`/`high_contrast`/`contrast` → HighContrast；`as_id()` 返回规范字符串。
- `ui::Theme::from_spec` 逐 token 映射：RGB → `Color::Rgb(r,g,b)`，ANSI → `Color::Reset` / `Color::Black` / `Color::LightCyan` 等 ratatui 常量。
- `App::theme()` 每帧解析一次（13 个颜色的小结构体，开销可忽略），所以设置页改主题即时可见；主题文件本身不参与每帧解析。

### 7.4 相关测试

- `src/theme/mod.rs`：`parses_hex_and_ansi_colors`、`resolves_auto_and_missing_theme`、`custom_theme_lookups_are_case_insensitive`。
- `src/theme/load.rs`：`loads_partial_custom_theme_inheriting_dark`、`rejects_invalid_files_and_reserved_ids`（4 个坏文件 = 4 条告警）、`rejects_unknown_base_and_unknown_token`、`uppercase_filename_is_normalized_to_lowercase_id`。
- `src/ui/theme.rs`：`maps_rgb_values`、`maps_ansi_values`。
- `tests/app_drive.rs`：`custom_theme_cycles_and_renders`、`cli_theme_override_is_applied`。
- `tests/ui_snapshots.rs`：`all_builtin_themes_render_home`、`home_uses_theme_surfaces`（三套内置主题的背景/面板/强调色确实出现在缓冲区里）。

## 8. 常见问题

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| 改完主题文件不生效 | 主题只在启动时加载一次 | 退出并重启 TUI；设置页切换内置/已加载主题不需要重启 |
| `themes/` 目录不存在 | 程序不会创建该目录 | 手工创建 `<home>/themes/`，放入 `*.json` 后重启 |
| 新主题不出现在设置页 | 文件名/JSON 校验失败，被跳过 | 看启动时的 Warning Toast，或运行 `tw doctor` 查看目录与 `!` 告警 |
| 颜色发灰、偏色、出现怪色 | 深色/浅色主题使用 24-bit RGB，终端不支持真彩色时会近似 | 确认终端支持 truecolor（`COLORTERM=truecolor`），或改用高对比/自定义 ANSI 主题 |
| 高对比主题里选中行没有背景变化 | 该主题 `selection` 与 `bg` 同为黑色，选中态靠 `accent` 左条 + 粗体表示 | 属预期设计；自定义主题可自行给 `selection` 一个可见的块面 |
| `--theme` 指定后进设置页第一次跳到的不是指定项 | 设置页循环用精确字符串匹配 | 用设置页重新选择，或把 `setting.json` 的 `theme` 改成主题 id（小写） |
| `--theme` 写了错名字 | 回退深色并启动告警；该值会被 `save_all` 写回设置 | 设置页选一个正确主题，或关闭程序后编辑 `setting.json` 的 `theme` |
| 主题名显示为 id 而不是中文/自定义名 | `name` 缺省或为空白 | 在 JSON 里补 `name` |
| 用 `examples/shot.rs` 截图时自定义主题不生效 | 截图工具使用临时 `home`，没有 `themes/` 目录，只加载内置主题 | 需要看自定义主题效果时用真实 `home` 启动程序；截图流程见 [开发指南](development.md) |
| 文件命名为 `dark.json` 被拒绝 | 使用内置保留名 | 换一个文件名（保留名不区分大小写） |

## 相关文档

- [文档索引](README.md)
- [渲染层](ui.md)（token 在布局/绘制中的使用）
- [设置项参考](settings.md)（「主题」行的行为与默认值）
- [存档与数据格式](persistence.md)（`<home>` 路径与 `setting.json` 字段）
- [与上游对照](upstream-parity.md)（Web 端 CSS 变量主题与 TUI 主题的差异）
