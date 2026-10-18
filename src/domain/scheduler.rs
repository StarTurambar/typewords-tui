//! FSRS 调度（评级映射 + 间隔计算）。
//!
//! 与 Web 端 ts-fsrs 的差异见 docs/upstream-parity.md：评级完全一致；间隔使用
//! `fsrs` crate 的 FSRS-6 计算，学习阶段采用固定步进（1/6/10 分钟）。
//! 设置 `enable_fuzz` 开启时，对 >= 2.5 天的复习间隔做 ±5% 的随机抖动（近似 ts-fsrs，
//! 不逐项复刻其离散随机规则）。

use chrono::{DateTime, Duration, Utc};
use fsrs::{FSRS, MemoryState};
use rand::RngExt;

use crate::domain::rng::Rng;

use crate::domain::types::{
    CARD_STATE_LEARNING, CARD_STATE_NEW, CARD_STATE_RELEARNING, CARD_STATE_REVIEW, Card,
    RATING_AGAIN, RATING_EASY, RATING_GOOD, RATING_HARD, Rating, SettingState,
};

/// 学习阶段步进（分钟）。
const STEP_AGAIN_MINUTES: i64 = 1;
const STEP_HARD_MINUTES: i64 = 6;
const STEP_GOOD_MINUTES: i64 = 10;

/// 卡片计数字段（打包传递，避免调度方法的参数列表过长）。
#[derive(Clone, Copy)]
struct CardCounters {
    reps: u32,
    lapses: u32,
    elapsed_days: u32,
}

pub struct Scheduler {
    fsrs: FSRS,
    retention: f32,
    maximum_interval: f64,
    easy_limit: i64,
    good_limit: i64,
    hard_limit: i64,
    enable_fuzz: bool,
}

impl Scheduler {
    pub fn new(settings: &SettingState) -> Self {
        let weights = settings.fsrs_parameters.effective_w();
        let fsrs = FSRS::new(&weights).unwrap_or_else(|_| FSRS::default());
        Self {
            fsrs,
            retention: settings.fsrs_parameters.effective_retention(),
            maximum_interval: settings.fsrs_parameters.effective_maximum_interval(),
            easy_limit: settings.fsrs_easy_limit,
            good_limit: settings.fsrs_good_limit,
            hard_limit: settings.fsrs_hard_limit,
            enable_fuzz: settings.fsrs_parameters.enable_fuzz,
        }
    }

    /// 对较长间隔做 ±5% 的随机抖动（仅 `enable_fuzz` 开启且间隔 >= 2.5 天）。
    fn fuzz_interval(&self, interval: f64, rng: &mut Rng) -> f64 {
        const FUZZ_MIN_DAYS: f64 = 2.5;
        if !self.enable_fuzz || interval < FUZZ_MIN_DAYS {
            return interval;
        }
        let factor = rng.random_range(0.95..=1.05);
        (interval * factor).max(1.0)
    }

    /// 按错误次数评级（与 Web 的 getGradeByWrongTimes 一致）。
    pub fn grade_by_wrong_times(&self, wrong_times: Option<i64>) -> Rating {
        match wrong_times {
            None => RATING_EASY,
            Some(times) if times <= self.easy_limit => RATING_EASY,
            Some(times) if times <= self.good_limit => RATING_GOOD,
            Some(times) if times <= self.hard_limit => RATING_HARD,
            Some(_) => RATING_AGAIN,
        }
    }

    fn memory_state(card: &Card) -> Option<MemoryState> {
        if card.stability > 0.0 && card.difficulty > 0.0 {
            Some(MemoryState {
                stability: card.stability as f32,
                difficulty: card.difficulty as f32,
            })
        } else {
            None
        }
    }

    fn next_interval_days(
        &self,
        previous: Option<&Card>,
        rating: Rating,
        elapsed_days: u32,
    ) -> (f64, f64, f64) {
        let memory = previous.and_then(Self::memory_state);
        let fallback = previous
            .map(|card| (card.stability.max(1.0), card.difficulty.max(1.0)))
            .unwrap_or((1.0, 1.0));
        let Ok(states) = self.fsrs.next_states(memory, self.retention, elapsed_days) else {
            return (fallback.0, fallback.1, 1.0);
        };
        let item = match rating {
            RATING_AGAIN => states.again,
            RATING_HARD => states.hard,
            RATING_GOOD => states.good,
            RATING_EASY => states.easy,
            _ => states.good,
        };
        (
            item.memory.stability as f64,
            item.memory.difficulty as f64,
            item.interval as f64,
        )
    }

