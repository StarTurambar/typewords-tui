//! FSRS-6 调度：与上游 ts-fsrs 5.4.1 行为一致。
//!
//! 逐条移植 ts-fsrs 的算法层（`algorithm.ts`）、状态机（`impl/basic_scheduler.ts`
//! 与 `abstract_scheduler.ts`）、学习步进策略（`strategies/learning_steps.ts`）、
//! fuzz 区间（`help.ts`）与随机源（`alea.ts`），使同一张卡在两端得到**相同的**
//! 下次复习时间。
//!
//! 关键不变量：
//! - `enable_short_term` 决定用 BasicScheduler（学习步进）还是 LongTermScheduler；
//! - fuzz 的随机种子取自卡片状态（`{时间戳}_{reps}_{难度×稳定性}`），
//!   因此同一次复习里四个评级得到同一个随机因子，与上游一致；
//! - `elapsed_days` 按 **UTC 日期** 计算（上游 `dateDiffInDays` 用 `Date.UTC`）。

use chrono::{DateTime, Duration, Utc};

use crate::domain::types::{
    CARD_STATE_LEARNING, CARD_STATE_NEW, CARD_STATE_RELEARNING, CARD_STATE_REVIEW, Card,
    FsrsParameters, RATING_AGAIN, RATING_EASY, RATING_GOOD, RATING_HARD, Rating,
};

/// 最小稳定性。
const S_MIN: f64 = 0.001;
/// 最大稳定性。
const S_MAX: f64 = 36500.0;
/// 初始稳定性的上限（参数夹取用）。
const INIT_S_MAX: f64 = 100.0;
/// FSRS-6 的默认衰减系数（`w[20]`）。
pub const FSRS6_DEFAULT_DECAY: f64 = 0.1542;
/// FSRS-5 的默认衰减系数：17 / 19 项权重的旧存档迁移时补的就是它。
///
/// 与 [`FSRS6_DEFAULT_DECAY`] 不是一个值——旧存档的权重来自 FSRS-5 时代，
/// 迁移时按上游补 FSRS-5 的衰减，而不是拿 FSRS-6 的新默认值顶替。
const FSRS5_DEFAULT_DECAY: f64 = 0.5;
/// `w[17]` / `w[18]` 的乘积上限。
const W17_W18_CEILING: f64 = 2.0;

/// 学习步进里的分钟数上限：达到一天即毕业进 Review。
const MINUTES_PER_DAY: f64 = 1440.0;

/// 间隔（天）上限：与「高级 FSRS」页的最大间隔可调区间（1 ~ 36500）以及
/// `S_MAX` 取同一个值。
///
/// 也是「手改 `setting.json` 不至于把到期时间推到 chrono 上界」的兜底：
/// chrono 的 `TimeDelta` 上界约 1e11 天，而 `request_retention` 取极小值时
/// `interval_modifier` 能到 1e58 量级，只有把最大间隔封在这里，`now +
/// Duration::days(_)` 才不可能越界 panic。
pub const MAX_INTERVAL_DAYS: f64 = 36500.0;

/// 学习步进（分钟）上限：换算后超过 36500 天的步进按上限封顶。
///
/// 步进串同样是手改字段（`"1e300d"` 也符合 `step_to_minutes` 的语法），
/// 原样转成 `Duration::minutes` 会在结算路径上 panic。
const MAX_STEP_MINUTES: f64 = MAX_INTERVAL_DAYS * MINUTES_PER_DAY;

// ── JS 数值语义 ────────────────────────────────────────────────────────────

/// JS `Math.round`：向 +∞ 取整（半值进位）。与 Rust 的 `f64::round` 在负数上不同。
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// JS `Math.round(num * 10^decimals) / 10^decimals`。
fn round_to(value: f64, decimals: i32) -> f64 {
    let factor = 10f64.powi(decimals);
    js_round(value * factor) / factor
}

/// JS `Math.min(Math.max(value, min), max)`。
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

/// JS `ToUint32`：向零取整后对 2^32 取模。
fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    value.trunc().rem_euclid(4294967296.0) as u32
}

/// JS `ToInt32`：向零取整后按 32 位有符号回绕。
fn to_int32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let modulo = value.trunc().rem_euclid(4294967296.0);
    if modulo >= 2147483648.0 {
        (modulo - 4294967296.0) as i32
    } else {
        modulo as i32
    }
}

// ── Alea 随机源 ────────────────────────────────────────────────────────────

/// Alea PRNG（seedrandom 的 alea 实现，MIT 许可）。
///
/// 移植自 ts-fsrs 的 `src/alea.ts`：fuzz 的随机因子由卡片状态派生的种子决定，
/// 因此同一张卡、同一时间点得到的抖动与上游完全一致。
#[derive(Clone, Debug)]
struct Alea {
    c: f64,
    s0: f64,
    s1: f64,
    s2: f64,
}

impl Alea {
    fn new(seed: &str) -> Self {
        let mut mash = Mash::new();
        let mut s0 = mash.mash(" ");
        let mut s1 = mash.mash(" ");
        let mut s2 = mash.mash(" ");
        s0 -= mash.mash(seed);
        if s0 < 0.0 {
            s0 += 1.0;
        }
        s1 -= mash.mash(seed);
        if s1 < 0.0 {
            s1 += 1.0;
        }
        s2 -= mash.mash(seed);
        if s2 < 0.0 {
            s2 += 1.0;
        }
        Self { c: 1.0, s0, s1, s2 }
    }

    fn next(&mut self) -> f64 {
        let t = 2091639.0 * self.s0 + self.c * 2.3283064365386963e-10;
        self.s0 = self.s1;
        self.s1 = self.s2;
        self.c = to_int32(t) as f64;
        self.s2 = t - self.c;
        self.s2
    }
}

/// Alea 的种子散列（`Mash`）。注意其内部状态 `n` 在多次调用间共享。
#[derive(Clone, Debug)]
struct Mash {
    n: f64,
}

impl Mash {
    fn new() -> Self {
        Self {
            n: 0xefc8249du32 as f64,
        }
    }

    fn mash(&mut self, data: &str) -> f64 {
        // JS 的 `charCodeAt` 返回 UTF-16 码元
        for unit in data.encode_utf16() {
            self.n += unit as f64;
            let mut h = 0.02519603282416938 * self.n;
            self.n = to_uint32(h) as f64;
            h -= self.n;
            h *= self.n;
            self.n = to_uint32(h) as f64;
            h -= self.n;
            self.n += h * 4294967296.0;
        }
        to_uint32(self.n) as f64 * 2.3283064365386963e-10
    }
}

// ── 参数夹取 ───────────────────────────────────────────────────────────────

/// 参数夹取区间（对应上游 `constant.ts` 的 `CLAMP_PARAMETERS`）。
fn clamp_parameters(enable_short_term: bool) -> [(f64, f64); 21] {
    [
        (S_MIN, INIT_S_MAX),                               // 0 初始稳定性（Again）
        (S_MIN, INIT_S_MAX),                               // 1 初始稳定性（Hard）
        (S_MIN, INIT_S_MAX),                               // 2 初始稳定性（Good）
        (S_MIN, INIT_S_MAX),                               // 3 初始稳定性（Easy）
        (1.0, 10.0),                                       // 4 初始难度（Good）
        (0.001, 4.0),                                      // 5 初始难度乘数
        (0.001, 4.0),                                      // 6 难度乘数
        (0.001, 0.75),                                     // 7 难度乘数
        (0.0, 4.5),                                        // 8 稳定性指数
        (0.0, 0.8),                                        // 9 稳定性负幂
        (0.001, 3.5),                                      // 10 稳定性指数
        (0.001, 5.0),                                      // 11 失败稳定性乘数
        (0.001, 0.25),                                     // 12 失败稳定性负幂
        (0.001, 0.9),                                      // 13 失败稳定性幂
        (0.0, 4.0),                                        // 14 失败稳定性指数
        (0.0, 1.0),                                        // 15 稳定性乘数（Hard）
        (1.0, 6.0),                                        // 16 稳定性乘数（Easy）
        (0.0, W17_W18_CEILING),                            // 17 短期稳定性指数
        (0.0, W17_W18_CEILING),                            // 18 短期稳定性指数
        (if enable_short_term { 0.01 } else { 0.0 }, 0.8), // 19 短期最后稳定性指数
        (0.1, 0.8),                                        // 20 衰减
    ]
}

