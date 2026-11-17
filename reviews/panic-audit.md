# panic-audit：src/ 全量 .unwrap() / .expect() 专项审查

- 版本：typewords-tui v0.4.0（审查时工作区状态）
- 审查者：panic-audit 子任务
- 判定标准：CLAUDE.md 项目不变量（错误处理：启动期不可恢复错误进终端前 bail!；运行期可恢复错误转 Toast；不用 panic 作控制流）
- 方法：逐模块走查 src/ 全部源码 + 相关测试；每个疑点穷尽调用链确认可达性；对唯一疑点用临时集成测试 `cargo test --locked --test panic_audit_repro` 做了运行时验证（验证后已删除该测试文件，工作区无残留改动）。

## 0. 总览

| 统计项 | 数量 |
| --- | --- |
| src/ 中 .unwrap() | 85 |
| src/ 中 .expect() | 66 |
| 合计 | 151 |
| 其中位于 `#[cfg(test)]` 模块内（测试代码，不进产物） | 145 |
| 生产代码中的 .unwrap()/.expect() | **6** |
| 生产代码中这 6 处的危险定级 | 全部 (b)「有逻辑保证不可能」 |
| 生产代码中非 unwrap/expect 形式的 panic 位点（索引 / Duration 构造等，一并走查） | 11 处复核，其中 **1 组确认为可达崩溃（P0）** |
| 最终问题计数 | **P0 × 1，P1 × 0，P2 × 0，P3 × 2** |

生产代码的 panic 面非常克制：6 处 unwrap/expect 全部有局部逻辑保证；真正可达的崩溃不在 unwrap/expect 上，而在 chrono `Duration::days/minutes` 的隐式越界（见问题 1）。

## 1. 全量清单表

类别含义：(a) 启动期已校验，安全；(b) 有逻辑保证不可能（理由见「理由」列）；(c) 用户输入 / 损坏存档 / 极端参数可达 → 危险；T = 位于 `#[cfg(test)]` 模块内（测试代码，失败即测试失败，不属于产品 panic 面，本项目一贯允许）。

### 1.1 生产代码中的 .unwrap() / .expect()（全部 6 处）

| 位置 | 形式 | 类别 | 理由 |
| --- | --- | --- | --- |
| src/domain/article.rs:143 | unwrap | (b) | `parse_sentence` 主循环条件 `index < bytes.len()` 保证 `rest` 非空，`chars().next()` 必为 `Some`。char 边界同样有保证：初始 0 是边界；每次前进要么按 `character.len_utf8()`（完整字符），要么按四个 matcher 返回的长度——matcher（match_currency_number/match_number/match_abbreviation/match_word）只消费 ASCII 字节，ASCII 游程之后的位置必为 UTF-8 边界 |
| src/domain/types.rs:28 | expect | (b) | `int_enum!` 宏生成的 `Default`：`Self::from_i32(0)`。全部三处调用（WordPracticeMode/Type/Stage）都显式定义了值为 0 的变体（System=0 / FollowWrite=0 / FollowWriteNewWord=0），宏的注释也钉死「数值 0 的变体就是默认值」这一约定 |
| src/domain/flow/mod.rs:491 | expect | (b) | `builtin_flow("system")`：`builtin_flow` 是纯 match 表，"system" 是硬编码分支（src/domain/flow/mod.rs:272），不依赖任何外部输入 |
| src/theme/mod.rs:241 | expect | (b) | `ThemeRegistry::resolve` 取内置默认主题：`builtin_specs()` 第一项即 id="dark"（src/theme/builtin.rs:27），且 `ThemeRegistry::load` 会跳过与内置重名的用户主题（src/theme/mod.rs:215-219），"dark" 永远存在 |
| src/theme/mod.rs:297 | expect | (b) | `default_dark()`：同上，`builtin_specs()` 为编译期固定表 |
| src/ui/focus.rs:482 | expect | (b) | `bands()` 的 else 分支只在 `bands` 非空时到达（同一条件表达式 `bands.is_empty() || rect.y >= current_bottom` 的短路结构），`last_mut()` 必为 `Some`；消息文本本身写明了这个保证 |

### 1.2 测试模块内的 .unwrap() / .expect()（145 处，按文件汇总）

