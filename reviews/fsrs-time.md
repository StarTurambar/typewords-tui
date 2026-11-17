# fsrs-time：FSRS 调度 / 参数值域 / 按天统计 / 随机种子专项审查

- 版本：typewords-tui v0.4.0（审查时工作区状态，基线 fmt / clippy / 全量测试全绿）
- 审查者：fsrs-time 子任务
- 判定标准：CLAUDE.md 项目不变量（状态只在 app 层改；domain 不依赖 app/ui/store/theme；运行期可恢复错误转 Toast、不用 panic 作控制流；测试不 sleep、时间用注入的 now）
- 方法：逐模块走查源码与相关测试；对上游用 unpkg 拉取 ts-fsrs **5.4.1** 与 **5.2.3** 的 `dist/index.mjs` 逐段对照（算法层 / 状态机 / 学习步进 / fuzz / Alea / 参数夹取）；疑点用临时集成测试 `cargo test --locked --test fsrs_time_scratch` 做运行时验证（3 轮，验证后已删除临时文件）；为确认的 P0 按规则在 `tests/fsrs_parity.rs` 留下一个最小复现测试（当前失败，修复后转绿）。未跑全量测试。

## 0. 总览

| 级别 | 计数 |
| --- | --- |
| P0 | 1（chrono Duration 越界 panic，与 reviews/panic-audit.md 问题 1 同源，两条触发路径均已实测复现） |
| P1 | 0 |
| P2 | 1（w 长度 ≠ 21 时与 ts-fsrs 5.4.1 的迁移/回退行为不一致） |
| P3 | 10 |

黄金样本结论：**在样本覆盖的参数空间内（默认 21 权重、默认步进、0.9 留存率、36500 最大间隔、fuzz/短期调度开关），Rust 移植与 ts-fsrs 5.4.1 逐公式一致**——我从上游 bundle 逐段核对了 seed 派生、Alea/Mash（含 UTF-16 码元与 ToInt32/ToUint32 语义）、computeDecayFactor / interval_modifier、forgetting_curve、next_state 全部子公式（含难度推进的两处 roundTo(_, 8)）、next_interval / apply_fuzz / get_fuzz_range、BasicScheduler / LongTermScheduler 状态机（含 hard ≤ good < easy 排序、Review+Again 走 relearning_steps、毕业与 ≥1440 分钟分支）、dateDiffInDays 的 UTC 口径，全部对上；`tests/fsrs_parity.rs` 的逐字段比对（due 精确、数值 1e-7）也通过。但样本只覆盖默认参数（见 P2-1 与 P3-1/P3-2 的未覆盖差异），且上游 package.json 声明 `ts-fsrs ^5.2.3`（本地快照无 lockfile，无法确认 Web 端实际安装版本；5.2.3 与 5.4.1 算法确有差异，见 P3-9）。

## 1. 问题清单

### 问题 1 [P0] 手改 FSRS 参数的极端值使调度器在结算路径上 panic（chrono Duration 越界）

- **位置**：src/domain/fsrs.rs:850 与 src/domain/fsrs.rs:856（`apply_learning_steps`）；同类位点 src/domain/fsrs.rs:549/554/559（`review_state`）、src/domain/fsrs.rs:617（`long_term_state`）。根因之一在 src/domain/fsrs.rs:294-306（`step_to_minutes` 接受任意 f64）与 src/domain/fsrs.rs:426-436（留存率/最大间隔只做下界兜底，不设上限）。
- **证据**（真实代码）：
  ```rust
  // src/domain/fsrs.rs:848-851（步进 ≥ 1 天的分支）
  if scheduled_minutes >= MINUTES_PER_DAY {
      next.learning_steps = next_step as f64;
      next.due = now + Duration::minutes(js_round(scheduled_minutes) as i64);
  ```
  ```rust
  // src/domain/fsrs.rs:854-856（毕业分支）
  let interval = self.next_interval(next.stability, current.elapsed_days, seed);
  next.scheduled_days = interval;
  next.due = now + Duration::days(interval as i64);
  ```
  实测（`cargo test --locked --test fsrs_time_scratch`，两轮）：
  - 路径 A：`request_retention: 1e-9` + `maximum_interval: 1e300`，新卡评 Easy（或复习卡评 Good）→ `thread panicked at chrono-0.4.45\src\lib.rs:717: TimeDelta::days out of bounds`；
  - 路径 B：`learning_steps: ["999999999999999d"]` + Again、或 `["1m", "1e300d"]` + Good → `TimeDelta::minutes out of bounds`。
  复现测试已按规则落在 `tests/fsrs_parity.rs` 的 `extreme_fsrs_parameters_do_not_panic_at_settle`（两条路径合一，当前失败）。