/// 完整的参数夹取区间：基础表 + 按重学步数收紧的 `w[17]`/`w[18]` 上限。
///
/// `num_relearning_steps > 1` 时收紧上限，保证「多次重学步进」不会让
/// 稳定性超过上限；设置页编辑权重用的 [`weight_bounds`] 与
/// [`clip_parameters`] 都从这里取区间，界面上的范围因此就是调度器实际
/// 生效的范围。
fn clamp_ranges(
    parameters: &[f64],
    num_relearning_steps: usize,
    enable_short_term: bool,
) -> [(f64, f64); 21] {
    let mut clip = clamp_parameters(enable_short_term);
    if num_relearning_steps > 1 {
        // PLS * e^(num_relearning_steps * w17 * w18) <= S
        // => w17 * w18 <= -(ln(w11) + ln(2^w13 - 1) + w14 * 0.3) / num_relearning_steps
        let w11 = clamp(
            parameters.get(11).copied().unwrap_or(0.0),
            clip[11].0,
            clip[11].1,
        );
        let w13 = clamp(
            parameters.get(13).copied().unwrap_or(0.0),
            clip[13].0,
            clip[13].1,
        );
        let w14 = clamp(
            parameters.get(14).copied().unwrap_or(0.0),
            clip[14].0,
            clip[14].1,
        );
        let value =
            -(w11.ln() + (2.0f64.powf(w13) - 1.0).ln() + w14 * 0.3) / num_relearning_steps as f64;
        let ceiling = clamp(round_to(value.max(0.0).sqrt(), 8), 0.01, W17_W18_CEILING);
        clip[17].1 = ceiling;
        clip[18].1 = ceiling;
    }
    clip
}

/// 把参数逐项夹到合法区间（对应上游 `default.ts` 的 `clipParameters`）。
///
/// 区间与 [`clamp_ranges`] 同源；输出长度与输入一致（上游是
/// `parameters.map(...)`），不做补齐——17 / 19 项权重的补齐由
/// [`migrate_parameters`] 在夹取之后做，长度分流只有那一处。
pub fn clip_parameters(
    parameters: &[f64],
    num_relearning_steps: usize,
    enable_short_term: bool,
) -> Vec<f64> {
    let clip = clamp_ranges(parameters, num_relearning_steps, enable_short_term);
    parameters
        .iter()
        .take(clip.len())
        .enumerate()
        .map(|(index, value)| {
            // 上游写的是 `parameters[index] || 0`：0 与 NaN 都当 0
            let value = if value.is_finite() { *value } else { 0.0 };
            clamp(value, clip[index].0, clip[index].1)
        })
        .collect()
}

/// 把任意长度的权重迁移成 21 项（对应上游 `migrateParameters`）。
///
/// 上游按长度分流，这里逐条照搬：
/// - 21 项：只夹取，行为与逐项夹取完全一致；
/// - 19 项：夹取后补 `[0, FSRS5_DEFAULT_DECAY]`（FSRS-5 → FSRS-6 新增的两项）；
/// - 17 项：夹取后先把 `w[4]` / `w[5]` / `w[6]` 按 FSRS-5 → FSRS-6 的公式
///   变换（各取 8 位小数），再补 `[0, 0, 0, FSRS5_DEFAULT_DECAY]`；
/// - 其它任何长度（空、22 项及以上）：**整组回退默认权重**，一项都不保留。
///
/// 长度分流只在这里做一次：[`Fsrs::new`] 与设置页写权重都走它，界面上的
/// 合法区间仍由 [`weight_bounds`] 提供。
pub fn migrate_parameters(
    parameters: &[f64],
    num_relearning_steps: usize,
    enable_short_term: bool,
) -> Vec<f64> {
    match parameters.len() {
        21 => clip_parameters(parameters, num_relearning_steps, enable_short_term),
        19 => {
            let mut w = clip_parameters(parameters, num_relearning_steps, enable_short_term);
            w.extend([0.0, FSRS5_DEFAULT_DECAY]);
            w
        }
        17 => {
            let mut w = clip_parameters(parameters, num_relearning_steps, enable_short_term);
            // 顺序与上游一致：`w[4]` 用的是**变换前**的 `w[5]`。
            w[4] = round_to(w[5] * 2.0 + w[4], 8);
            w[5] = round_to((w[5] * 3.0 + 1.0).ln() / 3.0, 8);
            w[6] = round_to(w[6] + 0.5, 8);
            w.extend([0.0, 0.0, 0.0, FSRS5_DEFAULT_DECAY]);
            w
        }
        _ => default_w().to_vec(),
    }
}

/// 调度器实际使用的 21 项权重。
///
/// [`Fsrs::new`] 与设置页的权重显示 / 编辑预填共用这一个入口：`w` 不是 21 项
/// 时先按 [`migrate_parameters`] 迁移，重学步数取**净化后**的列表长度（非法
/// 步进整组回退默认，与调度器的夹取区间同源）。界面因此显示的是真正参与
/// 调度的值，而不是存档里未迁移的原始值。
pub fn effective_w(parameters: &FsrsParameters) -> Vec<f64> {
    let relearning_steps = sanitized_steps(&parameters.relearning_steps, &DEFAULT_RELEARNING_STEPS);
    effective_w_for(parameters, relearning_steps.len())
}

/// `Fsrs::new` 已经净化过步进，直接复用其长度，避免再分配一份步进列表。
fn effective_w_for(parameters: &FsrsParameters, num_relearning_steps: usize) -> Vec<f64> {
    migrate_parameters(
        &parameters.w,
        num_relearning_steps,
        parameters.enable_short_term,
    )
}

/// 第 `index` 项权重在当前设置下的合法区间。
///
/// 供「高级 FSRS」页编辑权重时夹取：重学步数取 [`Fsrs::new`] 的有效值
/// （空列表 / 非法步进回退默认，按 1 步算），`enable_short_term` 决定
/// `w[19]` 的下限；参数先按 [`effective_w`] 迁移，因此界面显示的区间与
/// 调度器实际夹取的结果一致。
pub fn weight_bounds(parameters: &FsrsParameters, index: usize) -> (f64, f64) {
    let relearning_steps = sanitized_steps(&parameters.relearning_steps, &DEFAULT_RELEARNING_STEPS);
    let num_relearning_steps = relearning_steps.len();
    let w = effective_w_for(parameters, num_relearning_steps);
    clamp_ranges(&w, num_relearning_steps, parameters.enable_short_term)
        .get(index)
        .copied()
        .unwrap_or((0.0, f64::INFINITY))
}

/// FSRS-6 的 21 项默认权重（与上游 `constant.ts` 一致）。
pub fn default_w() -> [f64; 21] {
    [
        0.212,
        1.2931,
        2.3065,
        8.2956,
        6.4133,
        0.8334,
        3.0194,
        0.001,
        1.8722,
        0.1666,
        0.796,
        1.4835,
        0.0614,
        0.2629,
        1.6483,
        0.6014,
        1.8729,
        0.5425,
        0.0912,
        0.0658,
        FSRS6_DEFAULT_DECAY,
    ]
}

// ── 学习步进 ───────────────────────────────────────────────────────────────