| 文件 | 处数 | 行号 | 说明 |
| --- | --- | --- | --- |
| src/fs.rs | 38 | 206-390 | `#[cfg(test)]`（198 行起）：原子写 / 并发写 / 锁的用例，tempdir + IO 断言 |
| src/crash.rs | 13 | 435-475 | `mod tests`（380 行起）：崩溃报告文件名唯一性 / 目录降级用例 |
| src/backup.rs | 11 | 570-660 | `mod tests`（560 行起）：导入回滚事务用例 |
| src/domain/article.rs | 10 | 912-969 | `resource_tests`（898 行起，标注 `#[cfg(test)]`）：资源加载用例 |
| src/paths.rs | 9 | 117-165 | `mod tests`（111 行起）：assets 查找 / 数据目录锁用例 |
| src/theme/load.rs | 9 | 165-241 | `mod tests`（159 行起）：用户主题解析用例 |
| src/domain/session.rs | 9 | 1014-1089 | `mod tests`（926 行起）：进度推进用例 |
| src/logging.rs | 7 | 355-448 | `mod tests`（346 行起）：日志落点 / 尾读用例 |
| src/theme/mod.rs | 5 | 307-340 | `mod tests`（300 行起）：颜色解析 / 主题回退用例 |
| src/domain/statistics.rs | 5 | 179-223 | `mod tests`（169 行起）：跨天统计用例 |
| src/ui/widgets.rs | 5 | 408-446 | `mod tests`（368 行起）：滚动条滑块用例 |
| src/ui/bigtext.rs | 5 | 326-382 | `mod tests`（319 行起）：大字渲染用例 |
| src/audio/tts.rs | 4 | 297-374 | `mod tests`（254 行起）：TTS 命令构造 / 常驻会话用例 |
| src/domain/study_task.rs | 4 | 294-330 | `mod tests`（227 行起）：任务生成用例 |
| src/domain/shortcut.rs | 3 | 717-830 | `mod tests`（698 行起）：键位解析用例 |
| src/domain/types.rs | 2 | 1222, 1228 | `mod tests`（1088 行起）：`set_study_dict` 用例 |
| src/ui/insights.rs | 2 | 208, 216 | `mod tests`：周视图用例 |
| src/cli.rs | 2 | 481, 491 | `mod tests`（422 行起）：参数解析用例 |
| src/domain/fsrs.rs | 1 | 888 | `mod tests`（874 行起）：时间解析 helper |
| src/domain/scheduler.rs | 1 | 80 | `mod tests`（55 行起）：`next_card` 稳定性用例 |

这些测试断言的期望值都由紧邻的构造代码保证（tempdir、显式 push、固定日期），属于健康的测试写法，不构成产品风险。

### 1.3 非 unwrap/expect 的 panic 位点复核（生产代码）

| 位置 | 形式 | 结论 |
| --- | --- | --- |
| src/domain/fsrs.rs:549/554/559（review_state）、:617（long_term_state）、:856（apply_learning_steps） | `now + Duration::days(interval as i64)` | **(c) 可达崩溃 → P0，见问题 1** |
| src/domain/fsrs.rs:850 | `now + Duration::minutes(js_round(scheduled_minutes) as i64)` | **(c) 可达崩溃 → P0，见问题 1（触发路径 B）** |
| src/domain/fsrs.rs:845 | 同上但被 `scheduled_minutes < MINUTES_PER_DAY`（1440）上界约束 | (b) 安全 |
| src/domain/statistics.rs:118, 133 | `day_map[key]`（HashMap Index） | (b)：`day_order` 只在 `day_map.entry().or_insert_with` 的闭包里 push，两者键集合同步，索引永不 miss |
| src/domain/typing.rs:287 | `KeyInput::Backspace => unreachable!()` | (b)：type_key 开头 279-282 已对 Backspace 提前 return |
| src/backup.rs:147 | `_ => unreachable!("targets 表与校验分支必须一一对应")` | (b)：match 的三个分支与上一行 `targets` 常量表逐字对应（KEY_DICT/KEY_SETTING/KEY_PRACTICE_WORD） |
| src/domain/article.rs:205 | `lines.len() - 1`（usize 下溢风险） | (b)：`section_text` 来自 `trim()` 后的非空串，`split('\n')` 过滤空行后至少 1 行 |
| src/ui/bigtext.rs:220 | `&glyphs[index]`（Vec 索引） | (b)：range 由 `0..characters.len()` 或 `start..end`（end ≤ len）构造，glyphs 长度恒等于 characters.len() |
| src/app/action.rs:789/791 | `checked_add_signed(chrono::Duration::days(7))` | 安全：checked API + 固定 7 天 |
| src/ui/stats.rs:194 | `first + chrono::Duration::days(31)` | (b)：`first` 为当月一号，+31 天必然落在 NaiveDate 合法范围 |
| src/domain/flow/navigator.rs:291-293 | `state.data.index - group_size`（usize 下溢） | (b)：`index` 先自增且是 `group_size.max(1)` 的正倍数，必然 ≥ group_size |