- **触发条件**：退出程序 → 手改 `setting.json` 的 `val.fsrsParameters`（docs/settings.md §「仍需手改的 FSRS 参数」明确支持手改 `learning_steps`；留存率/最大间隔同理）写入极端值（步进 ≥ ~1.07e11 天，或科学计数法如 `"1e300d"`；留存率极小 + 最大间隔极大）→ 启动练习 → 结算（`session.rs:754 next_card` 或批量标记 `app/mod.rs:3479`）→ panic。评级到达路径：首步极大时需 Again（错 ≥7 次）或 Hard（错 4~6 次）；次步极大时 Good（错 1~3 次）即触发；留存率路径 Easy/复习 Good 直接触发。
- **违反的不变量**：「不用 panic 作控制流」「运行期可恢复错误转 Toast」；也打破「与上游一致」——ts-fsrs 两条路径都不会崩：留存率/间隔在 `generatorParameters` 与 `Math.min/Math.max` 里被夹住，步进用 `parseInt`（`"1e300"` 解析成 1）加 `Date` 毫秒加法，溢出只会得到 Invalid Date。
- **影响**：结算路径崩溃（crash-*.txt 崩溃报告），当轮练习进度在落盘前丢失；存档本身不受影响。
- **建议方向**：`step_to_minutes` 给结果设上限（如 ≤ 36500 天）或拒绝超限步进；`next_interval` 结果与步进分钟数在构造 Duration 前统一 clamp 到 chrono 安全区间（≈1e11 天）；或改用 `Duration::try_days/try_minutes` 并在越界时回退最大间隔。
- **置信度**：确认（两条路径均已运行复现；与 reviews/panic-audit.md 问题 1 交叉一致——该报告另列了 549/554/559/617 同类位点）。

### 问题 2 [P2] w 长度 ≠ 21 时与 ts-fsrs 5.4.1 的迁移/回退行为不一致，「同一张卡两端相同」在非 21 长度下不成立

- **位置**：src/domain/fsrs.rs:219-240（`clip_parameters`）。
- **证据**（真实代码）：
  ```rust
  // src/domain/fsrs.rs:225-239：取前 min(len,21) 项夹取，不足 21 项用默认值补齐
  let count = parameters.len().min(clip.len());
  ...
  // 不足 21 项时用默认值补齐
  let defaults = default_w();
  result.extend(defaults.iter().skip(count).copied());
  ```
  上游 ts-fsrs 5.4.1 `migrateParameters`（unpkg dist/index.mjs 606-639 行）按长度分流：19 → `clipParameters(...).concat([0, FSRS5_DEFAULT_DECAY])`（即 w[19]=0、w[20]=0.5）；17 → 还要变换 w[4]/w[5]/w[6] 再补 `[0,0,0,0.5]`；**其它长度（含 >21）→ 整组回退 default_w**。Rust 对 19 长度补的是 FSRS-6 默认 w[19]=0.0658、w[20]=0.1542（而非 0/0.5）；对 22 长度保留前 21（而非整组默认）。
- **触发条件**：手改 `setting.json` 的 `fsrsParameters.w` 长度为 17/19/22+，或 `tw import-web` 导入带 FSRS-4/5 时代 17/19 权重的网页版存档（上游 TypeWords 现用 FSRS-6 21 权重，docs/upstream-parity.md L75，因此 17/19 只来自更早的存档或手改）。
- **违反的不变量**：fsrs.rs 模块头与 docs/upstream-parity.md §3.1 声明的「同一张卡在两端得到相同的下次复习时间」。
- **影响**：非 21 长度 w 下，TUI 与 Web 对同一份存档算出的间隔不同；同时 docs/settings.md L196「`w` 长度不对或不是 21 个有限数会静默回退默认权重」与代码不符（22 长度保留前 21、19/17 只补默认不整组回退；「不是有限数」一半不可达——serde_json 拒绝 NaN/Infinity 字面量与 1e999 溢出字面量，已实测）。黄金样本全部用 21 长度 w，测不出该差异。
- **建议方向**：按上游移植 `migrateParameters` 的长度分支（19/17 迁移、其它整组默认），或在文档明确只支持 21 并在装载时整组回退 + Toast 提示；同步修 docs/settings.md L196。
- **置信度**：确认（两侧源码逐行对照；上游 bundle 已读）。

