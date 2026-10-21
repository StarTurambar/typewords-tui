# TypeWords TUI 领域层详解

`src/domain/` 是 typewords-tui 的领域层：它只回答“今天练哪些词、怎么判对错、练完后记录什么”，不依赖 ratatui/crossterm，也不持有终端输入或页面状态。应用层（`src/app/`）把键盘、鼠标与时钟翻译成领域调用；渲染层（`src/ui/`）只读取领域对象；`src/cli.rs` 复用同一批函数实现 `tw task`、`tw import-dict`，`src/bootstrap.rs` 负责把存档与词库装进内存。

本文件是理解练习闭环（词库 → 今日任务 → 键入推进 → 结算 → 统计）的入口，重点解释算法与不变量，不展开按键、布局与设置界面；这些分别见 [应用层](app.md)、[渲染层](ui.md) 与 [设置项参考](settings.md)。

> 适用范围：仓库维护者、贡献者，以及需要核对练习算法行为的进阶用户；事实以 `src/domain/`、`src/store/` 与 `tests/` 为准。对应版本：typewords-tui 0.1.0。

## 1. 概览

领域层的调用关系如下（箭头表示“调用/读取”，不代表模块依赖顺序）：

```text
终端键鼠 ──▶ src/app ──▶ src/ui；src/cli / src/bootstrap 复用 domain
                  │ 调用
                  ▼
            src/domain（纯数据与算法，不依赖终端）
```

| 文件 | 职责 | 关键导出 |
| --- | --- | --- |
| `src/domain/mod.rs` | 模块声明 | 13 个 `pub mod`（flow 含 navigator 子模块） |
| `src/domain/types.rs` | 数据模型与序列化 | `Word`、`Dict`、`BaseState`、`SettingState`、`PracticeState`、`PracticeData`、`Card`、`TaskWords`、`PracticeCache`、整数枚举 |
| `src/domain/constants.rs` | 与 Web 对齐的常量与默认值 | `GROUP_SIZE`、`CURRENT_FLOW_VERSION`、系统词典 id、`SIMPLE_WORDS`、`DEFAULT_FSRS_W`、三个存档版本 |
| `src/domain/catalog.rs` | 词库目录、词条文件、导入 | `Catalog`、`DictResource`、`import_dict` |
| `src/domain/study_task.rs` | 今日任务生成 | `create_study_task`、`create_shuffle_task`、`get_current_study_word`、`StudyTaskResult` |
| `src/domain/fsrs.rs` | FSRS-6 算法（ts-fsrs 移植） | `Fsrs`、`Fsrs::next_card`、`clip_parameters`、`step_to_minutes` |
| `src/domain/scheduler.rs` | FSRS 评级与到期判定 | `Scheduler`、`Scheduler::grade_by_wrong_times`、`next_card`、`is_due_today` |
| `src/domain/timer.rs` | 计时片段、空闲暂停、跨天拆分 | `tick`、`pause`、`resume`、`normalize_practice_timer`、`format_duration` |
| `src/domain/typing.rs` | 键入状态机 | `TypingEngine`、`TypingSettings`、`TypingEvent`、`KeyInput` |
| `src/domain/question.rs` | 自测选择题 | `build_question`、`Candidate`、`Question` |
| `src/domain/flow/store.rs` | 自建练习流程的存档（`flows.json`） | `FlowStore`、`StoredFlow` |
| `src/domain/article.rs` | 文章模型、断句与分词、句子跟打状态 | `Article`、`parse_sentence`、`gen_sections`、`SentenceTyping` |
| `src/domain/wordbook.rs` | 词条搜索过滤、自定义词典、批量标记、排序 | `filter_words`、`mark_mastered`、`mark_know`、`sort_words` |
| `src/domain/flow/mod.rs` | 流程配置、游标、相位、展示模型 | `PracticeFlowConfig`、`PracticeFlowCursor`、`builtin_flow`、`resolve_phase_by_cursor`、`display_state` |
| `src/domain/flow/navigator.rs` | 会话内的位置推进 | `Navigator`、`NavState`、`Notice` |
| `src/domain/session.rs` | 会话控制器（初始化/恢复/结算） | `PracticeSession`、`PracticeViewState`、`cache_to_compact`、`cache_from_compact` |
| `src/domain/statistics.rs` | 统计落库与按日聚合 | `flush_stat_to_store`、`daily_totals`、`DayTotals` |
| `src/domain/i18n.rs` | 中文文案表 | `tr(key)` |
| `src/domain/rng.rs` | 可注入随机源 | `Rng`、`seeded`、`random_seed`、`shuffle`、`pick_index` |

边界规则：domain 不引用 `ratatui`、`crossterm`、`src/app`、`src/ui`；唯一做文件 I/O 的模块是 `catalog.rs`（依赖 `Paths` 与 `store::write_atomic`），其余模块只处理内存数据与时间。应用层通过 `NavState` 按字段借用 `PracticeData`/`PracticeState` 调用推进器，从而避免领域层反向依赖应用层。

## 2. 数据模型（types.rs）

### 2.1 序列化约定

存档字段名以“能与 Web 端 JSON 互通”为原则，但并非全库统一：

| 结构 | 命名 | 特殊处理 |
| --- | --- | --- |
| 大多数结构 | camelCase（`#[serde(default, rename_all = "camelCase")]`） | 缺字段回退默认值 |
| `Card` | snake_case | 对齐 ts-fsrs；`due`/`last_review` 为 chrono 的 RFC3339 UTC 字符串 |
| `FsrsParameters` | snake_case | 没有 `rename_all`，字段原本就是 snake_case |
| `SettingState` | camelCase | `tuiBigWord`、`_ignoreWatch` 用显式 `rename`；未知字段进 `extra` |
| `Dict` / `DictResource` | camelCase | 字段 `en_name` 序列化为 `enName`，反序列化时兼容 `en_name` 别名；未知字段进 `extra`；`id` 数字/字符串都接受 |
| `Word` | camelCase | `id` 兼容数字、字符串或缺失 |
| `PracticeFlowCursor` | camelCase | `loop_range` 序列化为 `loop` |
| `Statistics` | camelCase | `new_words` 字段序列化为 `new`；无 `extra`，未知字段丢弃 |
| `PracticeCacheCompact` | camelCase | 词表以单词字符串保存（见 12.4） |

### 2.2 整数枚举（数值与 Web 一致）

| 枚举 | 值 | 变体 | 中文名/映射 |
| --- | --- | --- | --- |
| `WordPracticeMode` | 0 | `System` | 学习（智能学习） |
| | 1 | `Free` | 自由练习 |
| | 2 | `IdentifyOnly` | 自测 |
| | 3 | `DictationOnly` | 默写 |
| | 4 | `ListenOnly` | 听写 |
| | 5 | `Shuffle` | 随机复习 |
| | 6 | `Review` | 复习 |
| | 7 | `ShuffleWordsTest` | 随机测试（TUI 无流程） |
| | 8 | `ReviewWordsTest` | 复习测试（TUI 无流程） |
| | 9 | `Custom` | 自定义（TUI 无流程） |
| `WordPracticeType` | 0 | `FollowWrite` | 跟写 |
| | 1 | `Spell` | 拼写 |
| | 2 | `Identify` | 自测 |
| | 3 | `Listen` | 听写 |
| | 4 | `Dictation` | 默写 |
| `WordPracticeStage` | 0 | `FollowWriteNewWord` | 跟写新词 |
| | 1 | `IdentifyNewWord` | 自测新词 |
| | 2 | `ListenNewWord` | 听写新词 |
| | 3 | `DictationNewWord` | 默写新词 |
| | 4 | `FollowWriteReview` | 跟写旧词 |
| | 5 | `IdentifyReview` | 自测旧词 |
| | 6 | `ListenReview` | 听写旧词 |
| | 7 | `DictationReview` | 默写旧词 |
| | 12 | `Shuffle` | 随机复习 |
| | 13 | `Complete` | 完成学习 |

说明：内置流程只使用 0–6 七种模式；7–9 在 `get_flow_id_for_mode` 中兜底为 `system`，且 `PRACTICE_MODE_NAMES` 只登记 7 种，其余名称回退为“学习”。`WordPracticeStage` 仅作为 `PracticeState.stage` 的存档字段保留：TUI 从不更新该字段（始终保持默认值 `FollowWriteNewWord`），流程位置由 `PracticeFlowCursor` 表达。名称映射由 `practice_mode_name`（7 种）与 `stage_name`（全部 10 个阶段值）提供，名称到 i18n 键的对应见第 14 节。

