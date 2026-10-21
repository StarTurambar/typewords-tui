# 存档与数据格式

typewords-tui 不使用数据库：学习进度、设置和断点缓存都是本地 JSON 文件，词库则是只读 JSON 资源。程序启动时把这些文件读进内存，运行中按固定时机原子写回。本文是这些文件的权威说明：目录与路径解析、存档信封与写入协议、三份带版本号的存档与一份无版本号的流程存档的字段、资源词库格式、`import-dict`、与上游 Web 端的数据互通，以及备份/恢复与隐私注意事项。

> 适用范围：`src/store/`、`src/paths.rs`、`src/bootstrap.rs`、`src/domain/catalog.rs`、`src/cli.rs` 的 `doctor`/`import-dict`、`src/domain/types.rs` 的存档模型；对应版本：typewords-tui 0.1.0。

## 1. 数据目录与路径解析

程序区分两套目录：**数据目录（home，可写）** 存所有用户数据；**资源目录（assets，只读）** 存随程序分发的词库。两者的解析入口都是 `src/paths.rs` 的 `Paths::resolve`。

### 1.1 目录布局

| 路径 | 读/写 | 内容 |
| --- | --- | --- |
| `<home>/dict.json` | 读写 | 词典状态 `BaseState`，版本 4 |
| `<home>/setting.json` | 读写 | 设置 `SettingState`，版本 23 |
| `<home>/practice-word.json` | 读写/删除 | 断点练习缓存 `PracticeCacheCompact`，版本 2 |
| `<home>/flows.json` | 读写 | 自建练习流程 `FlowStore`（无版本号，见 [领域逻辑 · 流程](domain.md)） |
| `<home>/themes/*.json` | 启动时只读 | 自定义主题（格式见 [主题系统](themes.md)） |
| `<home>/dicts/<language>/word/<url>` | 导入时写、运行时读 | `import-dict` 导入的用户词库 |
| `<home>/list/word.json` | 导入时写、启动时读 | 用户词库目录，与内置目录合并 |
| `<home>/.<文件名>.tmp` | 临时 | 原子写的中间文件，正常结束后不存在 |
| `<home>/<文件名>.corrupt-<时间戳>.json` | 隔离产物 | 解析失败的坏存档备份 |
| `<assets>/list/word.json` | 只读 | 内置词库目录 `DictResource[]` |
| `<assets>/dicts/en/word/*.json` | 只读 | 内置词条数据 `Word[]` |
| `<assets>/dicts/en/article/*.json` | 不读写 | 上游的文章数据，TUI 不加载 |

`<home>` 下的目录不会全部预先创建：`dicts/` 与 `list/` 由 `import-dict` 按需创建，`themes/` 需要用户自己创建。程序不写日志文件（没有日志设施），排障依赖 `tw doctor`、Toast 与复现步骤。

### 1.2 数据目录（home）的解析顺序

1. `--home DIR` 命令行参数；
2. 环境变量 `TYPEWORDS_HOME`（空值视为未设置）；
3. `dirs::data_local_dir()` 下的 `typewords-tui` 子目录——Windows 通常为 `%LOCALAPPDATA%\typewords-tui`，Linux 为 `$XDG_DATA_HOME/typewords-tui`（缺省 `~/.local/share/typewords-tui`），macOS 为 `~/Library/Application Support/typewords-tui`；
4. 以上都不可用时启动失败：`无法确定数据目录，请使用 --home 指定`。

数据目录本身由 `store::FileStore::new` 用 `create_dir_all` 创建，TUI 启动、`tw doctor`、`tw task` 都会触发；创建失败会报 `创建数据目录失败 <路径>: <原因>`。注意 `tw doctor`/`tw task` 只创建目录，不写存档文件。

### 1.3 资源目录（assets）的解析顺序

`locate_assets` 依次尝试，返回**第一个包含 `list/word.json` 的目录**：

1. `--assets DIR`（目录不合格时不会报错，而是继续尝试后续候选）；
2. 环境变量 `TYPEWORDS_ASSETS`；
3. 可执行文件同级的 `assets/`；
4. 当前目录及其最多 3 级祖先下的 `assets/`（每个祖先先试 `assets/` 再试 `public/`，用于开发期或从上游仓库内运行）；
5. 全部失败时启动失败：`找不到词库资源目录（需要包含 list/word.json）。可用 --assets 或环境变量 TYPEWORDS_ASSETS 指定；发布包可把 assets/ 放在可执行文件同级`。

## 2. 存档信封与写入协议

### 2.1 信封

三份存档共用 `src/store/mod.rs` 的 `SaveEnvelope<T>`，形式为 `{ "val": <状态>, "version": <版本>, "updated_at": "<RFC 3339>" }`：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `val` | 由文件决定 | 真正的状态对象；`practice-word.json` 允许 `null`（等价于没有缓存） |
| `version` | `u32` | 存档 schema 版本，读取时强制要求存在 |
| `updated_at` | `Option<String>` | 写入时的 UTC RFC 3339 时间；读取端只保存不使用，TUI 没有多端同步与冲突检测 |

写入使用 `serde_json::to_string` 的**紧凑格式**（单行、无缩进）；手写或格式化编辑不影响读取。`version` 的常量在 `src/domain/constants.rs`：`DICT_STATE_VERSION = 4`、`SETTING_VERSION = 23`、`PRACTICE_CACHE_VERSION = 2`，与上游 Web 端保持一致，便于手工迁移（见第 8 节）。

### 2.2 原子写（`write_atomic`）

1. 确保目标文件的父目录存在（`create_dir_all`）；
2. 在同目录创建临时文件 `.<文件名>.tmp`（例如 `.dict.json.tmp`）；
3. `write_all` 写入内容并 `sync_all`（fsync；失败被忽略，不阻断写入）；
4. `rename` 覆盖正式文件。临时文件与目标同目录，保证 rename 在同一文件系统内完成。