### 问题 3 [P3] `request_retention == 1.0` 时 Rust 回退 0.9，ts-fsrs 接受 1.0（intervalModifier=0）

- **位置**：src/domain/fsrs.rs:426-431。
- **证据**：`if parameters.request_retention > 0.0 && parameters.request_retention < 1.0 { ... } else { 0.9 }`（代码注释自述为有意为之：「取 1 时 interval_modifier 恒为 0，所有复习卡都会被排到次日」）。上游 `calculate_interval_modifier`（bundle 716-724 行）的校验是 `<= 0 || > 1` 才抛错——1.0 合法，intervalModifier=0、间隔恒 1 天。
- **触发条件**：手改 `setting.json` 写 `request_retention: 1`（UI 夹取 50~99%，不可达；serde 反序列化可读入 1.0）。
- **违反的不变量**：与上游「逐条一致」的边界差异（黄金样本不含非 0.9 留存率）。
- **影响**：边界值行为与上游不同（Rust 更稳健，但两端不一致）；设置页主列表行显示原始值 9500% 式异常（见问题 4）。
- **建议方向**：在 docs/upstream-parity.md 或 fsrs.rs 注释里把这处「有意偏差」登记成已知差异清单，避免后续对表时误判。
- **置信度**：确认（两侧源码对照）。

### 问题 4 [P3] 同一个留存率在设置页主列表与高级 FSRS 页显示口径不一致（原始值 vs 生效值）

- **位置**：src/app/mod.rs:4343-4345（主列表行显示原始值 ×100）与 src/app/mod.rs:4253-4260（高级页显示生效值）；对照 src/app/mod.rs:4349（最大间隔行用生效值）。
- **证据**：`EditTarget::Retention => (settings.fsrs_parameters.request_retention * 100.0).round() as i64`（原始值）；而 `fsrs_request_retention()` 对 `(0,1)` 之外的值回退 0.9 后再显示；`EditTarget::MaximumInterval => self.fsrs_maximum_interval().round()`（生效值）。手改 `request_retention: 95` 时主列表显示「9500%」、高级页显示「90%」；`maximum_interval: 0` 时两行都显示 36500。
- **触发条件**：手改 `setting.json` 写出 `(0,1)` 之外的留存率后进设置页。
- **违反的不变量**：同一状态在两处 UI 的展示一致性（fsrs_request_retention 的文档注释自己主张「界面显示生效值，避免三处不一致」，但主列表行没走它）。
- **影响**：仅显示混乱，不落盘坏值（编辑保存仍按 50~99 夹取）。
- **建议方向**：主列表 Retention 行改用 `fsrs_request_retention() * 100`。
- **置信度**：确认。

### 问题 5 [P3] 数字行空输入静默取 0 再夹取，与权重行的「请输入数字」拒绝行为不一致

- **位置**：src/app/action.rs:568-569（`edit_save` 的整数路径 `parse::<i64>().unwrap_or_default()`）与 src/app/mod.rs:4287-4295（权重路径显式拒绝非有限数）。
- **证据**：留存率/最大间隔行清空后直接 Enter → `parse("")` 失败 → 0 → `clamp(50,99)` / `clamp(1,36500)` 静默保存 50% / 1 天；权重行同样的输入会 Toast「请输入数字：如 0.212」并保留原值（tests/adv_fsrs.rs 的 `invalid_weight_input_keeps_the_previous_value` 锁定了这个语义，注释还写明「直接 unwrap_or_default() 会把空输入静默写成 0」——但整数行正是这么做的）。
- **触发条件**：高级 FSRS 页「目标留存率」/「最大间隔」行清空输入框后 Enter。
- **违反的不变量**：同类输入的校验一致性。
- **影响**：一次误操作把最大间隔改成 1 天（下次结算全部卡片次日到期），无提示。
- **建议方向**：整数路径对解析失败同样拒绝并提示（或至少对空串跳过保存）。
- **置信度**：确认。

