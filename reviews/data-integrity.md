# data-integrity 专项审查报告

- 对象：typewords-tui v0.4.0（D:\Projects\typewords-tui）
- 范围：5 份 JSON 存档（dict.json / setting.json / practice-word.json / practice-article.json / flows.json）的写入路径、版本保护、损坏隔离、数据目录锁，以及 import-dict / import-web / export-dict / export-web 与 backup.rs 往返。
- 方法：逐模块走查源码 + 相关测试同读 + 与 docs/persistence.md 对照。未运行任何测试（全部结论来自代码走查，无需运行时验证；个别外部库行为标了验证思路）。
- 结论：P0 = 0，P1 = 0，P2 = 2，P3 = 10。原子写原语、版本判定次序、import-web 事务与锁策略整体质量高，问题集中在「IO 读错误与解析错误混淆」「文档承诺与实现脱节」两类边界。

---

## 问题列表

### 1. [P2] `tw export-web` 在任一存档不可解析时整体失败，文档承诺的「空信封占位」不存在

- 位置：`src/backup.rs:543-553`（read_envelope_raw）、`src/backup.rs:396-400`（export_web 的三处 `?`）
- 证据：
  ```rust
  fn read_envelope_raw(store: &FileStore, name: &str) -> Result<Option<Value>> {
      let path = store.file_path(name);
      if !path.exists() {
          return Ok(None);
      }
      let text =
          std::fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
      let value: Value = serde_json::from_str(&text)
          .with_context(|| format!("{} 不是合法的 JSON", path.display()))?;
      Ok(Some(value))
  }
  ```
  ```rust
  let dict = read_envelope_raw(store, "dict.json")?
      .unwrap_or_else(|| empty_envelope(DICT_STATE_VERSION));
  ```
  `docs/persistence.md:315`：「读出四份存档原文，**缺失或不可解析的**用空信封 `{"val": {}, "version": N}` 占位」。代码里「缺失」（文件不存在 → `Ok(None)` → 空信封）有实现，「不可解析」（JSON 非法）走 `Err` 直接让 `tw export-web` 整体报错退出。
- 触发条件：`dict.json` / `setting.json` / `practice-word.json` 任一存在但内容不是合法 JSON（外部编辑器写坏、半截文件、BOM），执行 `tw export-web`（含 `--out`）。
- 违反的不变量：CLAUDE.md「文档与代码不一致时以代码为准，但要同步修文档」——这里代码比文档弱，且没有任何一处与文档对齐。
- 影响：存档损坏时恰恰是最需要导出备份抢救数据的时刻，导出通道整体失效；用户只能手工搬文件。
- 建议方向：二选一。按文档实现（解析失败时以空信封占位，并在 stderr 提示哪份读不出来）；或修 docs/persistence.md:315 明确「不可解析的存档会让导出失败，请先修复 / 用隔离文件」。
- 置信度：确认（代码路径无回退分支；tests/backup_roundtrip.rs 只覆盖「文件缺失 → 空信封」「文章缓存版本不符 → 空信封」，没有坏 dict.json 的导出用例）。

### 2. [P2] 瞬时读错误（IO 失败）与解析失败同样触发隔离：好档会被移走、再被默认值覆盖

- 位置：`src/store/mod.rs:148-149`（read_raw 读文件）、`src/store/mod.rs:203`（load_typed 把读失败交给 corrupt）、`src/store/mod.rs:181-187`（corrupt = 默认值 + 隔离）、`src/bootstrap.rs:145-146`（readonly 只认 VersionTooNew）
- 证据：
  ```rust
  let text = fs::read_to_string(&path)
      .with_context(|| format!("读取存档失败: {}", path.display()))?;
  ```
  ```rust
  Err(err) => return self.corrupt(name, &err),
  ```
  ```rust
  fn corrupt<T: Default>(&self, name: &str, err: &anyhow::Error) -> Loaded<T> {
      Loaded {
          value: T::default(),
          status: LoadStatus::Corrupt(self.quarantine_reason(name, err)),
          updated_at: None,
      }
  }
  ```
  `Corrupt` 不设置 `readonly_dict`（`src/bootstrap.rs:145` 只匹配 `VersionTooNew`），所以退出时 `save_all` 会照常写盘。