/// JS `parseInt(text, 10)` 的数值部分：可选正负号 + 连续十进制数字。
///
/// 与 `f64::parse` 的区别就是上游与 TUI 过去的区别：前导空白与数字后面的
/// 垃圾都被忽略（`"10 m"` 的前缀是 `"10 "` → 10、`"1.5"` → 1、
/// `"1e300"` → 1、`"0x10"` → 0）。一个数字都没有、或累加到非有限数
/// （JS 的 `parseInt` 对超长数字串返回 Infinity）时返回 `None`。
fn js_parse_int(text: &str) -> Option<f64> {
    let mut chars = text.trim_start().chars().peekable();
    let mut negative = false;
    match chars.peek() {
        Some('+') => {
            chars.next();
        }
        Some('-') => {
            negative = true;
            chars.next();
        }
        _ => {}
    }
    let mut value = 0.0f64;
    let mut digits = 0usize;
    while let Some(character) = chars.next_if(|c| c.is_ascii_digit()) {
        value = value * 10.0 + f64::from(character.to_digit(10).unwrap_or(0));
        digits += 1;
    }
    if digits == 0 || !value.is_finite() {
        return None;
    }
    Some(if negative { -value } else { value })
}

/// 步长单位转分钟（对应上游 `ConvertStepUnitToMinutes`）。
///
/// 语义与上游逐条一致：**单位是最后一个字符**（`m` / `h` / `d`），前缀按
/// `parseInt(_, 10)` 解析（见 [`js_parse_int`]）。因此 `"10 m"` 是 10 分钟、
/// `"1.5m"` 是 1 分钟、`"1e300d"` 是 1 天、`"0x10m"` 是 0 分钟；无数字、
/// 负数、单位不在 `m` / `h` / `d` 都返回 `None`（上游抛 `FSRSValidationError`）。
///
/// 单位按**字符**切分而不是字节——步进串来自用户可编辑的 `setting.json`，
/// `"1分"` 之类的非 ASCII 写法按字节切会落在 UTF-8 边界中间而 panic。
///
/// 结果封顶到 [`MAX_STEP_MINUTES`]（第二道防线）：`"999999999999999d"` 换算后
/// 是天文数字，原样变成到期时间会让 `now + Duration::minutes(_)` 超出 chrono
/// 的 `TimeDelta` 上界而在结算路径上 panic。
pub fn step_to_minutes(step: &str) -> Option<f64> {
    let mut chars = step.chars();
    let unit = chars.next_back()?;
    let value = js_parse_int(chars.as_str())?;
    if value < 0.0 {
        return None;
    }
    let minutes = match unit {
        'm' => value,
        'h' => value * 60.0,
        'd' => value * 1440.0,
        _ => return None,
    };
    Some(minutes.min(MAX_STEP_MINUTES))
}

/// 列出无法解析的步进串（`learning_steps` 与 `relearning_steps` 一起）。
///
/// [`Fsrs::new`] 遇到非法步进会把**整组**回退成内置默认值；这个函数把「哪些
/// 没被采用」交给启动路径（`bootstrap::load` 推进 notices，界面上是 Warning
/// Toast，同时写进日志）。domain 保持纯函数：不弹 UI、不依赖上层。
///
/// 空列表不算非法——那是「用默认值」的正常写法，不会出现在结果里。
pub fn invalid_steps(parameters: &FsrsParameters) -> Vec<String> {
    parameters
        .learning_steps
        .iter()
        .chain(parameters.relearning_steps.iter())
        .filter(|step| step_to_minutes(step).is_none())
        .cloned()
        .collect()
}

/// 内置默认学习步进（与 `FsrsParameters::default` 一致）。
const DEFAULT_LEARNING_STEPS: [&str; 2] = ["1m", "10m"];
/// 内置默认重学步进（与 `FsrsParameters::default` 一致）。
const DEFAULT_RELEARNING_STEPS: [&str; 1] = ["10m"];

/// 步进列表的兜底：空列表或含非法步进时整组回退内置默认值。
///
/// 非法步进绝不能按「没有下一步」处理——那会让卡片直接毕业、排到 ≥1 天的
/// FSRS 间隔，比上游「解析失败抛 `FSRSValidationError`」危险得多。整套回退
/// 至少保住「学习卡仍留在学习阶段」的语义；「哪些步进没被采用」由
/// [`invalid_steps`] 交给启动路径提示用户。
fn sanitized_steps(steps: &[String], defaults: &[&str]) -> Vec<String> {
    if steps.is_empty() || steps.iter().any(|step| step_to_minutes(step).is_none()) {
        return defaults.iter().map(|step| (*step).to_string()).collect();
    }
    steps.to_vec()
}

/// 学习步进策略的结果。
#[derive(Clone, Copy, Debug, PartialEq)]
struct StepInfo {
    /// 距下次复习的分钟数；`0` 表示毕业。
    scheduled_minutes: f64,
    /// 步进序号。
    next_step: usize,
}

/// 上游 `BasicLearningStepsStrategy`。
///
/// `state` 为 Review / Relearning 时用 `relearning_steps`，其余用 `learning_steps`。
fn learning_step_info(
    learning_steps: &[String],
    relearning_steps: &[String],
    state: u8,
    current_step: usize,
    grade: Rating,
) -> Option<StepInfo> {
    let steps = if state == CARD_STATE_RELEARNING || state == CARD_STATE_REVIEW {
        relearning_steps
    } else {
        learning_steps
    };
    if steps.is_empty() || current_step >= steps.len() {
        return None;
    }
    let first = step_to_minutes(&steps[0])?;
    let step_at = |index: usize| -> Option<f64> {
        if index >= steps.len() {
            return None;
        }
        step_to_minutes(&steps[index])
    };
    let current = step_at(current_step)?;

    if state == CARD_STATE_REVIEW {
        // 复习态只有 Again 走学习步进
        if grade != RATING_AGAIN {
            return None;
        }
        return Some(StepInfo {
            scheduled_minutes: current,
            next_step: 0,
        });
    }

    match grade {
        RATING_AGAIN => Some(StepInfo {
            scheduled_minutes: first,
            next_step: 0,
        }),
        RATING_HARD => {
            let minutes = if steps.len() == 1 {
                js_round(first * 1.5)
            } else {
                let next = step_to_minutes(&steps[1])?;
                js_round((first + next) / 2.0)
            };
            Some(StepInfo {
                scheduled_minutes: minutes,
                next_step: current_step,
            })
        }
        RATING_GOOD => {
            let next = step_at(current_step + 1)?;
            if next == 0.0 {
                return None;
            }
            Some(StepInfo {
                scheduled_minutes: js_round(next),
                next_step: current_step + 1,
            })
        }
        _ => None,
    }
}

// ── FSRS-6 算法 ────────────────────────────────────────────────────────────

/// FSRS-6 调度器。
#[derive(Clone, Debug)]
pub struct Fsrs {
    w: Vec<f64>,
    maximum_interval: f64,
    enable_fuzz: bool,
    enable_short_term: bool,
    learning_steps: Vec<String>,
    relearning_steps: Vec<String>,
    /// `decay = -w[20]`
    decay: f64,
    /// `factor = e^(ln(0.9)/decay) - 1`
    factor: f64,
    /// `interval_modifier = (r^(1/decay) - 1) / factor`
    interval_modifier: f64,
}