因此进程在写入中途被杀，最坏情况是留下一个过期的 `.tmp` 文件，正式存档要么是旧内容、要么是新内容，不会出现半截 JSON。整个协议没有文件锁：同一 `<home>` 同时运行两个实例会互相覆盖，同名临时文件也会互相覆盖，应避免。

### 2.3 读取、空文件与 `LoadStatus`

`FileStore::read_envelope` 的处理规则：文件不存在 → `Ok(None)`；文件只有空白字符 → 也按 `Ok(None)` 处理（视为缺失，不隔离、不报错）；读取或解析失败 → 进入隔离流程；成功则返回 `(val, version, updated_at)`。

`load_typed` 把结果归类为 `LoadStatus`：

| 状态 | 触发条件 | 内存值 | 启动提示（`Loaded::notice`） |
| --- | --- | --- | --- |
| `Loaded` | 解析成功且 `version <= 期望值` | 文件内容 | 无 |
| `Missing` | 文件不存在或为空 | 类型的 `Default` | 无 |
| `Corrupt(reason)` | 读取/解析失败，文件已重命名隔离 | 类型的 `Default` | `存档已损坏，已隔离并恢复默认值：<reason>` |
| `VersionTooNew { found, expected }` | `version >` 期望值 | 类型的 `Default`，原文件保留 | `存档版本过高（<found> > <expected>），本次不会覆盖，请升级后使用` |
| `UnsupportedVersion { found, expected }` | 版本不受支持（练习缓存 `version != 2`） | 类型的 `Default`（缓存为 `None`），原文件保留 | `存档版本不受支持（<found> != <expected>），已忽略；下次保存会覆盖` |

`bootstrap::load` 把 dict/setting 的提示收集进 `notices`，由 `App::bootstrap` 转成 Warning Toast；练习缓存的提示在恢复流程中弹出。Toast 只显示约 3.2 秒（最多同时保留 4 条），长期排障请看第 9 节的 `tw doctor`。

### 2.4 损坏隔离

解析失败时，`FileStore::quarantine` 把坏文件**重命名**为同目录下的 `<文件名>.corrupt-<YYYYMMDD-HHMMSS>.json`（时间戳取本地时间），然后在 `Corrupt` 的 `reason` 中记录 `<错误>（已移动到 <隔离路径>）`。原文件名随即恢复为缺失状态，程序用默认值继续运行，下次保存会生成新的存档。隔离文件不会被自动清理，确认无用后可手工删除。

`practice-word.json` 的解析失败同样会隔离；而**版本不支持**使用独立的 `UnsupportedVersion` 状态：不移动文件、提示「版本不受支持，已忽略」，避免把「不支持」误报成「损坏、已隔离」（见 5.4）。

### 2.5 版本策略

TUI 没有迁移框架：**低于等于期望版本**的存档直接按当前类型反序列化，缺失字段由各结构体的 `#[serde(default)]` 补默认值；**高于期望版本**才触发 `VersionTooNew`。各文件的具体后果：

| 文件 | 期望版本 | `version < 期望` | `version == 期望` | `version > 期望` |
| --- | --- | --- | --- | --- |
| `dict.json` | 4 | 接受，缺字段补默认 | 接受 | 默认值 + 只读保护（`readonly_dict`） |
| `setting.json` | 23 | 接受，缺字段补默认 | 接受 | 默认值 + 只读保护（`readonly_settings`） |
| `practice-word.json` | 2 | 视为不支持（`UnsupportedVersion`） | 接受 | 视为不支持（`UnsupportedVersion`） |

两个容易踩坑的后果：

- **dict 版本过高**：`bootstrap::load` 置 `readonly_dict = true`，`App::save_dict_now` 直接返回——本次运行的词库进度、FSRS 卡片、统计都不会落盘（关闭程序即丢），但原 `dict.json` 完好。设置与练习缓存仍会按各自规则保存。此时升级程序即可找回数据。
- **setting 版本过高**：`readonly_settings = true`，`App::save_settings_now` 直接返回——本次运行修改的设置、`--mode`/`--theme` 覆盖都不会写回，原 `setting.json` 完好；提示「本次不会覆盖」对 dict 与 setting 均成立。练习缓存仍会正常保存。降级运行同样受保护，但升级前仍建议备份数据目录。

### 2.6 `shake_common_dict`

`FileStore::save_dict` 在序列化前先克隆状态并调用 `BaseState::shake_common_dict`（定义在 `src/domain/types.rs`）：

```text
word.bookList 中 custom == false 且 system == false 的词典        → words 清空
article.bookList 中 system == false 的词典                        → articles 清空
```

效果是 `dict.json` 只保存「官方资源词库的进度/统计元数据 + 系统词典词条 + 自定义词典词条」，官方词表在下次启动时按 `url` 从资源目录重新加载，避免把约 4.5 MB 词表写进存档。清空发生在**克隆体**上，内存里的当前词表不受影响。给上游的对应实现是 `shakeCommonDict`（`app/core/utils/index.ts`），规则基本一致。

`custom` 标记由目录项（`home/list/word.json` 里的 `"custom": true`）经 `DictResource::to_dict` 带进 `Dict`；标记丢失会让自建词典的词表在下一次保存时被清空，因此 `create_custom_dict` 与 `BaseState::set_study_dict` 都要保证它跟着词典走。

## 3. `dict.json`（`BaseState`，版本 4）

### 3.1 示例

下面省略了大部分词条与卡片，只保留结构（数值为示意）：

