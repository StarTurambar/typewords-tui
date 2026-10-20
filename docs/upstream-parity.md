# 与上游 TypeWords 的差异与路线图

typewords-tui 不是把上游 Web 端逐行改写成 Rust，而是沿用上游的词库资源、存档字段命名和「单词练习闭环」，重新实现一套终端客户端。判断「还缺什么」时不能只看页面数量：音频、动画、WebView 这类能力在终端里无法对等实现，而跳组、笔记这类能力只是这一版没有接线。

本文对照上游 TypeWords 的 Web / Tauri 桌面端，给出两端定位、逐项功能矩阵、行为差异、数据互通方式、分优先级路线图，以及维护者随上游升级更新对照结论的方法。项目本身的定位与功能概览见 [根 README](../README.md)。结论只以源码（TUI 的 `src/**`、`tests/**` 与上游的 `app/**`）为事实来源；文中「上游路径」均指上游仓库内的路径，不用本地解包目录的路径。

> 适用范围：评估两端定位差异、功能覆盖、数据互通与补齐顺序；用户操作见使用手册，模块细节见对应的模块文档。对应版本：typewords-tui 0.1.0。

## 1. 基线与对照方法

### 1.1 两端定位

上游 TypeWords 是 Nuxt 4 + Vue 3 + Pinia + UnoCSS + vue-i18n 的 Web 应用，`app/` 下共 15 个页面、208 个文件、约 3.1 万行；同一份前端产物装进 Tauri v2 壳即为 Windows 桌面端（`package.json` 的 `desktop:*` 脚本、`docs/DESKTOP.md`）。数据存在浏览器 IndexedDB（`idb-keyval`），练习调度用 `ts-fsrs`，声音、在线查词、翻译、云同步都依赖网络能力。

本仓库是独立进程的终端客户端：Rust 2024 + ratatui 0.30 + crossterm 事件层，FSRS-6 算法由 `src/domain/fsrs.rs` 自实现（逐条移植上游 ts-fsrs），`src/` 下 44 个文件、约 1.4 万行；界面为 7 个屏幕（主页 / 练习 / 词库 / 统计 / 设置 / 快捷键 / 帮助）加 3 类弹层（结算 / 输入 / 确认），数据是本地 JSON，无网络、无音频、无 WebView。

### 1.2 对照基线记录

| 项 | 记录 |
| --- | --- |
| 上游仓库 | `zyronon/TypeWords` |
| 对照快照 | 上游 `feature/tui` 分支（2026-10）解包快照 |
| 记录的上游提交 | origin/master `6da7641`、upstream/dev `bf50eae`（拆分时记录；解包内容不含 `.git`，本仓库无法复核哈希） |
| 上游 TUI 子目录 | 上游仓库内含 `tui/` 子目录，本仓库是从中拆出的独立仓库：`6520fb6`、`7cbfb22` 继承自上游 TUI 历史，`a7f1ea4` 起为独立历史 |
| 数据版本 | dict 4、setting 23、练习缓存 2、内置流程 6；Web 备份格式 5、文章缓存 1 |
| 词库资源 | 快照 `public/list/word.json` 仅登记 CET-4（2607 词），`public/dicts/en/word/CET4_T.json` 约 4.5 MB |
| 本文更新时间 | 2026-10-18 |

### 1.3 状态判定与对照方法

先读完全部上游页面与 `app/core`（stores / hooks / composables / utils），再逐项在 TUI 源码中找实现与调用点；README 与上游文档只作线索，遇到不一致以源码和测试为准。状态定义：

| 状态 | 含义 |
| --- | --- |
| 已对齐 | TUI 有等价实现，差异不影响用户可见行为 |
| 部分 | 主链路存在，但缺子能力、入口受限或只覆盖一部分场景 |
| 行为不同 | 两端都有同名功能，但实现语义或交互不同 |
| 缺失 | 上游有、TUI 没有对应实现 |
| TUI 独有 | 上游没有对应实现，TUI 新增的能力 |
| 不适用 | 终端或独立进程形态下没有对等意义 |

## 2. 总体定位对比

| 维度 | 上游 TypeWords（Web / Tauri 桌面） | typewords-tui 0.1.0 |
| --- | --- | --- |
| 技术栈 | Nuxt 4.2、Vue 3.5、Pinia 3、UnoCSS 66、vue-i18n 11、ts-fsrs 5.2、idb-keyval、Tauri v2 | Rust 2024（rust-version 1.90）、ratatui 0.30.2、crossterm、fsrs 6.6.2、serde / chrono / clap |
| 产物 | `nuxt generate` 静态站（`.output/public`）；桌面端为 Tauri 安装包，WebView2 加载同一份前端 | 单个 `tw.exe` + 只读 `assets/` 目录；无运行时、无 Node、无 WebView2 |
| 运行环境 | 浏览器（含移动端）或 Windows 桌面窗口 | 终端（Windows Terminal 等），最小 60 × 20 |
| 代码共享 | 与 TUI 不共享代码 | 不复制上游 TS/Vue 实现，只沿用资源格式、字段命名与行为约定 |
| 词库资源 | `public/list/word.json` + `public/dicts/<lang>/word/*.json`（另有 article 目录） | 同布局的 `assets/`（当前仅 CET-4），可 `--assets` 替换、`tw import-dict` 扩展 |
| 存档 | IndexedDB 4 个 key：`typing-word-dict`(4)、`typing-word-setting`(23)、`PracticeSaveWord`(2)、`PracticeSaveArticle`(1) | 数据目录下 3 个 JSON 文件，同构信封 `{val, version, updated_at}`，版本 4 / 23 / 2 |
| 网络 | 在线查词、百度翻译、有道发音、Supabase 云同步、umami 埋点 | 无网络依赖（`Cargo.toml` 无 HTTP 客户端），无账号、无遥测 |
| 测试 | `package.json` 的 `test` 为空，本次快照未见测试目录 | `tests/` 6 组集成测试 + 源码 `#[cfg(test)]` 单测；CI 跑 fmt / clippy / Linux+Windows 测试 |
| 分发 | 静态站 + Tauri NSIS 安装包 | GitHub Release 的 Windows zip（`tw.exe`、`assets/`、`docs/`、README、LICENSE、SHA256） |
| 许可证 | GPL-3.0（上游仓库内含 LICENSE，词库数据同许可证） | GPL-3.0（见仓库 LICENSE，词库数据沿用上游许可证） |

上游 15 个页面与 TUI 的对应关系（页面清单来自 `app/pages`）：

