//! FSRS 与上游 ts-fsrs 的黄金样本比对。
//!
//! 样本由 `tools/fsrs_golden.mjs` 在上游 TypeWords 仓库里用 ts-fsrs 5.4.1 生成，
//! 覆盖新卡起步、学习步进、毕业复习、同日短期记忆、fuzz 与长期调度六类路径。
//! 任何一侧改动算法都会在这里暴露。

use chrono::{DateTime, Utc};
use serde::Deserialize;
use typewords_tui::domain::fsrs::Fsrs;
use typewords_tui::domain::types::{Card, FsrsParameters};

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