```json
{
  "val": {
    "simpleWords": ["a", "an", "the"], "load": false,
    "word": {
      "bookList": [
        { "id": "wordCollect", "enName": "wordCollect", "name": "收藏", "system": true, "words": [] },
        { "id": "wordWrong", "system": true }, { "id": "wordKnown", "system": true },
        { "id": "1", "enName": "cet4", "name": "CET-4", "url": "CET4_T.json", "length": 2607,
          "language": "en", "translateLanguage": "zh_CN", "lastLearnIndex": 40,
          "perDayStudyNumber": 20, "words": [], "custom": false, "system": false,
          "statistics": [{ "startDate": 1792300000000, "spend": 125000, "total": 20,
                           "new": 20, "review": 0, "wrong": 1, "sessionRole": "single" }] }
      ],
      "studyIndex": 3
    },
    "article": { "bookList": [{ "id": "articleCollect", "system": true }], "studyIndex": -1 },
    "dictListVersion": 1,
    "fsrsData": {
      "cancel": { "due": "2026-10-20T08:00:00Z", "stability": 1.69, "difficulty": 4.31,
                  "elapsed_days": 0, "scheduled_days": 0, "learning_steps": 0,
                  "reps": 1, "lapses": 0, "state": 1, "last_review": "2026-10-18T13:30:00Z" }
    },
    "noteData": {}, "_ignoreWatch": false
  },
  "version": 4, "updated_at": "2026-10-18T13:34:19.123456789+00:00"
}
```

`studyIndex` 是 `word.bookList` 的下标，示例中 3 表示第 4 本（3 本系统词典之后的第一本学习词库）；没有任何学习词库时为 `-1`。

### 3.2 顶层字段

| JSON 字段 | Rust 字段 | 类型 | 说明 |
| --- | --- | --- | --- |
| `simpleWords` | `simple_words` | `string[]` | 「简单词」忽略列表，默认 42 个（`SIMPLE_WORDS`），与 Web 默认值相同 |
| `load` | `load` | `bool` | 上游遗留的装载标记，TUI 不使用 |
| `word` | `word` | `BookGroup` | 单词词典组：`bookList` + `studyIndex` |
| `article` | `article` | `BookGroup` | 文章词典组；TUI 不读取文章，只有 1 本系统「收藏」 |
| `dictListVersion` | `dict_list_version` | `u32` | 上游词库目录版本，TUI 默认 1、不参与逻辑 |
| `fsrsData` | `fsrs_data` | `map` | 单词 → FSRS 卡片，见 3.4 |
| `noteData` | `note_data` | `map` | 单词 → 笔记字符串；TUI 只读写不展示 |
| `_ignoreWatch` | `ignore_watch` | `bool` | 上游「忽略监听」标记，TUI 不使用 |

### 3.3 词典（`Dict`）字段

| JSON 字段 | 说明 |
| --- | --- |
| `id` | 词典身份；资源文件里的数字 id 会在读取时转成字符串（如 `1`） |
| `enName` | 上游路由名；读取时也接受 `en_name` 别名，两者都可作为 `--dict` 的取值 |
| `name` / `description` / `url` / `category` / `tags` / `language` / `translateLanguage` | 展示与资源定位元数据；`url` 是 `dicts/<language>/word/` 下的文件名 |
| `length` | 词数；有效词数还应参考 `words.len()`（`Dict::effective_length`） |
| `lastLearnIndex` / `perDayStudyNumber` / `complete` | 学习进度、每日目标、是否已完成；`select_dict` 切换词库时会保留既有值 |
| `words` | 词条数组，结构见 6.2；官方资源词库保存时被清空（见 2.6） |
| `articles` | 文章数组；TUI 原样透传、不读取 |
| `statistics` | 结算记录数组：`startDate`（毫秒）、`spend`（毫秒）、`total`、`new`、`review`、`wrong`，以及可选的 `title`、`segments`（`[起, 止]` 毫秒片段）、`sessionRole`（`single/start/middle/end`，跨天练习用） |
| `custom` / `system` | 是否自定义词典 / 是否系统虚拟词典；系统词典为收藏（`wordCollect`）、错词（`wordWrong`）、已掌握（`wordKnown`）与文章收藏（`articleCollect`） |
| `sourceId` | 官方资源副本的原始 id；TUI 默认为空字符串 |
| `cover` | 可选封面色，无实际渲染 |
| 其它未知字段 | 通过 `Dict` 的 `#[serde(flatten)] extra` 原样保留 |

字段命名规则：所有结构体默认用 `rename_all = "camelCase"`（`translateLanguage`、`lastLearnIndex` 等）；例外是 `_ignoreWatch` 与统计里的 `new` 显式改名，以及 `Card` 保持 ts-fsrs 的 snake_case（见 3.4）。**`Word` 没有 `extra` 透传字段**：资源里的 `inflections`、`e2e`、`examsSrc`、`langType` 等未建模字段读取时被忽略，写回 `dict.json` 的系统词典词条时也不会保留。

### 3.4 FSRS 卡片（`fsrsData`）

`Card` 对应 ts-fsrs 的序列化格式（`src/domain/types.rs` 的 `Card`）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `due` | RFC 3339 时间 | 下次到期时间（UTC） |
| `stability` / `difficulty` | `f64` | FSRS 记忆稳定度 / 难度 |
| `elapsed_days` / `scheduled_days` / `learning_steps` | `f64` | 距上次复习天数、计划间隔、学习步进 |
| `reps` / `lapses` | `u32` | 复习次数 / 遗忘次数 |
| `state` | `u8` | 0 新卡、1 学习中、2 复习中、3 重学中 |
| `last_review` | 可选 RFC 3339 时间 | 上次复习时间 |

评级常量与 Web 一致（`Rating = u8`）：Again 1、Hard 2、Good 3、Easy 4；评级不落在这张表里，而是先写进练习缓存的 `ratingMap`，结算时由 `PracticeSession::settle` 按 `src/domain/fsrs.rs`（自实现的 FSRS-6）计算新卡片后写入 `fsrsData`。