| 上游页面（`app/pages/`） | 作用 | TUI 对应 |
| --- | --- | --- |
| `index.vue` | 营销落地页（打字 demo、下载入口） | 无（不适用） |
| `(words)/words.vue` | 单词学习主页：当前词库、今日任务、统计、词典、推荐 | 主页 `src/ui/home.rs` |
| `(words)/dict-list.vue` | 词库目录：搜索、分类标签、在线资源 | 词库页 `src/ui/dict_picker.rs`（本地目录，无搜索/分组） |
| `(words)/dict.vue` | 单词管理：增删改、批量、导入导出、复制词库 | 无 |
| `(words)/practice-words/[id].vue` | 单词练习页 | 练习页 `src/ui/practice.rs` |
| `(words)/words-test/[id].vue` | 单词测试（4 选 1） | 无 |
| `(articles)/articles.vue` | 文章学习主页 | 无 |
| `(articles)/book-list.vue` | 书籍目录 | 无 |
| `(articles)/book/[id].vue` | 书籍详情与文章列表、音频 | 无 |
| `(articles)/practice-articles/[id].vue` | 文章跟打/默写页 | 无 |
| `setting.vue` | 设置（10 个 tab） | 设置页 `src/ui/settings.rs`（6 分区 24 行） |
| `help.vue` | 常见问题（FAQ） | 帮助页 `src/ui/help.rs`（快捷键与鼠标说明） |
| `doc.vue` | 英语学习资源分享 | 无 |
| `about.vue` | 关于 | 无 |
| `feedback.vue` | 反馈 | 无 |

界面截图见 [主页](screenshots/home-120x40-dark.png)、[练习](screenshots/practice-120x40-dark.png)、[设置](screenshots/settings-120x40-dark.png)；主题差异见 [高对比主题](screenshots/home-120x40-high-contrast.png)。

## 3. 功能矩阵

表内「上游」列是能力载体（页面 / 组件 / hooks / 运行时文件），「TUI 现状」是当前实现位置。

