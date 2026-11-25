# 存档与数据格式

本文是 typewords-tui 本地数据格式的权威说明：数据目录与资源目录如何解析，5 份存档的结构、写入协议与版本策略，各文件的字段与不变量，资源词库 / 文章格式，`import-dict`，与网页版的数据互通，以及备份 / 恢复 / 重置。读者是要排查存档问题或改动持久化代码的维护者；用户视角的操作步骤见 [使用手册](usage.md)。

## 1. 数据目录与路径解析

程序区分两套目录：可写的**数据目录**（home，存全部用户数据）与只读的**资源目录**（assets，随包分发的词库）。两者都由 `src/paths.rs` 的 `Paths::resolve` 解析。

### 1.1 目录布局

| 路径 | 读写 | 内容 |
| --- | --- | --- |
| `<home>/dict.json` | 读写 | 词典状态 `BaseState`，版本 4 |
| `<home>/setting.json` | 读写 | 设置 `SettingState`，版本 23 |
| `<home>/practice-word.json` | 读写、可删 | 单词练习断点 `PracticeCacheCompact`，版本 2 |
| `<home>/practice-article.json` | 读写、可删 | 文章跟打断点 `ArticleCacheEnvelope`，版本 1 |
| `<home>/flows.json` | 读写 | 自建练习流程 `FlowStore`，无版本号 |
| `<home>/themes/*.json` | 启动时只读 | 自定义主题（格式见 [主题系统](themes.md)） |
| `<home>/list/word.json` | 导入 / 新建词典时写，启动时读 | 用户词库目录，与内置目录合并 |
| `<home>/dicts/<language>/word/<url>` | 导入 / 编辑词条时写，运行时读 | 用户词库词条 |
| `<home>/.tw.lock` | 写入口 | 数据目录锁（`src/fs.rs` 的 `LOCK_FILE_NAME`），内容是持锁 PID，常驻不删 |
| `<home>/.<文件名>.tw-<pid>-<n>.tmp` | 临时 | 原子写中间文件；正常结束后不存在，崩溃可能残留，可安全删除 |
| `<home>/<文件名>.corrupt-<时间戳>.json`、`<home>/<文件名>.bak-<时间戳>.json` | 隔离 / 导入产物 | 解析失败被移走的坏存档；`tw import-web` 覆盖前留下的旧内容 |
| `<home>/logs/tw-<日期>.log` | 追加写、自动清理 | 运行日志（默认开启，`--log` / `--no-log` 覆盖），保留 14 天，见 1.5 |
| `<home>/logs/crash-<时间戳>.txt` | panic 时写、自动清理 | 崩溃报告，保留 30 天；`--no-log` 下仍生成，见 1.5 |
| `<assets>/list/*.json`、`<assets>/dicts/en/{word,article}/*.json` | 只读 | 内置词库目录与词条、文章书目与正文 |

`<home>` 下的子目录不预建：`dicts/`、`list/` 在导入或新建词典时按需创建，`themes/` 由用户自建，`logs/` 在第一次写运行日志时创建（`--no-log` 下只有 panic 写报告时才创建）。日志与崩溃报告的落点、保留与备份关系见 1.5。

### 1.2 数据目录解析顺序

1. `--home DIR`；
2. 环境变量 `TYPEWORDS_HOME`（空值视为未设置）；
3. `dirs::data_local_dir()/typewords-tui`——Windows 通常 `%LOCALAPPDATA%\typewords-tui`，Linux 为 `$XDG_DATA_HOME/typewords-tui`（缺省 `~/.local/share/…`），macOS 为 `~/Library/Application Support/typewords-tui`；
4. 都不可用则启动失败：`无法确定数据目录，请使用 --home 指定`。

目录由 `create_dir_all` 创建（只读入口经 `store::FileStore` 的构造，写入口经 `Paths::lock_home` 的 `ensure_home`），失败时启动报错并以「创建数据目录失败」开头、带上路径与原因。只读入口只创建目录，不写文件、不创建锁文件。

### 1.3 数据目录锁：写入口与只读入口

写入口启动时对 `<home>/.tw.lock` 取独占锁：`Paths::lock_home` → `src/fs.rs` 的 `try_lock_dir` 返回 `DirLock` 守卫，`lib::run` 让守卫活到进程退出。取锁的入口是 TUI、`tw task`、`tw import-dict`、`tw import-web`；`tw doctor`、`tw export-dict` 与 `tw export-web` 在 `lib::is_read_only` 白名单里，不取锁，可在别的实例运行时排障与导出。

- 用非阻塞 `try_lock`：抢不到锁立刻报错退出（终端不会被弄坏），退出码非 0，文案为 `另一个实例正在使用该数据目录（<home>）：请先退出那个 tw 进程，或用 --home 换一个数据目录`。
- 锁随进程退出或崩溃自动释放（句柄关闭即释放），锁文件**不删除**——删一个正被持锁的文件会制造「两把锁」竞态。文件内容只是 PID，判断是否被占用永远以操作系统锁为准；跨进程用例在 `tests/data_dir_lock.rs`。
- 锁文件**打不开**（数据目录只读、属主是 root / 另一个账号、被 ACL 拒绝）时启动直接报错，文案给出可操作动作：「若曾用管理员 / sudo 运行过 tw，删除该文件或修正属主后重试；数据目录不可写时用 --home 指定可写目录」。此时 5 份存档其实都还可读，处理掉锁文件即可正常启动。
- 日志与崩溃报告的写入**不取锁**：`logging::init` 在取锁之前执行，所以只读入口（以及抢锁失败后立刻退出的第二个实例）也会留下日志行。

### 1.4 资源目录解析顺序

`locate_assets` 取**第一个包含 `list/word.json` 的目录**（`is_assets_root`），依次尝试：

1. `--assets DIR` 与环境变量 `TYPEWORDS_ASSETS`（不合格不报错，继续尝试后续候选）；
2. 可执行文件同级的 `assets/`；
3. 当前目录及最多 3 级祖先的 `assets/`、`public/`（每个祖先先试 `assets/` 再试 `public/`，供开发期或从上游仓库内运行）；
4. 全部失败则启动失败：`找不到词库资源目录（需要包含 list/word.json）…`。

### 1.5 运行日志与崩溃报告

日志与崩溃报告写在同一个目录，默认 `<home>/logs/`。它们**不是存档**：没有信封、没有版本号，不参与版本校验、备份迁移与 Web 互通，删掉不影响任何学习进度。