### 2.3 核心结构

| 结构 | 关键字段 | 说明 |
| --- | --- | --- |
| `Word` | `id?`、`custom`、`word`、`phonetic0/1`、`trans`、`sentences`、`phrases`、`synos`、`rel_words`、`etymology` | 词条；`lower()`、`is_usable()`（仅要求 `word` 非空）、`trans_joined()`、`phonetic(uk)` |
| `Trans` / `WordSubContent` | `pos`、`cn`、`frequency?`；`c`、`cn` | 释义、例句与短语 |
| `Syno` / `RelGroup` / `RelWords` / `Etymology` | `pos`、`cn`、`ws`；`pos`、`words`；`root`、`rels`；`t`、`d` | 同义词、同根词、词源 |
| `Dict` | `id`、`en_name`、`name`、`description`、`url`、`length`、`category`、`tags`、`translate_language`、`language`、`last_learn_index`、`per_day_study_number`、`words`、`articles`、`statistics`、`custom`、`system`、`source_id`、`complete`、`cover?`、`extra` | 词库或系统词典；`identity_list`/`matches_id`（id 与 enName 都可作路由）、`effective_length`、`progress_percent`、`per_day` |
| `BookGroup` | `book_list`、`study_index` | `study_index < 0` 表示没有当前词典 |
| `Card` | `due`、`stability`、`difficulty`、`elapsed_days`、`scheduled_days`、`learning_steps`、`reps`、`lapses`、`state`、`last_review?` | 与 ts-fsrs `Card` 对齐；`state` 取值 0–3 |
| `Statistics` | `start_date`、`spend`、`total`、`new`、`review`、`wrong`、`title?`、`segments?`、`session_role?` | 一条学习记录；`segments` 为 `[startMs, endMs]` 数组 |
| `FsrsParameters` | `request_retention`、`maximum_interval`、`w`、`enable_fuzz`、`enable_short_term`、`learning_steps`、`relearning_steps` | 三个 `effective_*` 方法做非法值回退 |
| `BaseState` | `simple_words`、`load`、`word`、`article`、`dict_list_version`、`fsrs_data`、`note_data`、`_ignoreWatch` | `dict.json` 根；`fsrs_data` 的 key 是单词字符串（跨词典共享） |
| `SettingState` | 见 2.4 | `setting.json` 根 |
| `PracticeState` | `stage`、`start_date`、`spend`、`total`、`new_word_number`、`review_word_number`、`input_word_number`、`wrong`、`timer_paused`、`timer_pause_reason`、`segments` | 一次会话的计时与计数；`reset_session` 清零并写入新的 `start_date` |
| `PracticeData` | `index`、`words`、`wrong_words`、`exclude_words`、`all_wrong_words`、`is_typing_wrong_word`、`wrong_times_map`、`wrong_times`、`rating_map`、`question` | 当前词表、本阶段错词、本轮排除词、累计错次与评级覆盖 |
| `TaskWords` | `new`、`review` | 今日任务词表；`total()`、`is_empty()` |
| `Question` / `Candidate` | `candidates`、`correct_index`；`word`、`similarity` | 自测题 |
| `PracticeCache` / `PracticeCacheCompact` | 见第 12 节 | 断点续练 |

### 2.4 SettingState 中被领域层消费的字段

完整设置项参考见 [设置项参考](settings.md)。领域层实际读取的字段：

| 字段 | 默认值 | 领域用途 |
| --- | --- | --- |
| `word_practice_mode` | `System` | 启动练习时的默认模式 |
| `word_practice_type` | `FollowWrite` | 透传字段；TUI 的练习类型由流程决定 |
| `word_review_ratio` | 3 | 复习词与随机复习的倍数 |
| `auto_add_random_review_when_no_due` | false | 无到期复习时随机填充 |
| `ignore_simple_word` | false | 忽略集合是否并入 `simple_words` |
| `ignore_case` | true | 键入比较 |
| `auto_next_word` | false | 跟写/拼写完成后自动切词 |
| `repeat_count` / `repeat_custom_count` | 1 / 无 | 跟写重复次数；`repeat_count == 100` 时使用 `repeat_custom_count`（设置页两行都可编辑） |
| `wait_time_for_change_word` | 300 | 自动切词/重复等待，读时 clamp 到 0–10000 ms |
| `space_cooldown_time` | 0 | 完成后空格冷却，clamp 到 0–10000 ms |
| `input_wrong_clear` | false | 错误提示出现 500 ms 后是否清空已输入 |
| `fsrs_easy_limit` / `good_limit` / `hard_limit` | 0 / 3 / 6 | 错误次数 → 评级 |
| `fsrs_parameters` | 见 3.2 | FSRS 权重、留存率、最大间隔 |
| `tui_big_word` | true | TUI 独有：练习页是否允许 2 倍大字（渲染层读取） |
| `theme`、`sound_type`、`show_word_question` 等 | — | 分别由主题、音标与自测面板读取，不属于领域算法 |

`repeat_count == 100` 时取 `repeat_custom_count.unwrap_or(0)`，再 `max(1)`，由 `effective_repeat_count()` 统一处理；`repeat_custom_count` 可在设置页「重复次数(自定义)」行编辑。

### 2.5 三本系统词典与 TUI 扩展

`BaseState::default()` 在 `word.book_list` 中预置三本系统词典，在 `article.book_list` 中预置一本文章收藏（TUI 不加载文章数据）：

| 词典 | id / enName | 默认名称 | 数据形态与维护者 |
| --- | --- | --- | --- |
| 收藏 | `wordCollect` | 收藏 | `system: true`、`words` 由 `PracticeSession::toggle_collect` 维护，`length` 同步为词数 |
| 错词 | `wordWrong` | 错词 | 键入错误时由 `on_type_wrong` 自动追加；自测答对或“认识”时由 `resolve_identify_correct` 移除 |
| 已掌握 | `wordKnown` | 已掌握 | `description` 为“已掌握后的单词不会出现在练习中”；反引号或自测“已掌握”写入 |
| 文章收藏 | `articleCollect` | 收藏 | 仅保留元数据；`BaseState::shake_common_dict` 在保存时清空非系统文章词表 |

三本单词典都是普通 `Dict` 对象，与学习词典同构（`system: true`、`per_day_study_number: 20`、初始 `words` 为空）；领域层允许把它们设为当前学习词典，但界面词库页只列出 `Catalog` 中的词库资源，系统词典不在可选列表中。默认 `study_index: -1`，即启动时不指向任何词典，由 `bootstrap::ensure_study_dict` 选中词库资源并加载词表。

TUI 对存档模型的扩展是 `SettingState.tui_big_word`（序列化名 `tuiBigWord`）：上游设置结构没有该字段，TUI 用它控制练习页是否按空间放大点阵大字。其余 `BaseState.load`、`dict_list_version`、`note_data`、`PracticeState.stage`、`PracticeData.is_typing_wrong_word` 等字段只保持与 Web 存档同名并原样读写，TUI 不改变其语义或从不写入（`stage` 永远保持默认值）。

## 3. 常量（constants.rs）

### 3.1 通用常量

| 常量 | 值 | 用途/对应 |
| --- | --- | --- |
| `GROUP_SIZE` | 7 | 跟写小组词数；对应 Web 的 `PRACTICE_WORD_GROUP_SIZE`，同时用于 System 新词跟写与错词巩固 |
| `CURRENT_FLOW_VERSION` | 6 | 写入每个内置流程的 `version`；与 Web 的 `CURRENT_FLOW_VERSION` 相同 |
| `DICT_WORD_COLLECT` | `wordCollect` | 收藏 |
| `DICT_WORD_WRONG` | `wordWrong` | 错词 |
| `DICT_WORD_KNOWN` | `wordKnown` | 已掌握 |
| `DICT_ARTICLE_COLLECT` | `articleCollect` | 文章收藏（TUI 仅保留元数据） |
| `SIMPLE_WORDS` | 42 个：a an i my me you your he his she her it what who where how when which be am is was are were do did can could will would the that this and not no yes to of for at in | 默认简单词；与 Web 的 `getDefaultBaseState` 相同 |
| `DICT_STATE_VERSION` | 4 | `dict.json` 存档版本（Web `SAVE_DICT_KEY.version`） |
| `SETTING_VERSION` | 23 | `setting.json` 存档版本（Web `SAVE_SETTING_KEY.version`） |
| `PRACTICE_CACHE_VERSION` | 2 | `practice-word.json` 存档版本（Web `PRACTICE_WORD_CACHE.version`） |