### 问题 6 [P3] 权重输入允许 >6 位小数，但显示/预填按 6 位舍入，重开编辑框再保存会静默改值

- **位置**：src/app/mod.rs:473-485（`weight_text`：`format!("{value:.6}")` + 去尾零）与 src/app/action.rs:325-329（输入框允许 ≤12 字符的数字串）。
- **证据**：输入 `0.1234567`（9 字符，被接受）→ 夹取后按全精度落盘 → 行显示与再次编辑预填都是 `0.123457` → 直接 Enter 保存就把存储值改成 0.123457。数值影响可忽略，但「打开再保存」会悄悄改参数，与 `weight_text` 自己注释的「两处格式不一致时……会把精度悄悄改掉」的初衷相悖。
- **触发条件**：高级 FSRS 页任一权重行输入第 7 位小数后，再次进入该行编辑并保存。
- **违反的不变量**：设置往返一致性（读入-显示-写回不应改变值）。
- **建议方向**：输入时按 6 位小数截断/拒绝，或保存时按 `weight_text` 归一。
- **置信度**：确认。

### 问题 7 [P3] `is_due_today` 存在两份相同实现，公共那份无生产调用者

- **位置**：src/domain/scheduler.rs:50-52（`pub fn is_due_today`，无调用者）与 src/domain/study_task.rs:75-77（私有同名函数，唯一被用的那份，study_task.rs:149）。
- **证据**：两份实现逐字符相同（`card.due.with_timezone(&chrono::Local).date_naive() <= today`）；grep 全仓只有 study_task 的私有版本被调用。
- **触发条件**：无直接触发；属于漂移风险——本地时区午夜边界是到期判断的核心口径，改其中一份不会被告警。
- **违反的不变量**：CLAUDE.md「同一份布局同时服务绘制与命中」式的单一来源精神（此处是日期口径）。
- **影响**：维护风险。
- **建议方向**：删除 scheduler.rs 的公共版本或让 study_task 复用它。
- **置信度**：确认。

### 问题 8 [P3] `tw task` 预览固定 seed 42，与实际练习会话的随机顺序可能不同；随机种子不落日志，帮助页建议无法执行

- **位置**：src/cli.rs:184（`rng::seeded(cli.seed.unwrap_or(42))`）、src/app/mod.rs:875（`cli.seed.unwrap_or_else(rng::random_seed)`，`pub seed` 存下但从未输出）、src/ui/help.rs:212（「随机行为请附 --seed 值」）。
- **证据**：`tw task` 不带 `--seed` 时用 42，开了「无到期随机补复习」时预览列表与真实会话（随机 seed）的抽取顺序可以不同；未传 `--seed` 的 TUI 运行里随机 seed 既不显示也不进 `logs/`，用户无法按帮助页建议附上种子。
- **触发条件**：`tw task`（不带 --seed）预览 vs 随后真实练习；或随机行为报障时。
- **违反的不变量**：rng.rs 的可复现设计在「未显式给 seed」的场景断了线索。
- **影响**：预览与实际不一致的困惑；报障信息缺失。
- **建议方向**：启动时把实际 seed 写一行 `logging::info`；`tw task` 预览改用随机 seed 或文档注明「预览顺序固定」。
- **置信度**：确认（行为）；「预览应随机」是否为产品意图待定（可能是有意的确定性预览，建议在 docs/usage.md 注明）。

### 问题 9 [P3] 文档与代码三处失实：1/6/10 分钟步进的来源、「w 长度不对回退默认」、upstream-parity 自相矛盾 + ts-fsrs 版本漂移