| 项 | 规则 |
| --- | --- |
| 创建时机 | `lib::run` 解析出数据目录后立即 `logging::init`（所有入口都走这条，包括只读的 `tw doctor` / `tw export-dict` / `tw export-web`）：创建目录并追加写；写失败只在 stderr 警告一次，程序继续运行 |
| 落点 | 默认 `<home>/logs/tw-<启动当天本地日期>.log`；`--log FILE` 改为该文件（目录不存在会创建，相对路径视为当前目录），此时默认目录不再创建。崩溃报告跟着日志走：用 `--log` 就是它所在的目录，否则 `<home>/logs/` |
| 关闭 | `--no-log` 不写日志、不建日志目录、也不做启动清理；panic 时仍写 `<home>/logs/crash-<时间戳>.txt` |
| 保留 | 启动时清理日志所在目录：日志保留 14 天、崩溃报告 30 天（按修改时间，只匹配 `tw-*.log` 与 `crash-*.txt`，其它文件不动）；panic 写报告前再清一次过期报告 |
| 崩溃报告 | panic 时写 `crash-<YYYYMMDD-HHMMSS>.txt`（同一秒冲突用 `create_new` 原子占名并追加 `-2`、`-3`，并发进程也不覆盖），内容是数据目录 / 资源目录 / 日志文件等路径、panic 消息与位置、全量回溯、最近 40 行日志；写失败只提示一次，不递归。连数据目录都还没解析出来、或报告目录不可创建时，降级到系统临时目录的 `typewords-tui-crash/` |
| 与备份的关系 | 整目录备份时 `logs/` 可留可删；恢复到新机器不需要它，也不会影响存档读取 |

## 2. 存档信封与写入协议

### 2.1 信封与 5 份存档

`src/store/mod.rs` 的 `SaveEnvelope<T>` 形式为 `{ "val": <状态>, "version": <u32>, "updated_at": "<RFC 3339>"? }`：`val` 是状态对象（两份断点缓存允许 `null`，等价「没有缓存」），`version` 是 schema 版本（读取时必须存在），`updated_at` 可选、读取端只保存不使用（`tw export-web` 拿它当文章统计的 `startDate`）。

| 文件 | 版本 | 版本常量（`src/domain/constants.rs`） | 对应网页版 key |
| --- | --- | --- | --- |
| `dict.json` | 4 | `DICT_STATE_VERSION` | `typing-word-dict` |
| `setting.json` | 23 | `SETTING_VERSION` | `typing-word-setting` |
| `practice-word.json` | 2 | `PRACTICE_CACHE_VERSION` | `PracticeSaveWord` |
| `practice-article.json` | 1 | `PRACTICE_ARTICLE_CACHE_VERSION` | `PracticeSaveArticle` |
| `flows.json` | 无 | — | `localStorage['PracticeFlowV2']` |

除 `flows.json` 是裸结构外，其余 4 份都套信封。写入用 `serde_json::to_string` 的紧凑单行格式（`flows.json` 例外，用 `to_string_pretty`）；手写或格式化编辑不影响读取。读取时机：`dict.json` / `setting.json` / `flows.json` 在启动装载时各读一次；两份断点缓存在启动时也各读一次（`refresh_practice_resume` / `refresh_article_resume`，用于提示读不出来的缓存、决定主页是否显示「上次任务」与文章列表是否标「上次第 N 句」），之后按需再读（首页 `Enter` 尝试恢复、进文章页刷新标记）。

### 2.2 原子写

`src/fs.rs` 的 `write_atomic` 是 `domain` 与 `store` 共用的唯一写原语：先确保父目录存在，再在同目录以 `create_new` 建**本次独占**的临时文件 `.<文件名>.tw-<pid>-<n>.tmp`（`<n>` 为进程内单调计数，撞名就换下一个序号、最多 16 次），`write_all` 后 `sync_all`（fsync 失败只表示不保证落盘，不阻断保存），最后 `rename` 覆盖正式文件——同目录保证 rename 在同一文件系统内完成；Unix 上 `rename` 之后还会 `fsync` 父目录（`sync_parent_dir`），让「新名字已经生效」这件事本身也落盘——只同步临时文件时，断电后可能出现「旧文件还在、新文件没了」：内容仍是完整的（不会半截），但本次保存整份丢失。Windows 没有目录 fsync 的语义，这一步按平台编译掉；两次 fsync 失败都只表示「不保证已落盘」，不阻断保存。写入或 rename 失败时删除刚建的临时文件，正式文件保持原内容。

Windows 的 rename 是 `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`：目标带只读属性、或正被别的进程以不共享方式打开时都会返回「拒绝访问」，而 Unix 的 `rename(2)` 只看目录写权限。因此 Windows 上覆盖前先清掉目标的只读属性（否则给 `dict.json` 加只读就等于每次保存都失败，进度只留在内存里），并对瞬时占用按 10 / 20 / 40 ms 退避重试 3 次（总计约 70 ms，不拖累帧率）；仍失败时错误文案点出「可能被同步盘或杀毒软件占用，请稍后重试」。注意这会把用户手工加在存档上的只读属性去掉——想防误删请用备份，而不是只读属性。保存**最终失败**时会把清掉的属性恢复回去：旧存档内容没变，属性也不该变（占用导致的失败是个例外——恢复同样要打开那个文件，会被同一把占用挡住，此时尽力而为）。Unix 不受这两者影响，保持一次调用。

`write_atomic_bytes` 是同一路径的字节版本，`copy_atomic` 在它之上做「整份读出 + 原子写入」，供备份与回滚这类「要么完整旧内容、要么原样不动」的场景使用（`std::fs::copy` 会先截断目标，中断就留半截文件）。`copy_atomic` 会把源文件权限复制给目标（Unix 上保住用户手工收紧的 0600），但 Windows 上不复制只读位——那里的 `Permissions` 只有只读属性，复制它会让「存档 → `.bak` → 回滚」互相传染只读位，下一次写入直接失败。

因此进程中途被杀不会留下半截 JSON：正式文件要么旧内容、要么新内容。残留的 `.tmp` 不属于任何后续写入，不会被复用或删除，可直接清理；临时名唯一化后，同进程内并发写（存档与词库目录同时落盘）也不会互相截断。原子性只覆盖**单份文件**：`tw doctor` / `tw export-dict` / `tw export-web` 不取锁，读到的每份文件各自完整，但多份文件可能来自不同时刻，需要一致快照就先退出所有实例。

### 2.3 读取状态 `LoadStatus`

