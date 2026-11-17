# typewords-tui 文档

本目录是 [typewords-tui](../README.md) 的完整文档：根 README 负责「这是什么、怎么开始」，这里按主题展开。文档以源码为唯一事实来源；代码行为变更时同步更新对应文档。

> 对应版本：typewords-tui 0.4.0（见 `Cargo.toml`）。

## 我想……

| 我想…… | 从哪里开始 |
| --- | --- |
| 安装、第一次打开界面、完成一轮练习 | [根 README · 快速开始](../README.md#快速开始) → [使用手册](usage.md) |
| 弄懂某个设置项、默认值与生效方式 | [设置项参考](settings.md) |
| 换配色、写一套自己的主题 | [主题系统](themes.md) |
| 知道存档在哪、怎么备份 / 迁移、字段含义 | [存档与数据格式](persistence.md) |
| 参与开发，先跑起来、跑测试、提 PR | [开发指南](development.md) → [总体架构](architecture.md) |
| 评估「和网页版比缺什么、差在哪」 | [与上游对照](upstream-parity.md) |
| 遇到启动失败、乱码、存档损坏 | [使用手册 · 常见问题与排障](usage.md#常见问题与排障) |
| 程序崩溃 / 想找运行日志与崩溃报告 | [使用手册 · 日志与崩溃报告](usage.md#日志与崩溃报告) |
| 查某个版本改了什么、什么时候发布的 | [变更日志](../CHANGELOG.md) |

## 文档地图

| 文档 | 内容 | 主要读者 |
| --- | --- | --- |
| [usage.md](usage.md) | 安装、CLI、日志与崩溃报告、界面导览、10 种练习模式、按键、鼠标、FAQ / 排障 | 使用者 |
| [settings.md](settings.md) | 设置页 33 行、高级 FSRS 页、FSRS 参数、无 UI 的透传字段 | 使用者 / 维护者 |
| [shortcuts.md](shortcuts.md) | 42 个动作、默认绑定、改绑、TUI 专有键、与上游差异 | 使用者 / 维护者 |
| [themes.md](themes.md) | 主题模型、6 套内置、13 token、自定义主题格式与校验 | 使用者 / 主题作者 |
| [persistence.md](persistence.md) | 数据目录、5 份存档与资源 JSON、版本策略、Web 互通、备份 | 贡献者 / 维护者 |
| [architecture.md](architecture.md) | 分层、代码地图、生命周期、状态与输入、子系统契约、扩展指南、技术债 | 贡献者 / 维护者 |
| [development.md](development.md) | 环境、构建 / 测试、调试、截图、词库同步、CI 与发布、文档维护 | 贡献者 / 维护者 |
| [upstream-parity.md](upstream-parity.md) | 与上游的功能矩阵、行为差异、数据互通、同步方法 | 决策者 / 贡献者 |
| [CHANGELOG.md](../CHANGELOG.md) | 全部版本的新增、变更、弃用、移除、修复与安全记录（Keep a Changelog 格式，GitHub Release 正文来源） | 使用者 / 维护者 |

截图在 `screenshots/`，由仓库自带工具生成；方法与清单见 [开发指南 · 截图与视觉检查](development.md#6-截图与视觉检查)。

## 推荐阅读路线

- **第一次使用**：[根 README · 快速开始](../README.md#快速开始) → [usage.md](usage.md) 的安装 / 界面导览 / 练习流程 → 需要时查 [settings.md](settings.md)、[shortcuts.md](shortcuts.md)、[themes.md](themes.md)。
- **参与开发**：[development.md](development.md)（先跑通构建与测试）→ [architecture.md](architecture.md)（分层、生命周期、契约、扩展）→ 按任务查 [persistence.md](persistence.md)。
- **排查问题**：[usage.md · 常见问题与排障](usage.md#常见问题与排障) → [persistence.md](persistence.md) → [development.md](development.md) 的调试一节；[upstream-parity.md](upstream-parity.md) 记录与上游的功能差距。

## 写作与维护约定

- **事实来源**：代码 > 测试 > 文档。文档与源码不一致属于缺陷；先改文档，确认是代码缺陷再改代码。
- **单一权威**：同一事实只在一个文档展开。用户操作在 [usage.md](usage.md)，设置项在 [settings.md](settings.md)，键位在 [shortcuts.md](shortcuts.md)，数据字段在 [persistence.md](persistence.md)，跨层结构与扩展在 [architecture.md](architecture.md)，上游差异在 [upstream-parity.md](upstream-parity.md)。
- **变更日志**：用户可见改动在根目录 [CHANGELOG.md](../CHANGELOG.md) 的 `[Unreleased]` 下补一条（Keep a Changelog 六分类），发版时移到版本标题下；GitHub Release 正文由它生成，不要再单独写发布说明。
- **源码引用**：只写路径与符号名，不写行号，避免重构后失效。
- **链接**：docs/ 内互链用文件名；指向根 README 用 `../README.md`；指向源码用 `../src/...`。新增文档后在本文登记（`cargo test` 会校验索引与相对链接）。
- **版本**：只在本文写「对应版本」；发版时改这一处，`tests/docs.rs` 会校验它与 `Cargo.toml` 一致。

## 术语表

| 术语 | 含义 |
| --- | --- |
| 词库 / 词典 | 一组可学习的单词。资源词库由 `assets/list/word.json` 注册；用户词库经 `tw import-dict` 放进数据目录。 |
| 练习模式 | 一次练习的整体形态，共 10 种（学习、自由练习、自测、默写、听写、随机复习、复习、随机单词测试、单词测试、自定义流程）。 |
| 流程（flow） | 由「节点 → 步骤」组成的练习编排；TUI 有 7 个内置流程，另有可编辑的自定义流程。 |
| 阶段（stage） | 当前轮次中的语义阶段，如跟写新词、自测旧词、错词巩固、随机复习，用于展示与结算统计。 |
| 错词巩固 | 阶段结束仍有错词时自动进入的循环，按「跟写 + 7 词小组 + 组内拼写」重练，直到错词清空。 |
| FSRS / 卡片 | Free Spaced Repetition Scheduler；卡片保存每个词的 due、stability、difficulty、state 等调度状态，字段名沿用 ts-fsrs。 |
| 断点缓存 | `practice-word.json` / `practice-article.json`：未结算的练习游标、词表与统计快照；结算后删除。 |
| 主题 token | 主题的 13 个颜色语义槽位（`bg/panel/element/border/fg/muted/dim/accent/accent_alt/warn/good/bad/selection`），渲染层不硬编码颜色。 |