| 分组 | 功能 | 上游（页面/文件） | TUI 现状 | 状态 | 说明 |
| --- | --- | --- | --- | --- | --- |
| 主页 | 学习主页 dashboard | `(words)/words.vue` | `src/ui/home.rs` | 部分 | 当前词库、今日任务、统计、我的词典、推荐齐全；无日历、任务词表预览、改学习进度入口 |
| 主页 | 营销落地页 | `index.vue` | 无 | 不适用 | 终端客户端不需要网站首页 |
| 主页 | 首访引导 Tour | `words.vue`、`dict-list.vue`、`components/word/PracticeOnboardingHost.vue` | 无 | 缺失 | TUI 只用 Toast 提示 |
| 词库 | 词库目录与切换 | `(words)/dict-list.vue` | `src/ui/dict_picker.rs` | 部分 | 本地目录 + 详情 + 设为当前；无搜索、分类分组、封面、在线目录 |
| 词库 | 每日词数与复习比例调整 | `components/word/PracticeSettingDialog.vue` | 设置页 + 主页/词库页 `+`/`-` | 部分 | TUI 步进 5 词；无滑块、预计完成天数、同一弹窗改复习比例 |
| 词库 | 修改学习进度 / 跳组 | `components/word/ChangeLastPracticeIndexDialog.vue`、`components/word/GroupList.vue` | 状态行「第 N 组」可点击，弹层选组跳转 | 已对齐 | `create_task_from_group` 已接线 |
| 词库 | 词条增删改查、批量删除、排序 | `(words)/dict.vue`、`components/BaseTable.vue`、`components/word/WordItem.vue` | 无 | 缺失 | 用户无法在 TUI 内编辑词条 |
| 词库 | 自定义词典、词库副本 | `(words)/dict.vue`、`components/word/WordCollectPopover.vue` | 无 | 缺失 | 只能使用资源目录内的词库 |
| 词库 | 词库导入 / 导出 | `(words)/dict.vue`（xlsx/json）、`/import` 入口 | `tw import-dict`（词条数组 JSON） | 部分 | 仅 CLI、仅 JSON、无管理界面 |
| 词库 | 收藏到指定词典 | `WordCollectPopover.vue`、`useWordCollectPicker.ts` | `Shift+Enter` 弹层选择目标词典 | 已对齐 | 目标是「收藏」+ 用户自建词典 |
| 词库 | 批量标记 | `components/word/WordMarkPickList.vue` | 无 | 缺失 | 只有单词级 1/2/3 与反引号标记 |
| 练习 | 7 个内置流程 | `practice-flow-config.ts` 的 `BUILTIN_FLOWS`（v6） | `src/domain/flow/mod.rs` 的 `builtin_flow` | 已对齐 | 节点、步骤、7 词小组、错词清空与上游一致，`tests/flow_parity.rs` 覆盖 |
| 练习 | 练习模式切换入口 | `words.vue` 的模式下拉（学习/复习/随机/单词测试/随机单词测试） | `m` 键循环 7 种模式；`--mode` 覆盖 | 行为不同 | TUI 无下拉菜单，也没有两种测试模式 |
| 练习 | 自定义流程（PracticeFlowV2） | `practice-flow-runtime.ts`（用户流程存 localStorage） | 无 | 缺失 | 枚举值 `Custom` 仅为存档兼容存在 |
| 练习 | 单词测试模式 | `(words)/words-test/[id].vue`、`core/utils/word-test.ts` | 无 | 缺失 | 枚举值 7/8 未接入 |
| 练习 | 随机复习范围/数量设置 | `ShufflePracticeSettingDialog.vue` | 固定：已学范围 + 每日词数 × 复习比例 | 行为不同 | TUI 无「已学 10 词」门槛；学习进度为 0 时取整本词库 |
| 练习 | 结算报告 | `components/word/Statistics.vue` | `src/ui/overlays.rs` | 已对齐 | 含错词 Top10 与本周记录（当前词典口径）；无分享入口 |
| 练习 | 重学 / 下一组 | 结算弹层按钮、`createRepeatTask` / `createNextTask` | 结算弹层 + 练习中 `Ctrl+Enter` / `Alt+←` / `Alt+→` | 已对齐 | 练习中可随时重学本组、上下组 |
| 练习 | 跳阶段 / 上一词 / 下一词 / 跳过 | `usePracticeWordNavigator` + 快捷键 | `domain/flow/navigator.rs` + 键位 | 已对齐 | 键位有差异，语义一致（见 4.2） |
| 练习 | 自测选择题 | `components/word/WordIdentifyPanel.vue`、`utils/word-test.ts` | `src/domain/question.rs` | 已对齐 | 4 选 1、相似度排序、随机扰动 |
| 调度 | 评级阈值 | `core/hooks/fsrs.ts` 的 `getGradeByWrongTimes` | `src/domain/scheduler.rs` | 已对齐 | 默认 0 / 3 / 6 次错误分档 |
| 调度 | 学习步进 | ts-fsrs 的 `learning_steps` / `relearning_steps` | 读设置，同上游 | 已对齐 | 详见 4.1 |
| 调度 | 复习间隔 | ts-fsrs（FSRS-6，21 权重） | `src/domain/fsrs.rs` 逐条移植 ts-fsrs | 已对齐 | 同一张卡的下次复习时间相同；黄金样本见 `tests/fsrs_parity.rs` |
| 调度 | 到期复习筛选 | `core/hooks/dict.ts` 的 `getCurrentStudyWord` | `src/domain/study_task.rs` | 部分 | 都按本地自然日；上游会顺手删除已掌握词的 FSRS 记录，TUI 只忽略不删 |
| 调度 | 高级 FSRS 参数 | `components/setting/FsrsSetting.vue` | 仅留存率有 UI | 部分 | `maximum_interval`/`w`/`enable_fuzz` 改文件后生效但无 UI；fuzz 为 ±5% 近似实现（见 4.3） |
| 调度 | 无到期时随机填充复习 | `study-task.ts` 的 `addRandomReviewWhenNoDue` | `src/domain/study_task.rs` | 已对齐 | 数量、排除集合、随机来源一致 |
| 统计 | 主页三项统计与本周记录 | `(words)/words.vue` | `src/ui/home.rs`、`src/ui/insights.rs` | 已对齐 | 今日时长 / 学习天数 / 累计时长 + 周一至周日 |
| 统计 | 日历与按日记录钻取 | `base/Calendar.vue`、`words.vue` 学习记录弹窗 | 无（统计页展示最近 14 天） | 部分 | 无日历、无按词典拆分的当日明细、无学习记录弹窗 |
| 统计 | 独立统计页 | 无（Web 无对应页面） | `src/ui/stats.rs` | TUI 独有 | TUI 新增页面，见第 6 节 |
| 统计 | 统计口径 | `Statistics.vue`、`flushStatToStore` | `src/domain/statistics.rs` | 部分 | 跨天拆分、sessionRole、正确率算法一致；聚合时按 role 去重计数（见 4.5） |
| 设置 | 设置页与分组 | `setting.vue`（10 个 tab） | `src/ui/settings.rs`（6 分区 24 行） | 部分 | 覆盖练习手感与 FSRS 阈值；详见 4.3 与设置项参考 |
| 设置 | 自定义快捷键 | `setting.vue` 快捷键 tab、`config/env.ts` 的 `DefaultShortcutKeyMap`（42 个动作） | 快捷键编辑页，可改绑/清除/恢复默认 | 已对齐 | 存于 `shortcutKeyMap`，与上游同构；详见 [快捷键](shortcuts.md) |
| 设置 | 练习内快捷设置 | `components/setting/SettingDialog.vue` | 无 | 缺失 | 只能回到全局设置页 |
| 设置 | 字号设置 | `components/setting/WordSetting.vue`（滑杆） | 无 | 不适用 | 终端字号由终端与字体决定 |
| 外观 | 主题 | CSS 变量 + `hooks/theme.ts`（light / dark / auto） | 13 token、3 套内置 + 自定义 JSON | 行为不同 | TUI 独有自定义主题；`auto` 等同深色，不跟随系统 |
| 外观 | 响应式布局 | CSS 断点 + 移动端顶部导航 | 终端宽窄外壳（100 / 120 列阈值，60×20 下限） | 行为不同 | 见第 6 节与 [渲染层](ui.md) |
| 外观 | 动画 / 过渡 | SCSS 动画、Vue Transition、Shepherd Tour | 无 | 缺失 | 终端字符渲染无法对等（见第 9 节） |
| 交互 | 键鼠操作 | 浏览器键鼠 + 可配置快捷键 | 可配置键位 + 终端鼠标（点击/滚轮） | 已对齐 | 见 4.2 与 4.6 |
| 交互 | 拖拽 / 悬停 | 列表拖拽排序、hover 显示单词 | 无拖拽、无 hover | 缺失 | 终端鼠标没有悬停语义 |
| 交互 | 文本选择与复制 | 浏览器原生选择 | 鼠标捕获下用 `Shift+拖拽` | 行为不同 | 见 [使用手册](usage.md) 的鼠标一节 |
| 断点 | 本地练习缓存 | `PracticeSaveWord` v2 | `practice-word.json` v2 | 已对齐 | 同构 compact 结构（含 `sessionSnapshot`） |
| 断点 | v1 缓存升级 | `checkAndUpgradePracticeWordCache` | 仅接受 v2 | 部分 | v1 缓存直接忽略，文件在下一次保存时覆盖 |
| 断点 | 跨设备恢复与冲突 | `useDataSyncPersistence.ts`（Supabase） | 无 | 缺失 | 无网络与远端比较 |
| 文章 | 文章背诵 | `(articles)/*` 4 个页面、`hooks/article.ts`、`components/article/*` | 无 | 缺失 | TUI 不加载 `dicts/en/article` |
| 音频 | 发音与键盘音效 | `hooks/sound.ts`、`base/Audio.vue`、`components/article/ArticleAudio.vue` | 无 | 缺失 | 不引入音频依赖；听写阶段改看音标与释义 |
| 查词 | 练习中查词与在线翻译 | `WordLookupPopover.vue`、`libs/translate/*`、`public.word/query` | 无 | 缺失 | 离线设计目标 |
| 笔记 | 单词笔记 | `TypeWord.vue` 编辑区、`(words)/dict.vue` 笔记字段 | 仅 `BaseState.note_data` 透传 | 缺失 | 数据保留但不能显示或编辑 |
| 多语言 | 界面语言切换 | `i18n/locales/*.json`（14 种） | 中文硬编码（`src/domain/i18n.rs`） | 缺失 | 键名对齐上游但无语言包 |
| 云同步 | Supabase 同步 | `utils/supabase.ts`、`useDataSyncPersistence.ts` | 无 | 缺失 | 上游设置页已把该功能标为「过时功能」 |
| 备份 | 备份导入导出 | `hooks/export.ts`（ZIP：`data.json` + mp3；JSON） | 无 UI | 缺失 | 只能手工复制数据目录 |
| 帮助 | 帮助 / 关于 / 文档 / 反馈 | `help.vue`、`about.vue`、`doc.vue`、`feedback.vue` | `src/ui/help.rs` | 部分 | TUI 帮助页是快捷键 + 鼠标说明，无 FAQ / 关于 / 资源页 |