### 3.5 写入时机

`dict.json` 只在以下时机写盘（`App::save_dict_now`，`readonly_dict` 时跳过）：

| 触发 | 说明 |
| --- | --- |
| 结算 `finish_session` | `PracticeSession::settle` 写入 `statistics` 与 `fsrsData` 后立即保存 |
| 离开练习 | `Ctrl+C`（无结算时）、结算后「回主页」不写（已保存过） |
| `Ctrl+Enter`（重学本组） | 重建会话后立即保存练习缓存（新会话） |
| 切换词库、主页/词库页 `+`/`-`、设置页每日词数（`PerDay`） | 修改当前词典元数据后立即保存 |
| 确认退出 / 事件循环结束 | `save_all` 保存；首次运行即使没进练习，也会在退出时生成 `dict.json` |

练习过程中的收藏/错词/已掌握改动只改内存，靠上述时机落盘；中途崩溃最多丢掉这次会话的未保存改动（练习缓存另算）。FSRS 卡片与统计**只在结算时产生**，练习到一半退出不会写入评级。

### 3.6 只读保护（`readonly_dict` / `readonly_settings`）

`Bootstrapped::readonly_dict` 仅在 `dict.json` 命中 `VersionTooNew` 时为真：内存从 `BaseState::default()` 开始，所有 `save_dict_now` 静默跳过（不弹错误提示），保证旧版本程序绝不覆盖新版存档。`readonly_settings` 同理对应 `setting.json`，为真时 `save_settings_now` 直接返回，本次运行的设置修改与 `--mode`/`--theme` 覆盖都不会写回。二者都不影响练习缓存的保存/删除。

## 4. `setting.json`（`SettingState`，版本 23）

### 4.1 结构

`setting.json` 的 `val` 是上游 `SettingState` 的子集；TUI 只建模它需要读写的字段，其余字段通过 `#[serde(flatten)] extra` 原样保留。示例（截断）：

```json
{
  "val": {
    "soundType": "us", "wordReviewRatio": 3, "theme": "auto", "tuiBigWord": true,
    "fsrsParameters": { "request_retention": 0.9, "maximum_interval": 36500, "w": ["...21 个"] },
    "shortcutKeyMap": { "nextWord": "Control+ArrowRight" },
    "_ignoreWatch": false
  },
  "version": 23, "updated_at": "2026-10-18T13:34:19+00:00"
}
```

### 4.2 已建模字段

下表覆盖 `SettingState` 的全部字段（键名为 JSON 键，默认值为 `SettingState::default`）。设置项的业务含义与取值范围见 [设置项参考](settings.md)，这里只说明存档层面。

| JSON 键 | 默认值 | 说明 |
| --- | --- | --- |
| `soundType` | `"us"` | 音标取英式/美式 |
| `wordReviewRatio` | `3` | 复习比例 |
| `autoAddRandomReviewWhenNoDue` | `false` | 无到期复习时随机补足 |
| `wordPracticeMode` | `0` | 默认练习模式（枚举整型，见 `WordPracticeMode`） |
| `wordPracticeType` | `0` | 默认练习类型（枚举整型，`WordPracticeType`） |
| `ignoreCase` / `inputWrongClear` / `autoNextWord` | `true` / `false` / `false` | 键入行为 |
| `repeatCount` / `repeatCustomCount` | `1` / `null` | 重复次数；`repeatCount == 100` 时取自定义值（设置页「重复次数(自定义)」行可编辑） |
| `waitTimeForChangeWord` / `spaceCooldownTime` | `300` / `0` | 毫秒；读取时分别夹在 0–10000 |
| `ignoreSimpleWord` / `showWordQuestion` / `showEtymologyAndRelWords` | `false` / `true` / `false` | 显示与过滤 |
| `showNearWord` / `showUsageTips` / `alwaysShowNote` | `true` / `true` / `true` | 显示项 |
| `theme` | `"auto"` | 主题 id，见 [主题系统](themes.md) |
| `dictation` / `translate` / `ignoreSymbol` | `false` / `true` / `true` | 练习显示 |
| `fsrsEasyLimit` / `fsrsGoodLimit` / `fsrsHardLimit` | `0` / `3` / `6` | 评级阈值 |
| `fsrsParameters` | 见 4.4 | FSRS 参数（snake_case 键） |
| `tuiBigWord` | `true` | **TUI 独有**：练习页是否用 2 倍点阵大字 |
| `_ignoreWatch` | `false` | 上游「忽略监听」标记，TUI 不使用 |

### 4.3 透传字段与 `serde` 行为

- 上游 `SettingState` 有 58 个顶层字段，TUI 建模 43 个具名字段（40 个与上游同名 + TUI 专有的 `tuiBigWord`、`ttsSource`、`_ignoreWatch`）；其余 18 个上游顶层字段（`fontSize`、`showToolbar`、`first`、`webAppVersion`、`ttsVoiceMap` 等）在读取时进入 `extra: serde_json::Map<String, Value>`，保存时原样写回。测试 `tests/store_roundtrip.rs` 的 `settings_roundtrip_preserves_unknown_fields` 固定了这一行为：读入 `someFutureField` 后保存，输出 JSON 中仍然存在。
- 结构体级 `#[serde(default)]` 保证缺失字段取默认值；枚举字段用整型序列化（如 `wordPracticeMode`），未知数值会解析失败并触发隔离。
- `FsrsParameters` 的参数兜底在 `src/domain/fsrs.rs` 的 `Fsrs::new` 里统一做：`FsrsParameters::effective_w` 只负责权重长度（不是 21 个有限数就回退默认权重），留存率必须落在 `(0,1)`、最大间隔至少一天，否则回退默认值，因此手改坏参数不会让调度崩溃。

### 4.4 默认值与 FSRS 参数

