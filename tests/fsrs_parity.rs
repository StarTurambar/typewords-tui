//! FSRS 与上游 ts-fsrs 的黄金样本比对。
//!
//! 样本由 `tools/fsrs_golden.mjs` 在上游 TypeWords 仓库里用 ts-fsrs 5.4.1 生成，
//! 覆盖新卡起步、学习步进、毕业复习、同日短期记忆、fuzz 与长期调度六类路径。
//! 任何一侧改动算法都会在这里暴露。

use chrono::{DateTime, Utc};
use serde::Deserialize;
use typewords_tui::domain::fsrs::Fsrs;
use typewords_tui::domain::types::{Card, FsrsParameters, RATING_AGAIN, RATING_EASY, RATING_GOOD};

#[derive(Deserialize)]
struct Golden {
    base: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    parameters: GoldenParameters,
    /// 每次操作的时刻与评级
    inputs: Vec<GoldenStep>,
    /// 每一步之后的上游卡片
    results: Vec<GoldenCard>,
}

#[derive(Deserialize)]
struct GoldenStep {
    at: i64,
    grade: u8,
}

#[derive(Deserialize)]
struct GoldenParameters {
    request_retention: f64,
    maximum_interval: f64,
    w: Vec<f64>,
    enable_fuzz: bool,
    enable_short_term: bool,
    learning_steps: Vec<String>,
    relearning_steps: Vec<String>,
}

#[derive(Deserialize)]
struct GoldenCard {
    due: String,
    stability: f64,
    difficulty: f64,
    elapsed_days: f64,
    scheduled_days: f64,
    learning_steps: f64,
    reps: u32,
    lapses: u32,
    state: u8,
    last_review: Option<String>,
}

fn golden() -> Golden {
    let text = include_str!("fixtures/fsrs_golden.json");
    serde_json::from_str(text).expect("黄金样本应可解析")
}

fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= tolerance
}

fn parse_time(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("时间应可解析")
        .with_timezone(&Utc)
}

/// 逐步复现上游的调度序列并逐字段比对。
#[test]
fn fsrs_matches_upstream_golden_sample() {
    let golden = golden();
    let base = parse_time(&golden.base);

    for scenario in &golden.scenarios {
        let parameters = FsrsParameters {
            request_retention: scenario.parameters.request_retention,
            maximum_interval: scenario.parameters.maximum_interval,
            w: scenario.parameters.w.clone(),
            enable_fuzz: scenario.parameters.enable_fuzz,
            enable_short_term: scenario.parameters.enable_short_term,
            learning_steps: scenario.parameters.learning_steps.clone(),
            relearning_steps: scenario.parameters.relearning_steps.clone(),
        };
        let scheduler = Fsrs::new(&parameters);

        let mut previous: Option<Card> = None;
        for (index, step) in scenario.inputs.iter().enumerate() {
            let now = base + chrono::Duration::milliseconds(step.at);
            let card = scheduler.next_card(previous.as_ref(), step.grade, now);
            previous = Some(card.clone());

            let Some(expected) = scenario.results.get(index) else {
                continue;
            };
            let context = format!("场景 {} 第 {} 步", scenario.name, index + 1);

            assert_eq!(
                card.due,
                parse_time(&expected.due),
                "{context}：到期时间不一致"
            );
            assert!(
                close(card.stability, expected.stability, 1e-7),
                "{context}：稳定性不一致，实际 {} 期望 {}",
                card.stability,
                expected.stability
            );
            assert!(
                close(card.difficulty, expected.difficulty, 1e-7),
                "{context}：难度不一致，实际 {} 期望 {}",
                card.difficulty,
                expected.difficulty
            );
            assert!(
                close(card.elapsed_days, expected.elapsed_days, 1e-7),
                "{context}：经过天数不一致"
            );
            assert!(
                close(card.scheduled_days, expected.scheduled_days, 1e-7),
                "{context}：计划天数不一致"
            );
            assert!(
                close(card.learning_steps, expected.learning_steps, 1e-7),
                "{context}：学习步进不一致"
            );
            assert_eq!(card.reps, expected.reps, "{context}：复习次数不一致");
            assert_eq!(card.lapses, expected.lapses, "{context}：遗忘次数不一致");
            assert_eq!(card.state, expected.state, "{context}：卡片状态不一致");
            match (&card.last_review, &expected.last_review) {
                (Some(actual), Some(expected)) => assert_eq!(
                    *actual,
                    parse_time(expected),
                    "{context}：上次复习时间不一致"
                ),
                (None, None) => {}
                _ => panic!("{context}：上次复习时间的有无不一致"),
            }
        }
    }
}