评级与卡片状态常量：`Rating` 是 `u8`，`RATING_AGAIN/HARD/GOOD/EASY = 1/2/3/4`；`CARD_STATE_NEW/LEARNING/REVIEW/RELEARNING = 0/1/2/3`，均与 ts-fsrs 对齐。

### 3.2 默认 FSRS 参数

`default_fsrs_parameters()` 与上游 `getDefaultSettingState().fsrsParameters` 一致：

| 字段 | 默认值 |
| --- | --- |
| `request_retention` | 0.9 |
| `maximum_interval` | 36500.0 |
| `w`（21 个） | 0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542 |
| `enable_fuzz` | false |
| `enable_short_term` | true |
| `learning_steps` | `["1m", "10m"]` |
| `relearning_steps` | `["10m"]` |

调度器消费全部七个参数：`w`、`request_retention`、`maximum_interval`、`enable_fuzz`、`enable_short_term`、`learning_steps`、`relearning_steps` 都已生效（见第 6 节）。

## 4. 词库目录（catalog.rs）

资源布局与上游一致：

```text
<root>/
  list/word.json                  # 词库目录：DictResource 数组
  dicts/<language>/word/<url>     # 词条文件：Word 数组
```

`<root>` 有两处：只读资源目录 `assets` 与可写数据目录 `home`。`Catalog::load` 先读 `assets/list/word.json`、再读 `home/list/word.json`，按 `id` 去重（后读到的 home 条目覆盖同名条目）；只接受 `type` 缺省或为 `"word"` 的条目，`id` 为空则跳过；合并后为空时报错。词条文件查找同样是数据目录优先（`home/dicts/<lang>/word/<url>` 先于 `assets/...`）。

| `DictResource` 字段 | 说明 |
| --- | --- |
| `id` | `serde_json::Value`：字符串或数字；`id_string()` 统一转字符串 |
| `en_name` | 也可作为路由/切换参数；反序列化兼容 `en_name` |
| `name`、`description`、`category`、`tags`、`cover?` | 展示元数据 |
| `url` | 词条文件名，相对 `dicts/<language>/word/` |
| `length` | 声明词数；`to_dict` 在缺省时回退实际词数 |
| `language`、`translate_language` | 缺省语言为 `"en"` |
| `type` / `kind` | 只有 `word`（或缺省）会被收录 |
| `version` | 词库资源版本，TUI 不做升级处理 |
| `extra` | 其余未知字段原样保留 |

| `Catalog` API | 行为 |
| --- | --- |
| `load(paths)` | 合并两处目录并校验非空，返回 `Catalog` |
| `resources()` / `find(id)` | 列出/按 `matches_id`（id 或 enName）查找 |
| `dict_file(resource)` | 返回实际词条文件路径（home 优先） |
| `load_words(resource)` | 读取词条 JSON，`retain(Word::is_usable)` 过滤空词；文件缺失或解析失败返回错误。词条文件存在但没有可用词时返回空列表——新建的自建词典一开始就是空的，用户要往里加词 |
| `user_dict_file(name)` | `home/dicts/en/word/<name>` |
| `read_user_catalog()` | 读 `home/list/word.json`；不存在返回空数组 |
| `write_user_catalog(value)` | 以 pretty JSON 原子写入用户目录 |
| `DictResource::to_dict(words)` | 生成可持久化 `Dict`：`per_day_study_number: 20`、`name` 缺省用 id、`length` 缺省用实际词数、`custom` 取自目录项的 `custom` 标记 |

`import_dict(paths, source)` 的完整行为：

1. 读取 `source`，按 `Vec<Word>` 反序列化（必须是词条数组），过滤不可用词；为空则报错。
2. 调用 `Catalog::load` 取得当前目录；文件名必须以 `.json` 结尾。
3. 把源文件按字节复制到 `home/dicts/en/word/<文件名>`（不重新序列化；同名文件直接覆盖）。
4. 生成 `DictResource`：`id`/`en_name`/`name` 取文件名去掉 `.json`，`description` 为“导入词库（N 词）”，`category` 为“自定义”，`tags` 为 `["导入"]`，`language: "en"`、`translate_language: "zh_CN"`、`type: "word"`、`version: 1`。
5. 读用户目录，按 `id` 判重后覆盖或追加，再用 `store::write_atomic` 写回 `home/list/word.json`。
6. 返回 `DictResource`；CLI 打印后提示用 `tw --dict <id>` 切换。

注意：判重按解析后的 `id` 语义比较（数字与字符串 id 都归一为字符串），重复导入同一文件名会替换原目录项，不会产生重复条目。导入后的词库与内置词库同构，可被选择为当前学习词典。

## 5. 今日任务（study_task.rs）

`create_study_task` 由 `get_current_study_word` 与 `add_random_review_when_no_due` 组合而成，返回 `StudyTaskResult { task_words, due_review_count, random_review_count }`。

`get_current_study_word(base, settings, rng)` 的步骤：

1. 取 `base.study_dict()`；没有词典或词表为空则返回空任务。
2. 计算忽略集合：`base.ignore_words_set(settings.ignore_simple_word)`，即“已掌握词”的小写集合；开启“忽略简单词”时再并入 `simple_words` 的小写形式。
3. 计算每日词数 `per_day = dict.per_day()`：词典完成后取 `per_day_study_number.max(1)`，否则 `per_day_study_number` clamp 到 `[1, effective_length]`。
4. 计算新词：从 `last_learn_index` 开始向后扫描 `dict.words`，跳过忽略集合中的词，收集到 `per_day` 个为止。若 `is_end`（扫描起点 `start >= dict.length - 1` 且 `length != 1`）或词典已完成，则不取新词。
5. 计算到期复习：当 `word_review_ratio >= 1` 或词典完成或已到末尾时执行；目标数量 `per_day * ratio`（`is_end` 时 ratio 至少为 1）。从 `base.fsrs_data` 中筛出同时满足以下条件的词：未在忽略集合、卡片按 `Local` 自然日到期（`due.date <= 今天`）、词条存在于当前词典、不属于本轮新词；按 `due` 升序取前 `total_need` 个，再整体洗牌。
6. 随机填充：若开关开启且到期复习为 0，则 `total_need = per_day * word_review_ratio`（小于 0 按 0），从已学范围 `words[..min(last_learn_index, len)]` 中排除忽略词与本轮新词/复习词后洗牌取前 `total_need` 个。

`create_shuffle_task(base, settings, total, rng)`（随机复习模式专用）从已学范围随机抽取：范围上界在 `last_learn_index == 0` 时取整个词表，否则取 `last_learn_index`；数量缺省为 `per_day * max(word_review_ratio, 1)` 且至少 1；结果全部放在 `review`，`new` 为空。

| 边界情况 | 行为 |
| --- | --- |
| 没有当前词典/词表为空 | 返回空任务；应用层提示“没有可学习的单词！”并且不进入练习 |
| 词典已完成或到末尾 | 不再取新词；仍按 `per_day * max(ratio,1)` 取到期复习 |
| 所有剩余词都在忽略集合 | 新词为空；应用层不启动空任务 |
| 无到期复习且随机填充关闭 | 复习为空，只剩新词 |
| 无到期复习且随机填充开启、`review_ratio = 0` | 填充数量为 0，复习仍为空 |
| 复习候选不足 | 只返回实际候选数，不补齐 |

## 6. FSRS 调度（fsrs.rs + scheduler.rs）

调度分两层：

- `src/domain/fsrs.rs` 的 `Fsrs` 是**算法本体**：逐条移植上游 ts-fsrs 5.4.1 的算法层、状态机、学习步进策略、fuzz 区间与随机源，因此「同一张卡」在 TUI 与 Web 上算出的间隔完全一致。
- `src/domain/scheduler.rs` 的 `Scheduler` 是**应用层包装**：只保留两块与用户设置直接相关的逻辑——按错误次数映射评级、按自然日判断到期。

`Scheduler::new(settings)` 用 `fsrs_parameters` 构造 `Fsrs`；三个评级阈值（`fsrs_easy_limit` / `fsrs_good_limit` / `fsrs_hard_limit`）直接读取设置。

### 6.1 错误次数 → 评级

