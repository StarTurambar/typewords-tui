//! FSRS 调度（评级映射 + 间隔计算）。
//!
//! 与 Web 端 ts-fsrs 的差异见 docs/TUI.md：评级完全一致；间隔使用
//! `fsrs` crate 的 FSRS-6 计算，学习阶段采用固定步进（1/6/10 分钟）。

use chrono::{DateTime, Duration, Utc};
use fsrs::{FSRS, MemoryState};

use crate::domain::types::{
    CARD_STATE_LEARNING, CARD_STATE_NEW, CARD_STATE_RELEARNING, CARD_STATE_REVIEW, Card,
    RATING_AGAIN, RATING_EASY, RATING_GOOD, RATING_HARD, Rating, SettingState,
};

/// 学习阶段步进（分钟）。
const STEP_AGAIN_MINUTES: i64 = 1;
const STEP_HARD_MINUTES: i64 = 6;
const STEP_GOOD_MINUTES: i64 = 10;

pub struct Scheduler {
    fsrs: FSRS,
    retention: f32,
    maximum_interval: f64,
    easy_limit: i64,
    good_limit: i64,
    hard_limit: i64,
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
        }
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
        reps: u32,
        lapses: u32,
        elapsed_days: u32,
    ) -> Card {
        let (stability, difficulty, interval_days) =
            self.next_interval_days(previous, rating, elapsed_days);
        let interval = interval_days
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
        reps: u32,
        lapses: u32,
        elapsed_days: u32,
    ) -> Card {
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

    /// 计算下一次复习卡片。
    pub fn next_card(&self, previous: Option<&Card>, rating: Rating, now: DateTime<Utc>) -> Card {
        let prev_state = previous.map(|card| card.state).unwrap_or(CARD_STATE_NEW);
        let reps = previous.map(|card| card.reps).unwrap_or(0) + 1;
        let mut lapses = previous.map(|card| card.lapses).unwrap_or(0);
        let elapsed_days = match previous.and_then(|card| card.last_review) {
            Some(last) => (now - last).num_days().max(0) as u32,
            None => 0,
        };

        if prev_state == CARD_STATE_REVIEW {
            if rating == RATING_AGAIN {
                lapses += 1;
                let graduated =
                    self.learning_card(previous, RATING_AGAIN, now, reps, lapses, elapsed_days);
                return Card {
                    state: CARD_STATE_RELEARNING,
                    scheduled_days: STEP_GOOD_MINUTES as f64 / 1440.0,
                    due: now + Duration::minutes(STEP_GOOD_MINUTES),
                    ..graduated
                };
            }
            return self.graduate(previous, rating, now, reps, lapses, elapsed_days);
        }

        // New / Learning / Relearning
        match rating {
            RATING_EASY => self.graduate(previous, rating, now, reps, lapses, elapsed_days),
            RATING_GOOD if prev_state != CARD_STATE_NEW => {
                self.graduate(previous, rating, now, reps, lapses, elapsed_days)
            }
            _ => self.learning_card(previous, rating, now, reps, lapses, elapsed_days),
        }
    }

    /// 卡片是否在今天到期（按自然日比较，与 Web 一致）。
    pub fn is_due_today(card: &Card, today: chrono::NaiveDate) -> bool {
        card.due.with_timezone(&chrono::Local).date_naive() <= today
    }
}