## 2. 问题清单

### 问题 1 [P0] 高级 FSRS 极端参数使调度器在结算路径上 panic（chrono TimeDelta 越界）

- **位置**：src/domain/fsrs.rs:549、:554、:559、:617、:850、:856（触发点在 Fsrs::next_card 的调用方：src/domain/session.rs:738 `settle`、src/app/mod.rs:1469 `finish_session`、src/app/mod.rs:3470 `rate_marked_words`）
- **证据**（真实代码引用）：
  - src/domain/fsrs.rs:549（review_state）：`hard.due = now + Duration::days(hard_interval as i64);`
  - src/domain/fsrs.rs:856（apply_learning_steps 兜底分支）：`next.due = now + Duration::days(interval as i64);`
  - src/domain/fsrs.rs:850：`next.due = now + Duration::minutes(js_round(scheduled_minutes) as i64);`
  - 间隔的来源没有任何「天/分钟数上限」约束：`next_interval` 只做 `clamp(raw, 1.0, self.maximum_interval)`（fsrs.rs:780-784），而 `maximum_interval` 的唯一校验是 `>= 1.0`（fsrs.rs:432-436）：
    ```rust
    let maximum_interval = if parameters.maximum_interval >= 1.0 {
        parameters.maximum_interval
    } else {
        36500.0
    };
    ```
  - `request_retention` 的唯一校验是落在 `(0, 1)`（fsrs.rs:426-431），0.01 完全合法；它直接决定 `interval_modifier = (r^(1/decay) - 1) / factor`（fsrs.rs:437），r 很小时该值可达 1e12 量级。
  - 学习步进字符串没有任何上限：`step_to_minutes`（fsrs.rs:294-306）接受任意可解析的 f64 并按 `d` 乘 1440。
- **触发条件**（两条独立路径，均已用临时集成测试实测确认，测试名 `extreme_retention_and_maximum_interval_do_not_panic_when_scheduling` 与 `an_astronomical_learning_step_does_not_panic_the_scheduler`）：
  - 路径 A：`setting.json` 里 `fsrsParameters` 写成 `{"requestRetention": 0.01, "maximumInterval": 1e12, ...}`（两者都通过 Fsrs::new 的校验）→ 学完任意一轮练习按结算（或词条页批量「认识」）→ 第二次评分后 `interval = clamp(stability × interval_modifier, 1, 1e12) = 1e12` → `Duration::days(1e12)` = 8.64e19 ms 超出 chrono TimeDelta 上界（≈9.2e18 ms）→ panic：`TimeDelta::days out of bounds`（chrono-0.4.45 lib.rs:717）。
  - 路径 B：`fsrsParameters.learningSteps` 写成 `["1e300d"]`（字符串能被 serde 原样收下，Fsrs::new 非空即透传，fsrs.rs:408-412）→ 新卡第一次打错（RATING_AGAIN）→ `scheduled_minutes = 1.44e303 ≥ 1440` 走 fsrs.rs:850 → `as i64` 饱和成 i64::MAX → panic：`TimeDelta::minutes out of bounds`。
  - 两种值的共同来源：手工编辑 `setting.json`（docs/persistence.md 公开该格式），或 `tw import-web` 导入网页版备份——backup.rs:145 的 `validate_envelope::<SettingState>` 只做类型级 serde 校验，任意 f64 / 任意步进字符串都能通过。