`FileStore::read_raw` 先只解析信封的版本头（`version` / `updated_at`），版本判定通过后 `decode_val` 才反序列化 `val`。约定：文件不存在或只有空白 → 视为缺失（不隔离、不报错）；**读不到**（权限不足、被同步盘 / 杀毒软件独占、路径是目录等 IO 错误）→ 不隔离、不改写、不算损坏；**读到了但版本头或载荷解析失败** → 隔离（这是隔离的唯一触发条件）；版本过高 / 缓存版本不符 → 直接归类，不解析 `val`、不移动文件。`load_typed` / `load_cache` 把结果归类为：

| 状态 | 触发条件 | 内存值 | 提示（`Loaded::notice`） |
| --- | --- | --- | --- |
| `Loaded` / `Missing` | 解析成功且 `version <=` 期望值；或文件不存在、为空 | 文件内容 / 类型的 `Default` | 无 |
| `Corrupt(reason)` | 版本头或载荷解析失败，文件已隔离 | 默认值（缓存为 `None`） | `存档已损坏，已隔离并恢复默认值：<reason>` |
| `Unreadable(reason)` | IO 读失败（`PermissionDenied`、Windows 共享冲突、`EISDIR` 等） | 默认值（缓存为 `None`），**不写回** | `存档读不出来：<reason>；可能被同步盘 / 杀毒软件占用，本次运行不会改写它` |
| `VersionTooNew{found,expected}` | `version >` 期望值 | 默认值，原文件保留 | `存档版本过高（…），本次不会覆盖，请升级后使用` |
| `UnsupportedVersion{found,expected}` | 缓存版本不符（单词缓存 `!= 2`、文章缓存 `!= 1`） | `None`，原文件保留 | `存档版本不受支持（…），已忽略；下次保存会覆盖` |

读不到与内容损坏的处理完全不同：

- `dict.json` / `setting.json` 为 `Unreadable` 时**启动直接失败**（进入终端前 `bail!`），文案含路径、原因与「关闭占用后重试」，不会以默认值继续运行、退出时再覆盖原档。
- `practice-word.json` / `practice-article.json` / `flows.json` 为 `Unreadable` 时**不隔离、不删除、当次不恢复**，本次运行内对这几份文件只读：保存 / 删除入口跳过并提示一次，退出时也不写默认值；`has_saved_practice` 与 `has_saved_article_practice` 都返回 false，不会谎称有可恢复的断点。
- 读取容忍开头的 UTF-8 BOM：5 份存档（含 `flows.json` 的独立读路径）、`tw export-web` 的读入与两个导入入口（`tw import-dict` 的词条文件、`tw import-web` 的备份）、主题文件（`<home>/themes/*.json`）与文章资源（`assets/list/article.json`、`assets/dicts/en/article/*.json`）都会在 JSON 解析前剥离开头的一个 U+FEFF；**写回一律不带 BOM**。上游网页版的 `JSON.parse` 不接受 BOM，这里接受是有意的宽容（Windows 记事本等工具默认带 BOM），不是与上游不一致的缺陷。

`bootstrap::load` 把 dict / setting 的损坏、版本过高等提示交给 `App::bootstrap` 转成 Warning Toast（`Unreadable` 是启动错误，不走 Toast）；两份断点缓存的提示都在启动时弹（单词缓存 `refresh_practice_resume`、文章缓存 `refresh_article_resume`，后者进文章页时还会再读一次；`--reset-practice` 只探测可读性、不读内容，损坏的缓存直接删，因此没有「已丢弃」提示），单词缓存若在恢复流程里才读出问题（同一次运行内文件被外部改坏）也会补弹一次；流程存档损坏有独立文案「流程存档已损坏，已隔离并恢复默认值」。读不出来的缓存 / 流程按文件各弹一次「本次运行不会写入」提示。Toast 只显示约 3.2 秒，长期排障看 `tw doctor`。

### 2.4 损坏隔离

解析失败时 `FileStore::quarantine` 把坏文件**重命名**为同目录 `<文件名>.corrupt-<YYYYMMDD-HHMMSS>.json`（本地时间；同一秒重名追加 `-2`、`-3`……，不覆盖既有隔离副本），并在 `reason` 里记 `<错误>（已移动到 <隔离路径>）`，隔离失败则记 `（隔离失败: <原因>）`。原文件名随即变为缺失，程序用默认值继续运行，下次保存生成新档。隔离文件不自动清理；两份断点缓存损坏时丢弃缓存、不阻断启动。IO 读错误（`Unreadable`）**不进入**这条流程。

### 2.5 版本策略

没有迁移框架：**低于等于**期望版本直接按当前类型反序列化，缺失字段由各结构体的 `#[serde(default)]` 补默认值；**高于**期望版本才算 `VersionTooNew`。`flows.json` 没有版本概念，解析成功即接受。

版本判定**先于**载荷反序列化：只解析版本头，`version` 过高就直接返回 `VersionTooNew`，不会去解析 `val`。因此高版本存档即使字段结构已被新版本改掉（当前程序读不懂），也仍然原文件保留 + 只读，而不会被误判成「损坏」隔离、再被默认值覆盖。断点缓存同理：版本不符时忽略且不移动文件。

| 文件 | 期望 | `version <` 期望 | `==` | `>` 期望 |
| --- | --- | --- | --- | --- |
| `dict.json` | 4 | 接受，缺字段补默认 | 接受 | 默认值 + 只读（`Bootstrapped::readonly_dict`） |
| `setting.json` | 23 | 接受，缺字段补默认 | 接受 | 默认值 + 只读（`readonly_settings`） |
| `practice-word.json` | 2 | 不支持 | 接受 | 不支持 |
| `practice-article.json` | 1 | 不支持 | 接受 | 不支持 |

只读的后果：`readonly_dict` 为真时 `App::save_dict_now` 直接返回，本次运行的词库进度、FSRS 卡片与统计都不落盘（原 `dict.json` 完好）；`readonly_settings` 同理拦住 `save_settings_now`，设置修改与 `--mode` / `--theme` 覆盖都不写回。两种只读都**不影响**断点缓存的保存与删除。降级运行同样受保护，升级前仍建议备份数据目录。

读不出来（IO 错误）与版本过高不是一回事：前者在 `bootstrap::load` 直接报错退出，不进入只读模式、也不隔离或写回默认值（见 2.3）。

### 2.6 落盘前的 `shake_common_dict`

`FileStore::save_dict` 先克隆状态并调 `BaseState::shake_common_dict`（`src/domain/types.rs`）：`word.bookList` 中 `custom == false && system == false` 的词典清空 `words`，`article.bookList` 中 `system == false` 的词典清空 `articles`。

结果是 `dict.json` 只存官方词库的进度 / 统计元数据 + 系统词典与自定义词典的词条；约 4.5 MB 的官方词表按 `url` 从资源目录重新加载。清空发生在克隆体上，内存词表不受影响。`custom` 标记由目录项（`"custom": true`）经 `DictResource::to_dict` 带进 `Dict`，标记丢了自建词典的词表会在下次保存时被清空。