## 4. 行为差异深入

### 4.1 FSRS

**已对齐。** 评级映射、到期筛选、统计字段、FSRS-6 公式、学习步进、fuzz 区间与随机种子两端一致：同一张卡在 TUI 与 Web 上算出的下次复习时间相同。

| 维度 | 上游（ts-fsrs 5.4.1） | TUI（`src/domain/fsrs.rs`） |
| --- | --- | --- |
| 评级阈值 | `getGradeByWrongTimes`，默认 0/3/6 | 同左（`Scheduler::grade_by_wrong_times`） |
| 新词 Again | 进入学习步骤 1（1 分钟） | 同左 |
| 新词 Hard | 学习步骤的中间值（默认 6 分钟） | 同左 |
| 新词 Good | 学习步骤 2（10 分钟） | 同左 |
| 新词 Easy | 按 FSRS 计算后毕业 | 同左 |
| 学习步进 | `learning_steps` / `relearning_steps` 可配 | 同左（读设置） |
| 复习卡 Again | 进入重学步骤（默认 10 分钟） | 同左 |
| 复习卡 Hard/Good/Easy | FSRS 间隔（≥ 1 天，且 hard < good < easy） | 同左 |
| `maximum_interval` | 生效 | 生效 |
| 21 个权重 `w` | 生效 | 生效（按上游 `CLAMP_PARAMETERS` 夹取） |
| `enable_fuzz` | 生效（分段区间 ±15% / ±10% / ±5%） | 同左（区间与随机源一致） |
| `enable_short_term` | 生效（切换 Basic / LongTerm 调度器） | 同左 |
| 随机种子 | 卡片状态派生（`{时间戳}_{reps}_{难度×稳定性}`） | 同左（移植上游 Alea PRNG） |
| 到期判断 | `dayjs(due).isSameOrBefore(now, 'day')` | `due` 换算本地日期后 `<= today` |
| 已掌握词 | 生成任务时从 `fsrsData` 中删除 | 只排除，不删除记录 |

一致性由 `tests/fsrs_parity.rs` 锁定：比对样本 `tests/fixtures/fsrs_golden.json` 由 `tools/fsrs_golden.mjs` 在上游仓库里用 ts-fsrs 生成，覆盖新卡起步、学习步进、毕业复习、同日短期记忆、fuzz 与长期调度六类路径。

**仍存在的差异**：已掌握词在 TUI 里只被排除、不从 `fsrsData` 删除（上游会删），因此两端存档里的卡片集合可能不同——但卡片本身的调度结果一致。

### 4.2 键位

上游 42 个动作全部可在设置里改键（`DefaultShortcutKeyMap`）；TUI 同样把键位做成数据（`src/domain/shortcut.rs` 的 `ShortcutMap`，存于 `shortcutKeyMap`），可在设置页改绑。默认键位与上游逐条一致，只有下列差异。同名动作的对照：

| 动作 | 上游默认键 | TUI | 差异 |
| --- | --- | --- | --- |
| 输入 / 确认 / 删除 | 字母、空格、退格 | 同左 | 已对齐 |
| 显示单词 | `Escape` | `Esc` | 已对齐（遮罩时计一次错误） |
| 跳过单词 | `Tab` | `Tab` | 已对齐 |
| 收藏 | `Enter` | `Enter` | 已对齐 |
| 收藏到指定词典 | `Shift+Enter` | 同左 | 已对齐 |
| 标记已掌握 | 反引号 | 反引号 | 已对齐 |
| 上/下一个词 | `Ctrl+←` / `Ctrl+→` | 同左，另有 `Ctrl+N` 为「下一个词」别名 | 已对齐（TUI 增补别名） |
| 跳到下一阶段 | `Shift+→` | 同左，另有 `Ctrl+J` 别名 | 已对齐（TUI 增补别名） |
| 上一章 / 下一章 | `Alt+←` / `Alt+→` | 同左（按学习组重放） | 已对齐（TUI 的「章」= 一个学习组） |
| 重复本章（重学） | `Ctrl+Enter` | 同左（重学本组） | 已对齐 |
| 默写本章 | `Alt+Enter` | 同左（本组改默写模式） | 已对齐 |
| 播放单词发音 | `Ctrl+P` | `Ctrl+P` 暂停 / 恢复计时 | 行为不同（TUI 复用该键） |
| 切换翻译 | `Ctrl+Z` | 同左 | 已对齐 |
| 切换默写遮罩 | `Ctrl+I` | 同左 | 已对齐 |
| 工具栏 / 词表侧栏 | `Ctrl+B` / `Ctrl+L` | 同左 | 已对齐 |
| 随机默写 | `Ctrl+R` | 同左 | 已对齐 |
| 切主题 / 简洁模式 / 编辑文章 | `Ctrl+Q` / `Ctrl+M` / `Ctrl+E` | `Ctrl+Q` / `Ctrl+G` / 显示隐藏例句与词源 | 主题与简洁模式已对齐；`Ctrl+M` 在终端等同回车，简洁模式改用 `Ctrl+G`；文章编辑尚未实现 |
| 自测三键 | `1` / `2` / `3` | 同左 | 已对齐 |
| 自测选择题 | `Alt+1`~ `Alt+4` | 同左 | 已对齐 |
| 播放例句 1~9 | `Ctrl+1`~ `Ctrl+9` | 无 | 缺失（无音频） |
| 单词测试下一题 | `Space` | 无 | 缺失（无测试模式） |

TUI 另有上游没有的全局键：`1`~`5` 切页、`d` / `s` / `t` 直达页面、`m` 循环练习模式、`f` 自由练习、`+` / `-` 调整每日词数、`q` 退出、`?` 帮助。部分终端会吞掉 `Alt+数字`、`Ctrl+方向键` 等组合键，TUI 因此提供了 ASCII 别名并允许直接拼写代替选择题；完整键表见 [使用手册](usage.md) 与 [快捷键](shortcuts.md)。

### 4.3 设置建模范围

上游 `SettingState`（`app/core/stores/setting.ts`）有 58 个顶层字段，设置页分 10 个 tab。TUI 的 `SettingState` 建模 29 个具名字段：28 个与上游同名（`theme`、`repeatCount`、`fsrsParameters`、`shortcutKeyMap` 等）+ TUI 专有的 `tuiBigWord`；其余 30 个上游顶层字段落入 `extra`，读写时原样保留。设置页覆盖 23 个配置项 + 3 个动作项（快捷键、清空练习进度、清除所有数据），共 26 行。

