# 设置项参考

本文是 `setting.json` 与设置页的权威参考：文件在哪、何时保存、24 行设置各自的分区 / 默认值 / 取值范围 / 生效方式，以及只有配置文件里才有的 FSRS 参数和仍不支持的字段。设置页的操作步骤见 [使用手册 · 设置](usage.md#设置)，存档格式、版本与备份见 [存档与数据格式](persistence.md)。

设置页由 `src/app/mod.rs` 的 `SETTING_ROWS` 定义、`src/ui/settings.rs` 渲染；默认值定义在 `src/domain/types.rs` 的 `SettingState::default`。本文所有数值都以这两处与 `src/domain/constants.rs` 为准。

> 适用范围：设置页全部 24 行、`setting.json` 可读写字段、FSRS 参数与手工修改；对应版本：typewords-tui 0.1.0。

## 设置文件与保存时机

- **位置**：`<home>/setting.json`。外层是 `SaveEnvelope`：`{ "val": { … }, "version": 23, "updated_at": "RFC3339" }`。
- **字段命名**：`val` 下的具名字段为 camelCase（与 Web 的 `typing-word-setting` v23 同名，如 `wordPracticeMode`、`fsrsEasyLimit`）；`fsrsParameters` 内部沿用 ts-fsrs 的 snake_case（如 `request_retention`、`maximum_interval`）。
- **载入**：启动时读取一次。文件缺失用默认值；JSON 解析失败会把文件改名为 `setting.json.corrupt-<时间戳>.json` 并在界面提示「存档已损坏，已隔离并恢复默认值」；`version` 高于 23 时以默认值启动并提示，且**本次运行只读**（`readonly_settings`，不会覆盖原文件）。
- **保存时机**：设置页每修改一项立即写盘；数字项在输入框按 `Enter` 时写盘；返回主页（`Esc` / `q`）再保存一次；退出程序时统一保存；`--mode` / `--theme` 的启动覆盖也会在退出时写回设置。`readonly_settings` 为真时以上保存全部跳过。
- **写盘方式**：同目录临时文件 + fsync + rename（`src/store/mod.rs` 的 `write_atomic`），不会出现写一半的 JSON。
- **手工修改前提**：先退出程序再改文件。程序运行中修改会被内存状态在下次保存时覆盖。

## 设置页操作

- **进入与选择**：主页 `s` / `4` 或侧栏「设置」；`↑` / `↓`（别名 `j` / `k`）移动，右侧 `↑` / `↓` 指示还有内容。练习中不能进入。
- **修改与返回**：`Enter` / 空格修改——开关类直接切换；模式 / 主题 / 音标循环；数字类弹输入框（只接受 ASCII 数字与 `Backspace`，最多 6 位，留空按 0 解析后按范围收敛，`Enter` 保存、`Esc` 取消）；「清空当前练习进度」与「清除所有数据」弹确认；`Esc` / `q` 保存并回主页。

- **恢复默认只有整体入口**：设置页没有单项重置；「清除所有数据」会把全部设置连同学习记录一起恢复默认（见下文「清除所有数据」）。想只重置某一项，只能删除或手工编辑 `setting.json`（删除后下次启动使用 `SettingState::default`）。
- **分区与顺序**：通用（4 行）→ 复习（3 行）→ 键入（7 行）→ 显示（4 行）→ FSRS（4 行）→ 其他（2 行），共 24 行 = 22 个配置项 + 2 个动作项。

## 分区总表

「生效」列中的「下次生成任务」指回主页刷新任务预览、结算后继续、或下一次启动时；本轮已经生成的词表不会追溯变化。

| 分区 | 配置键 | 界面名称 | 类型 | 默认值 | 可选值 / 范围 | 作用与生效 |
| --- | --- | --- | --- | --- | --- | --- |
| 通用 | `word_practice_mode` | 默认练习模式 | 枚举 | 学习 | 学习 / 自由练习 / 复习 / 随机复习 / 自测 / 默写 / 听写 | 主页 `Enter`、结算「继续 / 重学」使用的模式；下一轮练习生效 |
| 通用 | `theme` | 主题 | 枚举 | 自动（深色） | 自动 / 深色 / 浅色 / 高对比 / 自定义主题 | `auto` 等同 `dark`；切换后下一帧生效 |
| 通用 | `sound_type` | 音标 | 枚举 | 美式 (us) | 美式 / 英式 | 练习页显示英式（`phonetic0`）或美式（`phonetic1`）音标；立即生效 |
| 通用 | `tui_big_word`（`big_word` 行） | 大字单词 | 布尔 | 开启 | 开启 / 关闭 | 关闭后练习页点阵不再 2 倍放大；立即生效 |
| 通用 | `shortcutKeyMap`（`shortcut_settings` 行） | 快捷键 | 入口 | — | — | 进入快捷键编辑页改绑键位；详见 [快捷键](shortcuts.md)；立即生效 |
| 复习 | `word_review_ratio` | 复习比例 | 整数 | 3 | 0~10 | 复习上限 = 每日词数 × 比例；0 表示平时不抽到期复习（词库学完或末组时仍按 1 处理）；下次生成任务生效 |
| 复习 | `auto_add_random_review_when_no_due`（`auto_random_review` 行） | 无到期复习时随机填充 | 布尔 | 关闭 | 开启 / 关闭 | 无到期复习时随机补足「每日词数 × 比例」个已学词；下次生成任务生效 |
| 复习 | `ignore_simple_word` | 忽略简单词 | 布尔 | 关闭 | 开启 / 关闭 | 额外忽略 `SIMPLE_WORDS` 的 42 个常见词；影响今日任务与随机复习；下次生成任务生效 |
| 键入 | `auto_next_word` | 自动下一词 | 布尔 | 关闭 | 开启 / 关闭 | 跟写 / 拼写输入正确后按「切词等待」自动跳下一词；立即生效 |
| 键入 | `repeat_count` | 重复次数 | 整数 | 1 | 1~100 | 跟写阶段每个词重复输入的次数；设为 `100` 时改用下一行的自定义次数；立即生效 |
| 键入 | `repeat_custom_count` | 重复次数(自定义) | 整数 | 1 | 1~100 | `repeat_count == 100` 时实际使用的重复次数；否则不参与计算；立即生效 |
| 键入 | `wait_time_for_change_word`（`wait_time` 行） | 切词等待(ms) | 整数 | 300 | 0~10000 | 自动下一词与重复练习的等待时间；立即生效 |
| 键入 | `space_cooldown_time`（`space_cooldown` 行） | 空格冷却(ms) | 整数 | 0 | 0~10000 | 单词完成后这段时间内忽略空格，防误触连跳；立即生效 |
| 键入 | `input_wrong_clear` | 错误即清空 | 布尔 | 关闭 | 开启 / 关闭 | 输错后约 500ms 清空已输入内容；立即生效 |
| 键入 | `ignore_case` | 忽略大小写 | 布尔 | 开启 | 开启 / 关闭 | 判断对错与字母着色是否忽略大小写；立即生效 |
| 显示 | `show_word_question` | 自测选择题 | 布尔 | 开启 | 开启 / 关闭 | 自测显示 A~D 选项；关闭后只保留 1/2/3 与直接拼写；立即生效 |
| 显示 | `show_etymology_and_rel_words`（`show_etymology` 行） | 显示词源/相关词 | 布尔 | 关闭 | 开启 / 关闭 | 练习页词源卡片；还受练习页 `Ctrl+E` 总开关控制（半生效）；立即生效 |
| 显示 | `show_near_word` | 显示上下词 | 布尔 | 开启 | 开启 / 关闭 | 单词下方显示相邻词；遮罩时不显示；立即生效 |
| 显示 | `show_usage_tips` | 显示使用提示 | 布尔 | 开启 | 开启 / 关闭 | 控制练习页底部「提示：…」一行是否显示（Toast 不受影响）；立即生效 |
| FSRS | `fsrs_easy_limit` | FSRS Easy 阈值 | 整数 | 0 | 0~20 | 错误次数 ≤ 阈值 → Easy；结算时生效 |
| FSRS | `fsrs_good_limit` | FSRS Good 阈值 | 整数 | 3 | 0~20 | 超过 Easy 阈值且 ≤ 本阈值 → Good；结算时生效 |
| FSRS | `fsrs_hard_limit` | FSRS Hard 阈值 | 整数 | 6 | 0~20 | 超过 Good 阈值且 ≤ 本阈值 → Hard，更大 → Again；结算时生效 |
| FSRS | `fsrsParameters.request_retention`（`request_retention` 行） | 目标留存率 | 整数 % | 90 | 50~99 | 传给 FSRS 的期望留存率（文件内为小数）；结算时生效 |
| 其他 | `reset_practice_progress` | 清空当前练习进度 | 动作 | — | — | 确认后删除 `practice-word.json`；立即生效 |
| 其他 | `clear_all_data` | 清除所有数据 | 动作 | — | — | 确认后 `dict.json` 与 `setting.json` 一起恢复默认并删除 `practice-word.json`；立即生效 |

## 重点项详解

### 练习模式、主题、音标与大字

- 默认练习模式决定主页 `Enter` 与结算「继续 / 重学」使用哪套流程；设置页与主页 `m` 都按「学习 → 自由练习 → 复习 → 随机复习 → 自测 → 默写 → 听写」循环。
- 主页 `f`「自由练习」只以自由流程启动一次，**不修改**默认练习模式；`--mode` 会覆盖本次设置值（退出时随设置写回）。
- 主题一行按 `auto` → 深色 → 浅色 → 高对比 → 自定义主题（按文件名排序）循环；`auto` 渲染为「自动（深色）」；自定义主题的新增与修改需要重启进程。主题格式见 [主题系统](themes.md)。
- 音标只影响练习页音标行（`Word::phonetic`：英式取 `phonetic0`，美式取 `phonetic1`），不影响出题、判分或复习调度。
- 「大字单词」只控制练习页 5×7 点阵是否允许 2 倍像素放大；关闭后仍显示 1 倍点阵。即使开启，窗口正文高 < 20 行或本轮最长单词放不下时也会自动用 1 倍。

### 复习比例与随机填充

- 每日复习上限 = `每日词数 × 复习比例`；复习比例 0 时平时不抽到期复习，但词库已学完或学到末组（`is_end`）时仍按比例 1 抽取到期复习（该分支取 `max(复习比例, 1)`）。
- 「无到期复习时随机填充」开启后，候选范围是当前词库已学部分（`last_learn_index` 之前），排除已掌握 / 简单词 / 本轮新词 / 已抽中的词，随机补足 `每日词数 × 复习比例` 个；比例为 0 时不补。
- 「忽略简单词」只改变忽略集合：开启后 `SIMPLE_WORDS` 的 42 个词与已掌握词一起从任务、随机复习与练习推进中排除；关闭时只排除已掌握词。

### 键入相关

- **重复次数**：只对跟写生效（`TypingEngine::should_repeat` 判断练习类型必须为跟写）；听写、默写、自测不重复。范围 1~100：1~99 为固定次数；设为 `100` 时切换到下一行「重复次数(自定义)」的值（对应 Web 的 `repeatCustomCount`，默认 1）。
- **切词等待与空格冷却**：前者是自动下一词 / 重复练习的延迟，后者是单词完成后忽略空格的时间，上限均为 10 秒（默认 300ms / 0ms）。
- **错误即清空与自动下一词**：前者在跟写整词输错后约 500ms 清空输入；后者只对跟写与拼写生效，输入正确后按「切词等待」自动跳词。
- **忽略大小写**：同时影响判分与已输入字母着色；关闭后必须大小写完全一致。

### 显示相关

- **自测选择题**：关闭后自测不显示 A~D 选项，卡片只提示「1 我认识 2 不认识 3 已掌握 或直接拼写」。
- **显示词源/相关词**：练习页词源卡片还受 `Ctrl+E` 总开关控制（进入程序时默认显示），是半生效项。
- **显示上下词**：单词被遮挡（默写 / 听写）时不显示，避免泄露答案。
- **显示使用提示**：关闭后练习页底部的「提示：Ctrl+C 退出练习并保存…」一行不再显示；Toast、快捷键与状态行不受影响。

### FSRS 阈值与目标留存率

- 三个阈值把「本轮该词的错误次数」映射为评级，语义与匹配顺序见「FSRS 参数 · 阈值与评级映射」。
- 目标留存率即期望的记忆留存率，越高复习越频繁、间隔越短；默认 90%，可调 50%~99%。文件里保存为 0.5~0.99 的小数，越界值（0、1、负数等）在运行时回退到 0.9。

### 快捷键

设置页「通用 → 快捷键」是入口，进入后可以改绑练习页的键位。动作名与默认键位沿用上游 `ShortcutKey` / `DefaultShortcutKeyMap`，存档字段是 `shortcutKeyMap`。完整说明见 [快捷键](shortcuts.md)。

### 清空当前练习进度

- 确认后只删除 `practice-word.json`（断点缓存），不修改 `dict.json` 的学习进度、统计、FSRS 卡片与三本系统词典。
- 与启动参数 `--reset-practice` 等价；下次进入练习会重新生成任务。

### 清除所有数据

- 确认文案为「该操作将会清除所有数据，确认继续？」，确认后提示「清除成功」。**不可撤销**。
- 清掉的内容 = `BaseState::default()` + `SettingState::default()` + 删除 `practice-word.json`：统计记录、FSRS 卡片、笔记、错词 / 收藏 / 已掌握三本系统词典、当前学习词典的选择、练习进度，以及**全部设置**（主题、练习模式、FSRS 阈值、复习比例……）都回到默认值。
- 与上游 Web 的「设置 → 数据 → 清除所有数据」（`dataSyncPersistence.clear()`）一致：两端都是把 dict 与 setting 两份状态整体 `setState(getDefault*())` 并清空练习缓存，即「恢复出厂」。上游把该按钮放在「数据」分区，TUI 放在设置页「其他」分区。
- 清完会按全新安装的路径重新选一本学习词典（`catalog` 里有 `cet4` 就用它，否则用词库列表第一本），所以清完能直接开练，不需要再手选词典。
- `dict.json` 或 `setting.json` 的 `version` 高于程序支持版本时本次运行是只读的，此时**不清也不落盘**，只提示「存档版本高于当前程序，只读模式无法清除」，避免界面报成功而重启后数据原样回来。

## FSRS 参数

### 阈值与评级映射

结算时 `Scheduler::grade_by_wrong_times` 按错误次数从低到高匹配阈值（先 Easy、再 Good、最后 Hard），默认值 0 / 3 / 6 对应：

| 本轮错误次数 | 评级 | 说明 |
| --- | --- | --- |
| ≤ `fsrs_easy_limit`（默认 0） | Easy | 一次没错 |
| > easy 且 ≤ `fsrs_good_limit`（默认 3） | Good | 默认 1~3 次 |
| > good 且 ≤ `fsrs_hard_limit`（默认 6） | Hard | 默认 4~6 次 |
| > hard（默认 7 次及以上） | Again | 错得最多 |
| 词不在错误表（理论上不会发生） | Easy | `grade_by_wrong_times(None)` 返回 Easy |

- 错误次数来自 `PracticeData.wrong_times_map`：本轮该词每判错一次累加（输入错误、`Esc` 显示答案、自测「不认识」或选错、默写错误提交等）；同一词多轮巩固会继续累加。
- 自测回答「我认识」或选对答案时，该词在 `rating_map` 里固定为 Good，结算时优先于阈值计算结果。
- 代码按 Easy → Good → Hard 顺序匹配，**不校验**三个阈值是否单调递增；把 easy 设得比 good 大时，Good / Hard 档位可能永远不会命中。建议保持 `easy ≤ good ≤ hard`。

### 评级如何影响下次复习

| 卡片状态 | 评级 | 下次到期 |
| --- | --- | --- |
| 新卡 | Easy | 直接进入复习，FSRS 间隔（≥ 1 天，受最大间隔限制） |
| 新卡 | Good / Hard / Again | 学习步进：Good 10 分钟、Hard 6 分钟、Again 1 分钟 |
| 学习中 / 重学中 | Easy / Good | 进入复习，FSRS 间隔（≥ 1 天） |
| 学习中 / 重学中 | Hard / Again | 6 分钟 / 1 分钟 |
| 复习卡 | Hard / Good / Easy | FSRS 间隔（≥ 1 天，≤ maximum_interval） |
| 复习卡 | Again | 记为一次遗忘，进入重学（10 分钟） |

学习与重学步进由 `src/domain/scheduler.rs` 写死为 1 / 6 / 10 分钟；阈值与留存率只影响评级和 FSRS 间隔计算，不改变这些步进。

### 目标留存率 request_retention

- 默认 0.9；设置页允许 50%~99%。值越大，FSRS 计算的间隔越短、复习越频繁。
- 有效范围是 `0 < request_retention < 1`；文件里写成 `0`、`1` 或负数时运行时回退 0.9。
- 修改后从下一次结算开始影响新写入的卡片；已有的卡片保留原间隔。

### 无 UI 的 FSRS 参数与手工修改

`setting.json` 的 `val.fsrsParameters` 下还有以下字段，设置页没有入口：

| 字段（`fsrsParameters` 内） | 默认值 | 是否生效 | 说明 |
| --- | --- | --- | --- |
| `request_retention` | `0.9` | 生效（有 UI） | 见上节 |
| `maximum_interval` | `36500.0` | 生效 | 复习间隔上限（天），实际按 `clamp(1, maximum_interval)` 截断；小于 1 时回退 36500 |
| `w` | 21 个默认权重 | 生效 | 必须恰好 21 个有限数，否则整组回退默认权重 |
| `enable_fuzz` | `false` | 生效（近似） | 对 ≥ 2.5 天的复习间隔做 ±5% 随机抖动（每次结算用当前随机源，`--seed` 可复现）；不逐项复刻 ts-fsrs 的离散规则 |
| `enable_short_term` | `true` | 不生效 | 学习步进写死，不读取该字段 |
| `learning_steps` | `["1m", "10m"]` | 不生效 | 同上 |
| `relearning_steps` | `["10m"]` | 不生效 | 同上 |

默认权重（`DEFAULT_FSRS_W`）：

```text
0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542
```

手工修改步骤：

1. 退出程序（否则保存时会覆盖你的修改）。
2. 备份 `<home>/setting.json`。
3. 编辑 `val.fsrsParameters`，注意外层字段是 camelCase、`fsrsParameters` 内部是 snake_case；`w` 必须是 21 个数字。
4. 保存为 UTF-8 JSON，`version` 保持 `23`。
5. 启动后到设置页确认「目标留存率」显示正确；间隔变化在下一次结算后生效。

风险：

- JSON 语法错误、字段类型不符（如给整数字段写字符串）、枚举字段写非法数值，会让整份设置解析失败 → 文件被隔离为 `*.corrupt-*.json` 并回退默认值。
- 字段名拼错不会被发现：未知字段会进入 `extra` 原样保存，但不会生效。
- `w` 长度不是 21 或含非有限数时**静默**回退默认权重，不会提示。
- `maximum_interval` 设得过大（如默认 36500 天）等于不再截断；设得过小会让所有词很快反复到期。
- `enable_short_term`、`learning_steps`、`relearning_steps` 写了也不会改变行为（学习步进固定 1 / 6 / 10 分钟）。
- `version` 改成大于 23 的值会导致下次启动回退默认值，并且本次运行不会写回（只读保护）。

## 有 UI 的设置 vs 仅透传字段

| 类别 | 字段 |
| --- | --- |
| 有 UI 且生效（21 项） | `word_practice_mode`、`theme`、`sound_type`、`tui_big_word`、`word_review_ratio`、`auto_add_random_review_when_no_due`、`ignore_simple_word`、`auto_next_word`、`repeat_count`、`wait_time_for_change_word`、`space_cooldown_time`、`input_wrong_clear`、`ignore_case`、`show_word_question`、`show_etymology_and_rel_words`、`show_near_word`、`show_usage_tips`、`fsrs_easy_limit`、`fsrs_good_limit`、`fsrs_hard_limit`、`fsrsParameters.request_retention` |
| 有 UI、条件生效 | `repeat_custom_count`（仅 `repeat_count == 100` 时参与计算） |
| 无 UI、但内部会读取 | `fsrsParameters.maximum_interval`、`fsrsParameters.w`、`fsrsParameters.enable_fuzz` |
| 无 UI、当前不读取（只保存） | `word_practice_type`、`always_show_note`、`dictation`、`translate`、`ignore_symbol`、`_ignoreWatch`、`fsrsParameters.enable_short_term`、`learning_steps`、`relearning_steps` |
| 透传保留 | `extra`：`SettingState` 未建模的 Web 字段都原样保存在这里，保存时写回 |

`SettingState` 共建模 28 个具名字段 + `extra`，设置页只覆盖其中 22 个配置项（外加 2 个动作项：清空练习进度、清除所有数据）。兼容策略是 `#[serde(default)]` + `#[serde(flatten)] extra`：旧文件缺字段用默认值，未知字段原样保留（`tests/store_roundtrip.rs` 验证了 `someFutureField` 往返不丢失）；字段类型不匹配或枚举值非法则会解析失败并触发损坏隔离。

## 未生效 / 半生效设置项清单

| 项 | 状态 | 原因与现状 |
| --- | --- | --- |
| `show_etymology_and_rel_words` 显示词源/相关词 | 半生效 | 还受练习页 `Ctrl+E` 总开关（`app.show_meta`）控制；重启后 `Ctrl+E` 状态恢复为显示 |
| `repeat_custom_count` | 条件生效 | 只有 `repeat_count == 100` 时才参与计算；默认文件里为 `null`，UI 显示 1 |
| `word_practice_type`、`dictation`、`translate`、`ignore_symbol` | 不生效（透传） | Web 端的同名设置字段，TUI 不读取；键入方式由流程模板决定，显隐由 `Ctrl+I` / `Ctrl+Z` 在会话内控制 |
| `fsrsParameters.enable_short_term`、`learning_steps`、`relearning_steps` | 不生效（透传） | 学习步进固定 1 / 6 / 10 分钟；`Scheduler` 读取 `request_retention`、`maximum_interval`、`w`、`enable_fuzz` |
| `always_show_note` | 不生效（透传） | 笔记只有存储（`noteData`），TUI 没有展示或编辑 UI |
| `_ignoreWatch` | 不生效 | 与 Web 存档同名的兼容字段，不参与逻辑 |

## 与 Web 设置字段对照（版本 23）

- **版本**：`setting.json` 的 `version` 恒为 `23`，对应 Web 的 `typing-word-setting` v23；TUI 不升级也不降级该版本号。
- **同名字段**：上表 22 个有 UI 的配置项与 Web 使用相同 camelCase 键名与数值语义（枚举值同样用整数：练习模式 0=学习、1=自由、2=自测、3=默写、4=听写、5=随机复习、6=复习）；Web 其余字段通过 `extra` 原样保留。
- **FSRS 参数**：`fsrsParameters` 与 ts-fsrs 的参数结构一致（snake_case 的 `request_retention` / `maximum_interval` / `w` / `enable_fuzz` 等），因此两端的参数可以手工互相拷贝；TUI 的 fuzz 为近似实现。
- **TUI 新增**：`tuiBigWord` 是 TUI 专属字段，Web 设置里没有对应项。
- **不互通**：TUI 不读写浏览器 IndexedDB，所以设置不会自动双向同步；手工迁移方法见 [存档与数据格式](persistence.md)。

## 常见问题

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| 改了设置本轮没变化 | 词库相关项只在下次生成任务时生效（复习比例、随机填充、忽略简单词等） | 正常；见总表「生效」列，回主页或结算后继续即可 |
| 想恢复默认设置 | 单项重置没有入口，全部重置有 | 设置页「其他 → 清除所有数据」把设置与学习记录一起恢复默认（不可撤销）；只想重置某一项则退出程序后编辑 `setting.json` |
| 手工改完设置，重启丢失 | 修改时程序仍在运行，退出时被内存状态覆盖 | 退出程序 → 改文件 → 再启动 |
| 启动提示设置损坏 | JSON 语法错误、类型不匹配或枚举值非法 | 修复或删除 `setting.json`；坏文件已隔离为 `*.corrupt-*.json` |
| 手工改了 FSRS 参数没效果 | 字段名拼错（落进 `extra`）、`w` 不是 21 个、或程序运行中被覆盖；`enable_short_term` / `learning_steps` / `relearning_steps` 本就不生效 | 退出后修改；核对 snake_case 字段名与 `w` 长度；学习步进固定为 1/6/10 分钟 |
| 设置版本过高，改动不保存 | 存档来自更新版本，本次运行只读 | 备份后升级程序；本次运行的设置不会写回 |
| 设置能否跨设备同步 | v1 无云同步与账号 | 复制数据目录（见 [存档与数据格式](persistence.md)） |

## 相关文档

- [文档索引](README.md)
- [使用手册](usage.md)
- [存档与数据格式](persistence.md)
- [领域逻辑](domain.md)
- [与上游对照](upstream-parity.md)