## 3. `dict.json`（`BaseState`，版本 4）

### 3.1 顶层字段

| JSON 字段 | Rust 字段 | 类型 | 说明 |
| --- | --- | --- | --- |
| `simpleWords` | `simple_words` | `string[]` | 「简单词」忽略列表，默认 42 个（`SIMPLE_WORDS`） |
| `word` | `word` | `BookGroup` | 单词词典组：`bookList` + `studyIndex` |
| `article` | `article` | `BookGroup` | 文章词典组；TUI 只存元数据，正文来自资源目录 |
| `fsrsData` | `fsrs_data` | `map` | 单词 → FSRS 卡片（3.3） |
| `noteData` | `note_data` | `map` | 单词 → 笔记字符串；TUI 读写但不展示 |
| `dictListVersion` | `dict_list_version` | `u32` | 上游词库目录版本，默认 1、不参与逻辑 |
| `load` / `_ignoreWatch` | `load` / `ignore_watch` | `bool` | 上游遗留标记，TUI 不使用 |

`studyIndex` 是 `word.bookList` 的下标，`-1` 表示没有选中词典（新装默认），由词库页 `Enter` 与 `tw --dict` 写入。默认存档的三本系统词典（收藏 / 错词 / 已掌握）在 0 / 1 / 2，但**不要假设下标**：Web 存档的 `bookList` 顺序不保证相同，代码按 `system` 标记遍历（`BaseState::system_dict_index`）。词表为空的内置词典也是合法状态，启动时不会自动改指别的词典（`Bootstrapped::ensure_study_dict`）。

### 3.2 词典 `Dict` 字段

| JSON 字段 | 说明 |
| --- | --- |
| `id` | 词典身份；资源里的数字 id 读取时转成字符串 |
| `enName` | 上游路由名，读取时也接受 `en_name` 别名；`id` 与 `enName` 都可作 `--dict` 取值 |
| `name`/`description`/`url`/`category`/`tags`/`language`/`translateLanguage` | 展示与资源定位元数据；`url` 是 `dicts/<language>/word/` 下的文件名 |
| `length` | 词数缓存；有词表时一律等于 `words.len()`（`Dict::effective_length`）。装载时 `Dict::normalize_progress` 把 `length` 校准成实际词数，并把越过末尾的 `lastLearnIndex` 夹回、置 `complete = true` |
| `lastLearnIndex`/`perDayStudyNumber`/`complete` | 学习进度、每日词数（即每日目标）、是否已完成 |
| `words`（词条数组，8.2）、`articles`（文章数组，TUI 原样透传、不读取） | 词条在官方资源词库保存时被清空（2.6） |
| `statistics` | 结算记录数组：`startDate`（毫秒）、`spend`（毫秒）、`total`、`new`、`review`、`wrong`，可选 `title`、`segments`（`[起, 止]` 毫秒片段）、`sessionRole` |
| `custom`/`system` | 是否自定义词典 / 是否系统虚拟词典；系统词典为收藏 `wordCollect`、错词 `wordWrong`、已掌握 `wordKnown` 与文章收藏 `articleCollect` |
| `sourceId`、`cover` 与其它未知字段 | 资源副本原始 id（默认空串）、可选封面色（不渲染）；未知字段经 `#[serde(flatten)] extra` 原样保留 |

`startDate` 与 `segments` 里的时间戳按本地自然日归并。超范围时间戳（`chrono` 可表示范围约 ±262000 年之外，只可能来自手改存档）在聚合（`daily_totals` / `daily_totals_for_dict`）与写入（`flush_stat_to_store`）两侧**统一跳过**：既不记到「今天」，也不留一条永远显示不出来的记录；跨天片段的 `sessionRole` 仍按实际拆出的自然日给 start / middle / end。

命名规则：结构体默认 `rename_all = "camelCase"`；例外是 `_ignoreWatch`、统计的 `new`（显式 rename）与 `Card`（保持 ts-fsrs 的 snake_case）。**`Word` 没有 `extra` 透传**：资源里的 `inflections`、`e2e`、`examsSrc`、`langType` 等未建模字段读取时被忽略，写回系统词典词条时也不保留。

### 3.3 FSRS 卡片（`fsrsData`）

`src/domain/types.rs` 的 `Card` 与 ts-fsrs 序列化格式一致：`due`（RFC 3339 到期时间）、`stability` / `difficulty`（记忆稳定度 / 难度）、`elapsed_days` / `scheduled_days` / `learning_steps`（距上次复习天数 / 计划间隔 / 学习步进）、`reps` / `lapses`（复习 / 遗忘次数）、`state`（0 新卡、1 学习中、2 复习中、3 重学中）、`last_review`（可选）。

评级不落在这里（`Rating = u8`：Again 1、Hard 2、Good 3、Easy 4）：先写进练习缓存的 `ratingMap`，结算时由 `PracticeSession::settle` 按 `src/domain/fsrs.rs`（自实现 FSRS-6）算出新卡片再写入 `fsrsData`。

### 3.4 写入时机

`dict.json` 只在 `App::save_dict_now` 里写（`readonly_dict` 时跳过）：结算 `finish_session` / `finish_article_practice`（统计与卡片已在 `settle` 里写好）；切词库、打开词条页、改每日词数、保存笔记、词条增删排序（先重写词条文件并同步用户目录的 `length`）；确认退出与事件循环结束时的 `save_all`。首次运行即使没进练习，退出时也会生成 `dict.json`。练习中的收藏 / 错词 / 已掌握改动只改内存，靠上述时机落盘；FSRS 卡片与统计只在本轮结算时产生。

## 4. `setting.json`（`SettingState`，版本 23）

`val` 是上游 `SettingState` 的子集：43 个具名字段 + `#[serde(flatten)] extra`，其余上游顶层字段（`fontSize`、`showToolbar`、`webAppVersion`、`ttsVoiceMap` 等）原样透传。字段按用途分组：