- **违反的不变量**：「运行期可恢复错误转 Toast；不用 panic 作控制流」。这是结算路径上的崩溃，不是启动期错误。
- **影响**：panic 钩子会写 crash-*.txt 并退出进程；崩溃发生在 `settle` 写盘之前——本轮会话的统计、错词、FSRS 评级全部不落盘（等于丢当次会话数据），且每次重进练习、每次结算都会复现，形成「改错一次配置就永远打不完一轮」的硬阻塞。注意 UI 本身安全：高级 FSRS 页把留存率夹在 50%~99%、最大间隔夹在 1~36500（src/app/mod.rs:4381-4387），21 个权重也按 weight_bounds 夹取——问题只在「存档值绕过 UI 直达调度器」这条通道。
- **建议方向**：
  1. 在 `Fsrs::new` 里对 `maximum_interval` 加上界（如 ≤ 36500 或 ≤ 1e6 天），对 `request_retention` 加更窄的下界（如 ≥ 0.5），超出回退默认——与现有的「回退 0.9 / 36500」风格一致；
  2. `step_to_minutes` 给返回值设上限（如 ≤ 36500 分钟）或拒绝超限步进；`next_interval` 的结果与 `apply_learning_steps` 的分钟数统一 clamp 到 chrono 安全区间（约 1e11 天以内）后再构造 Duration；
  3. 或统一改用 `chrono::Duration::try_days/try_minutes` + 回退默认间隔，把构造失败变成可恢复错误。
- **置信度**：确认（已用最小复现测试实测，两条路径均在本仓库当前代码上 panic，测试文件验证后已删除；修复时可把该测试作为回归测试落盘）。

复现测试（建议随修复提交）：

```rust
use typewords_tui::domain::fsrs::Fsrs;
use typewords_tui::domain::types::{FsrsParameters, RATING_AGAIN, RATING_GOOD};

#[test]
fn extreme_retention_and_maximum_interval_do_not_panic_when_scheduling() {
    let parameters = FsrsParameters {
        request_retention: 0.01,
        maximum_interval: 1e12,
        ..FsrsParameters::default()
    };
    let fsrs = Fsrs::new(&parameters);
    let now = chrono::Utc::now();
    let first = fsrs.next_card(None, RATING_GOOD, now);
    let _second = fsrs.next_card(Some(&first), RATING_GOOD, now);
}

#[test]
fn an_astronomical_learning_step_does_not_panic_the_scheduler() {
    let parameters = FsrsParameters {
        learning_steps: vec!["1e300d".to_string()],
        ..FsrsParameters::default()
    };
    let fsrs = Fsrs::new(&parameters);
    let _ = fsrs.next_card(None, RATING_AGAIN, chrono::Utc::now());
}
```

### 问题 2 [P3] 自测题相似度的 i64 乘法在极端 frequency 下溢出（release 静默回绕）

- **位置**：src/domain/question.rs:67（`similarity += item.1 * other.1 * TRAN_EQUALS_BASE_SCORE;`，`TRAN_EQUALS_BASE_SCORE = 1 << 16`）
- **证据**：`Trans.frequency: Option<i64>` 直接来自词库 JSON（src/domain/types.rs:157），导入词库（`tw import-dict`）时不做值域校验；两词 frequency 各取 1e15 时乘积即溢出。
- **触发条件**：导入的词库里给同一释义写上 ~1e15 量级的 frequency，进入 Identify 模式出题。
- **违反的不变量 / 影响**：release 构建静默回绕（本项目 release 未开 overflow-checks），只影响干扰项排序，不崩溃；debug 构建会 panic（仅影响开发自测）。属于边界一致性问题。
- **建议方向**：相似度改 f64 累加，或对 frequency 做 `clamp`。
- **置信度**：确认（算术事实）；实际影响为 P3。

### 问题 3 [P3] 每日词数 × 复习比的 i64 乘法可被手改存档撑溢

- **位置**：src/domain/study_task.rs:42 与 :140（`(options.per_day_study_number as i64 * options.word_review_ratio)`）
- **证据**：UI 把 `word_review_ratio` 夹在 0~10（src/app/mod.rs:4367），但 `setting.json` 手写 i64::MAX 可直达该乘法；`per_day` 又被夹到 `[1, 词表长度]`，「超大导入词库」会放大它。
- **触发条件**：手改 `setting.json` 的 `wordReviewRatio` 为极大值（或导入含此值的网页备份）后刷新任务。
- **影响**：release 静默回绕，任务数量算错（可能 0 或随机值）；debug 构建才 panic。不构成产品崩溃。
- **建议方向**：乘法前对两个因子 clamp（如 ratio ≤ 100、total_need ≤ 10_000），与 UI 口径对齐。
- **置信度**：确认（算术事实）；实际影响为 P3。