impl Fsrs {
    /// 由设置构造调度器。
    pub fn new(parameters: &FsrsParameters) -> Self {
        // 空列表与非法步进都整组回退内置默认值（见 `sanitized_steps`）。
        let learning_steps = sanitized_steps(&parameters.learning_steps, &DEFAULT_LEARNING_STEPS);
        let relearning_steps =
            sanitized_steps(&parameters.relearning_steps, &DEFAULT_RELEARNING_STEPS);
        // 长度分流（21 / 19 / 17 / 其它）只在 `migrate_parameters` 里做一次。
        let w = effective_w_for(parameters, relearning_steps.len());
        let (decay, factor) = compute_decay_factor(&w);
        // 留存率必须落在 (0, 1)：取 1 时 interval_modifier 恒为 0，所有复习卡
        // 都会被排到次日；最大间隔至少一天，否则卡片当天反复到期。
        //
        // **与上游的有意差异**：ts-fsrs 的 `calculate_interval_modifier` 只拒绝
        // `<= 0 || > 1`，`1.0` 是合法值（interval_modifier = 0，间隔恒为 1 天）。
        // TUI 把 `1.0` 与越界值一起回退 0.9：宁可比上游保守，也不接受「所有
        // 复习卡都排到次日」这种退化调度。这条差异登记在 docs/upstream-parity.md
        // 的 FSRS 对照表里，改这里之前先改文档。
        //
        // 上界也要兜住：留存率取极小值（手改 `1e-9`）时 interval_modifier 会被
        // 推向 ~1e58，再叠加不加限制的最大间隔（`1e300`），
        // `now + Duration::days(_)` 就会超出 chrono 的 TimeDelta 上界，结算
        // 路径 panic、当轮进度在写盘前丢失。最大间隔与「高级 FSRS」页的可调
        // 区间同源，统一夹到 1 ~ MAX_INTERVAL_DAYS。
        let request_retention =
            if parameters.request_retention > 0.0 && parameters.request_retention < 1.0 {
                parameters.request_retention
            } else {
                0.9
            };
        let maximum_interval = if parameters.maximum_interval >= 1.0 {
            parameters.maximum_interval.min(MAX_INTERVAL_DAYS)
        } else {
            MAX_INTERVAL_DAYS
        };
        let interval_modifier = round_to((request_retention.powf(1.0 / decay) - 1.0) / factor, 8);
        Self {
            w,
            maximum_interval,
            enable_fuzz: parameters.enable_fuzz,
            enable_short_term: parameters.enable_short_term,
            learning_steps,
            relearning_steps,
            decay,
            factor,
            interval_modifier,
        }
    }

    /// 计算下一次复习卡片。
    ///
    /// `previous` 为 `None` 表示这是该词的第一张卡（等价上游的 `createEmptyCard`）。
    pub fn next_card(&self, previous: Option<&Card>, grade: Rating, now: DateTime<Utc>) -> Card {
        let mut current = previous.cloned().unwrap_or(Card {
            due: now,
            stability: 0.0,
            difficulty: 0.0,
            elapsed_days: 0.0,
            scheduled_days: 0.0,
            reps: 0,
            lapses: 0,
            learning_steps: 0.0,
            state: CARD_STATE_NEW,
            last_review: None,
        });

        // 上游 `AbstractScheduler.init()`
        let elapsed_days = if current.state != CARD_STATE_NEW {
            current
                .last_review
                .map(|last| date_diff_in_days(last, now))
                .unwrap_or(0)
        } else {
            0
        };
        current.last_review = Some(now);
        current.elapsed_days = elapsed_days as f64;
        current.reps += 1;

        // fuzz 种子：上游 `DefaultInitSeedStrategy`
        let seed = format!(
            "{}_{}_{}",
            now.timestamp_millis(),
            current.reps,
            current.difficulty * current.stability
        );

        match current.state {
            CARD_STATE_NEW => self.new_state(&current, grade, now, &seed),
            CARD_STATE_LEARNING | CARD_STATE_RELEARNING => {
                self.learning_state(&current, grade, now, &seed)
            }
            CARD_STATE_REVIEW => self.review_state(&current, grade, now, &seed),
            _ => current,
        }
    }

    /// 新卡：先算初始稳定性/难度，再走学习步进。
    fn new_state(&self, current: &Card, grade: Rating, now: DateTime<Utc>, seed: &str) -> Card {
        if !self.enable_short_term {
            return self.long_term_state(current, grade, now, seed, 0);
        }
        let mut next = self.next_ds(current, 0.0, grade, None);
        self.apply_learning_steps(&mut next, current, grade, CARD_STATE_LEARNING, now, seed);
        next
    }

    /// 学习态：难度/稳定性按当前状态推进，步进不变。
    fn learning_state(
        &self,
        current: &Card,
        grade: Rating,
        now: DateTime<Utc>,
        seed: &str,
    ) -> Card {
        if !self.enable_short_term {
            return self.long_term_state(current, grade, now, seed, current.elapsed_days as i64);
        }
        let elapsed = current.elapsed_days;
        let mut next = self.next_ds(current, elapsed, grade, None);
        self.apply_learning_steps(&mut next, current, grade, current.state, now, seed);
        next
    }

    /// 复习态：四个评级同时算出，Again 转入重学态。
    fn review_state(&self, current: &Card, grade: Rating, now: DateTime<Utc>, seed: &str) -> Card {
        if !self.enable_short_term {
            return self.long_term_state(current, grade, now, seed, current.elapsed_days as i64);
        }
        let elapsed = current.elapsed_days;
        let retrievability = self.forgetting_curve(elapsed, current.stability);

        let mut again = self.next_ds(current, elapsed, RATING_AGAIN, Some(retrievability));
        let mut hard = self.next_ds(current, elapsed, RATING_HARD, Some(retrievability));
        let mut good = self.next_ds(current, elapsed, RATING_GOOD, Some(retrievability));
        let mut easy = self.next_ds(current, elapsed, RATING_EASY, Some(retrievability));

        // 间隔约束：hard ≤ good < easy
        let mut hard_interval = self.next_interval(hard.stability, elapsed, seed);
        let mut good_interval = self.next_interval(good.stability, elapsed, seed);
        hard_interval = hard_interval.min(good_interval);
        good_interval = good_interval.max(hard_interval + 1.0);
        let easy_interval = self
            .next_interval(easy.stability, elapsed, seed)
            .max(good_interval + 1.0);

        hard.scheduled_days = hard_interval;
        hard.due = now + Duration::days(hard_interval as i64);
        hard.state = CARD_STATE_REVIEW;
        hard.learning_steps = 0.0;

        good.scheduled_days = good_interval;
        good.due = now + Duration::days(good_interval as i64);
        good.state = CARD_STATE_REVIEW;
        good.learning_steps = 0.0;

        easy.scheduled_days = easy_interval;
        easy.due = now + Duration::days(easy_interval as i64);
        easy.state = CARD_STATE_REVIEW;
        easy.learning_steps = 0.0;

        self.apply_learning_steps(
            &mut again,
            current,
            RATING_AGAIN,
            CARD_STATE_RELEARNING,
            now,
            seed,
        );
        again.lapses += 1;

        match grade {
            RATING_AGAIN => again,
            RATING_HARD => hard,
            RATING_GOOD => good,
            _ => easy,
        }
    }

    /// 长期调度器（`enable_short_term = false`）：不走学习步进，直接按间隔毕业。
    fn long_term_state(
        &self,
        current: &Card,
        grade: Rating,
        now: DateTime<Utc>,
        seed: &str,
        elapsed_days: i64,
    ) -> Card {
        let elapsed = elapsed_days as f64;
        let retrievability = if current.state == CARD_STATE_NEW {
            None
        } else {
            Some(self.forgetting_curve(elapsed, current.stability))
        };
        let mut again = self.next_ds(current, elapsed, RATING_AGAIN, retrievability);
        let mut hard = self.next_ds(current, elapsed, RATING_HARD, retrievability);
        let mut good = self.next_ds(current, elapsed, RATING_GOOD, retrievability);
        let mut easy = self.next_ds(current, elapsed, RATING_EASY, retrievability);

        let mut again_interval = self.next_interval(again.stability, elapsed, seed);
        let mut hard_interval = self.next_interval(hard.stability, elapsed, seed);
        let mut good_interval = self.next_interval(good.stability, elapsed, seed);
        let mut easy_interval = self.next_interval(easy.stability, elapsed, seed);
        again_interval = again_interval.min(hard_interval);
        hard_interval = hard_interval.max(again_interval + 1.0);
        good_interval = good_interval.max(hard_interval + 1.0);
        easy_interval = easy_interval.max(good_interval + 1.0);

        for (card, interval) in [
            (&mut again, again_interval),
            (&mut hard, hard_interval),
            (&mut good, good_interval),
            (&mut easy, easy_interval),
        ] {
            card.scheduled_days = interval;
            card.due = now + Duration::days(interval as i64);
            card.state = CARD_STATE_REVIEW;
            card.learning_steps = 0.0;
        }
        again.lapses += 1;

        match grade {
            RATING_AGAIN => again,
            RATING_HARD => hard,
            RATING_GOOD => good,
            _ => easy,
        }
    }

