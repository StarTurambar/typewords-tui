# typewords-tui 文档

本目录是 [typewords-tui](../README.md) 的完整文档：项目根目录的 README 负责「这是什么、怎么开始」，这里按主题展开——使用方法、架构与各模块、数据格式、主题系统、开发流程，以及与上游 TypeWords 的功能对照。

> 对应版本：typewords-tui 0.1.0（见 `Cargo.toml`）。文档以源码为唯一事实来源；代码行为变更时，请同步更新对应文档（见文末「文档维护」）。

## 我想……

| 我想…… | 从哪里开始 |
| --- | --- |
| 安装、第一次打开界面、完成一轮练习 | [根 README · 快速开始](../README.md#快速开始) → [使用手册](usage.md) |
| 查某个按键、鼠标操作、命令行参数 | [使用手册](usage.md) |
| 弄懂某个设置项是干什么的、默认值是多少 | [设置项参考](settings.md) |
| 改键位、查某个键位是什么作用 | [快捷键](shortcuts.md) |
| 换配色、写一套自己的主题 | [主题系统](themes.md) |
| 知道存档在哪、怎么备份/迁移、字段什么意思 | [存档与数据格式](persistence.md) |
| 参与开发，先跑起来、跑测试、提 PR | [开发指南](development.md) |
| 改动某个模块前先理解整体结构 | [总体架构](architecture.md) → [领域逻辑](domain.md) / [应用状态机](app.md) / [渲染层](ui.md) |
| 评估「和网页版比缺什么、差在哪」 | [与上游对照](upstream-parity.md) |
| 遇到启动失败、乱码、存档损坏等问题 | [使用手册 · 常见问题与排障](usage.md#常见问题与排障) |
| 给文档纠错或补充 | [开发指南 · 文档维护](development.md#10-文档维护) |

## 文档地图

### 用户文档

| 文档 | 内容 | 主要读者 |
| --- | --- | --- |
| [根 README](../README.md) | 项目定位、功能概览、下载/构建、常用命令、文档索引 | 所有人 |
| [usage.md](usage.md) | 完整使用手册：环境与终端、安装、CLI、界面导览、练习流程、按键、鼠标、FAQ/排障 | 使用者 |
| [settings.md](settings.md) | 设置项完整参考：分区、默认值、取值范围、生效方式、FSRS 参数、无 UI 的透传字段 | 使用者 / 维护者 |
| [shortcuts.md](shortcuts.md) | 快捷键完整参考：键位表模型、42 个动作与默认绑定、改绑方式、终端兼容别名、与上游差异 | 使用者 / 维护者 |
| [themes.md](themes.md) | 主题模型、内置主题、自定义主题 JSON 格式与校验规则、颜色语法、常见问题 | 使用者 / 主题作者 |

### 设计者与贡献者文档

| 文档 | 内容 | 主要读者 |
| --- | --- | --- |
| [architecture.md](architecture.md) | 技术栈、分层与依赖约束、进程生命周期、事件循环、数据流、关键设计决策、性能与隐私、技术债 | 贡献者 / 维护者 |
| [domain.md](domain.md) | domain 层：数据模型、常量、词库、今日任务、FSRS 调度、计时、键入状态机、流程编排、会话、统计、确定性 | 贡献者 / 维护者 |
| [app.md](app.md) | app 层：App 状态、启动、事件循环与 tick、输入路由、Action 管线、屏幕/弹层状态机、保存时机、扩展指南 | 贡献者 / 维护者 |
| [ui.md](ui.md) | ui 层：渲染管线、主题映射、响应式外壳、各页面与弹层、点阵大字、鼠标命中、渲染测试清单 | 贡献者 / 维护者 |
| [persistence.md](persistence.md) | 数据目录、存档信封与原子写、三份存档与资源 JSON 详解、版本策略、与 Web 数据互通、备份恢复 | 贡献者 / 维护者 |
| [development.md](development.md) | 环境、构建/测试/静态检查、测试体系、调试、截图工具、词库同步、CI 与发布、代码与文档约定 | 贡献者 / 维护者 |
| [upstream-parity.md](upstream-parity.md) | 与上游 TypeWords 的定位对比、功能矩阵、行为差异、数据互通、路线图、上游同步方法 | 决策者 / 贡献者 |

### 截图

`docs/screenshots/` 下的 PNG 由仓库自带的截图工具生成，用于 README、使用手册和主题文档。生成与更新方法见 [development.md · 截图与视觉检查](development.md#6-截图与视觉检查)。

## 推荐阅读路线

- **第一次使用**：根 README 的「快速开始」→ [usage.md](usage.md) 的「安装」「界面导览」「练习流程」→ 需要时查 [settings.md](settings.md)、[shortcuts.md](shortcuts.md)、[themes.md](themes.md)。
- **参与开发**：[development.md](development.md)（先跑通构建与测试）→ [architecture.md](architecture.md)（分层、生命周期、约束）→ 按任务读 [domain.md](domain.md) / [app.md](app.md) / [ui.md](ui.md) / [persistence.md](persistence.md)。
- **评估功能覆盖**：[upstream-parity.md](upstream-parity.md) 的「功能矩阵」「行为差异」→ 根 README 的「功能概览」。
- **排查问题**：[usage.md · 常见问题与排障](usage.md#常见问题与排障) → [persistence.md](persistence.md)（存档相关）→ [development.md](development.md)（调试工具）。

## 写作与维护约定

- **事实来源**：代码 > 测试 > 文档。README/文档中与源码不一致的表述属于缺陷；发现后先改文档，确认是代码缺陷再改代码。
- **源码引用**：只写路径与符号名（如 `src/domain/flow/mod.rs` 的 `resolve_phase_by_cursor`），不写行号，避免重构后失效。
- **单一权威**：同一事实只在一个文档里展开。用户操作细节在 [usage.md](usage.md)，设置项在 [settings.md](settings.md)，数据字段在 [persistence.md](persistence.md)，算法在 [domain.md](domain.md)，模块职责在 [app.md](app.md) / [ui.md](ui.md) / [domain.md](domain.md)，上游差异在 [upstream-parity.md](upstream-parity.md)。
- **链接**：文档之间用相对链接；从 docs/ 指向源码用 `../src/...`，指向根 README 用 `../README.md`。新增文档后同步更新本索引。
- **截图**：使用 [development.md · 截图与视觉检查](development.md#6-截图与视觉检查) 中的命令重新生成，不要手工截屏后缩放。
- **版本说明**：文档描述当前 main 分支行为；若发布版与 main 存在差异，以发布说明为准。

## 术语表

| 术语 | 含义 |
| --- | --- |
| 词库（dict / word book） | 一本可学习的单词集合，资源由 `assets/list/word.json` 注册、词条数据在 `assets/dicts/en/word/*.json`；用户导入的词库放在数据目录同名结构下。 |
| 系统词典 | 三本由程序维护的虚拟词典：收藏（`wordCollect`）、错词（`wordWrong`）、已掌握（`wordKnown`），不是可切换的学习词库。 |
| 练习模式 | 一次练习的整体形态：学习（system）、自由练习（free）、复习（review）、随机复习（shuffle）、自测（identify）、默写（dictation）、听写（listen）。 |
| 流程（flow） | 由若干「节点（node）→ 步骤（step）」组成的练习编排，决定每轮练什么、怎么练、何时结算；TUI 固定 7 个内置流程。 |
| 阶段（stage） | 当前练习轮次中的语义阶段，如「跟写新词」「自测旧词」「错词巩固」「随机复习」，用于界面展示与结算统计。 |
| 跟写 / 拼写 / 听写 / 默写 / 自测 | 五种键入方式：跟写是照着显示逐字母输入；拼写（Spell）在部分提示下拼写；听写只给音标/释义回忆；默写遮挡单词；自测用 1/2/3 或选择题判断认识程度。 |
| 小组（group） | 跟写阶段按 7 词一组推进：先整组跟写，再在组内逐词拼写巩固，然后进入下一组。 |
| 错词巩固（wrong word clear） | 某个阶段结束仍有错词时，自动进入的循环，直到错词清空或达到流程定义的结束条件。 |
| FSRS | Free Spaced Repetition Scheduler，间隔重复算法；TUI 用 `src/domain/fsrs.rs` 自实现 FSRS-6（逐条移植上游 ts-fsrs），复习间隔与 Web 端一致，评级（Again/Hard/Good/Easy）也一致。 |
| 卡片（Card） | 每个单词的调度状态：due、stability、difficulty、state、last_review、reps、lapses 等；字段名沿用 ts-fsrs。 |
| 到期（due） | 卡片的下次复习时间早于或等于今天（按本地自然日判断），会进入今日复习列表。 |
| 今日任务 / 每日目标 | 本次学习的新词数量（perDayStudyNumber）与到期复习的组合；新词用完可显示「已学完」。 |
| 评级阈值（easy/good/hard limit） | 依据一个词在一轮中的错误次数决定 FSRS 评级的阈值，设置页「FSRS」分区可调。 |
| 结算（summary） | 一轮练习结束后的报告弹层：时长、正确率、新词/复习/错词数、学习进度，提供继续/重学/回主页。 |
| 断点缓存（practice-word.json） | 当前练习的游标、词表与统计快照；进程退出或定时保存，下次启动恢复到同一词、同一阶段。 |
| Toast | 底栏的临时提示（成功/警告/错误/信息），数秒后消失。 |
| 弹层（overlay） | 覆盖在页面之上的结算、输入、确认三类对话框。 |
| 快捷键（shortcut） | 可配置动作与键位的对应关系，存于设置存档的 `shortcutKeyMap`；动作名沿用上游 `ShortcutKey` 枚举。详见 [shortcuts.md](shortcuts.md)。 |
| 主题 token | 主题的 13 个颜色语义槽位（bg/panel/element/border/fg/muted/dim/accent/accent_alt/warn/good/bad/selection），渲染层不硬编码颜色。 |
| 上游 | [zyronon/TypeWords](https://github.com/zyronon/TypeWords)：Nuxt + Vue 的 Web / Tauri 桌面端；本仓库是独立拆分出的终端客户端。 |

## 文档维护

- 本套文档按 0.1.0 的代码整理，覆盖 [architecture.md](architecture.md) 中列出的全部模块；新增模块或行为时，按「代码区域 → 权威文档」对照表更新对应文档并在本索引登记。
- 修改用户可见行为（按键、CLI、设置项、存档字段、主题格式）时，至少同步更新 [usage.md](usage.md)、[settings.md](settings.md)、[persistence.md](persistence.md)、[themes.md](themes.md) 中受影响的一篇。
- 修改内部结构（分层、流程、调度、渲染）时，同步更新 [architecture.md](architecture.md) 与对应模块文档。
- 修改上游对照结论或路线图时，在 [upstream-parity.md](upstream-parity.md) 的「基线」一节记录新的上游参考版本/日期。
- 文档中引用的相对链接与截图应在提交前检查（仓库的测试会校验文档相对链接存在，见 [development.md · 文档维护](development.md#10-文档维护)）。

## 相关文档

- [项目 README](../README.md)
- [使用手册](usage.md)
- [总体架构](architecture.md)
- [开发指南](development.md)
- [与上游对照](upstream-parity.md)
