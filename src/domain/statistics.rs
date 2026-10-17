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
            let Some(day) = Local
                .timestamp_millis_opt(item.start_date)
                .single()
                .map(|value| value.date_naive())
            else {
                continue;
            };
            let entry = map.entry(day).or_default();
            entry.spend += item.spend;
            entry.total += item.total;
            entry.new_words += item.new_words;
            entry.review += item.review;
            entry.wrong += item.wrong;
        }
    }
    map
}

fn day_key(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%d")
        .to_string()
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