    /// 依据当前卡片与评级算出新的难度/稳定性。
    fn next_ds(
        &self,
        current: &Card,
        elapsed_days: f64,
        grade: Rating,
        retrievability: Option<f64>,
    ) -> Card {
        let (difficulty, stability) = self.next_state(
            Some((current.difficulty, current.stability)),
            elapsed_days,
            grade,
            retrievability,
        );
        let mut card = current.clone();
        card.difficulty = difficulty;
        card.stability = stability;
        card
    }

    /// FSRS-6 的状态推进（对应上游 `algorithm.ts` 的 `next_state`）。
    fn next_state(
        &self,
        memory: Option<(f64, f64)>,
        elapsed_days: f64,
        grade: Rating,
        retrievability: Option<f64>,
    ) -> (f64, f64) {
        let (difficulty, stability) = memory.unwrap_or((0.0, 0.0));
        if difficulty == 0.0 && stability == 0.0 {
            return (
                clamp(self.init_difficulty(grade), 1.0, 10.0),
                self.init_stability(grade),
            );
        }
        if grade == 0 {
            return (difficulty, stability);
        }
        let r = retrievability.unwrap_or_else(|| self.forgetting_curve(elapsed_days, stability));

        let new_stability = if elapsed_days == 0.0 && self.enable_short_term {
            self.next_short_term_stability(stability, grade)
        } else if grade == RATING_AGAIN {
            let after_fail = self.next_forget_stability(difficulty, stability, r);
            let (w17, w18) = if self.enable_short_term {
                (self.w[17], self.w[18])
            } else {
                (0.0, 0.0)
            };
            let minimum = stability / (w17 * w18).exp();
            clamp(round_to(minimum, 8), S_MIN, after_fail)
        } else {
            self.next_recall_stability(difficulty, stability, r, grade)
        };

        let new_difficulty = self.next_difficulty(difficulty, grade);
        (new_difficulty, new_stability)
    }

    /// 初始稳定性 `S_0(G) = max(w[G-1], 0.1)`。
    ///
    /// `grade` 可能是 0（存档里的 `ratingMap` 是裸 `u8`，没有取值校验），
    /// 此时按上游 `Rating.Again` 处理，避免 `grade - 1` 下溢。
    fn init_stability(&self, grade: Rating) -> f64 {
        let index = (grade.max(1) as usize - 1).min(self.w.len() - 1);
        self.w[index].max(0.1)
    }

    /// 初始难度 `D_0(G) = w[4] - e^((G-1)·w[5]) + 1`。
    fn init_difficulty(&self, grade: Rating) -> f64 {
        let d = self.w[4] - ((grade as f64 - 1.0) * self.w[5]).exp() + 1.0;
        round_to(d, 8)
    }

    /// 难度推进（含均值回归）。
    ///
    /// 对应上游的 `linear_damping` + `mean_reversion`，两处都要做 8 位
    /// 四舍五入——少了它难度会带着 ~1e-9 的误差逐次累积，长复习史下会
    /// 反馈到间隔计算，破坏「同一张卡两端结果相同」的不变量。
    fn next_difficulty(&self, difficulty: f64, grade: Rating) -> f64 {
        let delta = -self.w[6] * (grade as f64 - 3.0);
        let next = difficulty + round_to((delta * (10.0 - difficulty)) / 9.0, 8);
        let reverted = round_to(
            self.w[7] * self.init_difficulty(RATING_EASY) + (1.0 - self.w[7]) * next,
            8,
        );
        clamp(reverted, 1.0, 10.0)
    }

    /// 记住时的稳定性。
    fn next_recall_stability(
        &self,
        difficulty: f64,
        stability: f64,
        retrievability: f64,
        grade: Rating,
    ) -> f64 {
        let hard_penalty = if grade == RATING_HARD {
            self.w[15]
        } else {
            1.0
        };
        let easy_bound = if grade == RATING_EASY {
            self.w[16]
        } else {
            1.0
        };
        let value = stability
            * (1.0
                + self.w[8].exp()
                    * (11.0 - difficulty)
                    * stability.powf(-self.w[9])
                    * (((1.0 - retrievability) * self.w[10]).exp() - 1.0)
                    * hard_penalty
                    * easy_bound);
        round_to(clamp(value, S_MIN, S_MAX), 8)
    }

    /// 忘记时的稳定性。
    fn next_forget_stability(&self, difficulty: f64, stability: f64, retrievability: f64) -> f64 {
        let value = self.w[11]
            * difficulty.powf(-self.w[12])
            * ((stability + 1.0).powf(self.w[13]) - 1.0)
            * ((1.0 - retrievability) * self.w[14]).exp();
        round_to(clamp(value, S_MIN, S_MAX), 8)
    }

    /// 短期（同日）复习的稳定性。
    fn next_short_term_stability(&self, stability: f64, grade: Rating) -> f64 {
        let sinc =
            stability.powf(-self.w[19]) * (self.w[17] * (grade as f64 - 3.0 + self.w[18])).exp();
        let masked = if grade >= RATING_HARD {
            sinc.max(1.0)
        } else {
            sinc
        };
        round_to(clamp(stability * masked, S_MIN, S_MAX), 8)
    }

    /// 记忆保持率 `R(t, S) = (1 + factor · t / S)^decay`。
    fn forgetting_curve(&self, elapsed_days: f64, stability: f64) -> f64 {
        round_to(
            (1.0 + self.factor * elapsed_days / stability).powf(self.decay),
            8,
        )
    }

    /// 下次复习间隔（天）。
    fn next_interval(&self, stability: f64, elapsed_days: f64, seed: &str) -> f64 {
        let raw = clamp(
            js_round(stability * self.interval_modifier),
            1.0,
            self.maximum_interval,
        );
        self.apply_fuzz(raw, elapsed_days, seed)
    }

    /// 间隔抖动（对应上游 `apply_fuzz` + `get_fuzz_range`）。
    fn apply_fuzz(&self, interval: f64, elapsed_days: f64, seed: &str) -> f64 {
        if !self.enable_fuzz || interval < 2.5 {
            return js_round(interval);
        }
        let mut generator = Alea::new(seed);
        let factor = generator.next();
        let (min_ivl, max_ivl) = self.fuzz_range(interval, elapsed_days);
        (factor * (max_ivl - min_ivl + 1.0) + min_ivl).floor()
    }

    /// 抖动区间（对应上游 `get_fuzz_range`）。
    fn fuzz_range(&self, interval: f64, elapsed_days: f64) -> (f64, f64) {
        const FUZZ_RANGES: [(f64, f64, f64); 3] = [
            (2.5, 7.0, 0.15),
            (7.0, 20.0, 0.1),
            (20.0, f64::INFINITY, 0.05),
        ];
        let mut delta = 1.0;
        for (start, end, factor) in FUZZ_RANGES {
            delta += factor * (interval.min(end) - start).max(0.0);
        }
        let interval = interval.min(self.maximum_interval);
        let mut min_ivl = js_round(interval - delta).max(2.0);
        let max_ivl = js_round(interval + delta).min(self.maximum_interval);
        if interval > elapsed_days {
            min_ivl = min_ivl.max(elapsed_days + 1.0);
        }
        (min_ivl.min(max_ivl), max_ivl)
    }

