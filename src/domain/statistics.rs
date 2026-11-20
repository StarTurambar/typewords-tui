//! 结算写入词典统计（对应 Web 的 flushStatToStore），以及按自然日聚合。

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;

use crate::domain::timer::ms_to_local;
use crate::domain::types::{BaseState, Dict, PracticeState, Statistics};

/// 某一个自然日的累计数据（跨词典聚合）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DayTotals {
    pub spend: i64,
    pub total: usize,
    pub new_words: usize,
    pub review: usize,
    pub wrong: usize,
}

/// 按本地自然日聚合所有词典的 statistics。
///
/// 超范围的时间戳（只有手改存档才造得出来）由 [`ms_to_local`] clamp 到安全端点
/// （约 ±10000 年）后照常归日：既不丢总计，也不会把荒谬值记到今天。
/// 此前这里「跳过该条」而 [`day_key`]「回退 Local::now()」，同一个坏值在两处
/// 统计里落到不同的日子。
pub fn daily_totals(base: &BaseState) -> BTreeMap<NaiveDate, DayTotals> {
    let mut map: BTreeMap<NaiveDate, DayTotals> = BTreeMap::new();
    for dict in &base.word.book_list {
        for item in &dict.statistics {
            let day = ms_to_local(item.start_date).date_naive();
            let entry = map.entry(day).or_default();
            entry.spend += item.spend;
            // 跨天会话的每条记录都重复携带整场会话的计数（与 Web 存档一致）。
            // 计数只归到开始日（single / start / 无角色的旧记录），中间日与结束日只计时长，
            // 否则累计统计会把同一场会话的新词/复习/错词重复累加多次。
            if !matches!(item.session_role.as_deref(), Some("middle") | Some("end")) {
                entry.total += item.total;
                entry.new_words += item.new_words;
                entry.review += item.review;
                entry.wrong += item.wrong;
            }
        }
    }
    map
}

/// 按本地自然日聚合**单本词典**的 statistics。
///
/// 与 [`daily_totals`] 的跨词典聚合相对：结算弹层的「本周记录」只统计当前词典，
/// 与上游 `Statistics.vue` 的口径一致。
pub fn daily_totals_for_dict(dict: &Dict) -> BTreeMap<NaiveDate, DayTotals> {
    let mut map: BTreeMap<NaiveDate, DayTotals> = BTreeMap::new();
    for item in &dict.statistics {
        let day = ms_to_local(item.start_date).date_naive();
        let entry = map.entry(day).or_default();
        entry.spend += item.spend;
        if !matches!(item.session_role.as_deref(), Some("middle") | Some("end")) {
            entry.total += item.total;
            entry.new_words += item.new_words;
            entry.review += item.review;
            entry.wrong += item.wrong;
        }
    }
    map
}

/// 归日键：与 [`daily_totals`] 同一口径（超范围 clamp，不记到今天）。
fn day_key(ms: i64) -> String {
    ms_to_local(ms).format("%Y-%m-%d").to_string()
}