默认 FSRS 参数（`domain/constants.rs` 的 `default_fsrs_parameters`，与上游 ts-fsrs 默认值一致）：`request_retention = 0.9`、`maximum_interval = 36500`、21 个权重 `0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542`、`enable_fuzz = false`、`enable_short_term = true`、学习步进 `["1m","10m"]`、重学步进 `["10m"]`。TUI 全部读取：`w`、`request_retention`、`maximum_interval`、`enable_fuzz`、`enable_short_term` 与两组步进都生效——FSRS-6 逐条移植自上游 ts-fsrs，黄金样本见 `tests/fsrs_parity.rs`。

### 4.5 如何安全新增一个设置项

1. **模型与默认值**：在 `src/domain/types.rs` 的 `SettingState` 加字段，并在 `impl Default` 里给值；新字段自然带 `#[serde(default)]` 语义。字段名用 Rust snake_case，JSON 由 `rename_all = "camelCase"` 生成。
2. **UI 入口**：布尔/枚举项在 `src/app/mod.rs` 的 `SETTING_ROWS` 加一行（key + `SettingKind`），数字项新增 `EditTarget` 并在 `App::setting_number_value` / `apply_number_value` 中读写与夹取范围，布尔项在 `toggle_setting_bool` 加分支。
3. **文案与显示**：`src/domain/i18n.rs` 的 `tr` 加中文文案；`src/ui/settings.rs` 的 `SECTIONS` 决定分区标题位置，`setting_value` / `setting_bool` / `number_text` 决定显示值与颜色。
4. **存档版本**：仅 TUI 自用的字段（如 `tuiBigWord`）**不要**提升 `SETTING_VERSION`——旧存档缺字段由 default 补，Web 端加载设置时只保留自身已知字段（不报错，但该字段在 Web 端会丢失）；只有跟随上游 schema 升级、字段语义变化或需要迁移时才提升版本，并同时确认读取端对旧版本的处理（当前是「直接接受 + 补默认」，没有迁移代码）。
5. **测试**：补 `tests/app_drive.rs` 的修改-重启-持久化用例；改 `Extra` 透传逻辑时跑 `tests/store_roundtrip.rs`；涉及界面的补 `tests/ui_snapshots.rs`。同步更新 [设置项参考](settings.md)。

## 5. `practice-word.json`（`PracticeCacheCompact`，版本 2）

### 5.1 结构与示例

`val` 为缓存对象；保存 `None` 时程序删除文件而不是写 `null`。词表以**字符串**保存（单词的 `word` 字段），恢复时再按当前词库还原成完整 `Word`。（示例截断）

```json
{
  "val": {
    "taskWordsStr": { "new": ["cancel", "explosive"], "review": [] },
    "practiceData": {
      "index": 1, "excludeWords": [], "allWrongWords": ["explosive"],
      "isTypingWrongWord": false, "wrongTimesMap": { "explosive": 1 }, "wrongTimes": 1,
      "ratingMap": { "explosive": 2 }, "wordsStr": ["cancel", "explosive", "..."],
      "wrongWordsStr": ["explosive"], "question": null
    },
    "statStoreData": {
      "stage": 0, "startDate": 1792300000000, "spend": 125000, "total": 20,
      "newWordNumber": 20, "reviewWordNumber": 0, "inputWordNumber": 5, "wrong": 1,
      "timerPaused": false, "timerPauseReason": null,
      "segments": [[1792300000000, 1792300125000]]
    },
    "sessionSnapshot": {
      "flowId": "system",
      "cursor": { "nodeIndex": 0, "stepIndex": 0, "inWrongWordClear": false, "loop": null, "endActionIndex": null },
      "nodeWorkingWordKeys": ["cancel", "explosive", "..."]
    }
  },
  "version": 2, "updated_at": "2026-10-18T13:34:19+00:00"
}
```

| 字段 | 说明 |
| --- | --- |
| `taskWordsStr.new` / `.review` | 本轮任务词表（新词 / 复习词）的单词字符串 |
| `practiceData.index` | 当前词下标；恢复时夹在 `0..words-1` |
| `practiceData.excludeWords` / `allWrongWords` / `isTypingWrongWord` | 已排除词、本轮错词、是否处于错词重输状态 |
| `practiceData.wrongTimesMap` / `ratingMap` | 单词 → 错误次数 / 单词 → FSRS 评级（1–4），结算时用于算卡片 |
| `practiceData.wordsStr` / `wrongWordsStr` | 当前节点词表与错词表（字符串） |
| `practiceData.question` | 自测题缓存；`cache_to_compact` 固定写 `null`，恢复后重新生成 |
| `statStoreData` | `PracticeState` 快照：阶段（`stage`）、计时、新词/复习/输入计数、错词、暂停状态与时间片段 |
| `sessionSnapshot.flowId` | 流程 id（`system/free/review/shuffle/identifyOnly/dictationOnly/listenOnly`） |
| `sessionSnapshot.cursor` | 流程游标：`nodeIndex`、`stepIndex`、`inWrongWordClear`、`loop`（`{startIndex,endIndex,subStepIndex}` 或 `null`）、`endActionIndex` |
| `sessionSnapshot.nodeWorkingWordKeys` | 当前节点的工作词表（单词字符串） |

### 5.2 写入与删除时机

| 触发 | 动作 |
| --- | --- |
| 开始一轮练习 `begin_session_with_task` | 立即保存（新缓存） |
| 单词推进、键入错误、自测操作、随机默写等改变进度 | 安排 500 ms 防抖保存（`pending_practice_save`），由 `on_tick` 到期落盘 |
| 本次 tick 确实累计了时长且跨过 30 秒整数倍 | 再安排一次防抖保存（暂停时不会重复调度） |
| `Ctrl+C` | 立即保存，随后离开练习 |
| `Ctrl+Enter`（重学本组） | 新会话建立后立即保存缓存 |
| 退出程序（事件循环结束/确认退出） | `save_all`：若仍有未结算会话则立即保存 |
| 结算 `finish_session` | 删除缓存（结算弹层出现后不再有断点） |
| 结算后回主页、设置页「清空当前练习进度」 | 删除缓存 |
| 设置页「清除所有数据」 | 删除缓存，并把 `dict.json` 与 `setting.json` 一起恢复默认（见第 9 节） |
| 启动参数 `--reset-practice` | 启动恢复前删除缓存并 Toast「已清空练习进度」 |