    /// 学习步进落到卡片上（对应上游 `applyLearningSteps`）。
    fn apply_learning_steps(
        &self,
        next: &mut Card,
        current: &Card,
        grade: Rating,
        to_state: u8,
        now: DateTime<Utc>,
        seed: &str,
    ) {
        let info = learning_step_info(
            &self.learning_steps,
            &self.relearning_steps,
            current.state,
            current.learning_steps as usize,
            grade,
        );
        let (scheduled_minutes, next_step) = match info {
            Some(info) => (info.scheduled_minutes, info.next_step),
            None => (0.0, 0),
        };

        if scheduled_minutes > 0.0 && scheduled_minutes < MINUTES_PER_DAY {
            next.learning_steps = next_step as f64;
            next.scheduled_days = 0.0;
            next.state = to_state;
            next.due = now + Duration::minutes(js_round(scheduled_minutes) as i64);
        } else {
            next.state = CARD_STATE_REVIEW;
            if scheduled_minutes >= MINUTES_PER_DAY {
                next.learning_steps = next_step as f64;
                next.due = now + Duration::minutes(js_round(scheduled_minutes) as i64);
                next.scheduled_days = (scheduled_minutes / MINUTES_PER_DAY).floor();
            } else {
                next.learning_steps = 0.0;
                let interval = self.next_interval(next.stability, current.elapsed_days, seed);
                next.scheduled_days = interval;
                next.due = now + Duration::days(interval as i64);
            }
        }
    }
}

/// 衰减系数与因子（对应上游 `computeDecayFactor`）。
fn compute_decay_factor(w: &[f64]) -> (f64, f64) {
    let decay = -w[20];
    let factor = round_to((decay.powi(-1) * 0.9f64.ln()).exp() - 1.0, 8);
    (decay, factor)
}