- 触发条件：启动装载瞬间 `dict.json` 被另一进程锁住（OneDrive / 杀毒扫描——`docs/persistence.md` §12 明确把同步目录列为使用场景）或权限瞬时异常 → `read_to_string` 失败 → 走 Corrupt 分支。验证思路（确定性复现）：Unix 上把 `dict.json` chmod 000 后启动，观察它被改名成 `.corrupt-*.json` 并在退出时生成默认值新档。
- 后果分两种：
  1. 隔离 rename 成功（Unix 上 rename 只需目录写权限）：好档被移到 `dict.json.corrupt-<时间戳>.json`，程序按默认值继续，退出时把基于默认值的新档写回原路径——原进度只剩隔离文件，需手工恢复，期间新练的进度写在同名新档上无法合并。
  2. 隔离 rename 也失败（Windows 上文件被独占锁时连 DELETE 访问都拿不到）：原文件留在原地，程序仍按默认值运行，下次保存直接整份覆盖好档（此时连隔离副本都没有）。
- 违反的不变量：损坏隔离应只针对「解析失败」（内容坏了）；读不到 ≠ 内容坏。docs/persistence.md 2.3 把「读取或版本头解析失败」并列归为隔离，正是问题所在。
- 影响：在文档支持的部署（同步目录）里一次瞬时锁即可让学习进度「被重置」；情形 2 是真正的好档覆盖。
- 建议方向：read_raw 区分错误类别——IO 错误（PermissionDenied / 共享冲突）不隔离、不视为损坏：可带原错误在进入终端前 bail（用户重试最安全），或置只读保护（复用 VersionTooNew 的拦截），把「隔离」严格留给解析失败。
- 置信度：确认（代码路径完整可达；外部锁时序无法在单测复现，但 chmod 000 可确定性地走同一条链）。

### 3. [P3] `quarantine` 隔离名无唯一化：同一秒重名会静默覆盖既有隔离文件

- 位置：`src/store/mod.rs:126-136`
- 证据：
  ```rust
  let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
  ...
  let target = path.with_file_name(format!("{file_name}.corrupt-{stamp}.json"));
  fs::rename(path, &target)
  ```
  `fs::rename` 语义是替换既有目标文件（Windows 走 MOVEFILE_REPLACE_EXISTING）。对比同仓库两处做对了的地方：`src/backup.rs:296-312` 的 `next_backup_path` 用 `!candidate.exists()` 循环追加 `-2`、`-3`；崩溃报告按 docs/persistence.md §1.5 用 `create_new` 原子占名。
- 触发条件：同一存档在同一秒内两次进入隔离分支（如外部进程 1 秒内两次写坏刚恢复的文件）。写入口被锁串行化，跨进程场景基本不存在，属边角。
- 影响：旧的隔离副本（已损坏数据的手工恢复依据）被静默覆盖；丢的本来就是坏档副本，低危。
- 建议方向：沿用 next_backup_path 的占名探测或 create_new。
- 置信度：确认（代码无唯一化；触发概率低）。

### 4. [P3] `export-web --out` 用非原子 `std::fs::write`，与 export-dict / 导入路径不一致

- 位置：`src/cli.rs:277-284`
- 证据：
  ```rust
  std::fs::write(
      path,
      format!("{text}\n")
  )
  .with_context(|| format!("写入 {} 失败", path.display()))?;
  ```
  同文件里 `export-dict --out` 走 `crate::fs::write_atomic(path, ...)`（`src/cli.rs:333`），且注释明说「中途失败不会把目标文件截成半截」。`fs::write` 是打开即截断。
- 触发条件：`--out` 指向已存在的文件，写入中途失败 / 被杀（Ctrl+C、磁盘满、目标被占用）。
- 影响：输出文件留半截 JSON。产物不是存档、不丢学习数据；docs 只对 export-dict 承诺原子写（persistence.md:299），因此是内部一致性破缺而非文档违约。
- 建议方向：改用 `write_atomic`（导出文本已在内存里，改动一行）。
- 置信度：确认。

### 5. [P3] UTF-8 BOM 一律判为解析失败：存档会被隔离重置、导入被拒收

- 位置：`src/store/mod.rs:148-154`（read_to_string + from_str，无 BOM 剥离）；`src/domain/catalog.rs:375-378`（import-dict）；`src/backup.rs:84-87`（import-web）
- 证据：所有 JSON 读取都是 `fs::read_to_string` 的结果直接交给 `serde_json::from_str`，全仓库无 `\u{feff}` 处理。
- 触发条件：Windows 上记事本等工具以「带 BOM 的 UTF-8」保存 `dict.json`（或给 import-dict / import-web 喂带 BOM 的文件）。
- 行为：serde_json 不把 U+FEFF 当空白 → 解析失败。存档路径：按损坏隔离，进度看似清零（内容在 `.corrupt-*.json` 可手工恢复）；导入路径：干净报错拒收（与上游 `JSON.parse` 拒 BOM 行为一致，属上游对齐）。
- 影响：误伤好档（可恢复）；本工具主平台是 Windows，BOM 是最常见的「外部编辑」副产品。
- 建议方向：读入后 `strip_prefix('\u{feff}')`，至少覆盖 5 份存档与两个导入口。
- 置信度：确认（「解析失败 → 隔离」的链路 100% 来自代码；「serde_json 不跳过 BOM」是库的已知行为，一步验证思路：`serde_json::from_str::<serde_json::Value>("\u{feff}{}")` 的临时单测应返回 Err）。