    fn graduate(
        &self,
        previous: Option<&Card>,
        rating: Rating,
        now: DateTime<Utc>,
        counters: CardCounters,
        rng: &mut Rng,
    ) -> Card {
        let CardCounters {
            reps,
            lapses,
            elapsed_days,
        } = counters;
        let (stability, difficulty, interval_days) =
            self.next_interval_days(previous, rating, elapsed_days);
        let interval = self
            .fuzz_interval(interval_days, rng)
            .clamp(1.0, self.maximum_interval)
            .round()
            .max(1.0);
        Card {
            due: now + Duration::days(interval as i64),
            stability,
            difficulty,
            elapsed_days: elapsed_days as f64,
            scheduled_days: interval,
            learning_steps: 0.0,
            reps,
            lapses,
            state: CARD_STATE_REVIEW,
            last_review: Some(now),
        }
    }

    fn learning_card(
        &self,
        previous: Option<&Card>,
        rating: Rating,
        now: DateTime<Utc>,
        counters: CardCounters,
    ) -> Card {
        let CardCounters {
            reps,
            lapses,
            elapsed_days,
        } = counters;
        let minutes = match rating {
            RATING_AGAIN => STEP_AGAIN_MINUTES,
            RATING_HARD => STEP_HARD_MINUTES,
            _ => STEP_GOOD_MINUTES,
        };
        let state = match previous.map(|card| card.state) {
            Some(CARD_STATE_RELEARNING) => CARD_STATE_RELEARNING,
            Some(CARD_STATE_LEARNING) => CARD_STATE_LEARNING,
            _ => CARD_STATE_LEARNING,
        };
        let learning_steps = previous
            .map(|card| card.learning_steps + 1.0)
            .unwrap_or(1.0);
        Card {
            due: now + Duration::minutes(minutes),
            stability: previous.map(|card| card.stability).unwrap_or(0.0),
            difficulty: previous.map(|card| card.difficulty).unwrap_or(0.0),
            elapsed_days: elapsed_days as f64,
            scheduled_days: minutes as f64 / 1440.0,
            learning_steps,
            reps,
            lapses,
            state,
            last_review: Some(now),
        }
    }

    /// 计算下一次复习卡片。`rng` 仅用于 `enable_fuzz` 的间隔抖动（关闭时不消耗随机数）。
    pub fn next_card(
        &self,
        previous: Option<&Card>,
        rating: Rating,
        now: DateTime<Utc>,
        rng: &mut Rng,
    ) -> Card {
        let prev_state = previous.map(|card| card.state).unwrap_or(CARD_STATE_NEW);
        let mut counters = CardCounters {
            reps: previous.map(|card| card.reps).unwrap_or(0) + 1,
            lapses: previous.map(|card| card.lapses).unwrap_or(0),
            elapsed_days: match previous.and_then(|card| card.last_review) {
                Some(last) => (now - last).num_days().max(0) as u32,
                None => 0,
            },
        };

        if prev_state == CARD_STATE_REVIEW {
            if rating == RATING_AGAIN {
                counters.lapses += 1;
                let graduated = self.learning_card(previous, RATING_AGAIN, now, counters);
                return Card {
                    state: CARD_STATE_RELEARNING,
                    scheduled_days: STEP_GOOD_MINUTES as f64 / 1440.0,
                    due: now + Duration::minutes(STEP_GOOD_MINUTES),
                    ..graduated
                };
            }
            return self.graduate(previous, rating, now, counters, rng);
        }

        // New / Learning / Relearning
        match rating {
            RATING_EASY => self.graduate(previous, rating, now, counters, rng),
            RATING_GOOD if prev_state != CARD_STATE_NEW => {
                self.graduate(previous, rating, now, counters, rng)
            }
            _ => self.learning_card(previous, rating, now, counters),
        }
    }

    /// 卡片是否在今天到期（按自然日比较，与 Web 一致）。
    pub fn is_due_today(card: &Card, today: chrono::NaiveDate) -> bool {
        card.due.with_timezone(&chrono::Local).date_naive() <= today
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rng;
    use crate::domain::types::FsrsParameters;

    fn scheduler(enable_fuzz: bool) -> Scheduler {
        let settings = SettingState {
            fsrs_parameters: FsrsParameters {
                enable_fuzz,
                ..FsrsParameters::default()
            },
            ..SettingState::default()
        };
        Scheduler::new(&settings)
    }

    #[test]
    fn fuzz_is_off_by_default_and_skips_short_intervals() {
        let mut rng = rng::seeded(7);
        assert_eq!(scheduler(false).fuzz_interval(100.0, &mut rng), 100.0);
        assert_eq!(scheduler(true).fuzz_interval(2.0, &mut rng), 2.0);
    }

    #[test]
    fn fuzz_interval_stays_within_five_percent() {
        let scheduler = scheduler(true);
        let mut rng = rng::seeded(42);
        for _ in 0..100 {
            let value = scheduler.fuzz_interval(100.0, &mut rng);
            assert!((95.0..=105.0).contains(&value), "fuzz 超出 ±5%: {value}");
        }
        let a = scheduler.fuzz_interval(100.0, &mut rng::seeded(1));
        let b = scheduler.fuzz_interval(100.0, &mut rng::seeded(2));
        assert_ne!(a, b, "不同种子应得到不同抖动");
    }
}
