# typewords-tui v0.4.0 审查汇总（批 1：正确性三维度）

- 审查日期：2026-11-17；复核人：指挥官（lead）
- **范围**：按用户指示，本次只做批 1 的三个维度：panic-audit、data-integrity、fsrs-time。**invariants / test-quality / cross-platform / doc-parity / perf / deps-release 六个维度未审查**（未派遣，见 §4 剩余盲区）。
- **基线（指挥官派遣前亲跑）**：`cargo fmt --all --check` ✅；`cargo clippy --all-targets --locked -- -D warnings` ✅；`cargo test --locked` ✅——599 个测试全部通过（174 单元 + 425 集成，31 个套件，0 失败），构建缓存已预热。
- **分报告**：[panic-audit.md](panic-audit.md) · [data-integrity.md](data-integrity.md) · [fsrs-time.md](fsrs-time.md)（本目录）
- **仓库状态变化**：新增 `reviews/`（4 份报告）；`tests/fsrs_parity.rs` 追加 44 行 P0 复现测试 `extreme_fsrs_parameters_do_not_panic_at_settle`（**当前失败，修复后转绿**；fmt / clippy 仍绿，黄金样本测试仍通过）。除此之外无任何代码改动。

## 1. 计数与总体结论

| 级别 | 数量 |
| --- | --- |
| P0 | 1 |
| P1 | 0 |
| P2 | 3 |
| P3 | 22 |
| **合计** | **26**（原始 27 条；P0 由 panic-audit 与 fsrs-time 独立发现，同一根因合并为 1 条） |

总体印象（三维度一致）：原子写原语、import-web 事务、锁策略、FSRS 黄金样本工程、panic 面的克制（生产代码仅 6 处 unwrap/expect，全部有逻辑保证）质量都很高。问题集中在三类边界：**存档直通通道的值域校验**（P0）、**IO 读错误与解析错误混淆**（P2）、**文档承诺与实现脱节**（P2/P3）。

## 2. P0 摘要（数据丢失风险，置顶）

### 问题 1 [P0] 手改 FSRS 极端参数使结算路径崩溃，当轮进度丢失

**触发路径**（用户可达成）：

1. 退出程序，手工编辑 `%LOCALAPPDATA%\typewords-tui\setting.json`（docs/settings.md 明确支持手改 FSRS 参数），或经 `tw import-web` 导入含极端值的网页备份（[backup.rs:141-148](../src/backup.rs) 的校验只做 serde 类型级检查，任意 f64 / 任意步进字符串均可通过）。
2. 在 `val.fsrsParameters` 写入极端值（两族独立路径）：
   - **路径 A**：`requestRetention: 1e-9` + `maximumInterval: 1e300`（或 0.01 + 1e12 等「极小留存率 + 极大最大间隔」组合）；
   - **路径 B**：`learningSteps: ["999999999999999d"]` 或 `["1m", "1e300d"]`（`step_to_minutes` 按任意 f64 解析，无上限）。
3. 启动练习并结算（[session.rs:754](../src/domain/session.rs)），或词条页批量「认识」（[app/mod.rs:3479](../src/app/mod.rs)）。
4. `Fsrs::new` 只做下界兜底：留存率仅查落在 `(0, 1)`、最大间隔仅查 `>= 1.0`（[fsrs.rs:426-436](../src/domain/fsrs.rs)）、学习步进非空即透传（[:408-412](../src/domain/fsrs.rs)）；`next_interval` 的上界就是 maximumInterval → `now + Duration::days/minutes(天文数字)` 超出 chrono TimeDelta 上界（≈9.2e18 ms）→ **panic：`TimeDelta::days/minutes out of bounds`**（触发点 [fsrs.rs:850/856](../src/domain/fsrs.rs)，同类位点 :549/:554/:559/:617）。

**影响**：panic 发生在 settle 写盘之前——**当轮练习的统计、错词、FSRS 评级全部不落盘（丢当次会话数据）**；同一存档每次结算都复现，形成「改错一次配置就永远打不完一轮」的硬阻塞。存档文件本身不损坏。UI 正常路径安全（留存率夹 50~99%、最大间隔夹 1~36500，[app/mod.rs:4381-4387](../src/app/mod.rs)），洞只在存档直通通道。上游 ts-fsrs 两条路径都不会崩（参数在 generatorParameters / Math.min/max 里夹住；步进用 parseInt + Date 毫秒加法）。