| 分组 | JSON 键 |
| --- | --- |
| 练习与复习 | `wordPracticeMode`、`wordPracticeType`、`wordReviewRatio`、`autoAddRandomReviewWhenNoDue`、`repeatCount`、`repeatCustomCount` |
| 键入 | `ignoreCase`、`inputWrongClear`、`autoNextWord`、`waitTimeForChangeWord`、`spaceCooldownTime` |
| 显示与主题 | `theme`、`ignoreSimpleWord`、`showWordQuestion`、`showEtymologyAndRelWords`、`showNearWord`、`showUsageTips`、`alwaysShowNote`、`dictation`、`translate`、`ignoreSymbol` |
| 音频 | `soundType`、`ttsSource`、`keyboardSoundFile`，开关 `wordSound`/`keyboardSound`/`effectSound`/`articleSound` 与各自的 `…Volume`（`wordSound`/`articleSound` 另有 `…Speed`），以及 `sentenceSoundVolume`/`sentenceSoundSpeed` |
| FSRS | `fsrsEasyLimit`、`fsrsGoodLimit`、`fsrsHardLimit`、`fsrsParameters` |
| 快捷键与 TUI 标记 | `shortcutKeyMap`（动作名 → 键位，非法条目读取时回退默认绑定）、`tuiBigWord`（练习页 2 倍大字）、`_ignoreWatch`（不参与逻辑） |

每个设置项的含义、默认值与取值范围见 [设置项参考](settings.md)，这里只记存档层面：

- 写入时机：设置页改值、快捷键改绑 / 清除 / 恢复默认、高级 FSRS 页改参数 / 恢复默认、切换练习模式、切回内置流程等操作后立即 `App::save_settings_now`；离开设置页、快捷键页与高级 FSRS 页、确认退出时再保存一次（`save_all`）。`readonly_settings` 为真时全部跳过。
- 枚举字段（`wordPracticeMode` / `wordPracticeType`）序列化为整型，未知数值解析失败会触发隔离。`waitTimeForChangeWord` / `spaceCooldownTime` 读取后夹在 0–10000 毫秒；`repeatCount == 100` 时取 `repeatCustomCount`。
- `fsrsParameters` 用 snake_case 键（`request_retention`、`maximum_interval`、`w`、`enable_fuzz`、`enable_short_term`、`learning_steps`、`relearning_steps`），默认值由 `src/domain/constants.rs` 的 `default_fsrs_parameters` 给出；参数兜底统一在 `src/domain/fsrs.rs` 的 `Fsrs::new` 里做，手改坏参数不会让调度崩溃。
- `extra` 透传由 `tests/store_roundtrip.rs` 的 `settings_roundtrip_preserves_unknown_fields` 固定；设置页的「每日词数」不在 `setting.json`，它改 `dict.json` 当前词典的 `perDayStudyNumber`（3.2）。
- 新增设置项：`SettingState` 加字段并在 `impl Default` 给值 → 接设置页入口（`src/app/mod.rs` 的 `SETTING_ROWS` / `EditTarget`）→ 补文案与渲染。仅 TUI 自用的字段**不要**提升 `SETTING_VERSION`：旧存档缺字段由 default 补，提升版本只会让旧客户端变成只读；只有跟随上游 schema 变化或需要迁移时才提升。

## 5. `practice-word.json`（`PracticeCacheCompact`，版本 2）

`val` 为缓存对象；保存 `None` 时删除文件而不是写 `null`。词表以**单词字符串**保存，恢复时按当前词库还原成完整 `Word`。

| 字段 | 说明 |
| --- | --- |
| `taskWordsStr.new` / `.review` | 本轮任务词表（新词 / 复习词）的单词字符串 |
| `practiceData.index` | 当前词下标；恢复时夹在 `0..words-1` |
| `practiceData.excludeWords` / `allWrongWords` / `isTypingWrongWord` | 已排除词、本轮错词、是否处于错词重输状态 |
| `practiceData.wrongTimesMap` / `wrongTimes` / `ratingMap` | 单词 → 错误次数 / 总错误数 / FSRS 评级（1–4），结算时用于算卡片 |
| `practiceData.wordsStr` / `wrongWordsStr`；`question` | 当前节点词表与错词表（字符串）；自测题缓存由 `cache_to_compact` 固定写 `null`、恢复后重新生成 |
| `statStoreData` | `PracticeState` 快照：`stage`、计时（`startDate` / `spend` / `segments`）、新词 / 复习 / 输入计数、`wrong`、暂停状态 |
| `sessionSnapshot.flowId` | 流程 id（内置流程 id 或 `flows.json` 里的自建流程 id） |
| `sessionSnapshot.cursor` | 流程游标 `PracticeFlowCursor`：`nodeIndex`、`stepIndex`、`inWrongWordClear`、`loop`、`endActionIndex` |
| `sessionSnapshot.nodeWorkingWordKeys` | 当前节点工作词表的单词字符串 |

**写入与删除时机**（`App::save_practice_now` → `store.save_practice`）：开一轮练习立即保存；单词推进、键入错误、自测操作等进度变化排 500 ms 防抖保存；本次 tick 确实累计时长且跨过 30 秒整数倍时再排一次（`should_schedule_periodic_save`）；`Ctrl+C` 与退出程序立即保存；结算 `finish_session` 删除文件。写入失败弹 `保存练习进度失败: …`，不中断练习。

**恢复条件**（`App::try_resume_saved_practice` → `restore_practice_from_cache`，首页 `Enter` 触发；启动时只读一次状态、不自动恢复）。任一步不满足就静默放弃、**不删文件**：

1. 读到 `version == 2` 的缓存（否则走 2.3 的提示与忽略），且当前学习词典存在、已有词表；
2. `cache_from_compact` 能按当前词典的 `words` 用单词字符串精确匹配还原任务词表（匹配不到的词被丢弃，任务词表全空则放弃），且 `practiceData`、`statStoreData`、`sessionSnapshot` 三者齐备；
3. 流程游标合法（`Navigator::restore_snapshot`），且断点里的 flow 模式与本次要跑的模式一致。

恢复成功后进入练习页并 Toast「已继续上次练习」。因此换过词库后旧缓存通常无法恢复；版本不符的缓存不被移动，下次保存时被覆盖。

版本不符的缓存不被移动、也不会被删除；损坏的缓存按既有规则隔离为 `*.corrupt-<时间戳>.json`（内容保留，但不在原路径）。两种情况启动时都弹一次 Warning，且主页**不**显示「上次任务」：`App::has_saved_practice` 的判据是「版本可读且含可恢复内容」，不是文件是否存在。否则主页会一直挂着「上次任务」、按 `Enter` 却静默新建，用户无从得知原因。

## 6. `practice-article.json`（`ArticleCacheEnvelope`，版本 1）

文章跟打的断点缓存，与单词练习缓存**分文件**：混在一起会让「清空练习进度」误删另一轮的进度。

| 字段 | 说明 |
| --- | --- |
| `practiceData.articleId` | 文章 id |
| `practiceData.sentenceIndex` | 当前句下标（段落展开后的扁平全局下标） |
| `practiceData.finished` / `wrongSentences` / `total` / `spend` | 已完成句数 / 出错句数 / 总句数 / 累计用时（毫秒） |
| `statStoreData` | 与上游同名的占位字段；TUI 写 `None`，用时记在 `practiceData.spend` |