### 6. [P3] `import-web` 会覆盖「版本过高的只读档」，无提示

- 位置：`src/backup.rs:258-293`（replace 只做备份与写入，不读本地版本头）；对比 `src/app/mod.rs:1104-1110` TUI 侧的 readonly 拦截
- 证据：
  ```rust
  let backup = if path.is_file() {
      let backup = self.next_backup_path(name);
      copy_atomic(&path, &backup).with_context(...)?;
      Some(backup)
  } else { None };
  match write(self.store) { ... }
  ```
- 触发条件：本地 `dict.json` 是 version 99（未来版本，TUI 运行时受只读保护）；用户用旧程序执行 `tw import-web old-backup.json`（备份里是 v4 条目，通过校验）。
- 行为：本地 v99 文件被复制为 `.bak` 后直接写入 v4 内容——「版本过高只读不覆盖」的保护在导入路径上完全不存在。
- 影响：用户误操作即降级自己的新版本存档（有 `.bak` 可手工恢复，回滚机制本身健全）。
- 建议方向：replace 前解析本地信封版本头（复用 EnvelopeHeader 思路），发现 VersionTooNew 时打印警告或要求确认。
- 置信度：确认。

### 7. [P3] `import-dict` 对文件名 `.json`（stem 为空）生成空 id 目录项：导入「成功」却永久不可见

- 位置：`src/domain/catalog.rs:389-391`（后缀校验）、`:404`（stem）、`:139-142`（Catalog::load 跳过空 id）、`:427-443`（写入用户目录）
- 证据：
  ```rust
  if !file_name.ends_with(".json") {
      return Err(anyhow!("词库文件需要是 .json 结尾: {file_name}"));
  }
  ...
  let stem = file_name.trim_end_matches(".json").to_string();
  ```
  ```rust
  let key = resource.id_string();
  if key.is_empty() {
      continue;   // Catalog::load 直接跳过空 id 条目
  }
  ```
- 触发条件：导入一个名为 `.json` 的合法词条文件（Windows / Unix 均可创建）。
- 行为：stem 为空 → 目录项 `id: ""` 写进 `<home>/list/word.json`，CLI 打印「已导入词库: （N 词）」与 `tw --dict  切换`（空 id）；下次 `Catalog::load` 跳过空 id，词库页 / doctor / `--dict` 全都看不到它，词条文件与目录项成为孤儿死数据。
- 影响：假成功 + 死数据；不影响其它存档。
- 建议方向：stem 为空时报错（与「需要 .json 结尾」同一类前置校验）；或装载目录时清掉空 id 项。
- 置信度：确认（三段代码都已读过；id_string 对 `Value::String("")` 返回空串）。

### 8. [P3] 导入 / 备份读入无大小上限，超大文件全量进内存

- 位置：`src/domain/catalog.rs:375`（import-dict read_to_string）、`src/backup.rs:84`（import-web）、`src/fs.rs:58`（copy_atomic 的 fs::read）
- 证据：`let text = std::fs::read_to_string(source)...`、`let bytes = fs::read(from)...` —— 均为整读，无 metadata 大小预检。
- 触发条件：导入一个数 GB 的「JSON」（误操作或恶意构造）。
- 行为：内容整份进内存，无流式、无上限；系统内存耗尽后进程被杀。导入是「先读后写」，被杀时目标文件未被触碰（import-dict 写盘发生在解析通过之后），无数据损坏。
- 影响：进程被杀（无数据损失）；本地单用户工具威胁低，但错误形态（OOM 被杀）对用户不可读。
- 建议方向：读前 `fs::metadata` 检查大小（例如 > 512 MB 直接报「文件过大」）。
- 置信度：确认（行为由代码结构与内存模型推出；未实测 OOM——验证思路：生成 3 GB 伪 JSON 走 import-dict 观察进程）。

### 9. [P3] `tw task`（预览命令）有未披露的写副作用：可能删除 practice-word.json