/// 手改 `setting.json` 的 FSRS 参数填进极端值时，结算路径不能 panic。
///
/// 参数兜底只在 `Fsrs::new` 与 `step_to_minutes` 做：留存率取 `1e-9` 时
/// `interval_modifier` 会被推向 ~1e58，最大间隔写 `1e300`、学习步进写
/// `"1e300d"` 也都能通过 serde 与类型校验；原样参与调度会让
/// `now + Duration::days/minutes(_)` 越过 chrono 的 `TimeDelta` 上界
/// （`TimeDelta::days out of bounds`）。结算在写盘前崩溃，当轮进度全丢，且
/// 同一存档每次结算都复现。这里锁定「极端值先夹到安全区间，再排到期」。
///
/// 上游 ts-fsrs 两条路径都不会崩：留存率 / 间隔在 `Math.min/max` 里被夹住，
/// 步进用 `parseInt` 加 `Date` 毫秒加法，溢出只会得到无效日期。
///
/// 与 `reviews/panic-audit.md` 问题 1 同源。
#[test]
fn extreme_fsrs_parameters_do_not_panic_at_settle() {
    let now = parse_time("2026-01-05T00:00:00.000Z");
    // 复习态对 again / hard / good / easy 依次叠加 +1 天约束（长期调度器最远 +3 天），
    // 上限因此略高于最大间隔；这里锁的是「别排到天文数字」，不是精确天数。
    let horizon_at = |base: DateTime<Utc>| base + chrono::Duration::days(36_503);
    let steps = |list: Vec<&str>| FsrsParameters {
        learning_steps: list.into_iter().map(str::to_string).collect(),
        ..FsrsParameters::default()
    };

    // 路径 A：极小留存率 + 极大最大间隔，新卡 Easy 直接毕业进复习。
    let extreme = FsrsParameters {
        request_retention: 1e-9,
        maximum_interval: 1e300,
        ..FsrsParameters::default()
    };
    let scheduler = Fsrs::new(&extreme);
    let card = scheduler.next_card(None, RATING_EASY, now);
    assert!(card.scheduled_days <= 36_500.0, "间隔应夹到最大间隔");
    assert!(
        card.due <= horizon_at(now),
        "到期不能越过 chrono 的安全范围"
    );

    // 路径 A-2：同一组参数落在复习卡上，`review_state` 一次算四张卡的到期时间。
    let later = parse_time("2026-02-05T00:00:00.000Z");
    for enable_short_term in [true, false] {
        let scheduler = Fsrs::new(&FsrsParameters {
            enable_short_term,
            ..extreme.clone()
        });
        let next = scheduler.next_card(Some(&card), RATING_EASY, later);
        assert!(
            next.due <= horizon_at(later),
            "复习态到期越界（enable_short_term = {enable_short_term}）：{next:?}"
        );
    }

    // 路径 B-1：首步极大，Again 会把卡片排到首步时间。
    let scheduler = Fsrs::new(&steps(vec!["999999999999999d"]));
    let card = scheduler.next_card(None, RATING_AGAIN, now);
    assert!(card.due <= horizon_at(now));

    // 路径 B-2：次步极大，Good 会走到下一步。
    let scheduler = Fsrs::new(&steps(vec!["1m", "1e300d"]));
    let card = scheduler.next_card(None, RATING_GOOD, now);
    assert!(card.due <= horizon_at(now));

    // 路径 B-3：非有限数按无法解析处理，退化成「立即毕业」，同样不崩。
    let scheduler = Fsrs::new(&steps(vec!["infm"]));
    let card = scheduler.next_card(None, RATING_AGAIN, now);
    assert!(card.due <= horizon_at(now));
}