**指挥官复核**：已亲自读完全部触发链（上述每个位置），并亲自运行复现测试——确认失败于 `chrono-0.4.45 lib.rs:717: TimeDelta::days out of bounds`；panic-audit 与 fsrs-time 两个子智能体各自独立实测复现。**结论成立。**

**修复方向**（任选或组合）：`Fsrs::new` 对留存率/最大间隔加上界并回退默认（沿用现有「回退 0.9 / 36500」风格）；`step_to_minutes` 设上限或拒绝超限步进；改用 `Duration::try_days/try_minutes` + 越界回退；或统一 clamp 到 chrono 安全区间再构造。复现测试已落盘，修复即转绿。

### 数据丢失风险次席（问题 2，P2）：瞬时 IO 读错误被当作存档损坏

启动装载瞬间 `dict.json` 被同步盘 / 杀毒软件瞬时锁住（docs/persistence.md §12 明示同步目录是支持场景）→ `read_to_string` 失败与「JSON 解析失败」走同一条 Corrupt 分支（[store/mod.rs:148](../src/store/mod.rs) 读失败 `?` → [:203](../src/store/mod.rs) `Err(err) => return self.corrupt(...)`）：好档被隔离移走、退出时以默认值覆盖；若隔离 rename 也失败（Windows 独占锁），原文件留在原地、下次保存直接整份覆盖——**真正的好档丢失且无隔离副本**。readonly 保护只认 VersionTooNew（[bootstrap.rs:145](../src/bootstrap.rs)）。修复方向：IO 错误（权限 / 共享冲突）不隔离、不覆盖（进终端前 bail 或复用只读保护），隔离严格留给解析失败；建议先写 chmod 000 / 独占锁场景的复现测试。

## 3. 问题总表（26 条，优先级排序）