- **位置**：docs/settings.md:132 与 docs/settings.md:194（「学习 / 重学步进由 src/domain/scheduler.rs 写死为 1 / 6 / 10 分钟」）、docs/settings.md:196（w 长度回退，见问题 2）、docs/upstream-parity.md:69 vs docs/upstream-parity.md:162。
- **证据**：scheduler.rs 全文没有步进常量；1/10 分钟来自 `fsrsParameters.learning_steps = ["1m","10m"]`（fsrs.rs `learning_step_info`），Hard 的 6 分钟是 `js_round((1+10)/2)` 算出来的——手改步进后这些值会变，文档说「写死」失实。upstream-parity.md L69 说「同一张卡在 TUI 与 Web 上算出的下次复习时间相同」，L162 又说「两端调度实现不同，『同一张卡』的后续间隔不可复现」，两句直接矛盾（应以后者限定的旧语境澄清或删改）。另外上游 package.json 声明 `ts-fsrs ^5.2.3`（本地快照 target/upstream/package.json:49，无 lockfile），而 5.2.3 与 5.4.1 的 `next_state`（5.2.3 无 r 参数、t==0 无短期门控）、scheduler 内联公式确有差异（已 diff 两个版本的 bundle）——Web 端实际安装版本取决于其 lockfile，无法从本仓库确认。
- **触发条件**：读者按文档手改步进后预期 1/6/10 不变；或按 L69/L162 任意一句做迁移决策。
- **违反的不变量**：CLAUDE.md「文档与代码不一致时以代码为准，但要同步修文档」。
- **影响**：文档误导。
- **建议方向**：改 settings.md 两处描述（步进来自 fsrsParameters.learning_steps）；upstream-parity.md 统一 §3.1 与 §4.3 的表述，并登记「上游 ts-fsrs 声明 ^5.2.3，TUI 基线 5.4.1」的版本漂移风险。
- **置信度**：确认（文档失实）；版本漂移待验证（思路：在上游仓库查 pnpm-lock.yaml 里 ts-fsrs 的解析版本）。

### 问题 10 [P3] 步进字符串解析与上游 \`ConvertStepUnitToMinutes\` 的语义差异（含非法步进时上游抛错、Rust 静默毕业）

- **位置**：src/domain/fsrs.rs:294-306（`step_to_minutes`）与 src/domain/fsrs.rs:332-341（`learning_step_info` 对 None 的处理）。
- **证据**：上游用 `parseInt(step.slice(0,-1), 10)`：`"10 m"` → parseInt("10 ")=10（10 分钟）、`"1.5m"` → 1、非法单位/负数直接 `throw FSRSValidationError`；Rust 用 `f64::parse`：`"10 m"` → None（尾随空格）、`"1.5m"` → 1.5、`"0x10"` → None（上游 parseInt 按十六进制得 16）。None 在 `learning_step_info` 里经 `?` 直接让整次步进计算返回 None，`apply_learning_steps` 落到毕业分支（≥1 天 FSRS 间隔）——非法步进不是报错而是把卡片立即毕业，行为差异远大于「差几分钟」。
- **触发条件**：手改 `setting.json` 的 learning_steps 写入带空格 / 小数 / 非法单位的步进。
- **违反的不变量**：与上游逐条一致（黄金样本只用 "1m"/"10m"，测不出）。
- **影响**：手改后调度语义与 Web 端不同（上游报错可见，Rust 静默改变调度）。
- **建议方向**：解析失败时 Toast 提示并回退默认步进，而不是静默毕业；或按上游语义解析（截断小数、允许尾随空格）。
- **置信度**：确认（两侧源码对照；"1分"/"10 m"/"" 的 None 行为有 fsrs.rs 单测 `non_ascii_step_units_do_not_panic` 佐证）。

### 问题 11 [P3] 按天统计的两个时区边界容错只在极端时间戳/DST 午夜时区触发，且行为不对称

- **位置**：src/domain/statistics.rs:24-30 与 54-59（`.single()` 失败即 `continue` 跳过记录）、src/domain/statistics.rs:73-80（`day_key` 失败回退 `Local::now()`）、src/domain/timer.rs:30-38（`split_segment_by_day` 的 `.single()` 失败回退 `end`，跨天片段不再拆分）。
- **证据**：`Local.timestamp_millis_opt(ms)` 从毫秒时间戳换算只在超范围（约 ±262000 年）时失败，因此 daily_totals 的「跳过」与 day_key 的「记到今天」实际只影响手改存档里的荒谬时间戳（如 9e18）；但 `split_segment_by_day` 用 `from_local_datetime(明天零点).single()`，在 DST 跳过/重复午夜的时区（如美洲部分地区）会失败并回退 `end`——跨那一个午夜的片段整段记到开始日，且 roles start/middle/end 少一天。
- **触发条件**：手改 dict.json 的 statistics.start_date 为超范围值；或在 DST 午夜时区（中国不受影响）跨夏令时边界练习。
- **违反的不变量**：按天聚合的本地时区午夜边界一致性（同一会话在边界两侧的归属）。
- **影响**：极端输入/特殊时区下单日统计错位；不崩溃、不丢总计。
- **建议方向**：超范围时间戳统一策略（跳过 + 一次性 Toast，或 clamp 到合法端点）；DST 午夜失败时改用下一秒可表示时刻继续拆分。
- **置信度**：确认（代码路径推演；DST 行为未做运行时验证——思路：设 `TZ=America/Santiago` 构造跨 9 月 DST 边界的片段跑 `normalize_practice_timer`）。