- 位置：`src/bootstrap.rs:68-76`（normalize_study_progress → remove_practice）；调用链 `src/cli.rs:182`（task_preview → bootstrap::load）
- 证据：
  ```rust
  fn normalize_study_progress(&mut self) {
      let repaired = match self.base.study_dict_mut() {
          Some(dict) if !dict.system => dict.normalize_progress(),
          _ => false,
      };
      if repaired {
          let _ = self.store.remove_practice();
      }
  }
  ```
- 触发条件：当前词典 `length` 与实际词数不一致或 `lastLearnIndex` 越界（如上次运行后词条文件被外部改短）时执行 `tw task`。
- 行为：作为修复手段删除单词断点缓存。设计上是自洽的（与 TUI 启动同一路径，代码注释说明了动机；锁策略也正确地把 task 归为写入口并取锁），但 `docs/usage.md:105` 只说「只预览今日任务…需取数据目录锁」，未披露「可能删除断点」。
- 影响：断点缓存被预览命令删掉（该设计认为缓存已失效，统计已在 dict.json），损失有限；属文档缺口而非行为错误。
- 建议方向：usage.md 补一句，或删除时在 stdout 输出一行提示。
- 置信度：确认。

### 10. [P3] 边角一致性合集：缓存隔离提示吞细节、import-web 静默忽略项

- 位置与证据（三个小点）：
  1. `src/store/mod.rs:243-251` / `:269-277`（load_cache）：
     ```rust
     Err(_) => {
         let path = self.file_path(name);
         let _ = self.quarantine(&path);
         return Loaded { ..., status: LoadStatus::Corrupt(corrupt_reason.to_string()), ... };
     }
     ```
     对比 `load_typed` 的 `quarantine_reason`（store/mod.rs:173-179）会把「（已移动到 <路径>）」写进 reason、隔离失败也会出现在 reason 里。断点缓存的 Toast 只有固定文案「练习缓存损坏，已丢弃」：用户不知道文件被移到哪、隔离是否成功。
  2. `src/backup.rs:500-502`（import_article_envelope）：`PracticeSaveArticle` 存在但 `val` 缺失 / 非对象时直接 `return Ok(())`——不进 skipped / rejected / discarded 任何一类，用户无从知晓这条被忽略。
  3. `src/backup.rs:92-96`：备份顶层 `version` 类型异常（如 -1、字符串）时报「备份缺少 version 字段」，语义略有误导（字段在，只是不是非负整数）。
- 触发条件：各自畸形输入 / 损坏缓存。
- 影响：仅提示与报告完整性，不影响数据本身。
- 建议方向：load_cache 复用 quarantine_reason；val 非对象记入 discarded；version 类型异常换文案。
- 置信度：确认。

### 11. [P3] 用户目录项的 url / language 未做单段校验，手工编辑可让读写逃出 `<home>/dicts/...`

- 位置：`src/domain/catalog.rs:178-186`（dict_file）、`:222-228`（dict_write_file）；写入方 `src/app/mod.rs:3141-3159`（persist_words）
- 证据：
  ```rust
  let relative = Path::new("dicts")
      .join(resource.language_or_default())
      .join("word")
      .join(&resource.url);
  self.home.join(relative)
  ```
  `url` 与 `language` 是 `<home>/list/word.json` 里原样读取的字符串。import-dict 生成的 url 恒为单段文件名（`source.file_name()`），无此通道；只有手工编辑目录文件才能注入 `..`。
- 触发条件：手工把目录项 `url` 写成 `../../x.json`（或 `language` 写 `..`）后打开该词库编辑词条。
- 影响：词条读写落到 `<home>` 之外。属于威胁模型内的「自担风险」操作（docs §12 手工编辑），定级为防御性改进。
- 建议方向：装载目录时校验 url / language 为单段安全组件（含路径分隔符即拒绝该项并告警）。
- 置信度：确认（join 语义与无过滤的事实；未实际构造目录项验证——思路：fixture 的 home 目录 json 写 url=`../escape.json` 后 persist_words，断言写入路径）。

### 12. [P3] rename 后未 fsync 父目录：断电语义为「旧内容或新内容」，本次保存可能整个丢失

- 位置：`src/fs.rs:68-92`（write_atomic_with：`sync_all` 只作用于临时文件句柄）
- 说明：fsync(临时文件) 保证内容落盘；rename 的目录项变更未同步。断电时 rename 可能未持久化：结果要么旧内容（安全），极端时新内容 + 残留 `.tmp`（也安全）——不会出现半截文件，因此 docs/persistence.md:91「断电时不会留下半个文件」的承诺成立。缺失的是更严格的崩溃一致性（rename 丢失 = 本次保存丢失，但状态仍是完整旧档，不会丢历史数据）。Windows 无目录 fsync 概念，只能 Unix 侧尽力。
- 建议方向：Unix 上 rename 后对父目录 `File::open(dir)` + `sync_all`（可选，代价是每次保存多一次 fsync）。
- 置信度：确认（代码事实）；影响评级为改进。