/// UTC 日期差（对应上游 `dateDiffInDays`）。
fn date_diff_in_days(last: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now.date_naive() - last.date_naive()).num_days()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::{CARD_STATE_REVIEW, RATING_GOOD};

    fn fsrs() -> Fsrs {
        Fsrs::new(&FsrsParameters {
            w: default_w().to_vec(),
            ..FsrsParameters::default()
        })
    }

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("时间")
            .with_timezone(&Utc)
    }

    /// `fsrs-6.6.2` 自带的已知答案（`inference.rs` 的 `test_next_states`）。
    #[test]
    fn new_card_matches_known_answers() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let close = |actual: f64, expected: f64| {
            assert!(
                (actual - expected).abs() < 1e-7,
                "期望 {expected}，实际 {actual}"
            );
        };

        let again = scheduler.next_card(None, RATING_AGAIN, now);
        close(again.stability, 0.212);
        close(again.difficulty, 6.4133);

        let hard = scheduler.next_card(None, RATING_HARD, now);
        close(hard.stability, 1.2931);
        close(hard.difficulty, 5.1121707);

        let good = scheduler.next_card(None, RATING_GOOD, now);
        close(good.stability, 2.3065);
        close(good.difficulty, 2.118104);

        let easy = scheduler.next_card(None, RATING_EASY, now);
        close(easy.stability, 8.2956);
        close(easy.difficulty, 1.0);
    }

    #[test]
    fn new_card_enters_learning_with_configured_step() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let card = scheduler.next_card(None, RATING_GOOD, now);
        assert_eq!(card.state, CARD_STATE_LEARNING);
        assert_eq!(card.learning_steps, 1.0);
        // learning_steps = ["1m", "10m"]，Good 之后停在第二步 → 10 分钟
        assert_eq!(card.due, now + Duration::minutes(10));
        assert_eq!(card.scheduled_days, 0.0);
    }

    #[test]
    fn again_returns_to_the_first_step() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let first = scheduler.next_card(None, RATING_GOOD, now);
        let again = scheduler.next_card(Some(&first), RATING_AGAIN, now);
        assert_eq!(again.learning_steps, 0.0);
        assert_eq!(again.due, now + Duration::minutes(1));
    }

    #[test]
    fn graduating_from_learning_enters_review() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let first = scheduler.next_card(None, RATING_GOOD, now);
        // 步进已到最后（learning_steps = 1），再 Good 一次没有下一步 → 毕业
        let second = scheduler.next_card(Some(&first), RATING_GOOD, now);
        assert_eq!(second.state, CARD_STATE_REVIEW);
        assert!(second.scheduled_days >= 1.0);
    }

    #[test]
    fn review_lapse_moves_to_relearning() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let mut card = scheduler.next_card(None, RATING_EASY, now);
        // 直接推进到 Review
        card.state = CARD_STATE_REVIEW;
        card.learning_steps = 0.0;
        card.last_review = Some(now);
        let later = now + Duration::days(3);
        let lapsed = scheduler.next_card(Some(&card), RATING_AGAIN, later);
        assert_eq!(lapsed.state, CARD_STATE_RELEARNING);
        assert_eq!(lapsed.lapses, 1);
    }

    #[test]
    fn review_intervals_are_ordered_hard_good_easy() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let mut card = scheduler.next_card(None, RATING_EASY, now);
        card.state = CARD_STATE_REVIEW;
        card.learning_steps = 0.0;
        card.last_review = Some(now);
        let later = now + Duration::days(5);

        let hard = scheduler.next_card(Some(&card), RATING_HARD, later);
        let good = scheduler.next_card(Some(&card), RATING_GOOD, later);
        let easy = scheduler.next_card(Some(&card), RATING_EASY, later);
        assert!(hard.scheduled_days <= good.scheduled_days);
        assert!(good.scheduled_days < easy.scheduled_days);
    }

    #[test]
    fn elapsed_days_uses_utc_dates() {
        let scheduler = fsrs();
        let mut card = scheduler.next_card(None, RATING_EASY, at("2026-01-01T23:30:00Z"));
        card.state = CARD_STATE_REVIEW;
        card.learning_steps = 0.0;
        card.last_review = Some(at("2026-01-01T23:30:00Z"));
        // 跨了 UTC 日界
        let next = scheduler.next_card(Some(&card), RATING_GOOD, at("2026-01-02T00:30:00Z"));
        assert_eq!(next.elapsed_days, 1.0);
    }

    #[test]
    fn fuzz_is_disabled_by_default() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let mut card = scheduler.next_card(None, RATING_EASY, now);
        card.state = CARD_STATE_REVIEW;
        card.learning_steps = 0.0;
        card.last_review = Some(now);
        let later = now + Duration::days(5);
        let first = scheduler.next_card(Some(&card), RATING_GOOD, later);
        let second = scheduler.next_card(Some(&card), RATING_GOOD, later);
        assert_eq!(
            first.scheduled_days, second.scheduled_days,
            "关闭 fuzz 时同一张卡的结果必须稳定"
        );
    }

    #[test]
    fn fuzz_is_deterministic_for_the_same_card() {
        let scheduler = Fsrs::new(&FsrsParameters {
            w: default_w().to_vec(),
            enable_fuzz: true,
            ..FsrsParameters::default()
        });
        let now = at("2026-01-01T00:00:00Z");
        let mut card = scheduler.next_card(None, RATING_EASY, now);
        card.state = CARD_STATE_REVIEW;
        card.learning_steps = 0.0;
        card.last_review = Some(now);
        let later = now + Duration::days(5);
        let first = scheduler.next_card(Some(&card), RATING_GOOD, later);
        let second = scheduler.next_card(Some(&card), RATING_GOOD, later);
        assert_eq!(first.scheduled_days, second.scheduled_days);
        assert_eq!(first.due, second.due);
    }

    /// 步进串的解析语义必须与上游 `ConvertStepUnitToMinutes` 一致。
    ///
    /// 单位是最后一个字符，前缀按 `parseInt(_, 10)` 解析（可选正负号 + 连续
    /// 十进制数字，前导空白与数字后面的垃圾都被忽略）。此前 TUI 用
    /// `f64::parse`：`"10 m"` 解析失败、`"1.5m"` 得到 1.5 分钟、
    /// `"1e300d"` 得到天文数字——与上游三处都不一样。
    #[test]
    fn step_units_are_parsed_like_the_upstream_parse_int() {
        let cases = [
            ("1m", Some(1.0)),
            ("10m", Some(10.0)),
            ("2h", Some(120.0)),
            ("1d", Some(1440.0)),
            // parseInt 忽略前导空白与数字后面的垃圾
            ("10 m", Some(10.0)),
            ("1.5m", Some(1.0)),
            // parseInt("1e300", 10) 是 1，不是 1e300
            ("1e300d", Some(1440.0)),
            // radix 10：`0x10` 只取到开头的 0
            ("0x10m", Some(0.0)),
            // 换算后超过上限的仍按上限封顶（第二道防线）
            ("999999999999999d", Some(MAX_STEP_MINUTES)),
            // 无数字 / 负数 / 非法单位
            ("", None),
            ("1分", None),
            ("-1m", None),
            ("abc", None),
            ("x", None),
            ("infm", None),
            ("NaNm", None),
        ];
        for (step, expected) in cases {
            assert_eq!(step_to_minutes(step), expected, "步进 {step:?} 解析错误");
        }
    }

    /// 超长数字串累加到非有限数：JS 的 `parseInt` 返回 Infinity 后上游按
    /// 非法处理，这里同样返回 `None`（不能把 Infinity 当成分钟数）。
    #[test]
    fn absurdly_long_step_numbers_are_rejected() {
        let step = format!("{}d", "9".repeat(400));
        assert_eq!(step_to_minutes(&step), None);
        // 这类步进同样触发整组回退：卡片留在学习阶段，不会毕业。
        let scheduler = Fsrs::new(&FsrsParameters {
            learning_steps: vec![step],
            ..FsrsParameters::default()
        });
        let now = at("2026-01-01T00:00:00Z");
        let card = scheduler.next_card(None, RATING_GOOD, now);
        assert_eq!(card.state, CARD_STATE_LEARNING);
        assert!(card.scheduled_days < 1.0);
    }

    /// 极端步进串不能把到期时间推到 chrono 的上界之外。
    ///
    /// 步进串可以手改 `setting.json`：`"999999999999999d"` 按上游语义是
    /// 1e15 天量级，原样转成 `Duration::minutes` 会让结算路径 panic，
    /// 因此换算结果封顶到 `MAX_STEP_MINUTES`。`"1e300d"` 则因为
    /// `parseInt("1e300", 10) == 1` 只得到 1 天（与上游一致）。
    #[test]
    fn extreme_learning_steps_are_capped() {
        assert_eq!(step_to_minutes("999999999999999d"), Some(MAX_STEP_MINUTES));
        assert_eq!(step_to_minutes("1e300d"), Some(1440.0));
        assert_eq!(step_to_minutes("infm"), None);
        assert_eq!(step_to_minutes("NaNm"), None);

        let now = at("2026-01-01T00:00:00Z");
        // 封顶生效：Again 把卡片排到上限（36500 天），而不是 1e15 天。
        let scheduler = Fsrs::new(&FsrsParameters {
            learning_steps: vec!["999999999999999d".to_string()],
            ..FsrsParameters::default()
        });
        let card = scheduler.next_card(None, RATING_AGAIN, now);
        assert_eq!(card.due, now + Duration::days(MAX_INTERVAL_DAYS as i64));
        // parseInt 语义：`"1e300d"` 是 1 天。
        let scheduler = Fsrs::new(&FsrsParameters {
            learning_steps: vec!["1e300d".to_string()],
            ..FsrsParameters::default()
        });
        let card = scheduler.next_card(None, RATING_AGAIN, now);
        assert_eq!(card.due, now + Duration::days(1));
    }

    /// 非法步进整组回退内置默认值：卡片留在学习阶段，而不是直接毕业。
    ///
    /// 此前 `step_to_minutes` 返回 `None` 会被 `learning_step_info` 的 `?`
    /// 一路传上去，`apply_learning_steps` 落到毕业分支——手改一个错别字就把
    /// 卡片排到 ≥1 天的 FSRS 间隔，与上游「抛 `FSRSValidationError`」的可见
    /// 错误相比完全是静默的语义改变。
    #[test]
    fn invalid_steps_fall_back_to_the_built_in_defaults() {
        let parameters = FsrsParameters {
            learning_steps: vec!["abc".to_string(), "10m".to_string()],
            relearning_steps: vec!["-1m".to_string()],
            ..FsrsParameters::default()
        };
        assert_eq!(
            invalid_steps(&parameters),
            vec!["abc".to_string(), "-1m".to_string()]
        );

        let scheduler = Fsrs::new(&parameters);
        assert_eq!(
            scheduler.learning_steps,
            vec!["1m".to_string(), "10m".to_string()],
            "非法学习步进应整组回退默认"
        );
        assert_eq!(
            scheduler.relearning_steps,
            vec!["10m".to_string()],
            "非法重学步进应整组回退默认"
        );

        let now = at("2026-01-01T00:00:00Z");
        let card = scheduler.next_card(None, RATING_GOOD, now);
        assert_eq!(card.state, CARD_STATE_LEARNING, "非法步进不应让新卡毕业");
        assert_eq!(card.due, now + Duration::minutes(10));
        assert!(card.scheduled_days < 1.0, "新卡不该被排到 ≥1 天");

        // 全部合法时原样保留，也不产生「非法步进」报告。
        let defaults = FsrsParameters::default();
        assert!(invalid_steps(&defaults).is_empty());
        assert_eq!(
            Fsrs::new(&defaults).learning_steps,
            vec!["1m".to_string(), "10m".to_string()]
        );
        // 空列表是「用默认值」的正常写法，不算非法。
        let empty = FsrsParameters {
            learning_steps: Vec::new(),
            relearning_steps: Vec::new(),
            ..FsrsParameters::default()
        };
        assert!(invalid_steps(&empty).is_empty());
    }

    /// 最大间隔超出界面可调区间时按 36500 生效。
    ///
    /// 手改 `setting.json` 写 `1e300` 能通过 serde（任意 f64 都合法），原样参与
    /// 调度会让 `next_interval` 返回天文数字、`now + Duration::days(_)` 越界
    /// panic。
    #[test]
    fn maximum_interval_outside_the_documented_range_falls_back_to_the_default() {
        let capped = |value: f64| {
            Fsrs::new(&FsrsParameters {
                maximum_interval: value,
                ..FsrsParameters::default()
            })
            .maximum_interval
        };
        assert_eq!(capped(1e300), MAX_INTERVAL_DAYS);
        assert_eq!(capped(f64::INFINITY), MAX_INTERVAL_DAYS);
        assert_eq!(capped(0.0), MAX_INTERVAL_DAYS);
        assert_eq!(capped(MAX_INTERVAL_DAYS), MAX_INTERVAL_DAYS);
        // 区间内的值保持原样，不破坏与上游一致的调度
        assert_eq!(capped(30.0), 30.0);
    }

    /// 非 ASCII 的步进串不能按字节切分。
    ///
    /// 步进串来自用户可编辑的 `setting.json`，`"1分"` 按字节切会落在
    /// UTF-8 边界中间直接 panic。单位取最后一个**字符**（不是字节），因此
    /// 多字节单位只是「不认识的单位」而返回 `None`。
    #[test]
    fn non_ascii_step_units_do_not_panic() {
        assert_eq!(step_to_minutes("1分"), None);
        assert_eq!(step_to_minutes(""), None);
        // 单位是 ASCII、前缀里有空格时按 parseInt 取数字
        assert_eq!(step_to_minutes("10 m"), Some(10.0));
        assert_eq!(step_to_minutes(" 10m"), Some(10.0));
        assert_eq!(step_to_minutes("10m"), Some(10.0));
    }

    /// 存档里的评级可能是 0，不能让它把下标算成 usize::MAX。
    #[test]
    fn zero_rating_does_not_underflow() {
        let scheduler = fsrs();
        let now = at("2026-01-01T00:00:00Z");
        let card = scheduler.next_card(None, 0, now);
        assert!(card.stability.is_finite());
        assert!(card.difficulty.is_finite());
        assert!(card.difficulty >= 1.0 && card.difficulty <= 10.0);
    }

    /// 难度推进必须与上游的 `roundTo(_, 8)` 一致。
    ///
    /// 少了这两处四舍五入，难度会带着 ~1e-9 的误差逐次累积，长复习史下
    /// 会反馈到间隔计算，破坏「同一张卡两端结果相同」的不变量。
    #[test]
    fn difficulty_matches_upstream_rounding() {
        let scheduler = fsrs();
        let mut difficulty = 7.0f64;
        let mut expected = 7.0f64;
        for _ in 0..16 {
            let actual = scheduler.next_difficulty(difficulty, RATING_GOOD);
            // 上游：linear_damping 与 mean_reversion 各做一次 roundTo(_, 8)
            let init = {
                let d = scheduler.w[4] - ((4.0f64 - 1.0) * scheduler.w[5]).exp() + 1.0;
                round_to(d, 8)
            };
            let damping = round_to((0.0 * (10.0 - expected)) / 9.0, 8);
            let reverted = round_to(
                scheduler.w[7] * init + (1.0 - scheduler.w[7]) * (expected + damping),
                8,
            );
            expected = clamp(reverted, 1.0, 10.0);
            assert_eq!(
                actual, expected,
                "难度推进应与上游逐位相同（difficulty={difficulty}）"
            );
            difficulty = actual;
        }
    }

    #[test]
    fn clipped_parameters_stay_in_range() {
        let clipped = clip_parameters(&[0.0; 21], 1, true);
        assert_eq!(clipped.len(), 21);
        assert!(clipped[20] >= 0.1 && clipped[20] <= 0.8);
    }

    /// 设置页显示的权重区间必须与调度器实际夹取用的是同一份表。
    #[test]
    fn weight_bounds_match_the_clipping_ranges() {
        let parameters = FsrsParameters::default();
        assert_eq!(weight_bounds(&parameters, 0), (S_MIN, INIT_S_MAX));
        assert_eq!(weight_bounds(&parameters, 16), (1.0, 6.0));
        assert_eq!(weight_bounds(&parameters, 19), (0.01, 0.8));
        assert_eq!(weight_bounds(&parameters, 20), (0.1, 0.8));
        // 关掉短期调度后 w[19] 的下限放宽到 0
        let long_term = FsrsParameters {
            enable_short_term: false,
            ..FsrsParameters::default()
        };
        assert_eq!(weight_bounds(&long_term, 19), (0.0, 0.8));
    }

    /// 多步重学收紧 w[17]/w[18] 的上限：界面区间与 clip 结果必须是同一个数。
    #[test]
    fn weight_bounds_tighten_w17_w18_with_two_relearning_steps() {
        let parameters = FsrsParameters {
            relearning_steps: vec!["10m".to_string(), "1d".to_string()],
            ..FsrsParameters::default()
        };
        let ceiling = weight_bounds(&parameters, 17).1;
        assert!(
            ceiling > 0.01 && ceiling < W17_W18_CEILING,
            "重学步数 > 1 时上限应被收紧，实际 {ceiling}"
        );
        assert_eq!(weight_bounds(&parameters, 18), (0.0, ceiling));
        let mut w = default_w().to_vec();
        w[17] = 1.9;
        w[18] = 1.9;
        let clipped = clip_parameters(&w, 2, true);
        assert!((clipped[17] - ceiling).abs() < 1e-12);
        assert!((clipped[18] - ceiling).abs() < 1e-12);
    }

    /// 上游 `migrateParameters` 的长度分流：21 只夹取、19 补 `[0, 0.5]`、
    /// 17 先变换 `w[4]` / `w[5]` / `w[6]` 再补 `[0, 0, 0, 0.5]`，其它长度整组
    /// 回退默认。
    ///
    /// 改前是「取前 min(len, 21) 项夹取 + 不足 21 用默认值补齐」：19 项会得到
    /// FSRS-6 的 `w[19] = 0.0658`（上游是 0），22 项会保留前 21 项（上游整组
    /// 默认）。这三处差异都锁在这里。
    #[test]
    fn migrate_parameters_follows_the_upstream_length_branches() {
        let defaults = default_w();

        // 21 项：只夹取，逐项与改前完全一致。
        let mut w = defaults.to_vec();
        w[0] = 1e9; // 越界 → 夹到 w[0] 的上限
        let migrated = migrate_parameters(&w, 1, true);
        assert_eq!(migrated.len(), 21);
        assert_eq!(migrated[0], INIT_S_MAX);
        let clipped = clip_parameters(&w, 1, true);
        for (index, value) in migrated.iter().enumerate() {
            assert_eq!(
                *value, clipped[index],
                "21 项应与逐项夹取一致（w[{index}]）"
            );
        }

        // 19 项：夹取前 19 项 + [0, FSRS5_DEFAULT_DECAY]。
        let migrated = migrate_parameters(&defaults[..19], 1, true);
        assert_eq!(migrated.len(), 21);
        assert_eq!(migrated[..19].to_vec(), defaults[..19].to_vec());
        assert_eq!(migrated[19], 0.0, "w[19] 应是 0，而不是 FSRS-6 的默认值");
        assert_eq!(migrated[20], FSRS5_DEFAULT_DECAY);

        // 17 项：w[4] / w[5] / w[6] 按 FSRS-5 → FSRS-6 的公式变换（8 位小数）。
        // 手算（默认权重前 17 项，全部落在夹取区间内）：
        //   w[4] = 0.8334 * 2 + 6.4133 = 8.0801
        //   w[5] = ln(0.8334 * 3 + 1) / 3 = 0.41760670…
        //   w[6] = 3.0194 + 0.5 = 3.5194
        let migrated = migrate_parameters(&defaults[..17], 1, true);
        assert_eq!(migrated.len(), 21);
        assert!(
            (migrated[4] - 8.0801).abs() < 1e-9,
            "w[4] 实际 {}",
            migrated[4]
        );
        assert!(
            (migrated[5] - 0.4176067).abs() < 1e-9,
            "w[5] 实际 {}",
            migrated[5]
        );
        assert!(
            (migrated[6] - 3.5194).abs() < 1e-9,
            "w[6] 实际 {}",
            migrated[6]
        );
        assert_eq!(
            migrated[17..].to_vec(),
            vec![0.0, 0.0, 0.0, FSRS5_DEFAULT_DECAY]
        );
        // 变换只动这三项，其余 14 项原样保留。
        for index in [0, 1, 2, 3, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16] {
            assert_eq!(migrated[index], defaults[index], "w[{index}] 不应被改动");
        }

        // 其它长度：空与 22 项都整组回退默认（一项都不保留）。
        assert_eq!(migrate_parameters(&[], 1, true), defaults.to_vec());
        let mut longer = defaults.to_vec();
        longer.push(9.0);
        longer.push(9.0);
        assert_eq!(migrate_parameters(&longer, 1, true), defaults.to_vec());
    }

    /// `request_retention == 1.0` 回退 0.9，这是登记过的有意差异。
    ///
    /// 上游 `calculate_interval_modifier` 只拒绝 `<= 0 || > 1`，`1.0` 合法，
    /// 但会让 `interval_modifier` 恒为 0、所有复习卡都排到次日。TUI 选择回退
    /// 到文档里的默认值 0.9（与越界值同一口径），行为与「填 0.9」完全一致。
    #[test]
    fn request_retention_of_one_falls_back_to_the_documented_default() {
        let with = |retention: f64| {
            Fsrs::new(&FsrsParameters {
                request_retention: retention,
                ..FsrsParameters::default()
            })
        };
        let baseline = with(0.9);
        for retention in [1.0, 1.000_000_1, 95.0, 0.0, -0.5] {
            assert_eq!(
                with(retention).interval_modifier,
                baseline.interval_modifier,
                "留存率 {retention} 应回退到 0.9"
            );
        }

        // 行为上等价：同一张新卡评 Easy 的到期与 0.9 一致。
        let now = at("2026-01-01T00:00:00Z");
        let one = with(1.0).next_card(None, RATING_EASY, now);
        let default = baseline.next_card(None, RATING_EASY, now);
        assert_eq!(one.due, default.due);
        assert_eq!(one.scheduled_days, default.scheduled_days);
    }
}