| # | 级别 | 标题 | 位置 | 来源 | 置信度 | 指挥官复核结论 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | P0 | 手改 FSRS 极端参数使结算路径 panic（chrono Duration 越界），当轮进度丢失 | src/domain/fsrs.rs:850/856（根因 :294-306、:408-412、:426-436；同类 :549/554/559/617） | panic-audit + fsrs-time（同源合并） | 确认 | 亲核全触发链 + 亲跑复现测试失败确认 |
| 2 | P2 | 瞬时 IO 读错误与解析失败同走损坏隔离：好档被移走或整份覆盖 | src/store/mod.rs:148→203、:181-187；src/bootstrap.rs:145 | data-integrity | 确认 | 亲核代码链；同步目录为文档 §12 支持场景 |
| 3 | P2 | tw export-web 在存档不可解析时整体失败，persistence.md:315 承诺的「空信封占位」未实现 | src/backup.rs:543-553、:396-400；docs/persistence.md:315 | data-integrity | 确认 | 亲核（含调用点与文档原文） |
| 4 | P2 | w 长度≠21 时 clip_parameters 与 ts-fsrs 5.4.1 migrateParameters 行为不一致；settings.md:196 描述失实 | src/domain/fsrs.rs:219-240 | fsrs-time | 确认 | Rust 侧亲核；上游侧经子智能体对 5.4.1 bundle 逐段对照 |
| 5 | P3 | export-web --out 用非原子 std::fs::write，与 export-dict 的 write_atomic 不一致 | src/cli.rs:277 vs :333 | data-integrity | 确认 | 亲核 |
| 6 | P3 | UTF-8 BOM 一律判解析失败：存档被隔离重置、导入被拒收（Windows 外部编辑常见副产品） | src/store/mod.rs:148-154；src/domain/catalog.rs:375-378；src/backup.rs:84-87 | data-integrity | 确认 | 抽查通过 |
| 7 | P3 | import-web 无提示覆盖「版本过高只读档」（有 .bak 可恢复） | src/backup.rs:258-293 | data-integrity | 确认 | 抽查通过 |
| 8 | P3 | import-dict 对文件名「.json」（空 stem）生成空 id 孤儿目录项，假成功 | src/domain/catalog.rs:389-443 | data-integrity | 确认 | 抽查通过 |
| 9 | P3 | 导入 / 备份读入无大小上限，超大文件整读进内存（OOM 被杀，无数据损坏） | src/domain/catalog.rs:375、src/backup.rs:84、src/fs.rs:58 | data-integrity | 确认 | 抽查通过 |
| 10 | P3 | tw task（预览命令）有未披露写副作用：可能删除 practice-word.json | src/bootstrap.rs:68-76；src/cli.rs:182 | data-integrity | 确认 | 抽查通过 |
| 11 | P3 | quarantine 隔离名同秒冲突时静默覆盖既有隔离副本（无唯一化） | src/store/mod.rs:126-136 | data-integrity | 确认 | 亲核 |
| 12 | P3 | 边角合集：缓存隔离 Toast 吞细节、import-web 静默忽略 PracticeSaveArticle.val、version 类型异常文案误导 | src/store/mod.rs:243-277；src/backup.rs:500-502、:92-96 | data-integrity | 确认 | 抽查通过 |
| 13 | P3 | 目录项 url / language 无单段校验，手工编辑可让词条读写逃出 <home>/dicts | src/domain/catalog.rs:178-186、:222-228 | data-integrity | 确认 | 抽查通过 |
| 14 | P3 | rename 后未 fsync 父目录：断电可能丢「本次保存」（仍是完整旧档，无半截，文档承诺成立） | src/fs.rs:68-92 | data-integrity | 确认 | 抽查通过 |
| 15 | P3 | 自测题相似度 i64 乘法在导入词库极端 frequency 下溢出（release 静默回绕） | src/domain/question.rs:67 | panic-audit | 确认 | 亲核 |
| 16 | P3 | 每日词数×复习比 i64 乘法可被手改 wordReviewRatio 撑溢（release 静默回绕，任务数量算错） | src/domain/study_task.rs:42、:140 | panic-audit | 确认 | 亲核 |
| 17 | P3 | request_retention==1.0 时 Rust 回退 0.9、上游接受（有意偏差未登记为已知差异） | src/domain/fsrs.rs:426-431 | fsrs-time | 确认 | 亲核 |
| 18 | P3 | 设置页主列表与高级 FSRS 页留存率显示口径不一（原始值 vs 生效值，可显示 9500%） | src/app/mod.rs:4343-4345 vs :4253-4260 | fsrs-time | 确认 | 抽查通过 |
| 19 | P3 | 数字行空输入静默按 0 夹取保存，与权重行「请输入数字」拒绝行为不一致 | src/app/action.rs:568-569 | fsrs-time | 确认 | 抽查通过 |
| 20 | P3 | 权重输入允许 >6 位小数，显示/预填按 6 位舍入，重开编辑框再保存静默改值 | src/app/mod.rs:473-485；src/app/action.rs:325-329 | fsrs-time | 确认 | 抽查通过 |
| 21 | P3 | is_due_today 双实现，scheduler.rs 公共版无调用者（到期口径漂移风险） | src/domain/scheduler.rs:50 vs src/domain/study_task.rs:75 | fsrs-time | 确认 | 亲核（grep 确认公共版无调用者） |
| 22 | P3 | tw task 固定 seed 42 预览与真实会话顺序可能不同；随机 seed 不落日志，帮助页「附 --seed」建议无法执行 | src/cli.rs:184；src/app/mod.rs:875；src/ui/help.rs:212 | fsrs-time | 确认 | 抽查通过 |
| 23 | P3 | 文档三处失实：步进「写死 1/6/10 分钟」、w 长度回退描述、upstream-parity §3.1 与 §4.3 自相矛盾；另上游声明 ts-fsrs ^5.2.3 与 TUI 基线 5.4.1 存在版本漂移 | docs/settings.md:132/194/196；docs/upstream-parity.md:69/162；target/upstream/package.json:49 | fsrs-time | 确认 | package.json 声明已亲核；「Web 端实际安装版本」无法本地验证（快照无 lockfile），保留为开放风险，不影响定级 |
| 24 | P3 | 步进字符串解析与上游 parseInt 语义不同（"10 m"/"1.5m"/非法步进）：上游抛错，Rust 静默毕业 | src/domain/fsrs.rs:294-306、:332-341 | fsrs-time | 确认 | 抽查通过 |
| 25 | P3 | 按天统计在 DST 午夜时区 / 超范围时间戳下容错不对称（中国时区不受影响） | src/domain/statistics.rs:24-80；src/domain/timer.rs:30-38 | fsrs-time | 确认（代码级） | 抽查通过；DST 运行时验证思路保留（TZ=America/Santiago） |
| 26 | P3 | 死代码：FsrsParameters::effective_w 与 rng::pick_index 无任何调用者 | src/domain/types.rs:489-496；src/domain/rng.rs:28-33 | fsrs-time | 确认 | 抽查通过 |