| 类别 | 项 |
| --- | --- |
| 有 UI 且生效 | 练习模式、主题、音标、大字单词、复习比例、无到期随机填充、忽略简单词、自动下一词、重复次数、重复次数(自定义)、切词等待、空格冷却、错误即清空、忽略大小写、自测选择题、词源、上下词、显示使用提示、FSRS 三阈值、目标留存率、清空进度、清除所有数据 |
| 建模但无 UI | `wordPracticeType`、`dictation`、`translate`、`ignoreSymbol`、`alwaysShowNote`、`_ignoreWatch` |
| 无 UI 但生效 | `maximum_interval`、`fsrsParameters.w`、`fsrsParameters.enable_fuzz`（手工编辑 `setting.json` 后 `Scheduler` 会读取） |
| 建模但不生效 | `enable_short_term`、`learning_steps`、`relearning_steps`（学习步进固定 1/6/10 分钟） |
| 上游有、TUI 完全没有 | 声音（音量/语速/键音/TTS 声色）、字号、快捷键表、练习内设置弹层、文章音效、`practiceSentence`、`autoPlayFirstSentence`、侧栏折叠、移动端导航等 |

完整的逐项说明、取值与手工编辑方法见 [设置项参考](settings.md)；字段透传规则见 [存档与数据格式](persistence.md)。上游设置页的 Supabase、更新日志、关于等 tab 在 TUI 中没有对应功能。

### 4.4 断点缓存

两端都用 v2 的 compact 结构：`taskWordsStr`、`practiceData`（词表以字符串存）、`statStoreData`、`sessionSnapshot`（`flowId` + `cursor` + `nodeWorkingWordKeys`）。差异：

- 上游保留 v1→v2 升级路径（`resolveLegacyPracticeWordCursor` 按旧 stage 反推游标）；TUI 只接受 `version == 2`，其余视为不支持并忽略缓存，不移动文件，下一次保存覆盖。
- TUI 要求 `taskWords` + `practiceData` + `statStoreData` + `sessionSnapshot` 齐全才恢复；上游允许只有 `taskWordsStr` 的「未开始」缓存。
- 上游在浏览器隐藏/可见、远程数据更新、组件卸载时都会保存或重载；TUI 在推进/后退/跳过/错误后 500 ms 防抖保存、每 30 秒保存、`Ctrl+C`/`Ctrl+Enter` 与退出时保存，没有跨设备重载。

### 4.5 统计口径

- 结算写入统计的算法与上游 `flushStatToStore` 对齐：按时间片段的自然日拆分，跨天时给多条记录标 `sessionRole`（`single` / `start` / `middle` / `end`），`tests/end_to_end.rs` 覆盖写入。
- 正确率两端都是 `(total - wrong) / total`，`wrong` 取本轮去重后的错词数。
- TUI 统计页聚合所有单词词典的 `statistics`（`domain/statistics.rs` 的 `daily_totals`），展示累计 + 今日增量 + 最近 14 天；聚合时 `middle`/`end` 记录只计时长，计数只归开始日，避免跨天会话重复累计。上游没有跨词典累计视图，因此不存在该问题；上游没有独立统计页，日历/日记录弹窗能看到每条记录的词典名，TUI 无法按词典拆分。
- TUI 主页本周记录按全部词典聚合；上游结算弹层的「本周记录」只看当前词典。
- TUI 结算弹层缺少上游的错词 Top10 与每周记录（数据其实都在 `wrongTimesMap` 与词典状态里，只是没渲染）。

### 4.6 交互与计时

- 上游有 CSS 动画、Vue 过渡、hover 显示单词、拖拽、Shepherd 引导；TUI 全部没有。终端鼠标只有点击与滚轮，没有悬停、拖拽和右键菜单。
- 上游可播放键盘音效、正确/错误音效与单词发音（受设置开关控制）；TUI 无音频，听写阶段靠音标与释义回忆。
- 计时：两端都有「无操作 3 分钟自动暂停」，且都在检测到键入活动时自动恢复并提示「已恢复计时」；手动暂停（`Ctrl+P`）不会被自动恢复。差异是上游还有「切走标签页即暂停」的可见性场景，终端没有对应概念。上游的设置项 `waitTimeForChangeWord` 在自动下一词时作为延迟，TUI 同义。
- TUI 事件循环是 100 ms 轮询 + 批量读事件；上游是浏览器事件驱动。对键盘手感影响可忽略，但 TUI 没有帧动画。

### 4.7 数据存储

上游用 `idb-keyval` 把 4 类数据存进 IndexedDB，老版本 localStorage 会自动迁移；Pinia `$subscribe` 防抖 1 秒自动保存，并叠加 Supabase 推送/拉取与冲突检测，解析失败时用 hash 快照兜底。TUI 用 3 个 JSON 文件：同目录临时文件 + fsync + rename 原子写，解析失败重命名为 `*.corrupt-<时间戳>.json` 并回退默认值，版本过高时 `dict.json` 与 `setting.json` 都只读不覆盖，练习缓存版本不受支持时忽略（详见 [存档与数据格式](persistence.md)）。

TUI 在保存时会清空非系统/非自定义词典的词表（`BaseState::shake_common_dict`，对应上游 `shakeCommonDict`），因此 `dict.json` 不会把 4.5 MB 的 CET-4 词表写进去；官方词库在启动时按 `url` 从 `assets/` 重新加载。上游的保存是「整份 store 状态」，TUI 是「显式调用 + 延迟保存」。

重置数据两端都只有整体入口：上游「设置 → 数据 → 清除所有数据」（`PopConfirm` + `dataSyncPersistence.clear()`）把 base store 与 setting store 一起 `setState(getDefault*())` 并清空 practice 缓存；TUI 的设置页「其他 → 清除所有数据」走 `App::clear_all_data`，同样是 `BaseState::default()` + `SettingState::default()` + 删除 `practice-word.json`，文案沿用上游的「该操作将会清除所有数据，确认继续？」与「清除成功」。差异只在位置（上游在「数据」tab，TUI 在设置页「其他」分区）与后续动作：上游清完让用户重新选词典（`studyIndex = -1`），TUI 按全新安装的路径自动选一本（有 `cet4` 用它）。上游没有「只清学习记录」或「只清统计」的入口，TUI 同样没有；TUI 额外的「清空当前练习进度」只删断点缓存，对应上游切词典 / 改练习设置时的 `resetCacheData` 副作用，上游没有把它做成按钮。

