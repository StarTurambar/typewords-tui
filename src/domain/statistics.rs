//! 结算写入词典统计（对应 Web 的 flushStatToStore），以及按自然日聚合。

use std::collections::{BTreeMap, HashMap};

use chrono::{Local, NaiveDate, TimeZone};

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
pub fn daily_totals(base: &BaseState) -> BTreeMap<NaiveDate, DayTotals> {
    let mut map: BTreeMap<NaiveDate, DayTotals> = BTreeMap::new();
    for dict in &base.word.book_list {
        for item in &dict.statistics {
            let Some(day) = local_date(item.start_date) else {
                continue;
            };
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
        let Some(day) = local_date(item.start_date) else {
            continue;
        };
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

/// 时间戳 → 本地自然日。
///
/// 超范围时间戳（`chrono` 可表示范围约 ±262000 年之外，只可能来自手改存档）
/// 返回 `None`：聚合与写入都跳过它们，不再一处跳过、一处记到「今天」——
/// 两处口径不一致时，同一条坏记录会在统计页上凭空出现在当天。
fn local_date(ms: i64) -> Option<NaiveDate> {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|value| value.date_naive())
}

/// 自然日的键（`YYYY-MM-DD`）；超范围时间戳返回 `None`。
fn day_key(ms: i64) -> Option<String> {
    local_date(ms).map(|day| day.format("%Y-%m-%d").to_string())
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
            let Some(key) = day_key(segment[0]) else {
                // 超范围时间戳与 `daily_totals` 同一策略：跳过。早期实现里
                // 这里会回退到「今天」，于是同一条坏记录在聚合时被跳过、在
                // 写入时却被记到当天，两处口径互相打架。
                continue;
            };
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

        // 片段全部超范围时没有可写的自然日，直接收手（下面按下标取首尾日，
        // 空表会算出 `len() - 1` 的下溢）。
        if day_order.is_empty() {
            return;
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
        // 起点超出可表示范围时整条记录都不写：`daily_totals` 读不到它，写进
        // 去只会在存档里留一条永远不会显示的记录。
        if day_key(stat.start_date).is_none() {
            return;
        }
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
    use crate::domain::types::{Dict, PracticeState};
    use chrono::TimeZone;

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

    /// 超范围时间戳（手改存档，约 ±262000 年之外）在两处聚合里都跳过，
    /// 不因为「无法归日」而回退到今天。
    #[test]
    fn out_of_range_timestamps_are_skipped_by_both_aggregations() {
        let day = local_noon(2026, 3, 1);
        let records = vec![
            record(i64::MAX, 10_000, Some("single")),
            record(i64::MIN, 20_000, Some("single")),
            record(day, 30_000, Some("single")),
        ];

        let base = base_with(records.clone());
        let totals = daily_totals(&base);
        assert_eq!(totals.len(), 1, "超范围时间戳不能凭空造出一天: {totals:?}");
        let only = totals
            .values()
            .next()
            .copied()
            .expect("有效记录应聚合出一天");
        assert_eq!(only.spend, 30_000);
        assert_eq!(only.total, 10);

        let dict = Dict {
            statistics: records,
            ..Dict::default()
        };
        let per_dict = daily_totals_for_dict(&dict);
        assert_eq!(
            per_dict.len(),
            1,
            "单本词典的聚合口径必须一致: {per_dict:?}"
        );
        assert_eq!(per_dict.values().next().map(|day| day.spend), Some(30_000));
    }

    /// 写入侧与聚合侧同一策略：不可表示的起点既不写「今天」，也不留一条
    /// 永远显示不出来的记录；片段全坏时不能因为空日表而下溢 panic。
    #[test]
    fn flushing_skips_out_of_range_timestamps_instead_of_recording_today() {
        let now = Local::now().timestamp_millis();
        let mut dict = Dict::default();
        let stat = PracticeState {
            start_date: i64::MAX,
            spend: 1_000,
            segments: vec![[i64::MIN, i64::MIN + 1_000], [now - 2_000, now]],
            ..PracticeState::default()
        };
        flush_stat_to_store(&mut dict, &stat);
        assert_eq!(dict.statistics.len(), 1, "只有可表示的那一段被写入");
        assert_eq!(dict.statistics[0].start_date, now - 2_000);
        assert_eq!(dict.statistics[0].spend, 2_000);

        let mut all_bad = Dict::default();
        let stat = PracticeState {
            spend: 5_000,
            segments: vec![[i64::MIN, i64::MIN + 5_000]],
            ..PracticeState::default()
        };
        flush_stat_to_store(&mut all_bad, &stat);
        assert!(all_bad.statistics.is_empty(), "片段全坏时不应写入任何记录");

        let mut no_segments = Dict::default();
        let stat = PracticeState {
            start_date: i64::MIN,
            spend: 1_000,
            ..PracticeState::default()
        };
        flush_stat_to_store(&mut no_segments, &stat);
        assert!(
            no_segments.statistics.is_empty(),
            "起点不可表示的记录写进去也永远不会显示，不该留垃圾"
        );
    }
}