| 错误次数（`wrong_times`） | 评级 | 默认阈值下的区间 |
| --- | --- | --- |
| `None` | Easy (4) | 没有输入记录 |
| `<= fsrs_easy_limit` | Easy (4) | 0 次 |
| `<= fsrs_good_limit` | Good (3) | 1–3 次 |
| `<= fsrs_hard_limit` | Hard (2) | 4–6 次 |
| 其余 | Again (1) | 7 次及以上 |

阈值是**上界**语义且按 Easy → Good → Hard 顺序判断；默认值 0/3/6 与 Web 一致。若用户手工把阈值写成倒序，先命中的分支生效。结算时若 `rating_map` 中存在该词的显式评级（自测「认识」或选择题答对写入 Good），优先于错误次数。

### 6.2 状态机

`Fsrs::next_card(previous, rating, now)` 是唯一入口，`previous` 为 `None` 表示该词的第一张卡（等价上游的 `createEmptyCard`）。流程与上游 `AbstractScheduler` + `BasicScheduler` 一致：

1. **初始化**：`elapsed_days` = `last_review` 到 `now` 的 **UTC 日期差**（上游 `dateDiffInDays` 用 `Date.UTC`）；`last_review = now`；`reps += 1`。
2. **按状态分派**：`New` → 走学习步进；`Learning` / `Relearning` → 走学习步进；`Review` → 四个评级同时算出，Again 转入 Relearning。
3. **学习步进**（`learning_steps` / `relearning_steps`，默认 `["1m","10m"]` / `["10m"]`）：
   - `Again` → 首步分钟数，步号归 0；
   - `Hard` → `steps.len() == 1 ? round(首步 × 1.5) : round((首步 + 次步) / 2)`，步号不变；
   - `Good` → 仅当存在下一步时给次步分钟数，步号 +1；
   - `Easy` → 无步进项，直接毕业。
4. **毕业判定**：`0 < 分钟 < 1440` 留在学习态（`scheduled_days = 0`）；`分钟 >= 1440` 进 Review（`scheduled_days = floor(分钟 / 1440)`）；`分钟 == 0` 进 Review，间隔由 FSRS 公式算出。

`enable_short_term` 决定调度器：开启（默认）用 BasicScheduler（走学习步进），关闭用 LongTermScheduler（直接按间隔毕业，不走学习步进）。

### 6.3 FSRS-6 公式

对应上游 `algorithm.ts`，全部按同一套权重 `w[0..20]` 计算：

| 量 | 公式 |
| --- | --- |
| 衰减 | `decay = -w[20]`，`factor = round(e^(ln0.9/decay) - 1, 8)` |
| 记忆保持率 | `R(t, S) = round((1 + factor·t/S)^decay, 8)` |
| 初始稳定性 | `S₀(G) = max(w[G-1], 0.1)` |
| 初始难度 | `D₀(G) = round(w[4] - e^((G-1)·w[5]) + 1, 8)` |
| 难度推进 | `Δd = -w[6]·(G-3)`；`next = D + Δd·(10-D)/9`；`D' = clamp(w[7]·D₀(4) + (1-w[7])·next, 1, 10)` |
| 记住时稳定性 | `S' = clamp(S·(1 + e^{w[8]}·(11-D)·S^{-w[9]}·(e^{(1-R)w[10]}-1)·hard·easy), 0.001, 36500)` |
| 忘记时稳定性 | `S'_f = clamp(w[11]·D^{-w[12]}·((S+1)^{w[13]}-1)·e^{(1-R)w[14]}, 0.001, 36500)` |
| 短期稳定性 | `S' = S·sinc`，`sinc = S^{-w[19]}·e^{w[17]·(G-3+w[18])}`，`G >= Hard` 时 `sinc = max(sinc, 1)` |
| 间隔 | `I = clamp(round(S·interval_modifier), 1, maximum_interval)`，再叠加 fuzz |

`interval_modifier = (request_retention^(1/decay) - 1) / factor`。参数按上游 `CLAMP_PARAMETERS` 的 21 段区间夹取；`relearning_steps` 超过 1 步时会额外收紧 `w[17]`/`w[18]` 的上限。

### 6.4 fuzz

`enable_fuzz` 开启时（默认关闭），间隔按上游 `get_fuzz_range` 的分段区间抖动：

| 间隔区间 | 抖动幅度 |
| --- | --- |
| 2.5 – 7 天 | ±15% |
| 7 – 20 天 | ±10% |
| ≥ 20 天 | ±5% |

`delta = 1 + Σ factor·max(min(ivl, end) - start, 0)`；`min_ivl = max(2, round(ivl - delta))`，`max_ivl = min(round(ivl + delta), maximum_interval)`；间隔大于已过天数时 `min_ivl` 至少为「已过天数 + 1」。间隔小于 2.5 天不抖动。

随机源是 Alea PRNG（移植自 ts-fsrs 的 `alea.ts`），种子取自卡片状态 `"{时间戳}_{reps}_{难度×稳定性}"`，因此**同一次复习里四个评级得到同一个随机因子**，且同一张卡的结果可复现。

### 6.5 Card 字段更新

| 字段 | 更新规则 |
| --- | --- |
| `due` | 学习/重学为 `now + 分钟`；毕业为 `now + interval 天` |
| `stability` / `difficulty` | 由 FSRS-6 公式推进（学习阶段也照常计算，只是步进优先于间隔） |
| `elapsed_days` | `last_review` 到 `now` 的 **UTC 日期差**；无 `last_review` 为 0 |
| `scheduled_days` | 复习为间隔天数；学习阶段为 0（分钟 >= 1440 时为整天数） |
| `learning_steps` | 按学习步进策略推进；毕业清零 |
| `reps` | 上一张卡 `reps + 1` |
| `lapses` | 仅在 Review 状态评 Again 时 +1 |
| `state` | New/Learning/Review/Relearning 之间按 6.2 切换 |
| `last_review` | 设为 `now` |

`is_due_today(card, today)` 按 `Local` 自然日比较，与今日任务筛选规则相同。

### 6.6 与 Web 的一致性

评级映射、到期按自然日筛选、`fsrs_data` 的键与 Card 序列化格式、FSRS-6 的公式、学习步进、fuzz 区间与随机种子**全部与上游一致**；同一张卡在两端得到相同的下次复习时间。

一致性由 `tests/fsrs_parity.rs` 锁定：样本 `tests/fixtures/fsrs_golden.json` 由 `tools/fsrs_golden.mjs` 在上游仓库里用 ts-fsrs 5.4.1 生成，覆盖新卡起步、学习步进、毕业复习、同日短期记忆、fuzz 与长期调度六类路径，共 21 步逐步逐字段比对。重新生成样本需要在上游执行 `pnpm install`，命令见 [开发指南 · 词库同步与上游对齐](development.md)。

## 7. 计时（timer.rs）

`PracticeState` 用两个字段表示时间：`segments: Vec<[i64; 2]>` 是本地时间毫秒的闭开片段 `[start, end)`，`spend` 是累计毫秒。会话创建时 `resume(stat, now)` 会压入一个零长度片段，之后每个 tick 把它拉长。

| 函数 | 行为 |
| --- | --- |
| `now_ms()` | `Local::now().timestamp_millis()`；全项目统一用它取本地时间戳 |
| `tick(state, last_activity, now)` | 暂停返回 `Paused`；空闲超时返回 `IdlePaused`；否则 `spend += 1000` 并把最后片段 `end` 设为 `now`，返回 `Ticked` |
| `pause(state, reason, now)` | 已暂停则忽略；否则把最后片段 `end` 夹到 `now`，记录 `timer_pause_reason` |
| `resume(state, now)` | 清除暂停标记并压入 `[now, now]` 新片段 |
| `close_open_segment(state, now)` | 未暂停时把最后片段 `end` 夹到 `now`；结算前调用 |
| `normalize_practice_timer(segments, spend, now)` | 清洗、合并、裁剪、跨天拆分，返回 `(segments, spend)` |
| `format_duration(ms)` | 至少 1 小时输出“x小时y分钟”，有分钟输出“y分钟”，否则“z秒” |

自动暂停：`IDLE_TIMEOUT_MS = 3 * 60 * 1000`。`tick` 在 `now - last_activity >= 180000` 时把最后片段结束在 `last_activity + 180000`（不超过 `now`），标记暂停、原因为 `"auto_idle"`；手动暂停（`Ctrl+P`）由应用层调用 `pause(state, "manual", now)`。