## 5. 数据互通

### 5.1 互通边界

两端共享「同名字段 + 同构信封」（`{val, version, updated_at?}`）；Web 把信封序列化成 JSON 字符串存进 IndexedDB，TUI 直接存成文件。TUI 不读写浏览器 IndexedDB，也没有导入/导出 UI，可搬运的只有 JSON 文本：

| TUI 文件 | Web 存储 key | 版本 | 备注 |
| --- | --- | --- | --- |
| `dict.json` | `typing-word-dict` | 4 | 字段同名；系统词典与当前词库进度可搬运 |
| `setting.json` | `typing-word-setting` | 23 | TUI 建模 28 个字段，其余透传 |
| `practice-word.json` | `PracticeSaveWord` | 2 | compact v2；TUI 额外要求 `sessionSnapshot` |
| 无 | `PracticeSaveArticle` | 1 | 文章与音频没有对应实现，迁移时直接丢弃 |

### 5.2 Web 备份结构与迁移方向

Web 端「设置 → 数据管理 → 导出」会生成 ZIP（`data.json` + `mp3/`）或 JSON，`data.json` 是导出格式 v5：

```text
Web 备份 data.json                       TUI 数据目录
{ version: 5, val: {                     <home>/
  dict:            <信封 version 4>  ->    dict.json
  setting:         <信封 version 23> ->    setting.json
  PracticeSaveWord:<信封 version 2>  ->    practice-word.json
  PracticeSaveArticle: <信封 v1>         (丢弃)
  type-words-app-version: 4              (丢弃)
} }                                      themes/*.json（仅 TUI）
```

手工迁移步骤：Web 导出 / 从 IndexedDB 取出 JSON → 关闭 TUI 并备份 `<home>` → 按上表写入三个文件（保留各自信封）→ `tw doctor` 核对 → 启动检查设置与断点。反向迁移则是按 Web 备份格式补齐 `dict` / `setting` / `PracticeSaveWord` 三个 key（`PracticeSaveArticle` 给空值），再在 Web 设置页导入。字段级细节与逐条风险见 [存档与数据格式](persistence.md) 的「与 Web 数据互通」一节。

### 5.3 字段语义差异与风险

| 差异 | 影响 |
| --- | --- |
| `tuiBigWord` | Web 没有该字段；上游 `checkAndUpgradeSaveSetting` 只保留自己认识的字段，经 Web 加载后再保存会丢失，TUI 再读时回到默认值（true） |
| 设置未知字段 | TUI 通过 `extra` 原样保留；Web 端会把不认识的字段丢掉。TUI → Web → TUI 往返会丢失 TUI 补充字段 |
| FSRS 卡片 | `due` 用 ISO 时间字符串、其余字段同名；但学习卡在 TUI 的 `stability` / `difficulty` 可能为 0，Web 继续调度这种卡片的行为未实测 |
| FSRS 参数 | 参数结构相同、可互拷，但两端调度实现不同，「同一张卡」的后续间隔不可复现 |
| 笔记 `noteData` | 两端同名字段；TUI 只透传，Web 可编辑 |
| 文章数据 | 系统「文章收藏」词典的词条会原样保留（`serde_json::Value`）；TUI 的 `shake_common_dict` 会清空所有非 system 文章书籍的文章列表（比上游只清非 custom/non-system 更激进），TUI 不读取文章正文 |
| 练习缓存 | Web 旧缓存若没有 `sessionSnapshot`，TUI 只能恢复词表与统计、无法恢复游标 |

### 5.4 未来互通建议

- **Web 导出适配器**：在 TUI 侧加一个 `tw import-web <data.json>`，解析 `BackupData` v5、按版本校验后拆成三个文件，并输出丢弃项（文章缓存、app version）与风险提示；对应地加 `tw export-web` 生成可被 Web 导入的 JSON。
- **版本先行**：导入前检查 `dict.version` / `setting.version` / `PracticeSaveWord.version`，高于 TUI 支持的版本时拒绝写入并提示升级，避免只读/覆盖问题。
- **卡片归一化**：对学习卡重新计算 `stability` / `difficulty` 或在导入时标记「需要重新学习」，避免跨端调度异常。
- **明确「不映射」清单**：文章缓存、mp3、Supabase 快照、快捷键/字体/主题等平台相关字段不应伪装成可互通。

## 6. TUI 独有

| 能力 | 实现位置 | 说明与限制 |
| --- | --- | --- |
| 自定义主题 | `src/theme/` | 13 个颜色 token、三套内置主题、`<home>/themes/*.json`、base 继承与校验告警；Web 用 CSS 变量，两端不共享主题文件 |
| 鼠标 | `src/app/mouse.rs`、`src/ui/hit.rs`、`src/ui/target.rs` | 侧栏/按钮/列表点击与滚轮滚动；练习页不响应鼠标（结算弹层除外）；终端鼠标捕获后需 Shift+拖拽复制文本 |
| 终端响应式外壳 | `src/ui/shell.rs` | < 100 列切窄屏顶栏标签，≥ 120 列侧栏 26 列；< 60 × 20 只渲染提示 |
| 点阵大字 | `src/ui/bigtext.rs`、`src/ui/practice.rs` | 5×7 点阵字体，空间足够时按本轮最长单词整体放大 2 倍；非 ASCII 回退普通文本 |
| 独立统计页 | `src/ui/stats.rs` | 累计 6 卡 + 最近 14 天，Web 无对应页面 |
| 完全离线 | `Cargo.toml`、`src/paths.rs` | 无 HTTP 客户端、无 WebView、无账号与遥测；数据目录可整体拷贝 |
| 单二进制分发 | `scripts/package.ps1`、`.github/workflows/release.yml` | `tw.exe` + `assets/` + docs/LICENSE 的 Windows zip 与 SHA256；无运行时依赖 |
| 命令行工具 | `src/cli.rs` | `tw doctor` 排障、`tw task` 预览任务、`tw import-dict` 导入词库 |
| 可复现界面 | `examples/shot.rs`、`examples/dump.rs`、`tools/render-snapshot.py` | 页面渲染成 JSON/文本，用于截图与视觉回归（开发用） |

## 7. 路线图

### 7.1 现状核实