- 与上游结构不同：上游 `practiceData` 是「段落 / 句 / 词」三级游标，TUI 在 `gen_sections` 后按扁平句子推进，只存一个 `sentenceIndex`；备份里的映射见第 10 节。
- 保存：每完成一句排 500 ms 防抖，另有 30 秒整点兜底；离开文章页（`Esc` / `Ctrl+C`）与退出程序立即保存。整篇练完时**删除**文件，否则下次会「恢复」一篇已练完的文章。
- 恢复：启动时只读取（`App::refresh_article_resume`）用于列表标记「上次第 N 句」，不直接进跟打页；选中该文章按 `Enter` 才从记录的句子续打。文章 id 在当前书目里找不到就忽略；资源变短时下标按当前句数夹取，0 句的文章不显示标记。
- 损坏 / 版本不符：同 2.3，损坏提示「文章练习缓存损坏，已丢弃：<原因>（已移动到 <隔离路径>）」，不阻断启动；读不出来时不隔离、不删除，本次运行只读。

## 7. `flows.json`（`FlowStore`，无版本号）

自建练习流程的存档，对应上游 localStorage 的 `PracticeFlowV2`，结构为 `{ "activeId": "<id 或空串>", "flows": { "<id>": { "name", "config", "createdAt", "updatedAt" } } }`。

- **没有信封、没有版本号**：上游用 localStorage，本来就没有版本概念；`config.version` 是流程配置自身的版本，与 `CURRENT_FLOW_VERSION`（当前 6）一致。
- `config` 是 `src/domain/flow/mod.rs` 的 `PracticeFlowConfig`：`id` / `version` / `mode` / `label` / `nodes`；节点为 `id` / `label` / `source` / `steps`；步骤为 `templateId` / `wordAdvance` / `onEnd` / `shuffleOnEnter`。`wordAdvance` 为 `wordLoop` 时带 `groupSize` 与 `subSteps`（`{ templateId }` 对象数组，不是裸 id 数组）。字段名逐字对齐上游，因此可与上游 localStorage 内容互换。
- `activeId` 为空表示走内置流程（`FlowStore::active` 返回 `None`）。写入用 pretty JSON + 原子写，启停 / 新建 / 删除 / 编辑流程后立即 `App::save_flow_store`，失败弹 `保存流程失败: …`。
- 解析失败时隔离为 `flows.json.corrupt-<时间戳>.json`（同一秒重名追加 `-2`、`-3`），启动 Toast「流程存档已损坏，已隔离并恢复默认值」，随后按空集合运行。读不出来（IO 错误）时不隔离、不删除，本次运行不保存流程并提示一次。

## 8. 资源词库与文章格式

### 8.1 `<assets>/list/word.json`

顶层是词库目录数组（`src/domain/catalog.rs` 的 `DictResource`）：

| 字段 | 说明 |
| --- | --- |
| `id` | 词库身份，字符串或数字，读取时归一化成字符串；空 id 的项被跳过 |
| `enName` | 上游路由名，可作 `--dict` 取值；也接受 `en_name` 别名 |
| `name` / `description` | 展示名与简介 |
| `url` | 词条文件名，拼成 `dicts/<language>/word/<url>` |
| `length` | 登记（名义）词数；词表加载后以实际词条数为准（`Dict::effective_length`） |
| `category` / `tags` / `language` / `translateLanguage` | 展示与语言元数据；`language` 为空按 `en` |
| `type` | 只处理 `"word"` 或缺省项，其它类型被过滤 |
| `custom` | 用户自建 / 导入标记（从 `extra` 读取，`DictResource::is_custom`） |
| `version` / `cover` / 其它 | 不参与 TUI 逻辑，未知字段忽略 |

内置文件只有一本：`id = 1`、`enName = "cet4"`、`url = "CET4_T.json"`、`length = 2607`。

### 8.2 `<assets>/dicts/en/word/*.json`

顶层是词条数组，`src/domain/types.rs` 的 `Word` 只读取：`id`（数字 / 字符串 / 缺省，统一转字符串）、`word`（本体，`trim` 为空即被过滤）、`phonetic0` / `phonetic1`（英式 / 美式音标）、`trans`（`{ pos, cn }`）、`sentences` / `phrases`（`{ c, cn }`）、`synos`（`{ pos, cn, ws }`）、`relWords`（`{ root, rels }`）、`etymology`（`{ t, d }`）、`custom`。缺省数组按空处理，`inflections`、`e2e` 等未建模字段静默忽略。

仓库内置 CET-4 为 `assets/dicts/en/word/CET4_T.json`：2607 词、约 4.5 MB，首词 `cancel`（`tests/real_assets.rs` 固定）。词条文件存在但没有可用单词不算错误（新建的自建词典一开始就是空的）；文件缺失才报 `找不到词库文件`。

### 8.3 目录合并与文件优先级

- `Catalog::load` 按 `[assets, home]` 读两个 `list/word.json`：先内置后用户，**同一个 `id` 的用户项替换内置项**（位置保留在首次出现处）；合并后为空报 `词库目录为空`，用户目录解析失败报 `解析词库目录失败: <path>`。
- 词条文件按相对路径先查 `<home>`、再查 `<assets>`：导入一个与内置文件**同名**的词库（如 `CET4_T.json`）会遮蔽内置文件；写入路径（`Catalog::dict_write_file`）永远落在 `<home>`，内置资源一个字节都不改。
- 界面上的词数一律以词条为准；目录登记的 `length` 只对「词条文件还没读过」的词库生效。词条编辑 / 删除 / 排序后 `App::persist_words` 会调 `Catalog::sync_user_catalog_length` 把用户目录的 `length` 改成真实词条数。

### 8.4 文章资源

书目在 `<assets>/list/article.json`（`src/domain/article.rs` 的 `ArticleBook`：`enName` / `name` / `description` / `url` / `category` / `tags` / 登记 `length`），正文在 `<assets>/dicts/en/article/<url>`（`Article` 数组：`id`、`title`、`titleTranslate`、`text`、`textTranslate`、`audioSrc`；`sections` 是运行时由 `gen_sections` 生成的断句结果，不落盘）。书目登记的篇数一律被 `load_books` 用实际篇数覆盖，文件读不到时记为未知（界面显示「篇数未知」）而不是拿登记值充数。当前只有一本书《新概念英语1》（`NCE_1.json`，5 篇）。

## 9. `tw import-dict` / `tw export-dict`