/// 把进行中的练习统计落库到 dict.statistics。
pub fn flush_stat_to_store(dict: &mut Dict, stat: &PracticeState) {
    let has_segment_spend = stat.segments.iter().any(|segment| segment[1] > segment[0]);
    if stat.spend == 0 && !has_segment_spend {
        return;
    }

    let base_total = stat.total;
    let base_wrong = stat.wrong;
    let base_new = stat.new_word_number;
    let base_review = stat.review_word_number;

    if !stat.segments.is_empty() {
        struct DayGroup {
            first_start: i64,
            total_spend: i64,
            segments: Vec<[i64; 2]>,
        }
        let mut day_map: HashMap<String, DayGroup> = HashMap::new();
        let mut day_order: Vec<String> = Vec::new();
        for segment in &stat.segments {
            let key = day_key(segment[0]);
            let entry = day_map.entry(key.clone()).or_insert_with(|| {
                day_order.push(key.clone());
                DayGroup {
                    first_start: segment[0],
                    total_spend: 0,
                    segments: Vec::new(),
                }
            });
            entry.total_spend += segment[1] - segment[0];
            entry.segments.push(*segment);
        }

        if day_order.len() == 1 {
            let key = &day_order[0];
            let group = &day_map[key];
            dict.statistics.push(Statistics {
                total: base_total,
                wrong: base_wrong,
                new_words: base_new,
                review: base_review,
                spend: group.total_spend,
                start_date: group.first_start,
                segments: Some(group.segments.clone()),
                session_role: Some("single".to_string()),
                title: None,
            });
        } else {
            let last_index = day_order.len() - 1;
            for (index, key) in day_order.iter().enumerate() {
                let group = &day_map[key];
                let role = if index == 0 {
                    "start"
                } else if index == last_index {
                    "end"
                } else {
                    "middle"
                };
                dict.statistics.push(Statistics {
                    total: base_total,
                    wrong: base_wrong,
                    new_words: base_new,
                    review: base_review,
                    spend: group.total_spend,
                    start_date: group.first_start,
                    segments: Some(group.segments.clone()),
                    session_role: Some(role.to_string()),
                    title: None,
                });
            }
        }
    } else {
        dict.statistics.push(Statistics {
            total: base_total,
            wrong: base_wrong,
            new_words: base_new,
            review: base_review,
            spend: stat.spend,
            start_date: stat.start_date,
            segments: None,
            session_role: Some("single".to_string()),
            title: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::Dict;
    use chrono::{Local, TimeZone};

    fn local_noon(year: i32, month: u32, day: u32) -> i64 {
        Local
            .with_ymd_and_hms(year, month, day, 12, 0, 0)
            .single()
            .expect("valid local date")
            .timestamp_millis()
    }

    fn record(start_date: i64, spend: i64, role: Option<&str>) -> Statistics {
        Statistics {
            start_date,
            spend,
            total: 10,
            new_words: 6,
            review: 4,
            wrong: 2,
            session_role: role.map(str::to_string),
            ..Statistics::default()
        }
    }

    fn base_with(records: Vec<Statistics>) -> BaseState {
        let dict = Dict {
            statistics: records,
            ..Dict::default()
        };
        let mut base = BaseState::default();
        base.word.book_list.push(dict);
        base
    }

    #[test]
    fn cross_day_session_counts_are_not_duplicated() {
        let day1 = local_noon(2026, 1, 10);
        let day2 = local_noon(2026, 1, 11);
        let base = base_with(vec![
            record(day1, 60_000, Some("start")),
            record(day2, 30_000, Some("end")),
        ]);

        let totals = daily_totals(&base);
        let first = totals
            .get(&NaiveDate::from_ymd_opt(2026, 1, 10).unwrap())
            .copied()
            .expect("start 日有记录");
        let second = totals
            .get(&NaiveDate::from_ymd_opt(2026, 1, 11).unwrap())
            .copied()
            .expect("end 日有时长");
        assert_eq!(first.spend, 60_000);
        assert_eq!(second.spend, 30_000);
        assert_eq!(first.new_words, 6);
        assert_eq!(second.new_words, 0, "结束日不重复累计新词");

        let sum = totals.values().fold(DayTotals::default(), |mut acc, day| {
            acc.spend += day.spend;
            acc.total += day.total;
            acc.new_words += day.new_words;
            acc.review += day.review;
            acc.wrong += day.wrong;
            acc
        });
        assert_eq!(sum.spend, 90_000);
        assert_eq!(sum.total, 10);
        assert_eq!(sum.new_words, 6);
        assert_eq!(sum.review, 4);
        assert_eq!(sum.wrong, 2);
    }

    #[test]
    fn single_and_legacy_records_still_count() {
        let day1 = local_noon(2026, 2, 1);
        let day2 = local_noon(2026, 2, 2);
        let base = base_with(vec![
            record(day1, 10_000, Some("single")),
            record(day2, 20_000, None),
        ]);
        let totals = daily_totals(&base);
        assert_eq!(totals.values().map(|day| day.total).sum::<usize>(), 20);
        assert_eq!(totals.values().map(|day| day.new_words).sum::<usize>(), 12);
    }

    /// 超范围时间戳的归日口径必须与 `day_key` 一致：clamp 到可表示端点后照常归日，
    /// 既不丢掉总计（旧实现直接跳过这条记录），也不把 9e18 记成「今天」（旧实现
    /// 在 `day_key` 里回退 `Local::now()`）。
    #[test]
    fn out_of_range_timestamps_are_clamped_instead_of_skipped_or_counted_as_today() {
        let base = base_with(vec![
            record(i64::MAX, 60_000, Some("single")),
            record(i64::MIN, 30_000, Some("single")),
        ]);

        let totals = daily_totals(&base);
        let today = Local::now().date_naive();
        assert!(
            !totals.contains_key(&today),
            "荒谬时间戳不能记到今天: {totals:?}"
        );
        assert_eq!(
            totals.values().map(|day| day.spend).sum::<i64>(),
            90_000,
            "总计不该丢: {totals:?}"
        );
        assert_eq!(totals.values().map(|day| day.total).sum::<usize>(), 20);
        assert_eq!(totals.len(), 2, "两个端点各成一天: {totals:?}");
    }

    /// 结算落库（`day_key`）与按天聚合（`daily_totals_for_dict`）对同一个荒谬
    /// 时间戳要落到同一天，不能一个记今天、一个记端点。
    #[test]
    fn settling_an_out_of_range_segment_lands_on_the_same_day_as_the_aggregate() {
        let mut dict = Dict::default();
        let stat = PracticeState {
            spend: 60_000,
            segments: vec![[i64::MAX - 60_000, i64::MAX]],
            ..PracticeState::default()
        };
        flush_stat_to_store(&mut dict, &stat);
        assert_eq!(dict.statistics.len(), 1, "单日会话只写一条");

        let totals = daily_totals_for_dict(&dict);
        let today = Local::now().date_naive();
        assert!(
            !totals.contains_key(&today),
            "结算不能把荒谬片段记到今天: {totals:?}"
        );
        assert_eq!(totals.values().map(|day| day.spend).sum::<i64>(), 60_000);
    }
}