- **已接线（本轮）**：
  - 跳组：状态行「第 N 组」可点击，弹层选组跳转；`create_task_from_group` 有了调用点。
  - 上一组 / 下一组 / 重学本组 / 默写本组：练习中 `Alt+←` / `Alt+→` / `Ctrl+Enter` / `Alt+Enter`。
  - `Shift+Enter`：弹层选择收藏目标词典。
  - 简洁模式（`Ctrl+G`）与切主题（`Ctrl+Q`）。
  - 快捷键表：42 个动作数据化，可在设置页改绑，存于 `shortcutKeyMap`（见 [快捷键](shortcuts.md)）。
  - FSRS：改为逐条移植上游 ts-fsrs 5.4.1，学习步进、`enable_short_term`、`enable_fuzz`、`maximum_interval` 全部生效，黄金样本见 `tests/fsrs_parity.rs`。
  - 结算报告：补齐错词 Top10 与本周记录（当前词典口径）。
  - 计时：空闲自动暂停后，输入即自动恢复。
- **仍待接线**：
  - **笔记**：`BaseState.note_data` 只出现在类型定义与默认值，无读取/写入/渲染代码。
  - **FSRS 高级项 UI**：`maximum_interval`、21 个 `w`、`enable_fuzz` 已生效，但设置页还没有入口。
  - **练习模式面板**：目前 `m` 键循环 7 种模式，没有上游那样的下拉（含单词测试模式）。
  - **单词测试模式**：`ShuffleWordsTest` / `ReviewWordsTest` 枚举值仍未接入。
  - **词条管理屏**：词条增删改查、批量标记、自定义词典都还没有。
- **可复用的已完成品**：7 个内置流程、跳阶段、自测选择题、错词清空、结算/重学/下一组、缓存恢复、统计写入都有测试覆盖，补齐时优先接线而不是重写。

### 7.2 优先级

| 优先级 | 内容 | 上游参考 | 代码落点 | 依赖 | 复杂度 | 验收标准 |
| --- | --- | --- | --- | --- | --- | --- |
| P0 | 跳组 | `GroupList.vue`、`createTaskFromGroup` | `src/domain/session.rs`（已有）+ `src/app/action.rs` + 练习页状态行入口 | 无 | S | 练习中可选组重建会话并更新缓存；补 `tests/app_drive.rs` |
| P0 | 练习中重学 / 下一组 | 结算弹层、`createRepeatTask` / `createNextTask` | `src/app/mod.rs` 练习键位 + 会话方法 | 无 | S | 组合键触发成功且统计/进度正确，不再只保存 |
| P0 | 笔记显示与编辑 | `TypeWord.vue` 笔记区 | `src/domain/types.rs`（字段已有）+ `src/ui/practice.rs` + 复用输入弹层 | 无 | M | 显示、编辑、删除并持久化到 `dict.json`；重启后保留 |
| P0 | `Ctrl+M` 简洁模式 | `toggleConciseMode` | `src/app/mod.rs` | 无 | S | 切换工具栏/词表显示 |
| P0 | 收藏到指定词典 | `WordCollectPopover.vue` | 新 Action + 词库选择弹层 + `session.toggle_collect` 扩展 | 多词库选择 UI | M | `Shift+Enter` 选择目标词典并写入 |
| P0 | FSRS 高级参数 UI | `FsrsSetting.vue` | `src/app/mod.rs` 的 `SETTING_ROWS` / `EditTarget` | 无 | M | `maximum_interval` / `w` / `enable_fuzz`（fuzz 已接入近似实现）可在设置页编辑并写入 |
| P1 | 词库 / 单词管理屏 | `dict.vue`、`BaseTable.vue`、`WordItem.vue` | 新 `src/ui` 页面 + `domain/catalog.rs` 写路径 + 自定义词典 | 词库文件读写 | L | 搜索/增删改/批量删除/自定义词典可用 |
| P1 | 单词测试模式 | `words-test/[id].vue`、`utils/word-test.ts` | `domain/question.rs`（已有）+ 新 Screen + 结算 | 无 | M | 4 选 1、判分、错词入错词本 |
| P1 | 统计按词典拆分 + 日历 | `app/base/Calendar.vue`、`(words)/words.vue` 日记录弹窗 | `src/ui/stats.rs`、`domain/statistics.rs` | 无 | M | 可按日/按词典查看，复用已有 `statistics` 数据 |
| P1 | `Ctrl+Q` 切主题 / 可配置键位 | `setting.vue` 快捷键 tab、`shortcutKeyMap` | `src/app/keymap.rs` 重构为映射表 + 设置页 | 与 `shortcutKeyMap` 存档对齐 | L | 快捷键可修改、持久化并在重启后生效 |
| P2 | Web 备份 JSON 导入导出 | `hooks/export.ts`、`setting.vue` 数据管理 | `src/cli.rs` 子命令 + `store` 适配器 | 无 | M | 能读写 `BackupData` v5（先 JSON，ZIP 后置） |
| P2 | 音频（发音 + 键音） | `hooks/sound.ts` | 新增音频后端依赖 + 练习页播放触发 + 设置项 | 新 crate 与音频资源 | L | 发音与键音可开关，失败可回退 |
| P2 | 文章背诵 | `(articles)/*`、`hooks/article.ts`、`practice-sentences/*` | 新领域模型 + 屏幕 + 资源加载 | 文章资源、音频 | XL | 句子级跟打/默写与结算 |
| P2 | 自定义流程编辑器 | `practice-flow-runtime.ts` | 流程配置序列化 + 新 Screen | 与上游 flow v6 兼容 | L | 可创建/选择/保存流程，缓存可恢复 |
| P2 | 云同步 | `useDataSyncPersistence.ts` | 新网络层 + 冲突合并 | HTTP 客户端、用户 Supabase | XL | 不建议近期做：上游已标「过时功能」 |

### 7.3 建议顺序与风险

1. 先做 P0 中不引入新依赖的项（跳组、重学/下一组、笔记、`Ctrl+M`），每项单独提交并补 `tests/app_drive.rs`。
2. 再做收藏到指定词典与 FSRS 高级参数：前者需要先有多词典选择 UI，后者改动会影响所有卡片，必须用设置备份 + 回归测试兜底。
3. P1 的管理屏与测试模式工作量最大，建议先做词条只读浏览、再做写操作；统计拆分可以复用现有聚合函数，风险最低。
4. P2 的音频、文章、云同步都会引入平台依赖或网络依赖，与「单二进制、离线」定位冲突，需要单独评审。

主要风险：键位可配置会与终端保留键（`Ctrl+C`、`Ctrl+Z`、`Ctrl+M` 等）冲突；FSRS 参数改动没有回滚 UI；音频/文章/网络需要新的 crate 与资源（体积、跨平台、许可证）；上游数据版本变化会让手工迁移失效，所以每次同步上游都要先核对版本号。