只接受**词条数组 JSON**（与 8.2 同格式），不支持词库目录对象、ZIP、Excel。步骤（`Catalog::import_dict`）：

1. 读入前先按 `metadata` 预检大小，超过 512 MiB（`src/fs.rs` 的 `IMPORT_SIZE_LIMIT`）直接报「文件过大」，不整读进内存；再解析为 `Vec<Word>` 并用 `Word::is_usable` 过滤，至少要有一个可用词；文件名必须以 `.json` 结尾，去掉 `.json` 后不能为空（文件名就叫 `.json` 时没有 id，目录项会被装载端跳过，因此直接拒绝），也不能含路径分隔符；
2. 加载词库目录（需要可用的 `assets/`），把源文件**原样**写入 `<home>/dicts/en/word/<文件名>`（走原子写，不会留下半个文件；源与目标同路径也安全）；
3. 生成目录项：`id = enName = name = 文件名去 .json`、`description = 导入词库（N 词）`、`url = 文件名`、`length = N`、`category = 自定义`、`tags = [导入]`、`language = en`、`type = word`、`custom = true`；
4. 写 `<home>/list/word.json`（pretty JSON + 原子写；按归一化 id 判重，同 id 替换、否则追加），打印导入结果并提示 `tw --dict <id>`；**不会自动切换**当前词库。

要点：`custom = true` 不能少（见 2.6）；命令**不是事务性的**——先复制词条文件、后写用户目录，用户目录写失败时词条文件已存在但未登记，重跑即可修复。常见失败文案：`读取词库失败: <path>`、`解析词库失败（需要词条数组 JSON）: <path>`、`词库中没有可用单词: <path>`、`词库文件需要是 .json 结尾: <文件名>`、`词库文件名不能是 .json（需要 <名称>.json）: <path>`、`词库文件名不能包含路径分隔符: <文件名>`、`文件过大：<实际大小>，上限 <上限>`（能精确换算时用 MiB / KiB，如 `512 MiB`、`1.5 KiB`；否则保留字节数再附近似值，如 `536870913 字节（约 512.0 MiB）`；路径由前一条「读取词库失败: <path>」给出，不在超限错误里重复）、`复制词库失败: <源> → <目标>`。

**`tw export-dict [DICT_ID] [--out FILE]`**（只读入口，不取锁）是它的逆操作：

1. `DICT_ID`（id 或 enName）先在词库目录里找，命中就读词条文件；全局 `--dict` 作为缺省值。两者都没有时取**当前学习词库**（`dict.json` 的 `studyIndex`），空数据目录还没有当前词库则报错并提示用 `tw doctor` 查 id。
2. 没在词库目录命中的再查存档：收藏 / 错词 / 已掌握的词条只在 `dict.json` 里，没有词条文件；普通词库的词表在存档落盘时被 `shake_common_dict` 清空，按 id / enName 回词条文件补齐，所以导出的始终是完整词表。
3. 词条按 `Word` 序列化成词条数组（pretty JSON，与 8.2 同格式），空词条被 `Word::is_usable` 过滤；过滤后为空直接报错 `词库中没有可用单词: <名称>`，而不是写出一个 `import-dict` 会拒收的 `[]`。
4. `--out` 缺省写到标准输出（只有 JSON，可直接重定向或接管道）；写文件走原子写（同 `import-dict`），中途失败不会留下半个词库文件。常见失败文案：`未找到词库: <id>（用 tw doctor 查看全部词库 id）`、`还没有当前学习词库，请指定 DICT_ID（tw doctor 可列出全部词库 id）`、`词库中没有可用单词: <名称>`、`写入词库文件失败: <path>`。

## 10. 与 Web 数据互通

TUI 不读写浏览器 IndexedDB，互通靠 `tw import-web` / `tw export-web`（`src/backup.rs`）或手工搬 JSON；两边使用同构信封与同名字段，文件与网页版 IndexedDB key 的对应关系见 2.1。

网页版导出的备份是 `BackupData` v5（`backup::WEB_BACKUP_VERSION`）：`{"version": 5, "val": { "dict": {…}, "setting": {…}, "PracticeSaveWord": {…}, "PracticeSaveArticle": {…} }}`，每个子对象本身就是 `{version, val, updated_at?}` 信封。

**`tw import-web FILE`**（写入口，取锁）：

- 备份文件读入前按 `metadata` 预检大小，超过 512 MiB（`src/fs.rs` 的 `IMPORT_SIZE_LIMIT`）直接报「文件过大」，不整读进内存；`tw import-dict` 的词条文件同一上限。自家存档的备份 / 回滚（`copy_atomic`）不设上限。
- 顶层 `version > 5` 整体拒绝；缺 `version` 报「备份缺少 version 字段」，`version` 不是非负整数（如 `-1`、字符串）报「备份的 version 字段不是非负整数（实际：<值>）」；缺 `val` 对象报错退出。
- 覆盖前先读本地存档的信封版本头：本地 `dict.json` / `setting.json` / `practice-word.json` / `practice-article.json` 的版本高于本程序支持值时**不覆盖**，记「已跳过」并点明本地版本与支持版本，提示先备份数据目录、升级程序后再导入。其中 `dict.json` / `setting.json` 运行时受只读保护（版本过高时只读、不写回默认值）；两份练习缓存的版本不符在运行时只是被忽略、保存时反而会被降级覆盖，导入按同一口径跳过，不去动本地文件。
- `dict.json` / `setting.json`：备份信封版本高于支持值 → 记「已跳过」，保留本地文件；缺 `val` / 缺 `version` / 字段类型不匹配 → 「已拒绝」，原文件不动。`practice-word.json`（备份键 `PracticeSaveWord`）必须**等于** 2（低版本 TUI 也读不懂），否则跳过并保留本地断点；`val: null` 被接受为「没有缓存」。报告里的条目一律用本地文件名（`dict.json`、`practice-article.json` …），不用备份键名。
- 校验策略是「校验即将落盘的原文」（`validate_envelope`），通过后**原样写入**（`FileStore::write_envelope_raw`），因此未建模字段不会丢，也不会出现「导入成功、下次启动被判损坏」。
- 覆盖前先把旧文件复制成 `<文件名>.bak-<时间戳>.json`（同一秒重名加 `-2`、`-3`；备份也走原子写，不会留下半截 `.bak`）。多个文件写入途中失败时按备份逆序回滚，恢复同样是原子写。错误信息按实际情况给：一份都没写成就报「没有存档被改动」，写过则分别列出「已回滚: …」与「未能回滚: …」，每一项都带对应的 `.bak`（导入前不存在的存档回滚时删除，没有 `.bak`）——回滚全部失败时磁盘上已是导入后的内容，不会谎称没改动。`.bak` 不自动清理。
- 文章断点按来源分流：带 `tuiPracticeData` 扩展（TUI 导出）或本身就是 TUI 原生 `practiceData` 的，规范化写入 `practice-article.json`；空占位（`{}`）不动本地；`val` 缺失或不是对象（如 `null`）记「已丢弃」并说明原因，本地文件不动；网页版自身的三级游标没有文章 id，无法映射，同样报「已丢弃」。