删除即 `fs::remove_file`；删除后 `practice_exists()` 为 false。文件写入失败会弹 Error Toast（`保存练习进度失败: ...`），不会中断练习。

### 5.3 恢复流程

启动时（未指定 `--reset-practice`）`App::try_restore_practice` 依次检查，任一步不满足就静默放弃恢复（**不删文件**，等下一次保存覆盖）：

1. `load_practice` 读到 `version == 2` 的值（否则走 5.4 的提示与忽略逻辑）；
2. 当前学习词典存在，且已有词表；
3. `cache_from_compact` 能用当前词典的 `words` 还原出任务词表——按单词字符串**精确匹配**，匹配不到的词被丢弃；任务词表全空则放弃；
4. 缓存同时包含 `practiceData`、`statStoreData`、`sessionSnapshot`；
5. `PracticeFlowCursor` 校验通过：`flowId` 必须是已知内置流程，节点/步骤下标存在，错词清空索引与循环区间合法（`Navigator::restore_snapshot`）。校验失败会回滚并放弃；
6. 恢复成功后进入练习页、重置键入并 Toast「已恢复上次练习进度」；计时若未暂停从当前时刻继续。

因此**切换词库后旧缓存通常无法恢复**（任务词不在新词库中）；缓存词表与 flow 都匹配时才能回到同一词、同一阶段。

### 5.4 版本不支持与损坏

- 只有 `version == 2` 被接受，其余（包括 Web 的 v1）一律视为不支持：`load_practice` 返回 `LoadStatus::UnsupportedVersion { found, expected: 2 }`，启动弹 Warning「存档版本不受支持（…），已忽略；下次保存会覆盖」并忽略缓存；文件**不会**被移动或删除，下一次保存练习缓存时被覆盖。TUI 不提供 v1→v2 升级（上游 Web 有，见 `checkAndUpgradePracticeWordCache`）。
- `val` 为 `null` 的信封被当作「没有缓存」（`Loaded(null)`），不算损坏。
- JSON 解析失败时文件被隔离为 `practice-word.json.corrupt-<时间戳>.json`，启动提示「练习缓存损坏，已丢弃」，随后按无缓存启动。

## 5bis. `flows.json`（`FlowStore`，无版本号）

自建练习流程的存档，对应上游浏览器 localStorage 的 `PracticeFlowV2` 键。结构与上游一致：

```json
{
  "activeId": "custom-1",
  "flows": {
    "custom-1": {
      "name": "我的流程",
      "config": { "id": "custom-1", "version": 6, "mode": 7, "label": "...", "nodes": [] },
      "createdAt": 1735689600000,
      "updatedAt": 1735689600000
    }
  }
}
```

- **没有版本号**：上游用 localStorage 存这份数据，本来就没有版本概念；`config.version` 是流程配置自身的版本（与上游 `CURRENT_FLOW_VERSION` 一致，当前 6）。
- 流程配置里的字段名与上游 `PracticeFlowConfig` 逐字一致（`templateId`、`wordAdvance`、`groupSize`、`subSteps`、`onEnd`、`shuffleOnEnter`），因此 `flows.json` 与上游的 localStorage 内容可以互换。
- `subSteps` 是 `{ templateId }` 对象数组（上游 `PracticeLoopSubStep`），不是裸的模板 id 数组。
- 损坏时文件被隔离为 `flows.json.corrupt-<时间戳>.json`，启动提示「流程存档已损坏，已隔离并恢复默认值」，随后按空集合启动。

## 6. 资源词库格式

### 6.1 `<assets>/list/word.json`（`DictResource`）

顶层是数组，元素为词库目录项（`src/domain/catalog.rs` 的 `DictResource`）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | 字符串或数字 | 词库身份；空 id 的项会被跳过 |
| `enName` | 字符串 | 上游路由名，可用作 `--dict` 取值；也接受 `en_name` 别名 |
| `name` / `description` | 字符串 | 展示名与简介 |
| `url` | 字符串 | 词条文件名，拼接为 `dicts/<language>/word/<url>` |
| `length` | 整数 | 词数；为 0 时加载后以实际词数为准 |
| `category` / `tags` / `language` / `translateLanguage` | 字符串 / 数组 | 展示与语言元数据；`language` 为空时按 `en` |
| `type` | 字符串，可选 | 只处理 `"word"` 或缺省的项，其它类型（如 article）被过滤 |
| `version` / `cover` / 其它 | — | 不参与 TUI 逻辑；未知字段被忽略 |

内置文件当前只有一本：`id = 1`、`enName = "cet4"`、`url = "CET4_T.json"`、`length = 2607`。

### 6.2 `<assets>/dicts/en/word/*.json`（`Word`）

顶层是词条数组。TUI 只读取下表字段（`src/domain/types.rs` 的 `Word`）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | 数字/字符串/缺省 | 上游词条 id；读取时一律转成字符串 |
| `word` | 字符串 | 单词本体；`word.trim()` 为空时该词条被过滤 |
| `phonetic0` / `phonetic1` | 字符串 | 英式 / 美式音标（`Word::phonetic(uk)` 中 `uk = true` 取 `phonetic0`） |
| `trans` | 数组 | 释义：`{ "pos": "v.", "cn": "取消" }`，可选 `frequency` |
| `sentences` / `phrases` | 数组 | 例句 / 短语：`{ "c": "英文", "cn": "中文" }` |
| `synos` | 数组 | 同义词：`{ "pos", "cn", "ws": ["..."] }` |
| `relWords` | 对象 | 相关词：`{ "root": "cancel", "rels": [{ "pos", "words": [{ "c", "cn" }] }] }` |
| `etymology` | 数组 | 词源：`{ "t": "标题", "d": "描述" }` |
| `custom` | 布尔 | 资源文件中通常缺省（`false`） |