`normalize_practice_timer` 的清洗顺序：

1. 丢弃 `end <= 0` 的片段，把 `end` 夹到 `now`，丢弃 `start < 0` 或 `end <= start` 的片段。
2. 按 `start` 排序，合并重叠或相接片段（`start <= last.end` 时取较大 `end`）。
3. 若片段总长超过 `spend + TIMER_DRIFT_TOLERANCE_MS`（2000 ms），反复从最长的片段尾部裁剪，直到不超限。
4. 把每个片段按本地自然日午夜拆分，得到跨天片段集合；返回的 `spend` 是拆分后片段总长（无片段时保留原 `spend`）。

计时与结算的关系：结算前先 `close_open_segment`，再 `normalize_practice_timer` 得到最终的 `segments`/`spend`；`flush_stat_to_store` 用片段按天写入统计，摘要页显示 `stat.spend`；练习进行中主页“今日学习时长”由应用层把已持久化统计与当前 `spend` 相加。计时只在练习页有会话时由应用层每秒驱动，详见 [应用层](app.md)。

## 8. 键入状态机（typing.rs）

`TypingEngine` 只处理“当前这个词怎么输入”，不直接改词典或会话分组；`TypingEvent` 交给应用层，再由会话层处理错词与推进。

```text
未锁定 ──字符/空格──▶ 输入中 ──整词正确──▶ 已锁定 ──空格/on_tick 到期──▶ Completed
   ▲                     │ 不匹配                      │ 退格
   │                     ▼                             ▼
   └────────────────  Wrong 事件 ◀── 错误提示     输入中
```

| 类型 | 取值 | 含义 |
| --- | --- | --- |
| `KeyInput` | `Char(char)` / `Space` / `Backspace` | 应用层把可打印键映射进来；后两者单独区分 |
| `TypingEvent` | `Completed` / `Wrong` / `ShowWordResult(bool)` / `Notice(TypingNotice)` / `ResetForRepeat` | 完成、记错、显隐结果、提示、重复练习重置 |
| `TypingNotice` | `DeleteReinput` / `SpaceContinue` | “请按删除键重新输入” / “按空格键继续” |
| `TypingSettings` | `ignore_case`、`repeat_count`、`wait_ms`、`space_cooldown_ms`、`auto_next_word`、`input_wrong_clear` | 由 `PracticeSession::typing_settings` 从 `SettingState` 归一化生成 |

五种练习类型的默认行为（默认显隐由 `resolve_view_defaults` 给出）：

| 练习类型 | 默认显隐（遮罩/翻译） | 键入规则 | 完成方式 |
| --- | --- | --- | --- |
| 跟写 `FollowWrite` | 否 / 是 | 整词输入（未遮罩），逐字符拼接后整体比较 | 完成后再按空格；开启自动下一词时等待 `wait_ms` 自动完成 |
| 拼写 `Spell` | 是 / 是 | 遮罩逐字符输入，错误字符不计入 `input` | 同跟写（自动下一词生效） |
| 听写 `Listen` | 是 / 否 | 遮罩逐字符输入；完成后触发 `ShowWordResult(true)` | 揭示后按空格 |
| 默写 `Dictation` | 是 / 是 | 整词输入，空格提交；输入与目标都做 `normalize_word` 后才比较 | 提交正确后揭示，再按空格继续 |
| 自测 `Identify` | 否 / 否 | 键入任意字符立即 `ShowWordResult(true)` 并记一次错；主要交互为 1/2/3 与 Alt+1..4 | 选择或标记后由应用层推进 |

关键规则：

- 未遮罩时走“整词尝试”：`whole_input_attempt = Some(true)`，输入长度达到目标长度时整体比较；错误后 `input_lock` 保持，任意键提示“请按删除键重新输入”，需退格重来。遮罩时走逐字符比较，错字符记入 `wrong` 并触发 500 ms 错误提示。
- 大小写：`ignore_case = true` 时逐字符 `to_lowercase()` 比较；关闭后严格相等。
- 全角与中文标点：`Dictation` 比较前对输入和目标做 `normalize_word`（如 `’`→`'`、`。`→`.`、`，`→`,`）；其他类型在逐字符比较时用 `normalize_input_character`，若目标字符是全角 ASCII 对应字符（如目标 `？`），则把输入的 ASCII 键（`/`）映射回目标字符。反向映射覆盖 `！￥…（—？》〈“”：）、《》。，’；【】` 等。
- 空格：默写类型中空格是提交键（输入非空且达到目标长度或目标不含空格时提交）；其他类型空格按普通字符参与输入；已锁定时空格是确认/继续键，并受 `space_cooldown_ms` 限制。
- 退格：默写已揭示时清空全部输入与错误；有错误提示时先清提示；否则退一格。输入被清空时发出 `ShowWordResult(false)` 并重置整词尝试状态。
- 自动下一词：仅跟写/拼写生效。整词正确后设置 `jump_deadline = now + wait_ms`，`on_tick` 到点且目标词未变时发出 `Completed`。
- 重复练习：仅跟写在 `repeat_count > word_repeat_count + 1` 时生效；正确后设置 `repeat_deadline`，到点清空输入与错误、`word_repeat_count += 1` 并发出 `ResetForRepeat`，不推进会话。
- 锁定状态下的按键：整词尝试已完成但结果错误时任何键都只提示删除重输；已正确但尚未继续时按非空格键，连续 3 次提示“按空格键继续”；揭示状态下连续 3 次空格提示删除重输。
- `on_tick` 统一处理三类定时器：自动跳词、重复重置、错误提示清除（`input_wrong_clear` 为真时同时清空输入）。`reset()` 在换词、跳阶段、恢复缓存后清空全部键入状态；`mark_correct(word)` 供自测按钮直接把词判对。`confirm()` 等价于一次空格。

## 9. 流程编排（flow/mod.rs）

流程把“模式 → 节点 → 步骤”描述成数据，会话层只按游标执行。

| 结构 | 字段/变体 | 说明 |
| --- | --- | --- |
| `PracticeFlowConfig` | `id`、`version`、`mode`、`label`、`nodes` | 内置流程配置（静态构造，不序列化） |
| `PracticeFlowNode` | `id`、`label`、`source`、`steps` | 一个学习阶段（新词/复习/自由/随机） |
| `PracticeFlowStep` | `template_id`、`word_advance`、`on_end`、`shuffle_on_enter` | 一个练习步骤 |
| `PracticeStepTemplateId` | `FollowWrite`、`Spell`、`Listen`、`Dictation`、`Identify` | 步骤模板，映射到 `WordPracticeType` |
| `PracticeWordsSource` | `TaskNew`、`TaskReview`、`Current`、`WrongWords` | 步骤的初始词源 |
| `WordAdvance` | `Increment`；`WordLoop { group_size, sub_steps }` | 单词推进方式 |
| `PracticeEndAction` | `WrongWordClear`、`CollectWrongWords`、`GenerateReport`、`NavigateNextStep`、`NavigateComplete` | 收尾队列 |
| `PracticePhase` | `practice_type`、`word_advance`、`on_end` | 由游标解析出的当前相位 |

游标 `PracticeFlowCursor` 是唯一位置指针：`node_index`、`step_index`、`in_wrong_word_clear`、`loop_range: Option<PracticeLoopRange>`（序列化为 `loop`）、`end_action_index`；`PracticeLoopRange` 为 `start_index`/`end_index`/`sub_step_index`。`at_start()` 用四个字段共同判断是否在起点。

七个内置流程（与 Web 的 `BUILTIN_FLOWS` 逐项一致）：

| 模式 | flow id / label | 节点（词源） | 步骤（推进方式，进入时洗牌） |
| --- | --- | --- | --- |
| System 学习 | `system` / `smart_learning` | `new`（`TaskNew`）+ `review`（`TaskReview`） | 新词：FollowWrite（WordLoop 7 + Spell 子步骤）、Listen（Increment，洗牌）、Dictation（Increment，洗牌）；复习：Identify（Increment）、Listen（洗牌）、Dictation（洗牌） |
| Free 自由练习 | `free` / `free_practice` | `practice`（`Current`） | FollowWrite（Increment） |
| Review 复习 | `review` / `review` | `review`（`TaskReview`） | Identify、Listen（洗牌）、Dictation（洗牌） |
| IdentifyOnly 自测 | `identifyOnly` / `identify` | `new` + `review` | 各一步 Identify（Increment） |
| DictationOnly 默写 | `dictationOnly` / `dictation` | `new` + `review` | 各一步 Dictation（Increment，洗牌） |
| ListenOnly 听写 | `listenOnly` / `listen` | `new` + `review` | 各一步 Listen（Increment） |
| Shuffle 随机复习 | `shuffle` / `random_review` | `practice`（`TaskReview`） | Dictation（Increment，洗牌） |

