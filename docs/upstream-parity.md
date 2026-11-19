# 与上游 TypeWords 的差异

本文对照上游 [TypeWords](https://github.com/zyronon/TypeWords) 的 Web / Tauri 桌面端，给出两端定位、功能矩阵、行为差异、数据互通与上游同步方法。结论只以源码为事实来源（TUI 的 `src/**`、`tests/**` 与上游的 `app/**`）；文中「上游路径」指上游仓库内的路径。

## 1. 基线与判定

### 1.1 两端定位

| 维度 | 上游 TypeWords（Web / Tauri） | typewords-tui |
| --- | --- | --- |
| 技术栈与规模 | Nuxt 4.2 + Vue 3.5 + Pinia + UnoCSS + vue-i18n + ts-fsrs（`package.json` 声明 `^5.2.3`；TUI 移植按 5.4.1）+ idb-keyval；Tauri v2 壳；`app/` 下 15 个页面、约 210 个文件、约 3 万行 | Rust 2024（`rust-version` 1.90）+ ratatui 0.30 + crossterm + serde / chrono / clap，FSRS-6 自实现；`src/` 下 67 个 Rust 文件、约 2.9 万行，13 个屏幕 + 7 类弹层 |
| 产物与环境 | `nuxt generate` 静态站；桌面端是 Tauri 安装包（WebView2 载入同一份前端），运行在浏览器或桌面窗口 | 单个 `tw` 二进制 + 只读 `assets/`，运行在终端（最小 60 × 20），无 WebView、无 Node 运行时 |
| 数据与网络 | IndexedDB 4 个 key：`typing-word-dict`(4)、`typing-word-setting`(23)、`PracticeSaveWord`(2)、`PracticeSaveArticle`(1)；在线查词、百度翻译、有道发音、Supabase 同步、umami 埋点 | 数据目录 5 份 JSON：`dict.json`(4)、`setting.json`(23)、`practice-word.json`(2)、`practice-article.json`(1)、`flows.json`（无版本）；默认离线，唯一联网能力是可选的有道发音（系统 `curl` 下载 mp3，外部播放器播放） |
| 测试与分发 | `package.json` 的 `test` 为空，快照内没有测试目录；静态站 + Tauri 安装包 | 31 个集成测试文件 + 模块内单测，CI 跑 fmt / clippy / Ubuntu + Windows；GitHub Release 按五平台矩阵产出 tar.gz（可执行文件 + assets + docs + LICENSE + 同名 `.sha256` 与聚合 `SHA256SUMS`） |

### 1.2 对照基线

| 项 | 记录 |
| --- | --- |
| 上游仓库与快照 | `zyronon/TypeWords`（GPL-3.0，词库数据同许可证；TUI 沿用），本仓库对照的是其 `feature/tui` 分支（2026-10）的解包快照 |
| 上游提交与仓库关系 | 记录 origin/master `6da7641`、upstream/dev `bf50eae`（拆分时记录；解包内容不含 `.git`，本仓库无法复核哈希）。上游内含 `tui/` 子目录，本仓库从中拆出：`6520fb6`、`7cbfb22` 继承上游 TUI 历史，`a7f1ea4` 起为独立历史 |
| 数据与流程版本 | dict 4 / setting 23 / 练习缓存 2 / 文章缓存 1；内置流程 `CURRENT_FLOW_VERSION` = 6；Web 备份格式 5 |

### 1.3 状态判定

做法是先读上游页面与 `app/core`（stores / hooks / composables / utils），再逐项在 TUI 源码里找实现与调用点；README 与上游文档只作线索，不一致时以源码和测试为准。状态分六档：**已对齐**（有等价实现，差异不影响用户可见行为）、**部分**（主链路存在但缺子能力或入口受限）、**行为不同**（同名功能实现语义不同）、**缺失**（上游有、TUI 没有）、**TUI 独有**（上游没有的新能力）、**不适用**（终端形态下没有对等意义）。

## 2. 功能矩阵

「上游」列是能力载体（页面 / 组件 / hooks / 运行时文件），「TUI 现状」是实现位置与差异。

| 分组 | 功能 | 上游（页面 / 文件） | TUI 现状 | 状态 |
| --- | --- | --- | --- | --- |
| 主页 | 学习主页 dashboard | `(words)/words.vue` | `src/ui/home.rs` | 部分：当前词库、今日任务、统计、我的词典、推荐齐全；无月历、任务词表预览、改进度入口 |
| 外观 | 侧栏一级导航 | `layouts/default.vue` | `src/ui/shell.rs` | 部分：单词 / 文章 / 词库 / 统计 / 帮助 / 设置六项，数字键 1~6；反馈与关于并入帮助页分页，无文档页与资源分享 |
| 帮助 | 快捷键参考 / FAQ / 关于 / 反馈 | `app/pages/help.vue`、`about.vue`、`feedback.vue`（about 与 feedback 复用 `components/About.vue`） | `src/ui/help.rs`：「快捷键」「常见问题」「关于与反馈」三个分页 | 部分：FAQ 与联系方式按终端形态重写（无微信群 / 工单 / 小程序，改为仓库 issue + 诊断信息），关于页展示当前数据目录；快捷键总览是 TUI 独有（上游在设置页看键位） |
| 外观 | 主题 | CSS 变量 + `hooks/theme.ts` | 13 token、6 套内置 + 自定义 JSON | 行为不同：自定义主题是 TUI 独有；`auto` 等同深色，不跟随系统 |
| 词库 | 词库目录、切换与每日词数 | `(words)/dict-list.vue`、`components/word/PracticeSettingDialog.vue` | `src/ui/dict_picker.rs`；设置页「复习 → 每日词数」+ 主页 / 词库页 `+` `-` | 部分：三本内置词典 + 本地目录，可设为当前并打开词条页；无搜索、分类分组、封面、在线目录；每日词数步进 5、设置页可精确输入，无滑块与预计完成天数 |
| 词库 | 修改学习进度 / 跳组 | `ChangeLastPracticeIndexDialog.vue`、`GroupList.vue` | 状态行「第 N 组」可点击 → 跳组弹层 | 已对齐 |
| 词库 | 词条增删改查、批量、排序、自定义词典与收藏 | `(words)/dict.vue`、`BaseTable.vue`、`WordItem.vue`、`WordCollectPopover.vue` | 词条管理页；词库页 `n` 新建、`c` 复制；`Shift+Enter` 选收藏目标词典 | 已对齐：见 [使用手册 · 词条管理](usage.md#词条管理) |
| 词库 | 词库导入 / 导出 | `(words)/dict.vue`（xlsx / json）、`/import` | `tw import-dict` / `tw export-dict`（词条数组 JSON，可往返） | 部分：仅 CLI、仅 JSON，没有 xlsx |
| 练习 | 7 个内置流程 | `practice-flow-config.ts`（v6） | `src/domain/flow/mod.rs` 的 `builtin_flow` | 已对齐：节点、步骤、7 词小组、错词清空一致，`tests/flow_parity.rs` 覆盖 |
| 练习 | 练习模式切换入口 | `words.vue` 的模式下拉 | `m` 打开 10 项模式选择弹层（主页「模式」标签也可点开） | 已对齐：写入 `wordPracticeMode`，`--mode` 仍可启动覆盖 |
| 练习 | 自定义流程（PracticeFlowV2） | `practice-flow-runtime.ts`（存 localStorage） | 流程页：新建副本 / 编辑步骤 / 启用，存 `flows.json` | 部分：可调步骤顺序、乱序、组内巩固；不能增删节点、不能改节点词表来源 |
| 练习 | 单词测试 | `(words)/words-test/[id].vue`、`core/utils/word-test.ts` | `src/ui/word_test.rs`（`1`~`4` 选择、`Space` 下一题） | 已对齐：4 选 1、答错入错词本、结算 |
| 练习 | 随机复习范围 | `ShufflePracticeSettingDialog.vue` | 固定：已学范围 + 每日词数 × 复习比例 | 行为不同：TUI 无「已学 10 词」门槛，进度为 0 时取整本词库 |
| 练习 | 结算报告 | `components/word/Statistics.vue` | `src/ui/overlays.rs` | 已对齐：含错词 Top10 与本周记录；无分享入口 |
| 练习 | 学完判定 / 总复习 | `words.vue` 的 `getCurrentStudyWord`、`updateCompletedDictProgress` | `domain/study_task.rs`、`domain/session.rs` | 行为不同：TUI 按词表实际词数判完、最后一个词仍作新词学完并置 `complete`；上游 `isEnd = length - 1` 会提前转复习 |
| 练习 | 练习内导航（跳阶段 / 上下词 / 重学 / 上下组）与自测选择题 | `usePracticeWordNavigator`、`createRepeatTask` / `createNextTask`、`WordIdentifyPanel.vue` | `domain/flow/navigator.rs` + 键位与结算弹层按钮；`src/domain/question.rs` | 已对齐：键位有别名差异，语义与题目生成一致 |
| 调度 | 评级阈值、学习步进与到期筛选 | `core/hooks/fsrs.ts`、`core/hooks/dict.ts`、`study-task.ts` | `src/domain/scheduler.rs`、`src/domain/fsrs.rs`、`src/domain/study_task.rs` | 部分：评级 0 / 3 / 6 分档、步进读设置、无到期随机填充一致；上游顺手删除已掌握词的 FSRS 记录，TUI 只忽略不删 |
| 统计 | 主页统计、本周记录、月历与按日明细、统计口径 | `(words)/words.vue`、`base/Calendar.vue`、`Statistics.vue` | `src/ui/home.rs`、`src/ui/insights.rs`、统计页（月历 + 选中日明细 + `v` 按词典视图）、`src/domain/statistics.rs` | 已对齐：跨天拆分、sessionRole、正确率算法一致 |
| 设置 | 设置页与分组 | `setting.vue`（10 个 tab） | `src/ui/settings.rs`（7 分区 33 行）+ 高级 FSRS 页 | 部分：覆盖练习手感、FSRS 阈值与全部单值调度参数；见 3.3 与 [设置项参考](settings.md) |
| 设置 | 自定义快捷键 | `setting.vue` 快捷键 tab、`DefaultShortcutKeyMap`（42 个动作） | 快捷键编辑页，可改绑 / 清除 / 恢复默认 | 已对齐：存 `shortcutKeyMap`，见 [快捷键](shortcuts.md) |
| 交互 | 键鼠操作 | 浏览器键鼠 + 可配置快捷键 | 可配置键位 + 终端鼠标（点击、滚轮） | 行为不同：终端鼠标没有悬停与拖拽，复制要 `Shift+拖拽` |
| 断点 | 本地缓存、续练与 v1 升级 | `PracticeSaveWord` v2、`PracticeSaveArticle` v1、`checkAndUpgradePracticeWordCache` | `practice-word.json` v2、`practice-article.json` v1、`try_resume_saved_practice` | 部分：单词缓存同构、主页「上次任务」提示已对齐；v1 缓存不迁移，被下一次保存覆盖；文章断点见 4.3 |
| 断点 | 跨设备恢复与冲突 | `useDataSyncPersistence.ts`（Supabase） | 无 | 缺失：默认离线 |
| 文章 | 文章背诵 | `(articles)/*` 4 个页面、`hooks/article.ts` | `src/domain/article.rs` + `src/ui/articles.rs` | 部分：断句、句子与译文对齐、整篇跟打已实现；缺逐句音频时间轴、课后题与查词 |
| 音频 | 发音与键盘音效 | `hooks/sound.ts`、`base/Audio.vue` | `src/audio/` | 部分：响铃 + 系统 TTS + 可选有道 mp3；无音量 / 音色选择与逐句音频 |
| 查词 | 练习中查词与在线翻译 | `WordLookupPopover.vue`、`libs/translate/*` | 无 | 缺失：离线设计目标 |
| 笔记 | 单词笔记 | `TypeWord.vue` 编辑区、`(words)/dict.vue` 笔记字段 | 练习页显示 + `Ctrl+O` 编辑；词条编辑弹层含笔记字段 | 已对齐 |
| 多语言 | 界面语言切换 | `i18n/locales/*.json`（14 种） | 中文硬编码（`src/domain/i18n.rs`） | 缺失：键名对齐上游，但没有语言包 |
| 云同步 | Supabase 同步 | `utils/supabase.ts` | 无 | 缺失：上游设置页已标为「过时功能」 |
| 备份 | 备份导入导出 | `hooks/export.ts`（ZIP 与 JSON） | `tw import-web` / `tw export-web`（JSON） | 部分：同一套 `BackupData` v5；不支持 ZIP 与 mp3 |

## 3. 行为差异

### 3.1 FSRS

评级映射、到期筛选、统计字段、FSRS-6 公式、学习步进、fuzz 区间与随机种子两端一致：同一张卡在 TUI 与 Web 上算出的下次复习时间相同。

| 维度 | 上游（ts-fsrs） | TUI（`src/domain/fsrs.rs`） |
| --- | --- | --- |
| 评级阈值 | `getGradeByWrongTimes`，默认 0 / 3 / 6 | 同左（`Scheduler::grade_by_wrong_times`） |
| 学习与重学步进 | `learning_steps` / `relearning_steps`，`enable_short_term` 切换调度器 | 同左，读设置 |
| 非 21 项权重 | `migrateParameters`：19 项补 `[0, 0.5]`、17 项先把 `w[4]` / `w[5]` / `w[6]` 按 FSRS-5 → FSRS-6 的公式变换再补 `[0, 0, 0, 0.5]`、其它长度整组默认 | 同左（`migrate_parameters`）；设置页改权重前也先迁移，避免改一项丢其余项 |
| 复习间隔、fuzz 与到期判断 | FSRS-6（21 个权重 `w`）+ `maximum_interval`；`enable_fuzz` 分段 ±15% / ±10% / ±5%；`dayjs(due).isSameOrBefore(now, 'day')` | 同左，权重按上游 `CLAMP_PARAMETERS` 夹取、随机源移植上游 Alea；`due` 换算本地日期后 `<= today`。另加参数上界兜底：最大间隔夹到 1 ~ 36500 天、学习步进换算后 ≤ 36500 天（上游只校验下界，极端手改值在 TUI 会被夹住而不是算到天文数字） |
| 目标留存率边界 | `calculate_interval_modifier` 只拒绝 `<= 0` 与 `> 1`：`1.0` 合法，此时 `interval_modifier` 恒为 0、复习卡全被排到次日 | 有意不同：`1.0` 与越界值一起回退 0.9（设置页显示、编辑预填与调度都用这个生效值）。宁可比上游保守，也不接受「所有复习卡都排到次日」的退化调度 |
| 已掌握词 | 生成任务时从 `fsrsData` 中删除 | 只排除，不删除记录 |

一致性由 `tests/fsrs_parity.rs` 锁定：样本 `tests/fixtures/fsrs_golden.json` 由 `tools/fsrs_golden.mjs` 用上游 ts-fsrs 生成，覆盖新卡起步、学习步进、毕业复习、同日短期记忆、fuzz 与长期调度。仍存在的差异有两处：「已掌握词不删卡片」——两端卡片集合可能不同，但调度结果一致；以及参数上界的兜底——上游对 `maximum_interval` 只校验 ≥ 1、学习步进按 `parseInt` 解析，TUI 把两者夹到 36500 天以内，避免手改出极端值时 chrono 的 `Duration` 越界（见 `src/domain/fsrs.rs` 的 `MAX_INTERVAL_DAYS`）。

### 3.2 键位

42 个动作全部数据化（`src/domain/shortcut.rs` 的 `ShortcutMap`，存于 `shortcutKeyMap`），默认绑定与上游 `DefaultShortcutKeyMap` 逐条一致，可在设置页改绑。差异只有下表几条：

| 动作 | 上游默认键 | TUI | 差异 |
| --- | --- | --- | --- |
| 简洁模式 | `Ctrl+M` | `Ctrl+G` | 终端里 `Ctrl+M` 等同回车 |
| 编辑文章、播放例句 1~9 | `Ctrl+E`、`Ctrl+1`~`Ctrl+9` | 无（`Ctrl+E` 改为显示 / 隐藏例句与词源） | 缺失：TUI 不能编辑文章，也没有逐句音频 |
| 上一个词 / 下一个词、跳下一阶段 | `Ctrl+←` `Ctrl+→`、`Shift+→` | 同左，另有 `Ctrl+N` / `Ctrl+J` 别名 | 终端兼容别名（不落盘） |
| 单词测试下一题 | `Space` | `Space`（`1`~`4` 选答案） | 已对齐 |

TUI 另有上游没有的全局键：`1`~`6` 切页、`d` / `s` / `t` 直达页面、`m` 选练习模式、`f` 自由练习、`+` / `-` 调整每日词数、`q` 退出、`?` 帮助。部分终端会吞掉 `Alt+数字`、`Ctrl+方向键`，因此 TUI 提供 ASCII 别名并允许用拼写代替选择题；完整键表见 [使用手册 · 按键参考](usage.md#按键参考) 与 [快捷键](shortcuts.md)。

**文章跟打页**：两端都复用同一张键位表。上游监听 `PreviousChapter` / `RepeatChapter` / `ToggleShowTranslate` / `ToggleTheme` / `PlayWordPronunciation` / `ShowWord`；TUI 认 `PlayWordPronunciation`（朗读当前句）、`ToggleShowTranslate`（切换当前句译文）、`ToggleTheme`，另有共用的 `Ctrl+,`（暂停）、`Ctrl+C`（保存返回）、`?`、`Esc`（返回）。差异：上游 `ShowWord` 是「按下高亮当前跟打位置、松手恢复」，TUI 把整篇正文常显、没有这层高亮，`Esc` 让给页面返回；上游的「上一章 / 下一章 / 重学本章」由 TUI 的文章结算弹层（返回列表 / 重打本篇 / 主页）承担。

### 3.3 设置建模范围

上游 `SettingState`（`app/core/stores/setting.ts`）有 58 个顶层字段、设置页分 10 个 tab。TUI 的 `SettingState` 建模 43 个具名字段：41 个与上游同名（`theme`、`repeatCount`、`fsrsParameters`、`shortcutKeyMap` 等）+ TUI 专有的 `tuiBigWord` / `ttsSource`；其余 17 个上游字段落入 `extra` 原样保留。设置页 33 行 = 7 个分区、29 个 `SettingState` 配置项 + 每日词数（写 `dict.json` 的词典元数据）+ 高级 FSRS 入口行 + 2 个动作项。

| 类别 | 项 |
| --- | --- |
| 有 UI 且生效 | 练习模式、主题、音标、大字、复习比例、无到期随机填充、忽略简单词、自动下一词、重复次数、切词等待、空格冷却、错误即清空、忽略大小写、自测选择题、词源、上下词、显示使用提示、FSRS 三阈值、目标留存率、清空进度、清除所有数据；高级 FSRS 页另有最大间隔、间隔抖动、短期调度与 21 个权重 |
| 建模但无 UI | `wordPracticeType`、`dictation`、`translate`、`ignoreSymbol`、`alwaysShowNote`、`_ignoreWatch`；`learning_steps` / `relearning_steps` 仍只能手改 `setting.json`（练习流程自己的学习步进固定 1 / 6 / 10 分钟，两套互不影响） |
| 上游有、TUI 完全没有 | 声音音量与语速、键音包、TTS 声色、字号、练习内设置弹层、文章音效、`practiceSentence`、`autoPlayFirstSentence`、侧栏折叠、移动端导航 |

逐项取值与默认值见 [设置项参考](settings.md)，透传规则见 [存档与数据格式](persistence.md)。

### 3.4 断点缓存

- 两端都用 v2 的 compact 结构：`taskWordsStr`、`practiceData`（词表以字符串存）、`statStoreData`、`sessionSnapshot`（`flowId` + `cursor` + `nodeWorkingWordKeys`）。上游保留 v1→v2 升级路径（`resolveLegacyPracticeWordCursor` 按旧 stage 反推游标），TUI 只接受 `version == 2`，其余视为不支持并忽略缓存、不移动文件；TUI 要求四块内容齐全才恢复，上游允许只有 `taskWordsStr` 的「未开始」缓存。
- 保存时机：上游在页面隐藏 / 可见、远程数据更新时保存或重载，TUI 在推进 / 后退 / 跳过 / 错误后 500 ms 防抖保存、每 30 秒保存、`Ctrl+C` / `Ctrl+Enter` 与退出时保存。两端刷新或重启都先回主页，断点只在显式操作（主页 `Enter`、文章列表选中该文章）时使用。

### 3.5 统计口径

结算写入算法与上游 `flushStatToStore` 对齐：按时间片段的自然日拆分，跨天时给多条记录标 `sessionRole`（`single` / `start` / `middle` / `end`），`tests/end_to_end.rs` 覆盖写入；正确率两端都是 `(total - wrong) / total`，`wrong` 取本轮去重后的错词数。TUI 统计页聚合全部单词词典的 `statistics`（`domain/statistics.rs` 的 `daily_totals`），`middle` / `end` 记录只计时长、计数只归开始日；上游没有跨词典累计视图，也没有独立统计页，其日历 / 日记录弹窗能看到每条记录的词典名，TUI 无法按词典拆分。TUI 主页「本周记录」按全部词典聚合，上游结算弹层只看当前词典。

### 3.6 交互与计时

- 上游有 CSS 动画、Vue 过渡、hover 显示单词、拖拽与 Shepherd 引导；TUI 全部没有，终端鼠标只有点击与滚轮，没有悬停、拖拽和右键菜单。音频上，上游可播放键盘音效、对错音与单词发音（受设置开关控制），TUI 用终端响铃代替键音 / 对错音，单词与文章朗读走系统 TTS（Windows `System.Speech`、macOS `say`、Linux `espeak-ng`，可选有道 mp3）；默写阶段不自动朗读以免念出答案，听写阶段仍同时显示音标与释义。
- 计时与事件循环：两端都有「无操作 3 分钟自动暂停」，且都在检测到键入活动时自动恢复并提示「已恢复计时」；手动暂停（TUI 用 `Ctrl+,`，上游没有）不会被自动恢复。上游还有「切走标签页即暂停」的可见性场景，终端没有对应概念；上游 `waitTimeForChangeWord` 在自动下一词时作为延迟，TUI 同义。TUI 是 100 ms 轮询 + 批量读事件（一次最多 64 个），上游是浏览器事件驱动：键盘手感影响可忽略，但 TUI 没有帧动画。

### 3.7 数据存储

上游用 `idb-keyval` 把 4 类数据存进 IndexedDB，Pinia `$subscribe` 防抖 1 秒自动保存并叠加 Supabase 推送 / 拉取与冲突检测。TUI 用 JSON 文件：同目录临时文件 + fsync + rename 原子写，解析失败重命名为 `*.corrupt-<时间戳>.json` 并回退默认值，版本过高时只读不覆盖（详见 [存档与数据格式](persistence.md)）。TUI 保存时会清空非系统、非自定义词典的词表（`BaseState::shake_common_dict`，对应上游 `shakeCommonDict`），因此 `dict.json` 不会把 4.5 MB 的 CET-4 词表写进去，官方词库在启动时按 `url` 从 `assets/` 重新加载；上游的保存是「整份 store 状态」，TUI 是「显式调用 + 延迟保存」。

重置数据两端都只有整体入口：上游「设置 → 数据 → 清除所有数据」把 base store 与 setting store 一起 `setState(getDefault*())` 并清空练习缓存；TUI 设置页「其他 → 清除所有数据」走 `App::clear_all_data`，同样是 `BaseState::default()` + `SettingState::default()` + 删除两份练习缓存，文案沿用上游。差异只在位置与后续动作：上游清完让用户重新选词典（`studyIndex = -1`），TUI 按全新安装的路径自动选一本（有 `cet4` 用它）。TUI 额外的「清空当前练习进度」只删断点缓存，对应上游切词典 / 改练习设置时的 `resetCacheData` 副作用，上游没有把它做成按钮。

## 4. 数据互通

### 4.1 互通边界

两端共享「同名字段 + 同构信封」（`{val, version, updated_at?}`）；Web 把信封序列化成 JSON 字符串存进 IndexedDB，TUI 直接存成文件。TUI 不读写浏览器 IndexedDB，可搬运的只有 JSON 文本：

| TUI 文件 | Web 存储 key | 版本 | 备注 |
| --- | --- | --- | --- |
| `dict.json` | `typing-word-dict` | 4 | 字段同名；系统词典与当前词库进度可搬运 |
| `setting.json` | `typing-word-setting` | 23 | TUI 建模 43 个字段，其余透传 |
| `practice-word.json` | `PracticeSaveWord` | 2 | compact v2；TUI 额外要求 `sessionSnapshot` |
| `practice-article.json` | `PracticeSaveArticle` | 1 | 部分：TUI 导出的备份可往返（真实断点在 `tuiPracticeData`）；网页版游标只有段落 / 句 / 词三级下标、没有文章 id，不映射 |
| `flows.json` | 无 | — | TUI 独有，自定义流程不进 Web 备份 |

### 4.2 Web 备份结构与迁移

Web 端「设置 → 数据管理 → 导出」生成 JSON（或含 `mp3/` 的 ZIP），`data.json` 的导出格式是 v5：

```text
Web 备份 data.json                        TUI 数据目录
{ version: 5, val: {                      <home>/
  dict:                <信封 v4>     ->    dict.json
  setting:             <信封 v23>    ->    setting.json
  PracticeSaveWord:    <信封 v2>     ->    practice-word.json
  PracticeSaveArticle: <信封 v1>     ->    practice-article.json（仅 TUI 来源可往返）
} }                                       themes/*.json、flows.json（TUI 独有；app version 丢弃）
```

`tw import-web <data.json>` 直接读网页版导出的备份：逐项校验版本与结构，覆盖前留 `.bak` 备份（解析失败则回滚并保留原文件），`dict` / `setting` / `PracticeSaveWord` 原样落盘，文章断点按来源分流；`tw export-web [--out FILE]` 把当前数据目录导出成网页版能导入的 JSON（缺省写标准输出）。手工迁移则按「Web 导出 JSON → 关闭 TUI 并备份 `<home>` → 按上表写入各文件（保留各自信封）→ `tw doctor` 核对 → 启动检查设置与断点」执行。

### 4.3 字段语义差异与风险

| 差异 | 影响 |
| --- | --- |
| `tuiBigWord` 与未知字段 | Web 没有 `tuiBigWord`；上游 `checkAndUpgradeSaveSetting` 与 Web 端都只保留自己认识的字段，经 Web 加载再保存会丢失 TUI 补充字段，TUI 再读时回到默认值 |
| FSRS 卡片与参数 | `due` 用 ISO 时间字符串、其余字段同名；学习卡在 TUI 的 `stability` / `difficulty` 可能为 0，参数结构可互拷但两端调度实现不同，「同一张卡」的后续间隔不可复现 |
| 文章数据 | 系统「文章收藏」的词条会原样保留；TUI 的 `shake_common_dict` 会清空所有非 system 文章书籍的文章列表（比上游更激进）。TUI 不读存档里的文章正文：跟打用的正文来自 `assets/dicts/en/article/*.json` |

## 5. TUI 独有

| 能力 | 实现位置 | 说明与限制 |
| --- | --- | --- |
| 自定义主题 | `src/theme/` | 13 个颜色 token、6 套内置、`<home>/themes/*.json`、base 继承与校验告警；Web 用 CSS 变量，两端不共享主题文件 |
| 内置词典可设为当前词典 | `src/app/dicts.rs`、`src/ui/dict_picker.rs` | 收藏 / 错词 / 已掌握与资源词库并列，`Enter` 设为当前学习词典（写 `studyIndex`，重启保持）、`w` 打开词条管理页；上游 `dict-list.vue` 只列资源目录 |
| 终端交互：鼠标与键盘焦点导航 | `src/app/mouse.rs`、`src/ui/hit.rs`、`src/ui/target.rs`、`src/ui/focus.rs`、`src/app/focus.rs` | 侧栏 / 按钮 / 列表点击与滚轮，练习页与跟打页可点底部动作按钮，鼠标捕获后需 `Shift+拖拽` 复制；套外壳的页面共用两栏焦点（宽屏侧栏竖向 / 窄屏顶栏横向，`Tab` 换栏，内容区按视觉行分带），进入页面默认选中内容区、底栏显示「选中 X」（帮助页内容区显示分页提示）。上游只有鼠标 / 触屏 |
| 终端响应式外壳 | `src/ui/shell.rs` | < 100 列切窄屏顶栏标签，≥ 120 列侧栏 26 列；小于 60 × 20 只渲染提示 |
| 点阵大字 | `src/ui/bigtext.rs`、`src/ui/practice.rs` | 5×7 点阵字体，空间足够时按本轮最长单词整体放大 2 倍；非 ASCII 回退普通文本 |
| 单二进制与命令行工具 | `src/cli.rs`、`scripts/package.ps1`、`scripts/package.sh`、`.github/workflows/release.yml` | `tw doctor` / `tw task` / `tw import-dict` / `tw export-dict` / `tw import-web` / `tw export-web`；发布包为 tar.gz（可执行文件 + assets + docs + LICENSE + sha256），没有 Node 与 WebView 运行时 |
| 可复现界面 | `examples/shot.rs`、`examples/dump.rs`、`tools/render-snapshot.py` | 页面渲染成 JSON / 文本，用于截图与视觉回归，见 [开发指南 · 截图与视觉检查](development.md#6-截图与视觉检查) |

## 6. 仍未对齐与不可对等

### 6.1 仍未对齐项

| 项 | 差距 | 原因 |
| --- | --- | --- |
| xlsx 词库导入导出 | 词库导入导出只走 JSON（`tw import-dict` / `tw export-dict`），不支持 xlsx | xlsx 需要新依赖，与「保持轻量」冲突 |
| 节点级流程编辑 | 流程页可增删步骤、调乱序与组内巩固，但不能增删节点、不能改节点词表来源 | 终端里编辑多级结构的交互成本高 |
| 文章逐句音频与课后题 | 没有 LRC 时间轴，无法做逐句高亮；课后题也没有实现 | 不打包 mp3，改用系统 TTS 整句朗读 |
| 文档页（资源分享） | 上游 `pages/doc.vue` 收录英语学习资源与网盘 / 站外链接，TUI 没有对应页面 | 资源靠扫码或打开外链，终端里都不成立；帮助页「关于」只说明完整文档在发布包 `docs/` |

### 6.2 不可对等能力

这些是运行形态决定的边界，不是路线图漏项。

| 能力 | 上游实现 | 终端中的边界 |
| --- | --- | --- |
| WebView / 桌面壳 / 窗口 | Tauri v2 加载 Nuxt 产物，窗口、菜单、系统下载 | 终端没有窗口系统，单进程事件循环替代宿主 |
| 动画、过渡、悬停与像素级布局 | CSS / Vue Transition / Shepherd Tour | 字符单元渲染，只能用颜色与字符位置表达 |
| 音频通道、图片与字体 | 有道音频、`speechSynthesis`、键音 mp3 池；Web 图片与图标库、自定义字体 | 依赖系统 TTS 或外部播放器，不打包音频资源；终端字体由用户终端决定，点阵大字是折中 |
| 系统集成与移动端 | 文件选择、另存为、剪贴板、外链打开；响应式 Web + 移动端导航；在线词库、翻译、查词、埋点 | 用 CLI 参数、路径与终端快捷键替代（`Shift+拖拽` 才能复制文本）；移动端与在线服务在终端不适用，默认离线（可选的有道发音除外） |

## 7. 上游同步维护

### 7.1 词库与文章同步

`node scripts/sync-assets.mjs --source <TypeWords 仓库路径>` 会先删除本地 `assets/`，再复制上游 `public/list/word.json`、`public/list/article.json`、`public/dicts/en/word/` 与 `public/dicts/en/article/`。当前仓库只提交 CET-4（2607 词）与《新概念英语1》（5 篇）；同步后跑 `cargo test`，尤其 `tests/real_assets.rs`。命令细节见 [开发指南](development.md)，资源格式见 [存档与数据格式](persistence.md)。

### 7.2 上游升级时的比对清单

| 区域 | 上游位置 | 关注点 |
| --- | --- | --- |
| 页面清单 | `app/pages/` | 15 个页面的增删；决定功能矩阵是否要新增行 |
| 数据模型与版本 | `app/core/types/types.ts`、`stores/base.ts`、`stores/setting.ts`、`config/env.ts` | `Word` / `Dict` / `SettingState` 字段与默认值；dict 4 / setting 23 / practice word 2 / article 1 / export 5 / app version 4 |
| 内置流程与练习引擎 | `core/composables/practice-words/*` | `CURRENT_FLOW_VERSION`（当前 6）、`BUILTIN_FLOWS` 的节点与步骤、游标、推进器、错词清空、键入状态机 |
| 调度与设置 UI | `core/hooks/fsrs.ts`、`stores/setting.ts` 的 `fsrsParameters`、`app/pages/setting.vue` | 默认权重、学习步进、评级阈值、设置项与分组 |
| 文章与音频 | `core/hooks/article.ts`、`components/article/*`、`composables/practice-sentences/*` | 文章模型、句音频、课后题（用于判定差距） |

### 7.3 基线记录与同步流程

每次随上游更新后，改第 1.2 节表格并补一行摘要（上游参考、日期、dict / setting / 缓存版本、流程版本、主要变化）。建议流程：

1. 取一份新的上游快照，记录 commit、日期与页面数；先比对 `env.ts` / `cache.ts` 的版本号与 `practice-flow-config.ts` 的 `CURRENT_FLOW_VERSION`，版本变化优先处理。
2. 对照页面清单与设置 UI，更新本文第 2、3 节；练习 / 调度行为变化时跑 `tests/flow_parity.rs`、`tests/fsrs_parity.rs`、`tests/end_to_end.rs`。
3. 数据格式变化先更新 [存档与数据格式](persistence.md)（含迁移与只读保护评估）；最后同步 [根 README](../README.md) 的「与上游 TypeWords 的关系」摘要，避免几处结论打架。

### 7.4 不要直接复制上游代码

- **架构不同**：上游是 Vue 组件 + Pinia + DOM 事件（还有隐藏 input 处理移动端输入、CSS 动画与 IndexedDB）；TUI 是 Rust 状态机 + ratatui + 终端事件，逐行翻译只会把 DOM / 响应式复杂度带进来。
- **上游含遗留与未完成路径**：登录 / 注册、Supabase 同步、只有引用找不到页面的路由（如 `/fsrs`、`/practice-flow-editor`）、大量 i18n 与平台分支，直接搬会一并带进 TUI。
- **许可证与可验证性**：本项目按 GPL-3.0 分发，复制上游代码会形成「上游衍生文件」，后续同步与声明成本更高；TUI 的做法是把行为翻译成 Rust 并写成测试（`tests/flow_parity.rs`、`tests/fsrs_parity.rs` 等），上游只作行为参照。

## 相关文档

[文档索引](README.md) · [总体架构](architecture.md) · [开发指南](development.md) · [存档与数据格式](persistence.md)