未建模字段（如 `inflections`、`e2e`、`examsSrc`、`langType`）会被静默忽略。所有数组字段在缺省时按空数组处理；整个文件没有可用词条时加载失败：`词库文件没有可用单词: <path>`。

仓库内置 CET-4 资源为 `assets/dicts/en/word/CET4_T.json`：2607 词、4,487,640 字节（约 4.5 MB），首个词是 `cancel`（`tests/real_assets.rs` 固定了这两个事实）。`dicts/en/article` 下的文章数据不读取，TUI 也不使用 `list/article.json`。

### 6.3 目录合并与文件优先级

- `Catalog::load` 按 `[assets, home]` 顺序读取两个 `list/word.json`：先内置、后用户；**同一个 `id` 的用户词库目录项会替换内置项**（位置保留在首次出现处）。
- 单词文件查找按相对路径 `dicts/<language>/word/<url>` 先查 `<home>`、再查 `<assets>`。因此导入一个与内置词库**同文件名**的文件（如 `CET4_T.json`）会遮蔽内置文件，即使两者 id 不同。

## 7. `import-dict` 行为

`tw import-dict FILE` 只接受**词条数组 JSON**（与 `dicts/en/word/*.json` 同格式），不支持词库目录对象、ZIP、Excel。执行步骤：

1. 读取并解析 `<FILE>` 为 `Vec<Word>`，用 `Word::is_usable` 过滤空词条；至少要有 1 个可用词；
2. 文件名必须以 `.json` 结尾；
3. 加载词库目录（需要可用的 `assets/`）；
4. 把源文件**原样复制**到 `<home>/dicts/en/word/<文件名>`（不改写内容）；
5. 生成目录项：`id = enName = name = 文件名去掉 .json`、`description = "导入词库（N 词）"`、`url = 文件名`、`length = N`、`category = "自定义"`、`tags = ["导入"]`、`language = "en"`、`translateLanguage = "zh_CN"`、`type = "word"`、`version = 1`；
6. 写入 `<home>/list/word.json`（pretty JSON + 原子写；按解析后的 id 判重：同 id 替换旧目录项，否则追加），打印「已导入词库: <name>（N 词）」并提示 `tw --dict <id>` 切换；**不会自动切换**当前词库。

行为要点与冲突规则：

- **判重规则**：`Catalog::import_dict` 把每个已有目录项反序列化为 `DictResource` 后比较归一化 id（`Value::String` 与 `Value::Number` 都转成字符串），因此字符串 id 也能正确匹配；重复导入同一文件名只替换原目录项，不会累积重复条目。
- 导入后立即出现在词库页（`d`）和 `tw --dict` 的可选值中；切换时 `Bootstrapped::select_dict` 会加载词表，并按 id/enName 找到旧书保留 `lastLearnIndex`/`perDayStudyNumber`/`complete`/`statistics`——重复导入同一文件名不会丢进度。
- 命令**不是事务性的**：先复制词条文件、后写用户目录。若目录写入失败，词条文件已存在于 `<home>/dicts/en/word/` 但未注册，可直接重跑命令修复。
- 用户 `list/word.json` 损坏会让词库目录加载失败，TUI/`tw task`/`import-dict` 启动即报错（`解析词库目录失败: <path>`），需要手工修复或删除该文件（删除后只剩内置词库）。

常见错误：

| 情形 | 错误文本 |
| --- | --- |
| 文件读不出 | `读取词库失败: <path>` |
| 不是词条数组 JSON | `解析词库失败（需要词条数组 JSON）: <path>` |
| 没有可用词条 | `词库中没有可用单词: <path>` |
| 文件名不是 `.json` | `词库文件需要是 .json 结尾: <文件名>` |
| 复制失败 | `复制词库失败: <源> → <目标>` |
| 没有资源目录/目录为空 | `找不到词库资源目录…` / `词库目录为空，请检查 …` |

## 8. 与 Web 端数据互通

TUI **不读写浏览器 IndexedDB**，也不实现 Web 的备份导入导出；互通方式是手工搬运 JSON。好在两边使用同名字段与同构信封：

| TUI 文件 | Web 端存储 | key | 版本 |
| --- | --- | --- | --- |
| `dict.json` | IndexedDB 中的 JSON 字符串 | `typing-word-dict` | 4 |
| `setting.json` | IndexedDB 中的 JSON 字符串 | `typing-word-setting` | 23 |
| `practice-word.json` | IndexedDB（早期为 localStorage，自动迁移） | `PracticeSaveWord` | 2 |

Web 的导出 ZIP 中 `data.json` 的结构是 `{"version": 5, "val": { "dict": {...}, "setting": {...}, "PracticeSaveWord": {...}, "PracticeSaveArticle": {...} }}`，其中每个子对象本身就是 `{version, val, updated_at?}` 信封——与 TUI 的单文件信封同构。手工迁移步骤：

1. 在 Web 端导出备份 ZIP，解压取出 `data.json`；或从浏览器开发者工具读取 IndexedDB 的 `typing-word-dict` / `typing-word-setting`（值是 JSON 字符串）。
2. 关闭 TUI，备份 `<home>`。
3. 写入文件：`dict.json` = `data.val.dict`；`setting.json` = `data.val.setting`；如需恢复断点，`practice-word.json` = `data.val.PracticeSaveWord`（`updated_at` 可缺省）。
4. 启动 TUI，用 `tw doctor` 核对路径、词库数量与存档文件大小，再进设置页确认主题等字段。
5. Web 备份里当前学习词典若在 TUI 资源中不存在，启动会回退到内置 CET-4（`ensure_study_dict` 的兜底顺序：原 id → `cet4` → 第一本词库）。

