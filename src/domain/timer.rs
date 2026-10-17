//! 练习计时：时间片段、跨天拆分、空闲暂停。

use chrono::{Local, TimeZone};

use crate::domain::types::PracticeState;

/// 允许的片段总长超出 spend 的漂移容差（毫秒）。
const TIMER_DRIFT_TOLERANCE_MS: i64 = 2000;

/// 空闲多久自动暂停（毫秒）。
pub const IDLE_TIMEOUT_MS: i64 = 3 * 60 * 1000;

pub fn now_ms() -> i64 {
    Local::now().timestamp_millis()
}

fn ms_to_local(ms: i64) -> chrono::DateTime<Local> {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Local::now)
}

/// 把跨天的时间片段按自然日拆分。
fn split_segment_by_day(start: i64, end: i64) -> Vec<[i64; 2]> {
    let mut result = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let start_dt = ms_to_local(cursor);
        let next_midnight = (start_dt.date_naive() + chrono::Days::new(1))
            .and_hms_opt(0, 0, 0)
            .and_then(|naive| {
                Local
                    .from_local_datetime(&naive)
                    .single()
                    .map(|dt| dt.timestamp_millis())
            })
            .unwrap_or(end);
        let part_end = end.min(next_midnight);
        if part_end > cursor {
            result.push([cursor, part_end]);
        }
        if part_end <= cursor {
            break;
        }
        cursor = part_end;
    }
    result
}

/// 清理异常时间片段并返回（片段, spend）。
pub fn normalize_practice_timer(
    segments: &[[i64; 2]],
    spend: i64,
    now: i64,
) -> (Vec<[i64; 2]>, i64) {
    let mut valid: Vec<[i64; 2]> = segments
        .iter()
        .filter(|segment| segment[1] > 0)
        .map(|segment| [segment[0], segment[1].min(now)])
        .filter(|segment| segment[0] >= 0 && segment[1] > segment[0])
        .collect();
    valid.sort_by_key(|segment| segment[0]);

    let mut merged: Vec<[i64; 2]> = Vec::new();
    for segment in valid {
        match merged.last_mut() {
            Some(last) if segment[0] <= last[1] => last[1] = last[1].max(segment[1]),
            _ => merged.push(segment),
        }
    }

    let safe_spend = spend.max(0);
    let total: i64 = merged.iter().map(|segment| segment[1] - segment[0]).sum();
    let mut excess = (total - (safe_spend + TIMER_DRIFT_TOLERANCE_MS)).max(0);

    while excess > 0 && !merged.is_empty() {
        let mut longest = 0usize;
        for (index, segment) in merged.iter().enumerate() {
            if segment[1] - segment[0] > merged[longest][1] - merged[longest][0] {
                longest = index;
            }
        }
        let duration = merged[longest][1] - merged[longest][0];
        let reduction = excess.min(duration);
        merged[longest][1] -= reduction;
        excess -= reduction;
        if merged[longest][1] <= merged[longest][0] {
            merged.remove(longest);
        }
    }

    let mut normalized: Vec<[i64; 2]> = Vec::new();
    for segment in &merged {
        normalized.extend(split_segment_by_day(segment[0], segment[1]));
    }
    if normalized.is_empty() {
        return (Vec::new(), safe_spend);
    }
    let normalized_spend: i64 = normalized
        .iter()
        .map(|segment| segment[1] - segment[0])
        .sum();
    (normalized, normalized_spend)
}

/// 暂停计时：定格最后一个片段。
pub fn pause(state: &mut PracticeState, reason: &str, now: i64) {
    if state.timer_paused {
        return;
    }
    if let Some(last) = state.segments.last_mut() {
        last[1] = last[1].min(now).max(last[0]);
    }
    state.timer_paused = true;
    state.timer_pause_reason = Some(reason.to_string());
}

/// 恢复计时：开启新片段。
pub fn resume(state: &mut PracticeState, now: i64) {
    state.timer_paused = false;
    state.timer_pause_reason = None;
    state.segments.push([now, now]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickResult {
    /// 计时暂停中，未累加。
    Paused,
    /// 正常累加 1 秒。
    Ticked,
    /// 因空闲自动暂停。
    IdlePaused,
}

/// 每秒 tick：累计 spend 并维护最后片段。
pub fn tick(state: &mut PracticeState, last_activity: i64, now: i64) -> TickResult {
    if state.timer_paused {
        return TickResult::Paused;
    }
    if now - last_activity >= IDLE_TIMEOUT_MS {
        if let Some(last) = state.segments.last_mut() {
            last[1] = (last_activity + IDLE_TIMEOUT_MS).min(now).max(last[0]);
        }
        state.timer_paused = true;
        state.timer_pause_reason = Some("auto_idle".to_string());
        return TickResult::IdlePaused;
    }
    state.spend += 1000;
    if let Some(last) = state.segments.last_mut() {
        last[1] = now;
    }
    TickResult::Ticked
}

/// 会话结束时闭合最后一个片段。
pub fn close_open_segment(state: &mut PracticeState, now: i64) {
    if !state.timer_paused
        && let Some(last) = state.segments.last_mut()
    {
        last[1] = now.max(last[0]);
    }
}

/// 毫秒 → “x小时y分钟 / y分钟 / z秒”。
pub fn format_duration(ms: i64) -> String {
    let total_minutes = ms.max(0) / 1000 / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours > 0 {
        format!("{hours}小时{minutes}分钟")
    } else if minutes > 0 {
        format!("{minutes}分钟")
    } else {
        format!("{}秒", ms.max(0) / 1000)
    }
}