所有步骤的 `on_end` 都是同一个默认动作：`WrongWordClear { template_id: FollowWrite, word_advance: WordLoop(7, [Spell]) }`，因此错词巩固也按“7 词小组跟写 + 组内拼写”执行。`get_flow_config` 对未知 id 回退 `system`；`flow_for_mode` 用 `get_flow_id_for_mode` 做 7 种模式的映射，其余模式（测试/自定义）回退 `system`。

关键函数：

| 函数 | 语义 |
| --- | --- |
| `resolve_flow_start(config, task_words, wrong_words, shuffle)` | 从第一个“词源非空”的节点起步，返回初始词表、`total`/`new_word_number`/`review_word_number` 与游标；全部节点都为空返回 `None` |
| `get_step(config, cursor)` | 取当前步骤；游标越界回退第一节点第一步 |
| `materialize_step(step)` | 把步骤模板展开成 `PracticePhase` |
| `resolve_phase_by_cursor(config, cursor)` | **唯一相位查询入口**：先解析主步骤；处于错词清空时改取当前 `end_action_index` 指定的 `WrongWordClear` 动作；存在 `loop_range` 时用子步骤替换练习类型（保留主相位的推进方式与 `on_end`） |
| `advance_step_cursor(config, cursor)` | 返回下一游标与“是否已到终点”；跨节点时进入下一节点第 0 步，否则步骤 +1，并清空错词/子步骤/动作下标 |
| `display_state(config, cursor, word_index, word_count)` | 生成状态文案与阶段条：子步骤显示“小组巩固”，错词清空显示“错词巩固”，多节点/多步骤时显示“节点 · 步骤”，否则显示流程名；单节点单步骤只返回一根进度条 |

小组循环（WordLoop）语义：每完成 `group_size` 个词，把该组回放一遍子步骤（System 新词跟写后回放 Spell）；最后一组不满 `group_size` 时在词表结束时补齐回放。进入循环时 `cursor.loop_range` 生效、`data.index` 回到组首；子步骤用尽后 `index = end_index + 1` 继续主词表。组首若仍是进入循环前的那个单词（如组内只剩 1 个词、错词巩固只剩 1 个错词），单词键不变、界面不会自行重置，此时 `enter_loop` 置位 `NavState.reset_word`，由会话层转存到 `PracticeSession::reset_word`、界面层在 `after_advance` 消费。`display_state` 的阶段条被 [练习页](screenshots/practice-120x40-dark.png) 使用。

`CURRENT_FLOW_VERSION` 只写入 `PracticeFlowConfig.version`，并参与会话的 `phase_key`（用于检测相位变化）；断点缓存不保存流程版本，恢复时依靠 `flow_id` 命中内置流程 + 游标结构校验（见 12.4）。缓存的版本兼容由 `PRACTICE_CACHE_VERSION` 控制。

## 10. 推进器（flow/navigator.rs）

`Navigator` 在会话内维护位置与“同一节点内的工作词表”，负责按游标推进、执行 `on_end` 队列、处理错词清空与快照恢复。

| API | 说明 |
| --- | --- |
| `new(flow)` / `load_flow(flow_id)` / `active_flow_id()` | 创建或切换流程 |
| `current_phase()` | 委托 `resolve_phase_by_cursor` |
| `initialize_node_words(words)` / `node_working_words()` | 当前节点的工作词表：**同一 Node 内跨 Step 传递**，跨 Node 时按新节点词源重新解析（`Current` 除外，沿用工作词表） |
| `set_cursor(cursor)` | 直接设置游标（恢复缓存/测试用） |
| `next(&mut NavState, is_typing, ignore_loop)` | 完成当前词并前进；`is_typing` 决定是否累计 `input_word_number`；`ignore_loop` 跳过子步骤收尾 |
| `prev(&mut NavState)` | 回到上一词；已在第一个词时返回 false 并产生警告提示 |
| `skip_step(&mut NavState)` | 跳到当前词表末尾并走正常收尾（不累计输入，跳过子步骤） |
| `complete_current_list(&mut NavState)` | 外部消费完当前词表后按 `on_end` 收尾；当前无调用点 |
| `build_snapshot()` / `restore_snapshot(...)` | 生成/恢复 `PracticeSessionSnapshot`（`flow_id` + 游标 + 工作词 key 列表） |

`next` 的循环逻辑：

1. 若当前词为空，直接 `run_step_advance`。
2. 把当前词的 `wrong_times` 累加进 `data.wrong_times_map` 并清零，按 `is_typing` 累加 `input_word_number`。
3. 若到达词表末尾（或循环区间末尾）执行 `handle_list_end`，否则按 `WordAdvance` 推进下标。
4. `handle_list_end` 依次处理：循环子步骤收尾 → 最后不足一组的主阶段补循环 → 错词清空（仍有错词则重新洗牌重练）→ 执行 `on_end` 队列 → 进入下一步骤。
5. 推进后若新词为空或属于“需要跳过”的词（空串、已掌握、`exclude_words` 中的词），继续循环，且不再重复累计输入与错次。

`on_end` 动作语义：

| 动作 | 行为 |
| --- | --- |
| `WrongWordClear` | 过滤本阶段 `wrong_words`；非空则进入 `in_wrong_word_clear`，把错词洗牌后作为词表重练并提示“还有错词，继续巩固一下吧”；为空则继续下一个动作 |
| `NavigateNextStep` | 跳出动作队列，进入下一静态步骤（`run_step_advance`） |
| `NavigateComplete` | 标记会话完成 |
| `CollectWrongWords` | 当前只发一条提示，结算阶段统一处理 |
| `GenerateReport` | 空操作 |

错词清空流程：`on_type_wrong` 会把错词加入 `data.wrong_words` 和全局错词本；步骤结束触发 `WrongWordClear` 时，先按忽略集合与 `exclude_words` 过滤，再洗牌重练；重练中再次出错的词会再次进入 `wrong_words`，直到练对或达到词表末尾，然后回到 `on_end` 队列的下一动作。`Notice` 携带 `NoticeLevel`（Success/Info/Warning/Error），由应用层转成 Toast。

快照恢复时逐项校验：`flow_id` 必须命中内置流程（否则回退 `system`）；节点/步骤下标越界、`in_wrong_word_clear` 但 `end_action_index` 未指向 `WrongWordClear`、`loop_range` 与 `WordLoop` 不匹配或范围超出当前词表，都会重置到 `system` 的初始游标；工作词表优先用 `node_working_word_keys` 从任务词/当前词/错词中按单词字符串重建，缺 key 时按当前节点词源重建。

现状与限制：`create_task_from_group`（跳组）已在会话层实现但未接到界面；`complete_current_list` 无调用点；“上一组/下一组”尚未实现。对应界面入口与路线图见 [应用层](app.md) 与 [与上游 TypeWords 的差异](upstream-parity.md)。

## 11. 自测题（question.rs）

`build_question(word, list, rng, max_count)` 为自测阶段生成 4 选 1 的释义题。候选来自当前词典的全部单词（应用层传入打乱后的 `all_words`），先排除“同词（忽略大小写）”和“中文释义拼接完全相同”的词。

相似度评分先构造每个词的释义映射 `cn -> 频次之和`（`frequency` 缺省记 0），再取释义较少的一方作为 `word1`，逐项累加：

| 评分项 | 分值 |
| --- | --- |
| 相同中文释义 | `freq1 * freq2 * TRAN_EQUALS_BASE_SCORE`（65536） |
| 两侧释义文本的字符集合重合 | 重合字符 +4，缺失 -1（双向各算一次） |
| 单词本身的字符集合重合 | 同上，乘以 `WORD_COMMON_FACTOR`（65536） |
| 一个单词包含另一个单词 | `WORD_CONTAIN_SCORE`（1 << 20） |
| 相关词（`rel_words`）中出现对方 | `WORD_CONTAIN_SCORE` |
| 随机扰动 | `rng.random_range(0..1)^2 * FUZZ_SCORE_BASE`（1 << 18） |