## 8. 上游同步维护

### 8.1 词库同步

`node scripts/sync-assets.mjs --source <TypeWords 仓库>` 会先删除本地 `assets/`，再复制上游 `public/list/word.json` 与 `public/dicts/en/word/`。当前仓库只提交了 CET-4（`assets/list/word.json` 仅 1 条、`assets/dicts/en/word/CET4_T.json`），且脚本不复制 `dicts/en/article`、`public/sound`、图片和推荐列表——这些都不在 TUI 的使用范围内。同步后要跑一次 `cargo test`，尤其是 `tests/real_assets.rs` 用真实 CET-4 数据校验加载。

### 8.2 上游升级时的比对清单

| 区域 | 上游位置 | 关注点 |
| --- | --- | --- |
| 页面清单 | `app/pages/` | 15 个页面的增删；决定功能矩阵是否要新增行 |
| 数据模型 | `app/core/types/types.ts`、`stores/base.ts`、`stores/setting.ts` | `Word` / `Dict` / `SettingState` 字段增减与默认值 |
| 存档与备份版本 | `app/core/config/env.ts`、`utils/cache.ts` | dict 4 / setting 23 / practice word 2 / article 1 / export 5 / app version 4 |
| 内置流程 | `core/composables/practice-words/practice-flow-config.ts` | `CURRENT_FLOW_VERSION`（当前 6）、`BUILTIN_FLOWS` 节点与步骤 |
| 练习引擎 | `core/composables/practice-words/*`、`usePracticeWordTyping.ts` | 游标、推进器、错词清空、键入状态机 |
| 调度 | `core/hooks/fsrs.ts`、`stores/setting.ts` 的 `fsrsParameters` | 默认权重、学习步进、评级阈值 |
| 设置 UI | `app/pages/setting.vue`、`components/setting/*` | 新增/删除设置项与分组 |
| 文章与音频 | `core/hooks/article.ts`、`components/article/*`、`composables/practice-sentences/*` | 文章模型、句音频、课后题（TUI 未实现，仅用于判定差距） |
| 桌面适配 | `app/core/utils/desktop.ts`、`src-tauri/`（本次快照未解包） | 仅了解产物差异 |

### 8.3 基线记录方式

每次随上游更新后，修改本文第 1.2 节的表格并补一行变更摘要（新增页面、schema 版本、流程版本）。建议格式：

| 上游参考 | 日期 | dict / setting / cache 版本 | 流程版本 | 主要变化 |
| --- | --- | --- | --- | --- |
| feature/tui 快照（master 6da7641 / dev bf50eae） | 2026-10 | 4 / 23 / 2 | 6 | 本拆分的初始对照基线 |

### 8.4 不要直接复制上游代码

- **架构不同**：上游是 Vue 组件 + Pinia + DOM 事件（还依赖隐藏 input 处理移动端输入、CSS 动画与 IndexedDB）；TUI 是 Rust 状态机 + ratatui + 终端事件。逐行翻译会把 DOM/响应式复杂度带进来，却得不到对应收益。
- **上游代码包含遗留与未完成路径**：登录/注册、Supabase 同步、只有引用而找不到页面的路由（如 `/fsrs`、`/practice-flow-editor`，无法确认完成度），还有大量 i18n 与平台分支；直接搬会把这些也带进 TUI。
- **许可证与维护**：本项目按 GPL-3.0 分发（词库数据沿用上游许可证）；复制上游代码会形成「上游衍生文件」，后续同步上游改动与许可证声明成本高。TUI 的做法是把行为翻译成 Rust 并写成测试（`tests/flow_parity.rs` 等），上游只作为行为参照。
- **可验证性**：TUI 的键位、tick、存档格式都由测试固定；复制代码无法得到这些保障。

### 8.5 建议同步流程

1. 取一份新的上游快照，记录 commit、日期与页面数。
2. 先比对 `env.ts` / `cache.ts` 的版本号与 `practice-flow-config.ts` 的 `CURRENT_FLOW_VERSION`；版本变化优先处理。
3. 对照页面清单与设置 UI，更新本文第 2、3 节的功能矩阵。
4. 若练习/调度行为变化，更新第 4 节并跑 `tests/flow_parity.rs` / `tests/end_to_end.rs`。
5. 若数据格式变化，先更新 [数据文件与互通](persistence.md)，再评估迁移与只读保护。
6. 最后同步根 README 的「与上游 TypeWords 的关系」摘要与文档索引，避免几处结论打架。

## 9. 不可对等能力

这些不是「路线图漏项」，而是运行形态决定的边界；路线图只保留可在终端内达成且值得做的项。

| 能力 | 上游实现 | 终端中的边界 |
| --- | --- | --- |
| WebView / 桌面壳 / 窗口 | Tauri v2 加载 Nuxt 产物，窗口、菜单、系统下载 | 终端没有窗口系统；TUI 用单进程事件循环替代宿主 |
| 动画、过渡、悬停、像素级布局 | CSS / Vue Transition / Shepherd Tour | 字符单元渲染，没有合成动画与 hover；只能用颜色和字符位置表达 |
| 音频与 TTS | 有道音频、`speechSynthesis`、键音 mp3 池 | 终端没有统一音频通道，需要额外进程/依赖；与「单二进制、离线」冲突 |
| 图片、图标、封面、字体 | Web 图片、图标库、自定义字体 | 终端字体由用户终端决定；点阵大字是折中方案 |
| 系统集成 | 文件选择、另存为、剪贴板、外链打开 | 用 CLI 参数、路径与终端快捷键替代；`Shift+拖拽` 才能复制文本 |
| 移动端与触控 | 响应式 Web + 移动端导航 | 终端场景不适用 |
| 在线服务 | 在线词库目录、百度翻译、查词 API、umami 统计 | 无网络是设计目标；也避免隐私与可用性问题 |
| 云同步 / 备份 UI | Supabase 用户自带后端、ZIP 导入导出、历史快照 | 需要账号、网络与服务端；与本地 JSON 模型冲突，优先级最低 |
| 多语言界面 | 14 种 locale JSON | 终端字体与宽度处理成本高；当前中文硬编码，键名已与上游对齐 |

## 相关文档

- [文档索引](README.md)
- [数据文件与互通](persistence.md)（存档格式、版本策略、手工迁移细节）
- [设置项参考](settings.md)（24 行设置与透传字段的逐项说明）
- [领域算法](domain.md)（FSRS、流程、会话与统计的实现）
- [应用状态机](app.md)（键位分发、保存时机与扩展点）