### 问题 12 [P3] 死代码：`FsrsParameters::effective_w` 与 `rng::pick_index` 无任何调用者

- **位置**：src/domain/types.rs:489-496、src/domain/rng.rs:28-33。
- **证据**：全仓 grep 只有定义处（`effective_w` 的 f32 转换还悄悄降精度，一旦被将来误用会引入与 f64 调度器不一致的第二套权重口径）。
- **触发条件**：无（死代码）。
- **违反的不变量**：CLAUDE.md 保持轻量/单一口径的精神。
- **影响**：误导后续维护者。
- **建议方向**：删除（或给 `effective_w` 标注仅供展示并统一回 f64）。
- **置信度**：确认。

## 2. 值域校验专项结论（任务要求逐项）

设置页 / 高级 FSRS 页放开的参数，逐项核对「0 除、NaN、负数、溢出、往返」：

| 参数 | UI 值域 | 0 除 | NaN | 负数 | 溢出 | setting.json 往返 |
| --- | --- | --- | --- | --- | --- | --- |
| 留存率 | 50~99 整数（action.rs:330 只收数字、≤6 位） | 无（interval_modifier 分母 factor≠0，decay∈[-0.8,-0.1]） | UI 不可达；JSON 字面量被 serde_json 拒绝（实测 NaN/Infinity/1e999 均报错） | UI 夹取；手改负值/0 → Fsrs::new 回退 0.9 | UI ≤999999；手改极小值 → **P0 路径 A** | ✔（tests/adv_fsrs.rs FileStore 重载 + 原始 JSON 断言） |
| 最大间隔 | 1~36500 整数 | 无 | 同上 | UI 夹取；手改 <1 → 回退 36500（上游保留真值，P3 级差异） | 手改 1e300 → **P0 路径 A** | ✔ |
| fuzz / 短期调度 | 布尔开关 | 无 | 不可达 | 不可达 | 不可达 | ✔（开关落盘有测试） |
| 21 权重 | 每项 weight_bounds 与调度器同源夹取 | w17*w18 无除法 | `apply_weight_text` 显式过滤非有限数（fsrs.rs:4291） | 区间下限全部 ≥0，clamp 生效 | 输入 ≤12 字符；JSON 拒绝溢出字面量 | ✔（P2：长度 ≠21 时行为与上游不一致；P3：>6 位小数精度） |
| learning_steps / relearning_steps | 无 UI（文档支持手改） | 无 | 同上 | `step_to_minutes` 拒绝负数 | **P0 路径 B** | 字符串原样透传 |

算法内部的 0 除 / NaN 兜底实测良好：自定义 `clamp`（fsrs.rs:49-51，用 Rust `f64::max/min`——对 NaN 取另一侧，与 JS `Math.max` 的 NaN 传播不同但更稳健）把 stability=0、负、1e308、1e-9、未来 last_review（elapsed 为负）等异常卡片全部 sanitize 到 [S_MIN, S_MAX]，卡片 JSON 往返完好（临时测试 28 个组合全部通过）。注意这是与上游的有意（且良性）差异：JS `Math.max(NaN, x)` 返回 NaN，上游对这类坏卡会产出 NaN 卡。

## 3. 做得好的地方