迁移风险：

| 风险 | 说明 |
| --- | --- |
| Web 版本更新 | setting v>23 与 dict v>4 都会触发只读保护（本次不写回）；练习缓存 v!=2 会被忽略（见 2.5）。迁移前确认 TUI 支持的版本 |
| TUI 独有字段 | `tuiBigWord` 只存在于 TUI 的 `SettingState`；Web 端加载设置时只保留它自己认识的字段（`checkAndUpgradeSaveSetting` 的 `checkRiskKey`），因此经 Web 往返后该字段会丢失、回到默认值 |
| 未建模的词条字段 | 写回系统词典（收藏/错词/已掌握）时，`Word` 未建模的字段（如 `inflections`）会被丢弃；词典/设置层级的未知字段则通过 `extra` 保留 |
| Web 自定义词库 | Web 备份中 `custom: true` 的词库保留词表，TUI 能作为当前学习词典读取；但词库页只列出资源目录，切走后就无法再切回 |
| FSRS 间隔 | 卡片字段（`due/stability/difficulty/...`）两边兼容；TUI 自实现的 FSRS-6 已逐条对齐上游 ts-fsrs（黄金样本见 `tests/fsrs_parity.rs`），同一张卡在两端得到相同的下次复习时间 |
| 练习缓存 | TUI 恢复还要求 `sessionSnapshot`；Web 旧缓存若缺这个字段则只迁移词表与统计、无法恢复断点 |
| 文章与音频 | `PracticeSaveArticle`、`dicts/en/article`、备份 ZIP 里的 `mp3/` 在 TUI 中都没有对应实现，迁移时直接丢弃即可 |

## 9. 备份、恢复与重置

- **完整备份**：关闭程序后复制整个 `<home>`（含 `dict.json`、`setting.json`、`practice-word.json`、`themes/`、导入的 `dicts/` 与 `list/word.json`）。只拷 `dict.json` 会丢设置、断点与导入词库。
- **恢复**：关闭程序 → 用备份覆盖 `<home>` → 启动；建议先跑 `tw doctor` 确认路径与词库数量，再打开设置页检查主题是否加载。
- **只重置练习**：`tw --reset-practice`（启动时删除缓存）或设置页「清空当前练习进度」（确认弹层后删除）。两者都只删 `practice-word.json`，不影响 `dict.json` 里的统计、FSRS 卡片和收藏/错词/已掌握。
- **重置学习记录（恢复出厂）**：设置页「其他 → 清除所有数据」，确认后 `boot.base = BaseState::default()`、`boot.settings = SettingState::default()`、删除 `practice-word.json`，随即落盘两份存档；清完按全新安装重新选一本学习词典。等价于手工删掉 `dict.json` + `setting.json` + `practice-word.json`，但不用退出程序。与上游 Web 的「设置 → 数据 → 清除所有数据」（`dataSyncPersistence.clear()`）行为一致。任一存档版本过高（本次运行只读）时该动作会拒绝执行并提示，避免假成功。
- **彻底重置**：关闭程序后删除整个 `<home>`（内置词库在 `assets/`，不会丢）；下次启动会重建目录并按默认值运行。
- **删除/隔离坏档**：启动 Toast 会给出隔离后的文件名（`*.corrupt-<时间戳>.json`）；确认不需要后可删除隔离文件与坏档本身。版本过高时**不要**为了启动而降级程序或手改 `version`，优先备份后升级程序。
- **版本降级风险**：`dict.json` 版本过高 → 本次运行只读、进度不落盘（数据仍安全）；`setting.json` 版本过高 → 同样只读、本次设置不写回；`practice-word.json` 版本不符 → 忽略并在下次练习时覆盖。三类都不会破坏原文件，但下次以新版本保存后会升级文件格式，降级前应先备份。

`tw doctor` 是排障入口：它打印版本行、数据/资源目录、词库清单（含每个词库的词条文件路径）、**存档文件大小**（不存在则标注）、自定义主题目录与全部主题、主题加载告警。它不解析存档内容，所以版本过高/损坏状态只会在启动 Toast 中出现：

```text
存档：
  - dict.json: 48213 bytes
  - setting.json: 不存在
  - practice-word.json: 1204 bytes
```

## 10. 安全与隐私

- **纯本地**：依赖树中没有 HTTP/WebSocket 客户端（见 `Cargo.toml`），程序不发起网络请求；无账号、无云同步、无遥测。上游 Web 端的 umami 统计代码没有被移植。
- **明文存储**：存档是可读 JSON，没有加密；笔记、进度、收藏等对同机有文件读权限的用户可见，取决于操作系统权限。
- **手工编辑注意**：先关闭程序并备份；保持 `{ "val": ..., "version": ... }` 信封、UTF-8 编码、合法 JSON（不能有注释/尾逗号）；改坏会导致下次启动解析失败并隔离为 `*.corrupt-*.json`（数据还在隔离文件里）；运行中编辑会被退出时的内存状态覆盖。
- **并发写入**：没有文件锁；同一 `<home>` 不要同时运行多个实例。把 `<home>` 放进 OneDrive 等同步目录时，注意多机同时运行没有冲突检测，最后写入者获胜。

## 相关文档

- [文档索引](README.md)
- [设置项参考](settings.md)
- [主题系统](themes.md)
- [应用状态机](app.md)（保存函数与触发时机）
- [领域算法](domain.md)（FSRS 卡片与统计的计算）