得分升序排序后取相似度最高的 `max_count - 1` 个候选，再把目标词以 `similarity = +∞` 追加，整体洗牌，`correct_index` 用目标词的原始字符串定位。边界：`max_count` 至少按 2 处理；候选不足时选项少于 4 个；即使候选为 0 也会返回“只有正确项”的题；应用层用 `max_count = 4`。随机扰动与洗牌都来自注入的 `Rng`，固定种子时题目可复现。

## 12. 会话（session.rs）

`PracticeSession` 是领域层对外的“一次练习”控制器，应用层只通过它推进练习；键位、渲染与存档时机属于应用层。

```text
new(mode) ─▶ initialize_task（重置 stat、开启计时）─▶ 键入/跳过/上一词/跳阶段 ─▶ complete
                  ▲                                                       │
                  │                                  settle（统计 + FSRS 卡 + 词典进度）
                  └────────── 下一组 / 重学（结算后选择） ◀────────────────┘
```

| 会话字段 | 说明 |
| --- | --- |
| `nav` | `Navigator`，持有流程、游标与节点工作词表 |
| `data` | 当前练习数据（词表、错词、错次、评级、题目） |
| `task_words` | 本次任务的新词/复习词 |
| `all_words` | 应用层设置的整本词典乱序词表，仅用于自测题 |
| `display` | `DisplayPolicy`：遮罩/翻译覆盖、揭示状态、已揭示词集合 |
| `mode` / `complete` / `last_word_key` | 当前模式、是否完成、上一词 key（用于换词检测） |

### 12.1 显隐规则

`resolve_view_defaults(practice_type)` 给出默认遮罩与翻译：

| 练习类型 | 遮罩单词 | 显示翻译 |
| --- | --- | --- |
| `Spell` | 是 | 是 |
| `Listen` | 是 | 否 |
| `Dictation` | 是 | 是 |
| `Identify` | 否 | 否 |
| `FollowWrite`（及其他） | 否 | 是 |

`PracticeViewState { practice_type, is_word_masked, is_show_translate, reveal_all }` 在默认值上叠加 `DisplayPolicy` 的覆盖；`reveal_all = show_full_word || show_word_result` 时强制取消遮罩并显示翻译。`refresh()` 返回 `(word_changed, phase_changed)`：相位变化（`phase_key` 变化）清空遮罩/翻译覆盖与揭示集合，换词清空揭示标记。`Esc` 显示完整单词时，若当前类型不是 `FollowWrite`，或单词处于遮罩状态，则记录一次错误（`show_word`）。

### 12.2 关键操作

| 操作 | 行为 |
| --- | --- |
| `next/prev/skip_step` | 构造 `NavState`（`is_simple` 为已掌握词小写集合）后调用推进器，返回 `Notice` 列表 |
| `skip` | 把当前词加入 `exclude_words`，再以 `is_typing = false` 前进 |
| `on_type_wrong` | `wrong_times += 1`，记入 `all_wrong_words` 与错词本，加入 `wrong_words`，并从 `exclude_words` 移除 |
| `on_word_know` | `rating_map[word] = Rating::Good`，然后按“自测答对”清理错词、置 `wrong_times = 0` 并排除该词 |
| `on_word_mastered` | 切到/切回“已掌握”词本，并做与答对相同的清理 |
| `toggle_word_simple_for_current` | 在已掌握词本中增删当前词（忽略大小写），同步当前会话 `exclude_words` |
| `toggle_collect` / `is_collect` | 在收藏词本中增删/查询当前词 |
| `show_word` / `hide_word` | 揭示完整单词（按规则记一次错误） / 收起 |
| `toggle_dictation` / `toggle_translate` | 切换遮罩与翻译覆盖 |
| `random_write` | 打乱当前词表、回到第一个词并打开遮罩 |
| `update_question` | 仅自测：用 `all_words` 生成题目；其他类型清空题目 |

### 12.3 写入时机

| 数据 | 写入时机 | 位置 |
| --- | --- | --- |
| 错词本 | 每次键入错误立即写入 | `on_type_wrong` → `BaseState::wrong_mut` |
| 收藏 | 用户按 Enter | `toggle_collect` |
| 已掌握 | 反引号键或自测“已掌握” | `toggle_word_simple_for_current` / `on_word_mastered` |
| FSRS 卡 | 会话结算 `settle` | `Scheduler::next_card` → `BaseState::fsrs_data` |
| statistics | 会话结算 `settle` | `flush_stat_to_store` |
| `last_learn_index` / `complete` | 结算（见 12.5）与继续下一组 | `advance_dict_progress` |
| 练习缓存 | 应用层的定时/推进/退出保存 | `build_cache` + `cache_to_compact` |

### 12.4 断点续练缓存

内存态 `PracticeCache { task_words, practice_data, stat_store_data, session_snapshot }` 不能直接序列化；落盘前转成 `PracticeCacheCompact`，恢复时再结合当前词典词表还原 `Word`：

| 函数 | 行为 |
| --- | --- |
| `build_cache()` | 克隆任务词、练习数据与 `nav.build_snapshot()`；`stat_store_data` 留空，由应用层填入当前 `PracticeState` |
| `cache_to_compact(cache)` | 任务词与 `words`/`wrong_words` 转成单词字符串；保留 `exclude_words`、`all_wrong_words`、`wrong_times_map`、`wrong_times`、`rating_map`、`index`；`question` 一律丢弃（置 `None`）；`stat_store_data` 与 `session_snapshot` 原样保存 |
| `cache_from_compact(compact, words)` | 按当前词典 `words` 的单词字符串重建 `Word`；任务词为空、缺少 `practice_data`/`stat_store_data`/`session_snapshot` 任一、或词典中找不到词时返回 `None` 或丢弃缺失项；`index` 夹到有效范围 |
| `apply_cache(cache, rng, stat, now)` | 先备份当前会话状态；恢复任务词/练习数据/统计并调用 `nav.restore_snapshot`；校验失败则整体回滚并返回 false；成功时按流程恢复模式、若未暂停则 `resume` 计时、重建题目 |
| `nav.restore_snapshot(...)` | 校验 `flow_id`、节点/步骤、错词清空动作、`loop_range` 后恢复游标；工作词表按 `node_working_word_keys` 重建 |

`PracticeSessionSnapshot` 只保存 `flow_id`、`cursor` 与 `node_working_word_keys`，不保存流程版本与词表内容；词表内容以任务词 + 当前词典为准。缓存版本不匹配（非 `PRACTICE_CACHE_VERSION = 2`）时，存储层直接丢弃并提示，不进入领域层。

### 12.5 结算（settle）

`settle(base, settings, stat, now)` 的顺序：

1. `stat.wrong = data.all_wrong_words.len()`（本轮仍错的唯一词数）。
2. 非 Shuffle 模式调用 `advance_dict_progress(base, stat.new_word_number, false)`：`last_learn_index += new_word_number`；从推进后的位置统计“已掌握词”数量并计入完成判断，达到 `length` 时置 `complete = true` 并把下标设为 `length`。
3. `close_open_segment` 后 `normalize_practice_timer`，把清洗结果写回 `stat`。
4. `flush_stat_to_store(base.study_dict_mut(), stat)` 写入统计。
5. 构造 `Scheduler`，遍历 `data.wrong_times_map`：评级优先取 `rating_map`，否则按错误次数映射；用 `scheduler.next_card` 更新 `base.fsrs_data`。`settle` 只在会话完成时调用一次，练习过程中不写 FSRS 卡。

应用层 `finish_session` 在 `settle` 后生成 `SummaryData`：

| 字段 | 来源 |
| --- | --- |
| `spend_ms` | `stat.spend`（归一化后） |
| `accuracy` | `round((total - wrong) / total * 100)`，`total == 0` 时为 100 |
| `new_words` / `review` / `wrong` | `stat.new_word_number` / `stat.review_word_number` / `stat.wrong` |
| `last_learn_index` / `length` / `complete` | 结算后的当前学习词典 |

后续会话：`create_next_task` 在 Shuffle 模式下重新 `create_shuffle_task`；其他模式若词典尚未完成，会再以 `check_all = true` 调一次 `advance_dict_progress`（与 Web 的 `createNextTask` 行为一致），然后 `create_study_task`。`create_repeat_task` 回滚 `last_learn_index -= stat.new_word_number`（Shuffle 除外），按忽略集合过滤任务词后重学。

### 12.6 模式差异