1. **黄金样本工程扎实且可再生**：`tests/fsrs_parity.rs` 逐字段比对（due 精确相等、数值 1e-7、reps/lapses/state/last_review 全查），`tools/fsrs_golden.mjs` 从上游 node_modules 直接生成、不改上游仓库；我逐公式对照 ts-fsrs 5.4.1 bundle，难度推进的两处 `roundTo(_,8)`（fsrs.rs:710-718，注释还解释了少一处会逐次累积 ~1e-9 误差）、Alea 的 UTF-16 码元与 ToInt32/ToUint32 语义（fsrs.rs:54-72、132-145）这些最容易漏的细节都对上了。
2. **界面区间与调度器同源**：`weight_bounds` 与 `Fsrs::new` 的夹取共用 `clamp_ranges`（含 relearning_steps>1 时收紧 w17/w18、enable_short_term 决定 w[19] 下限），有 `weight_bounds_match_the_clipping_ranges`、`weight_bounds_tighten_w17_w18_with_two_relearning_steps` 锁定，「显示的区间就是生效的区间」。
3. **异常卡片的数值兜底**：fsrs.rs:49-51 的自定义 clamp 借 Rust `f64::max/min` 忽略 NaN 的特性，把 0/负/溢出 stability 全部 sanitize 到 [S_MIN, S_MAX]（实测确认），避免了坏卡把 NaN 写进 dict.json 再被隔离的连锁；serde_json 对 NaN/Infinity/1e999 字面量的拒绝又把入口堵住。
4. **跨天统计的口径与测试**：`normalize_practice_timer` 先排序合并再按本地午夜拆分（timer.rs:25-49），结算按 start/middle/end 角色去重计数（statistics.rs:94-153），`cross_day_session_counts_are_not_duplicated` 锁定「计数只归开始日、时长逐日累计」，与 Web 的 flushStatToStore 对齐（upstream-parity §3.5）。
5. **随机源设计**：`StdRng::seed_from_u64` 跨平台确定，Fisher–Yates 实现正确（rng.rs:17-25），`--seed` 贯穿 CLI、Fixture（默认 42）与 examples，`fuzz` 的种子派生自卡片状态使同一张卡四次评级共享同一随机因子（与上游 DefaultInitSeedStrategy 一致）。

## 4. 覆盖声明

**逐行走查**：src/domain/fsrs.rs（全 1140 行，含内嵌测试）、rng.rs、statistics.rs（含测试）、timer.rs、scheduler.rs（含测试）、study_task.rs、session.rs（settle/create_* 与恢复段）、types.rs（Card/FsrsParameters/SettingState/Statistics/PracticeState 段）、constants.rs；src/app/mod.rs（AdvFsrsRow/activate_adv_fsrs/apply_weight_text/apply_number_value/setting_number_value/fsrs_request_retention/save_settings_now/bootstrap/on_tick 段）、src/app/action.rs（EditChar/edit_save/confirm 段）、src/cli.rs（task_preview/import_web 段）；src/ui/adv_fsrs.rs（全文）、settings.rs（数值显示段）、insights.rs、stats.rs（日聚合展示段）；src/store/mod.rs（版本/损坏隔离/写入）、src/backup.rs（导入校验段）；tests/fsrs_parity.rs、tests/adv_fsrs.rs（全文）、tests/fixtures/fsrs_golden.json（5 场景参数与逐步结果）、tools/fsrs_golden.mjs（全文）；docs/settings.md、docs/upstream-parity.md、docs/architecture.md 的 FSRS/统计相关段；CLAUDE.md。

**上游对照**：unpkg 拉取 ts-fsrs 5.4.1 与 5.2.3 的 dist/index.mjs（存于 target/，gitignore 内），逐段对照 algorithm/alea/学习步进/两个 scheduler/参数夹取/生成器，并对两版本做了 diff。

**定向测试**：3 轮临时集成测试（异常卡片 NaN/溢出扫描 28 组合、serde_json 溢出字面量、两条 panic 路径），验证后已删除临时文件；按规则为 P0 留下复现测试 `tests/fsrs_parity.rs::extreme_fsrs_parameters_do_not_panic_at_settle`（**当前失败，修复后转绿**；基线其余测试未动，`fsrs_matches_upstream_golden_sample` 仍通过）。未跑全量测试。

**未看及原因**：src/ui 其余页面、audio/、theme/、flow/、wordbook.rs、question.rs、article.rs（与 fsrs-time 维度无直接关系，由其它子任务覆盖）；tests/end_to_end.rs、store_roundtrip.rs、progress_consistency.rs 等只做了 grep 定位与关键断言抽查，未逐行读；target/upstream 快照只查了 package.json（无 node_modules/lockfile，无法确认 Web 端 ts-fsrs 实际版本——见问题 9 的待验证项）。
