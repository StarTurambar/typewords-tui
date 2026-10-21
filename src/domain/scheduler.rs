//! FSRS 调度（评级映射 + 间隔计算）。
//!
//! 间隔计算委托给 [`crate::domain::fsrs`]——那里是 ts-fsrs 5.4.1 的逐条移植，
//! 学习步进、`enable_short_term`、fuzz 区间与随机种子都与上游一致，因此
//! 「同一张卡」在两端得到相同的下次复习时间。
//!
//! 本模块只保留与用户设置直接相关的两块：按错误次数映射评级、算定到期日。

use chrono::{DateTime, NaiveDate, Utc};

use crate::domain::fsrs::Fsrs;
use crate::domain::types::{Card, Rating, SettingState};

pub struct Scheduler {
    fsrs: Fsrs,
    easy_limit: i64,
    good_limit: i64,
    hard_limit: i64,
}

impl Scheduler {
    pub fn new(settings: &SettingState) -> Self {
        Self {
            fsrs: Fsrs::new(&settings.fsrs_parameters),
            easy_limit: settings.fsrs_easy_limit,
            good_limit: settings.fsrs_good_limit,
            hard_limit: settings.fsrs_hard_limit,
        }
    }

    /// 按错误次数评级（与 Web 的 getGradeByWrongTimes 一致）。
    ///
    /// 错得越少评级越高：0 次 → Easy，1~3 次 → Good，4~6 次 → Hard，更多 → Again。
    pub fn grade_by_wrong_times(&self, wrong_times: Option<i64>) -> Rating {
        match wrong_times {
            None => crate::domain::types::RATING_EASY,
            Some(times) if times <= self.easy_limit => crate::domain::types::RATING_EASY,
            Some(times) if times <= self.good_limit => crate::domain::types::RATING_GOOD,
            Some(times) if times <= self.hard_limit => crate::domain::types::RATING_HARD,
            Some(_) => crate::domain::types::RATING_AGAIN,
        }
    }

    /// 计算下一次复习卡片。
    pub fn next_card(&self, previous: Option<&Card>, rating: Rating, now: DateTime<Utc>) -> Card {
        self.fsrs.next_card(previous, rating, now)
    }

    /// 卡片是否在今天到期（按自然日比较，与 Web 一致）。
    pub fn is_due_today(card: &Card, today: NaiveDate) -> bool {
        card.due.with_timezone(&chrono::Local).date_naive() <= today
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::{RATING_AGAIN, RATING_EASY, RATING_GOOD, RATING_HARD};

    fn scheduler() -> Scheduler {
        Scheduler::new(&SettingState::default())
    }

    #[test]
    fn rating_thresholds_match_upstream() {
        let scheduler = scheduler();
        assert_eq!(scheduler.grade_by_wrong_times(None), RATING_EASY);
        assert_eq!(scheduler.grade_by_wrong_times(Some(0)), RATING_EASY);
        assert_eq!(scheduler.grade_by_wrong_times(Some(1)), RATING_GOOD);
        assert_eq!(scheduler.grade_by_wrong_times(Some(3)), RATING_GOOD);
        assert_eq!(scheduler.grade_by_wrong_times(Some(4)), RATING_HARD);
        assert_eq!(scheduler.grade_by_wrong_times(Some(6)), RATING_HARD);
        assert_eq!(scheduler.grade_by_wrong_times(Some(7)), RATING_AGAIN);
    }

    #[test]
    fn next_card_is_stable_for_the_same_inputs() {
        let scheduler = scheduler();
        let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let first = scheduler.next_card(None, RATING_GOOD, now);
        let second = scheduler.next_card(None, RATING_GOOD, now);
        assert_eq!(first.stability, second.stability);
        assert_eq!(first.difficulty, second.difficulty);
        assert_eq!(first.due, second.due);
    }
}