---

## 做得好的地方

1. **版本判定先于载荷反序列化，且专门防了「高版本被误判损坏」**：`EnvelopeHeader`（src/store/mod.rs:87-92）只解析 `version` / `updated_at`，`updated_at` 用 `serde_json::Value` 接住（类型演进不让版本判定退化成损坏）；tests/store_roundtrip.rs:83-141 用「高版本 + 不兼容载荷」「高版本 + 数字时间戳」两个用例钉死该行为。这是本维度最容易做错的地方，做对了。
2. **原子写原语扎实且有故障注入测试**：唯一临时名（PID + 进程内单调计数 + `create_new` + 16 次换名，src/fs.rs:98-123）、写失败 / rename 失败都清理临时文件、绝不复用旧固定名残留；fs.rs:243-274 模拟「本次临时文件被外部删除 + 旧固定名被塞半截内容」，断言 rename 必须失败而原文保留；fs.rs:313-349 的 4 线程并发写测试保证只留某一次的完整内容。
3. **import-web 是一个真正的事务**：覆盖前字节级 `.bak`（同秒 `-2`/`-3` 序号、备份本身也走原子写，src/backup.rs:296-312）；「校验即将落盘的原文」（validate_envelope 与加载器同类型，src/backup.rs:141-152）保证「导入通过 = 下次启动能读」；多文件失败逆序原子回滚（src/backup.rs:318-342）；回滚失败时绝不谎称「没有存档被改动」（src/backup.rs:355-392 与三个单元测试）。tests/backup_roundtrip.rs 652 行逐场景覆盖（版本跳过、结构拒绝、null 缓存、文章来源分流、Web val 往返、回滚）。
4. **锁策略是白名单且默认加锁**：`is_read_only`（src/lib.rs:88-93）注释明说「宁可多锁一次，也不要漏掉互斥」，新增子命令默认进锁；只读入口（doctor / export-dict / export-web）的代码路径逐一核对后确认不写任何数据文件；跨进程用真实子进程验证（tests/data_dir_lock.rs:53-77），锁文件常驻不删避免了「两把锁」竞态。
5. **只读保护覆盖全部写入口且边界清晰**：`save_dict_now`（src/app/mod.rs:1105）、`save_settings_now`（:1114）、`clear_all_data`（:1165）三处 readonly 拦截齐全；断点缓存的保存 / 删除不受只读影响（有意的，缓存是一次性数据）；练习缓存版本不符时忽略且不移动文件，与 docs/persistence.md 2.5 的表格逐条一致。

## 覆盖声明

- 逐行走查的源码：`src/fs.rs`（全部）、`src/store/mod.rs`（全部）、`src/backup.rs`（全部）、`src/cli.rs`（子命令与导入导出路径）、`src/lib.rs`（全部）、`src/paths.rs`（全部）、`src/bootstrap.rs`（全部）、`src/domain/catalog.rs`（全部）、`src/domain/article.rs`（ArticleCache / ArticleCacheEnvelope 部分）、`src/domain/flow/store.rs`、`src/domain/types.rs`（Word::is_usable、shake_common_dict、PracticeCacheCompact、Statistics 等存档相关部分）、`src/app/mod.rs`（存档相关函数：bootstrap、load_practice_cache、refresh_practice_resume、refresh_article_resume、save_*、clear_*、persist_words、finish_session、finish_article_practice、on_tick、event_loop、save_all）、`src/app/action.rs`（退出 / 确认 / 保存调用点）。
- 一起读的测试：tests/store_roundtrip.rs、tests/backup_roundtrip.rs、tests/data_dir_lock.rs、tests/import_dict.rs、tests/common/mod.rs。
- 未逐行读及原因：`src/logging.rs` / `src/crash.rs`（日志与崩溃报告不属于 5 份存档，仅按 docs/persistence.md §1.5 引用其锁与占名结论）；`src/audio/`、`src/ui/`、`src/theme/`（与数据完整性无关）；tests/ 其余集成文件（home_resume / resume_cache / flows 等断言未逐行读，其行为已通过对应源码核对）；`src/domain/` 其余模块（fsrs / session / study_task 等不落盘）。
- 未运行任何测试：所有定级均由源码走查得出，无需运行时验证；唯一的外部库行为（serde_json 不跳过 BOM）已标注验证思路。