| 模式 | 任务生成 | 流程词源 | 结算推进词典进度 |
| --- | --- | --- | --- |
| System | `create_study_task` | 新词 + 复习 | 按 `new_word_number` 推进 |
| Free | `create_study_task` | `Current`（新词 + 复习） | 按 `new_word_number` 推进 |
| Review | `create_study_task` | 仅 `TaskReview` | `new_word_number = 0`，只做完成判定 |
| IdentifyOnly / DictationOnly / ListenOnly | `create_study_task` | 新词 + 复习 | 按 `new_word_number` 推进 |
| Shuffle | `create_shuffle_task` | `TaskReview` | 不推进 `last_learn_index` |

## 13. 统计（statistics.rs）

`flush_stat_to_store(dict, stat)` 把一次会话的 `PracticeState` 写入 `dict.statistics`：

1. `spend == 0` 且没有任何 `end > start` 的片段时直接返回，不产生记录。
2. 有片段时按片段 `start` 的本地自然日分组（保持首次出现顺序）；每组累加 `spend` 并保存该日片段。
3. 只有一天时写一条 `session_role: "single"`；跨天时每天一条，角色依次为 `"start"` / `"middle"` / `"end"`。每条记录都带本次会话的 `total`、`new`、`review`、`wrong`，`start_date` 取当日第一个片段起点，`title` 恒为 `None`。
4. 没有片段时写一条 `session_role: "single"`，`spend` 用 `stat.spend`、`start_date` 用 `stat.start_date`、`segments: None`。

`daily_totals(base)` 遍历 `word.book_list` 中所有词典的 `statistics`（包括历史学习过的词典与三本系统词典），按 `start_date` 的本地日期聚合成 `BTreeMap<NaiveDate, DayTotals>`；`DayTotals` 包含 `spend`、`total`、`new_words`、`review`、`wrong`。

聚合规则：`spend` 对每条记录都累加（跨天会话的时长按天正确拆分）；计数（`total`/`new`/`review`/`wrong`）只取 `session_role` 为 `single`、`start` 或缺失（旧存档）的记录，`middle`/`end` 只提供时长。这样跨天会话的计数只归到开始日，不会在多天里重复累计。

| 消费方 | 使用的聚合 |
| --- | --- |
| 应用层 `refresh_stats` | 今日 `statistics` 汇总 + 进行中会话的 `stat.spend`，供主页今日统计 |
| `src/ui/insights.rs` 的 `totals` / `week_days` / `recent_days` | 累计总量、正确率、本周记录、最近 14 天，供主页与统计页 |
| `src/ui/stats.rs` | 统计页概览与逐日条目 |

注意：跨天拆分的每条记录都重复携带整场会话的 `total`/`new`/`review`/`wrong`（与 Web 的 `flushStatToStore` 相同），这是存档格式的既定语义；`daily_totals` 依据 `session_role` 去重计数、时长仍按天累加，因此统计页的累计与逐日数据不会重复计数。

## 14. 文案与随机（i18n.rs / rng.rs）

`tr(key)` 是唯一的文案入口：内部是硬编码的 Rust `match`，覆盖 132 个键；键名与上游 `i18n/locales/zh.json` 对齐，其中 56 个键在上游同名存在（部分文案按 TUI 场景缩短，如 `another_group` 为“继续下一组”、`dict_list` 为“词库”），其余为 TUI 补充键（如 `step_group`、`timer_paused`、`big_word`）。项目只内置简体中文，不读取运行时 locale 文件。

| 键类别 | 代表键 | 使用方 |
| --- | --- | --- |
| 流程/阶段 | `smart_learning`、`free_practice`、`new_words`、`followWrite`、`spell`、`listen`、`dictation`、`identify`、`step_group`、`review_wrong_words` | `display_state` 与流程标签 |
| 练习提示 | `wrong_words_remain`、`already_first_word`、`press_delete_reinput`、`press_space_continue`、`press_space_dictation` | 推进器 `Notice` 与键入提示 |
| 页面/设置 | `home_word_practice`、`settings`、`statistics`、`word_practice_mode`、`fsrs_easy_limit` | 界面与设置页 |

未知键原样返回键名；`match` 命中空字符串时同样回退为键名，便于发现漏配。

`rng.rs` 是可注入随机源：`Rng = rand::rngs::StdRng`；`seeded(seed)` 用 `seed_from_u64` 构造，`random_seed()` 用 `rand::random()`；`shuffle` 是 Fisher–Yates 洗牌；`pick_index(len)` 返回 `[0, len)`，`len == 0` 时返回 0。领域层所有随机性（任务抽取、洗牌、题目扰动、错词重练）都来自调用方传入的 `Rng`；应用层由 `--seed` 固定种子时，今日任务、题目顺序与错词重练顺序都可复现（测试使用 `seeded(42)`、`seeded(7)` 等固定种子）。

## 15. 关键不变量与扩展指南

### 15.1 不变量

1. **游标单指针**：`PracticeFlowCursor` 是唯一位置来源；`PracticeData.index` 只是当前词表下标。持久化时保存 `flow_id` + 游标 + 工作词 key，不保存节点/步骤的副本状态。
2. **相位唯一入口**：所有“当前练什么”的查询都经 `resolve_phase_by_cursor`；`Navigator::current_phase` 只是委托，渲染层的 `display_state` 只读游标派生展示数据。
3. **工作词表传递**：同一 Node 内的步骤共享 `node_working_words`；进入新 Step 时按 `exclude_words` 与已掌握词过滤，跨 Node 才按新词源重新解析（`Current` 节点沿用工作词表）。
4. **分层边界**：domain 不依赖 `app`/`ui`/ratatui；应用层通过 `NavState` 借用字段驱动推进器，推进器不直接持有会话或存储。
5. **结算单点**：FSRS 卡与 statistics 只在 `settle` 写入；练习过程中只有错词本、收藏、已掌握会立即变化。计时字段在结算前统一清洗。
6. **自然日与时间**：到期判断、统计分天、摘要日期都基于 `Local` 自然日；时间戳统一为毫秒。
7. **随机可注入**：任何涉及顺序的算法都必须接收 `&mut Rng`，不得在领域层内部创建随机源。

### 15.2 新增一个内置流程

按顺序修改：`src/domain/types.rs`（若需新 `WordPracticeMode` 值，编号必须与 Web 枚举一致）→ `src/domain/constants.rs`（补 `PRACTICE_MODE_NAMES`）→ `src/domain/flow/mod.rs`（`builtin_flow` 增加配置、`get_flow_id_for_mode` 增加映射）→ `src/app/mod.rs` 的 `practice_modes()` → `src/cli.rs` 的 `parse_mode` 与帮助文案 → `src/domain/i18n.rs`（新 label 键）→ `tests/flow_parity.rs`（把模式加入遍历数组）。若流程引入新的 `PracticeWordsSource`/`PracticeEndAction`，还需扩展 `navigator.rs` 的解析分支。

### 15.3 新增一个设置项

按顺序修改：`src/domain/types.rs`（`SettingState` 字段 + 默认值；需要归一化时加 `effective_*` 方法）→ 若影响键入则 `session.rs::typing_settings` 与 `typing.rs::TypingSettings` → `src/app/mod.rs` 的 `SETTING_ROWS`、`setting_number_value`/`apply_number_value` 或 `toggle_setting_bool`（含取值 clamp）→ `src/ui/settings.rs` 的分区表与显示 → `src/domain/i18n.rs` 的标签键 → 视需要提升 `SETTING_VERSION`（仅当与 Web 存档不兼容时；普通新增字段走 `extra` 透传，无需升版本）→ 补 `tests/app_drive.rs`。

### 15.4 新增一个统计维度

按顺序修改：`src/domain/types.rs`（`Statistics` 新字段 + `PracticeState` 来源字段）→ `src/domain/statistics.rs`（`flush_stat_to_store` 写入、`DayTotals` 与 `daily_totals` 聚合）→ `src/app/mod.rs`（`refresh_stats`、必要时 `SummaryData`）→ `src/ui/insights.rs` 与 `src/ui/stats.rs`（展示）→ `tests/end_to_end.rs` 或存储往返测试。若要兼容旧存档，新字段必须有 serde 默认值。

## 相关文档

- [文档索引](README.md)
- [应用层状态机](app.md)
- [渲染层与组件](ui.md)
- [数据文件与互通](persistence.md)
- [与上游 TypeWords 的差异](upstream-parity.md)