## 3. 做得好的地方

1. **测试与生产彻底分离**：151 处 unwrap/expect 中 145 处在 `#[cfg(test)]` 内，生产代码只有 6 处且每处都有写进代码注释或消息文本里的局部保证（如 src/ui/focus.rs:482 的 expect 消息「bands 非空时才有 last_mut」、src/domain/types.rs:27 的「数值 0 的变体就是该枚举的默认值」）。
2. **损坏输入的防线在前而不在 unwrap**：store 层对存档做版本头先行 + 损坏隔离（src/store/mod.rs:143-187），domain 层对外部数据（词库目录、文章、自建流程）一律 Option/get + 显式回退（如 src/domain/flow/mod.rs:561-586 的 `get_step` 越界回退占位步骤并注释「而不是越界 panic」；navigator 的 restore_cursor 逐项校验快照）。FSRS 权重长度、留存率、最大间隔在 Fsrs::new 有集中兜底（src/domain/fsrs.rs:407-449）——问题 1 恰是这个兜底的量级没封住。
3. **键入/文章分词的 UTF-8 边界意识**：parse_sentence 的推进只走 ASCII 游程或完整字符长度（src/domain/article.rs:141-171），`step_to_minutes` 注释明确「按字符切分而不是字节——`1分` 之类的写法按字节切会落在 UTF-8 边界中间而 panic」并正确实现（src/domain/fsrs.rs:294-296）。
4. **usize 下溢与索引普遍用了 saturating/get 防御**：`advance_dict_progress` 的 `.saturating_add().min()`、`create_shuffle_task` 的 `.min(words.len())`、`advance_step_cursor` 的 `.min(nodes.len()-1)`（src/domain/session.rs:700-703、study_task.rs:207-215、flow/mod.rs:713），`init_stability` 甚至显式防了 `grade - 1` 的 u8 下溢（src/domain/fsrs.rs:694-697）。
5. **崩溃本身有兜底**：main 第一件事装 crash 钩子（crash.rs:66-85），报告写失败降级临时目录、防递归、不覆盖旧报告——P0 的崩溃至少能留下现场。

## 4. 覆盖声明

- **逐行走查过的生产 + 测试文件**（含其中的用例）：src/domain/ 全部 22 个文件（article.rs、catalog.rs、constants.rs、fsrs.rs、flow/{mod,navigator,store}.rs、i18n.rs（抽查）、question.rs、rng.rs、scheduler.rs、session.rs、shortcut.rs、statistics.rs、study_task.rs、timer.rs、types.rs、typing.rs、wordbook.rs）；src/fs.rs、src/crash.rs、src/logging.rs、src/paths.rs、src/backup.rs（1-300 行逐行 + 测试区）、src/store/mod.rs（1-200 行）、src/bootstrap.rs（1-50 行）、src/theme/{mod,load}.rs、src/theme/builtin.rs（grep 定位）、src/ui/{bigtext,focus,widgets,insights,adv_fsrs}.rs（相关区域）、src/audio/tts.rs（相关区域）、src/cli.rs（相关区域）、src/app/mod.rs（FSRS 编辑 / 结算 / 批量标记等 panic 相关区域 1430-1500、3440-3500、4200-4400）。
- **以检索 + 定点阅读覆盖**：src/ 其余文件（app/、ui/ 其它页面、audio/ 其它、main.rs、lib.rs）——经全仓 grep 确认这些文件不含 unwrap/expect，再对 Duration 构造、索引、下溢、unreachable!、panic! 做了补充检索（结果见 1.3 表）。
- **未逐行细读**：src/app/mod.rs 的非 panic 相关区域（约 4000 行中的大部分，仅读关键调用链）、src/ui/ 各页面的纯渲染分支——原因：这些文件无 unwrap/expect 命中，且 panic 面已由上述检索模式覆盖；`tests/` 目录本身不在本次范围（维度限定 src/）。
- **运行时验证**：仅针对问题 1 执行过 `cargo test --locked --test panic_audit_repro`（两条用例均按预期 panic，证据在报告问题 1；测试文件已删除，未改动任何产品代码）。未跑全量测试（基线已绿）。
- **声明**：除上述临时测试文件的创建与删除外，本审查未修改任何代码或文档。