**`tw export-web [--out FILE]`**（只读入口，不取锁）：读出四份存档原文，**缺失、不可解析或不是存档信封的**都用空信封 `{"val": {}, "version": N}` 占位——导出是抢救数据的通道，一份存档坏掉不该让整次导出失败；读不到 / 解析失败 / 形状不对（非对象、缺 `version` 或 `val`）的那几份各在 **stderr** 打一条警告（带文件名与原因，例如 `警告：dict.json 不可解析，已用空信封占位：…`），**标准输出始终只有备份 JSON**，可直接重定向或接管道；`--out` 写文件走原子写（同 `export-dict`），中途失败不会留下半截备份。拼成 v5 备份（pretty JSON）。文章断点被包成网页版能安全解析的信封：Web 三级游标置 0、`statStoreData` 用非空对象（带 `spend` 与来自 `updated_at` 的 `startDate`），真正的 TUI 断点放在 `tuiPracticeData` 里——网页版原样存取、不读它，只要不在文章页推进进度就能随备份往返。

**手工迁移**：先在网页版导出备份取 `data.json`（或从开发者工具读 IndexedDB 的两个 key），退出所有 tw 实例并备份 `<home>`，然后 `dict.json = data.val.dict`、`setting.json = data.val.setting`，需要断点时 `practice-word.json = data.val.PracticeSaveWord`（`updated_at` 可缺省）；文章断点不要手工拼。启动后用 `tw doctor` 核对路径与存档大小，再进设置页确认主题等字段。当前学习词典在资源里不存在时，启动按「原 id → `cet4` → 第一本词库」回退（`ensure_study_dict`）；`studyIndex` 指向系统词典时原样保留。

**迁移风险**：网页版版本更新时 setting v>23、dict v>4 触发只读保护，练习缓存版本不符被忽略；TUI 独有的 `tuiBigWord` 经网页版往返会丢失并回到默认值；写回系统词典时 `Word` 未建模字段（如 `inflections`）被丢弃，词典 / 设置层级的未知字段经 `extra` 保留；网页版的自定义词库若不在词库目录里，切走后无法再切回（需放进 `<home>/dicts/en/word/` 并登记 `<home>/list/word.json`）；备份里的 `mp3/` 在 TUI 中没有对应实现。

## 11. 备份、恢复与重置

- **完整备份**：退出所有 tw 实例后复制整个 `<home>`（5 份存档 + `.tw.lock` + `themes/` + 导入的 `dicts/` 与 `list/word.json`；`.tw.lock` 只是 PID 记录，复制或删除都无害；`logs/` 是运行日志与崩溃报告，可留可删）。只拷 `dict.json` 会丢设置、断点与导入词库。
- **恢复**：退出所有实例 → 用备份覆盖 `<home>` → 启动；先 `tw doctor` 核对路径与存档大小，再检查主题与设置。
- **只重置练习**：`tw --reset-practice`（启动时删除两份断点缓存）或设置页「清空当前练习进度」（先把本轮统计落盘再做清理）。两者都只删 `practice-word.json` 与 `practice-article.json`，不动 `dict.json` 里的统计、FSRS 卡片与三本系统词典；读不出来的缓存不删（内容只是本次读不到），提示改为「练习进度未清空」。`--reset-practice` 只探测可读性、不读缓存内容，因此损坏的缓存被**直接删除**而不是先隔离成 `.corrupt-*` 副本（这条启动参数的用途是「复现问题时清干净」）；「清空当前练习进度」与「清除所有数据」同样按实际结果提示，没删掉时不会报「已清空 / 清除成功」。
- **重置学习记录**：设置页「清除所有数据」确认后 `boot.base` / `boot.settings` 回到默认值、删除两份缓存并立即落盘，再按全新安装重新选一本学习词典；任一存档版本过高（本次只读）时拒绝执行并提示，避免假成功（`flows.json` 不在清除范围）。**彻底重置**则关闭程序后删除整个 `<home>`（内置词库在 `assets/`，不会丢），下次启动重建目录。
- **清理与退回**：隔离文件 `*.corrupt-<时间戳>.json` 与导入产生的 `*.bak-<时间戳>.json` 都不自动清理，确认无用后手工删除；需要退回导入前的状态时退出所有实例，用 `.bak` 覆盖同名存档即可。版本降级不会破坏原文件（过高只读、缓存版本不符忽略并覆盖），但新版本保存后格式会升级，降级前先备份。

`tw doctor` 是排障入口：打印版本行、数据目录、资源目录、日志目录、词库清单（含词条文件路径与词数）、5 份存档的大小（不存在则标注）、自定义主题目录与全部主题及加载告警。它不解析存档内容，版本过高 / 损坏只会在启动 Toast 里出现；`tw doctor`、`tw export-dict` 与 `tw export-web` 不取锁，但在别的实例运行时读到的多份文件不保证同一时刻，需要一致快照就先退出所有实例。

## 12. 安全与隐私

- **默认离线**：程序不主动联网；只有开启有道发音（`ttsSource = youdao`）时才下载音频，播放走外部播放器（`src/audio/player.rs`）。无账号、无云同步、无遥测。
- **明文存储**：存档是可读 JSON，没有加密；笔记、进度、收藏对同机有文件读权限的用户可见，取决于操作系统权限。
- **手工编辑与并发**：先退出程序并备份，保持信封结构、UTF-8 编码（读取容忍开头的 BOM，写回不带）、合法 JSON（不能有注释 / 尾逗号），改坏会在下次启动被隔离为 `*.corrupt-*.json`；运行时编辑会被退出时的内存状态覆盖。写入口互斥靠数据目录锁（1.3），但把 `<home>` 放进 OneDrive 等同步目录、或两台机器各自运行同一份数据时没有冲突检测，最后写入者获胜。

## 相关文档

- [文档索引](README.md)
- [使用手册](usage.md)（命令行、界面与排障）
- [设置项参考](settings.md)（`setting.json` 各字段的含义与默认值）
- [主题系统](themes.md)（`<home>/themes/*.json` 格式）
- [总体架构](architecture.md)（`src/store/` 与 `src/fs.rs` 在分层中的位置）