表注：「亲核」= 指挥官亲自读过该条引用的代码；「抽查通过」= 质检该报告时抽查的 2-6 处引用全部属实，本条引用未逐行重读。两个残留验证点均已在复核中处理（问题 23 的上游实际版本、问题 25 的 DST 运行时，见表内与 §4）。

## 4. 剩余盲区（各维度覆盖声明汇总）

**六个维度完全未审**（用户指示本次只做批 1）：

- invariants：Action→apply_action 路由完备性、ui 只读、layout()/targets() 同源、硬编码颜色、layering.rs 守卫盲区
- test-quality：模块覆盖地图、浅验证、脆弱测试、快照覆盖缺口
- cross-platform：audio 三平台分支（含 0.4.0 刚修的 PowerShell 拼参同类残留）、中文路径 / UNC / conhost
- doc-parity：文档双向漂移系统扫描（本次仅由 fsrs-time / data-integrity 顺带发现 4 处失实：问题 3、10、23 及 settings.md:196）
- perf：大词库载入 / 滚动 / 搜索 / 每帧分配、日志清理时钟回拨、resize 风暴
- deps-release：依赖最小性、GPL 兼容、release.yml 守卫、五平台矩阵

批 1 三维度各自声明的盲区：

- panic-audit：app/mod.rs 非 panic 相关区域（约 4000 行的大部分）与 ui 各页面纯渲染分支未逐行读（经 unwrap/expect/Duration/索引/下溢/unreachable 全模式检索覆盖）；tests/ 目录不在维度范围。
- data-integrity：logging / crash / audio / ui / theme 未逐行（与存档完整性无关）；其余集成测试未逐行读断言。
- fsrs-time：flow / wordbook / question / article / ui 其余页面 / audio 未看（其它维度职责）；上游 Web 端实际安装的 ts-fsrs 版本无法确认。

## 5. 修复批次建议（同模块 / 同根因合并 PR）

| 批次 | 主题 | 涉及问题 | 说明 |
| --- | --- | --- | --- |
| A | FSRS 参数值域与 Duration 安全（P0） | 1 | **复现测试已落盘且当前失败**，修复即转绿；改动集中在 fsrs.rs（Fsrs::new 校验 + step_to_minutes + Duration 构造），单 PR |
| B | 存档读取 / 损坏隔离 | 2、6、11 | 同根因（IO 与解析错误混淆 + 隔离行为），集中在 store/mod.rs；**问题 2 先写复现测试**（chmod 000 / 独占锁场景）再改 |
| C | 导入导出一致性 | 3、5、7、8、9、12 | backup.rs / cli.rs 导入导出族合并一个 PR |
| D | FSRS 边界与显示一致性 | 4、17、18、19、20、24 | fsrs.rs + 高级 FSRS 页；问题 4（migrateParameters 移植）改动面稍大，可单拆 |
| E | 小修合集 | 10、13、14、15、16、21、22、25、26 | 独立小改动，按顺手合并 |
| 文档 | 随对应代码批次同步 | 3（persistence.md）、4/23（settings.md、upstream-parity.md）、10（usage.md） | 全部记入 CHANGELOG [Unreleased] |

## 6. 发布 0.4.1 前的最小验收清单

1. `cargo fmt --all --check` / `cargo clippy --all-targets --locked -- -D warnings` / `cargo test --locked` 全绿——含复现测试 `extreme_fsrs_parameters_do_not_panic_at_settle` 转绿。
2. 手改 setting.json 极端 FSRS 值（路径 A / B 两族）→ 结算不 panic，参数回退默认并有 Toast / 日志提示。
3. w 长度 17 / 19 / 22 的存档启动与调度行为符合选定策略（若移植 migrateParameters，黄金样本补非 21 长度用例）。
4. 坏 JSON（BOM / 半截）与「启动瞬间被锁」两种场景：好档数据可恢复、不发生默认值整份覆盖（问题 2 / 6 验收）。
5. dict.json 损坏时 tw export-web 按选定方向工作（空信封占位或明确报错指引）（问题 3 验收）。
6. CHANGELOG [Unreleased] 记录全部用户可见修复；受影响文档（persistence / settings / usage / upstream-parity）同步更新。
