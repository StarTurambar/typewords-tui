//! 主页与统计页使用的纯数据洞察函数。

use chrono::{Datelike, Days, Local, NaiveDate, TimeZone};

use crate::domain::statistics::{DayTotals, daily_totals, daily_totals_for_dict};
use crate::domain::types::{BaseState, Dict};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub spend: i64,
    pub total: usize,
    pub new_words: usize,
    pub review: usize,
    pub wrong: usize,
    pub days: usize,
}

pub fn totals(base: &BaseState) -> Totals {
    let mut result = Totals::default();
    for (_, day) in daily_totals(base) {
        result.spend += day.spend;
        result.total += day.total;
        result.new_words += day.new_words;
        result.review += day.review;
        result.wrong += day.wrong;
        result.days += 1;
    }
    result
}

pub fn accuracy(totals: &Totals) -> u32 {
    if totals.total == 0 {
        return 100;
    }
    (((totals.total.saturating_sub(totals.wrong)) as f64 / totals.total as f64) * 100.0).round()
        as u32
}

/// 本周一~周日，缺失的日子为 None。
pub fn week_days(base: &BaseState, today: NaiveDate) -> [Option<DayTotals>; 7] {
    let map = daily_totals(base);
    let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
    let mut week = [None; 7];
    for (index, slot) in week.iter_mut().enumerate() {
        let date = monday + Days::new(index as u64);
        if let Some(value) = map.get(&date) {
            *slot = Some(*value);
        }
    }
    week
}

/// 本周一~周日，**只统计指定词典**（结算弹层的本周记录用）。
pub fn week_days_for_dict(dict: &Dict, today: NaiveDate) -> [Option<DayTotals>; 7] {
    let map = daily_totals_for_dict(dict);
    let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
    let mut week = [None; 7];
    for (index, slot) in week.iter_mut().enumerate() {
        let date = monday + Days::new(index as u64);
        if let Some(value) = map.get(&date) {
            *slot = Some(*value);
        }
    }
    week
}

/// 一本词典的累计数据。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DictTotals {
    pub spend: i64,
    pub total: usize,
    pub new_words: usize,
    pub review: usize,
    pub wrong: usize,
    pub days: usize,
}

/// 每本词典的累计数据（按 `book_list` 顺序，含系统词典）。
pub fn dict_totals(base: &BaseState) -> Vec<(String, DictTotals)> {
    base.word
        .book_list
        .iter()
        .map(|dict| {
            let map = daily_totals_for_dict(dict);
            let mut totals = DictTotals {
                days: map.len(),
                ..DictTotals::default()
            };
            for day in map.values() {
                totals.spend += day.spend;
                totals.total += day.total;
                totals.new_words += day.new_words;
                totals.review += day.review;
                totals.wrong += day.wrong;
            }
            (dict.name.clone(), totals)
        })
        .collect()
}

/// 某一天里各词典的学习记录（只列出当天有记录的词典）。
pub fn day_breakdown(base: &BaseState, day: NaiveDate) -> Vec<(String, DayTotals)> {
    base.word
        .book_list
        .iter()
        .filter_map(|dict| {
            let map = daily_totals_for_dict(dict);
            map.get(&day).map(|totals| (dict.name.clone(), *totals))
        })
        .collect()
}

/// 有学习记录的日期集合。
pub fn studied_days(base: &BaseState) -> std::collections::BTreeSet<NaiveDate> {
    daily_totals(base).into_keys().collect()
}

/// 最近有记录的日子，按时间倒序。
pub fn recent_days(base: &BaseState, count: usize) -> Vec<(NaiveDate, DayTotals)> {
    let mut days: Vec<(NaiveDate, DayTotals)> = daily_totals(base).into_iter().collect();
    days.sort_by_key(|(date, _)| std::cmp::Reverse(*date));
    days.truncate(count);
    days
}

/// 与 Web 的 _getAccomplishDate 一致：today + ceil(剩余词数 / 每日词数)。
pub fn estimated_finish(dict: &Dict, today: NaiveDate) -> Option<NaiveDate> {
    let total = if dict.words.is_empty() {
        dict.length
    } else {
        dict.words.len()
    };
    let remaining = total.saturating_sub(dict.last_learn_index);
    if dict.complete || remaining == 0 {
        return None;
    }
    let per_day = dict.per_day().max(1);
    let days = remaining.div_ceil(per_day);
    today.checked_add_days(Days::new(days as u64))
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn weekday_label(index: usize) -> &'static str {
    const LABELS: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];
    LABELS.get(index).copied().unwrap_or("-")
}

pub fn day_number(date: NaiveDate) -> u32 {
    date.day()
}

/// 便于测试：把本地日期时间转成毫秒时间戳。
pub fn local_ms(year: i32, month: u32, day: u32) -> i64 {
    Local
        .with_ymd_and_hms(year, month, day, 12, 0, 0)
        .single()
        .map(|value| value.timestamp_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::{Dict, Statistics};

    fn base_with_day(ms: i64) -> BaseState {
        let mut base = BaseState::default();
        base.word.book_list.push(Dict {
            id: "test".to_string(),
            name: "测试".to_string(),
            words: vec![crate::domain::types::Word::new("alpha")],
            length: 10,
            last_learn_index: 4,
            per_day_study_number: 2,
            statistics: vec![Statistics {
                start_date: ms,
                spend: 60_000,
                total: 10,
                new_words: 4,
                review: 6,
                wrong: 2,
                ..Statistics::default()
            }],
            ..Dict::default()
        });
        base
    }

    #[test]
    fn aggregates_totals_and_accuracy() {
        let ms = local_ms(2026, 10, 17);
        let base = base_with_day(ms);
        let totals = totals(&base);
        assert_eq!(totals.spend, 60_000);
        assert_eq!(totals.new_words, 4);
        assert_eq!(totals.review, 6);
        assert_eq!(totals.wrong, 2);
        assert_eq!(accuracy(&totals), 80);
    }

    #[test]
    fn week_is_monday_first() {
        let ms = local_ms(2026, 10, 14); // 2026-10-14 周三
        let base = base_with_day(ms);
        let week = week_days(&base, NaiveDate::from_ymd_opt(2026, 10, 17).unwrap());
        assert!(week[2].is_some(), "周三应有记录: {week:?}");
        assert!(week[0].is_none());
        assert!(week[6].is_none());
    }

    #[test]
    fn estimates_finish_date() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 17).unwrap();
        let mut dict = Dict {
            words: (0..10)
                .map(|index| crate::domain::types::Word::new(format!("w{index}")))
                .collect(),
            length: 10,
            last_learn_index: 4,
            per_day_study_number: 2,
            ..Dict::default()
        };
        assert_eq!(
            estimated_finish(&dict, today),
            NaiveDate::from_ymd_opt(2026, 10, 20)
        );
        dict.last_learn_index = 10;
        assert_eq!(estimated_finish(&dict, today), None);
    }
}
