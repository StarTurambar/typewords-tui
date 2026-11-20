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

/// 超范围毫秒的 clamp 端点：约 ±10000 年。
///
/// 不用 chrono 真正的可表示端点（约 ±262000 年）：`DateTime<Local>::date_naive()`
/// 要为 UTC 时间加上本地偏移，紧贴端点时这一步会把结果推出 `NaiveDateTime` 的
/// 表示范围并 panic（chrono 的 `naive_local` 是 `expect`）。留出一万年的余量后，
/// 任何本地偏移（实际最大 ±14 小时）都不可能把它推出边界；而真实的毫秒时间戳
/// （1970 前后数百年）离这里还差着数量级，正常数据永远不会被它影响。
const CLAMP_LIMIT_MS: i64 = 10_000 * 365 * 24 * 60 * 60 * 1000;

/// 把毫秒 clamp 到 [`CLAMP_LIMIT_MS`] 窗口内。
fn clamp_ms(ms: i64) -> i64 {
    ms.clamp(-CLAMP_LIMIT_MS, CLAMP_LIMIT_MS)
}

/// 毫秒 → 本地时间；超出可表示范围时 clamp 到 [`CLAMP_LIMIT_MS`]。
///
/// `Local.timestamp_millis_opt` 只在极端值（手改存档才造得出来）时失败。此前的
/// 回退是 `Local::now()`：一个荒谬的时间戳会被记成「今天」，与 `statistics`
/// 里「超范围就跳过」的口径正好相反，同一个坏值在两处统计里落到不同的日子。
/// 现在统一 clamp 到端点——总计不丢，也不会把 9e18 毫秒记到今天。
pub(crate) fn ms_to_local(ms: i64) -> chrono::DateTime<Local> {
    let clamped = clamp_ms(ms);
    Local
        .timestamp_millis_opt(clamped)
        .single()
        .unwrap_or_else(|| {
            // 防御：clamp 之后的区间在 chrono 的可表示范围内，理论上到不了这里。
            // 真的失败时退回 Unix 纪元（而不是「今天」），不 panic。
            Local
                .timestamp_millis_opt(0)
                .single()
                .unwrap_or_else(Local::now)
        })
}

/// 把跨天的时间片段按自然日拆分。
///
/// 两端先 clamp 到 [`CLAMP_LIMIT_MS`] 窗口内：窗口外的时间戳换算出来的日期与
/// 原始毫秒值对不上（`i64::MAX` 的本地日期是公元一万多年，而它的下一个午夜
/// 比它小得多），游标会因此原地打转或整段消失。窗口内的数据不受任何影响。
fn split_segment_by_day(start: i64, end: i64) -> Vec<[i64; 2]> {
    let mut result = Vec::new();
    let mut cursor = clamp_ms(start);
    let end = clamp_ms(end);
    while cursor < end {
        let start_dt = ms_to_local(cursor);
        let next_midnight = next_local_midnight(start_dt.date_naive()).unwrap_or(end);
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

/// `date` 之后那个本地午夜的毫秒时间戳。
///
/// 直接 `from_local_datetime(次日 00:00).single()` 在**DST 跳过午夜**的时区
/// （如 America/Santiago 9 月的夏令时切换）会返回 `None`，旧实现此时回退
/// `end`，跨那一个午夜的片段整段记到前一天，roles 也少一天。现在从 00:00 起
/// 按分钟向后找第一个可表示的本地时刻（最多 24 小时）：跳过的那一小时里没有
/// 任何可表示时刻，往后一步就是切换后的第一个瞬间，片段仍然在午夜附近断开。
/// 重复的时刻（秋季回拨）取最早的（`earliest`），与「按自然日拆分」的口径一致。
///
/// 仍然找不到（`date` 已是 chrono 能表示的最后一天，或 24 小时内全不可表示）
/// 时返回 `None`，由调用方回退到 `end`——这只可能发生在手改出来的荒谬时间戳
/// 上，宁可少拆一天也不 panic。
fn next_local_midnight(date: chrono::NaiveDate) -> Option<i64> {
    let midnight = date.succ_opt()?.and_hms_opt(0, 0, 0)?;
    for minute in 0..=(24 * 60) {
        let naive = midnight.checked_add_signed(chrono::Duration::minutes(minute))?;
        match Local.from_local_datetime(&naive) {
            chrono::LocalResult::Single(dt) => return Some(dt.timestamp_millis()),
            chrono::LocalResult::Ambiguous(earliest, _) => {
                return Some(earliest.timestamp_millis());
            }
            chrono::LocalResult::None => continue,
        }
    }
    None
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
        .map(|segment| [segment[0], segment[1].min(now.max(segment[1]))])
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
        // 片段的结束时间不能被拉早。
        //
        // 计时由 `tick` 按传入的 `now` 推进（事件循环用的时间戳），
        // 闭合时传的可能是更早的挂钟时间——早期实现直接用 `now` 覆盖，
        // 已经累计的时长会被抹掉一大截。
        last[1] = now.max(last[0]).max(last[1]);
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
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn local_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("valid local time")
            .timestamp_millis()
    }

    /// 正常跨天片段仍然在本地午夜断开，两段首尾相接、总长不变。
    #[test]
    fn a_segment_crossing_local_midnight_is_split_at_midnight() {
        let start = local_ms(2026, 3, 1, 23, 0);
        let end = local_ms(2026, 3, 2, 1, 0);
        let midnight = local_ms(2026, 3, 2, 0, 0);

        let (segments, spend) = normalize_practice_timer(&[[start, end]], end - start, end);

        assert_eq!(segments, vec![[start, midnight], [midnight, end]]);
        assert_eq!(spend, end - start, "拆分不改变总时长");
    }

    /// 超范围毫秒要 clamp 到可表示端点，而不是回退成「今天」。
    #[test]
    fn out_of_range_millis_are_clamped_instead_of_jumping_to_today() {
        let today = Local::now().date_naive();
        assert!(ms_to_local(i64::MAX).date_naive() > today);
        assert!(ms_to_local(i64::MIN).date_naive() < today);

        // 拆分同样不能把荒谬片段算到今天：clamp 之后两端落在同一个窗口端点上，
        // 拆不出片段（时长仍由返回的 spend 保留），绝不会归到「今天」。
        let start = i64::MAX - 60_000;
        let (segments, spend) = normalize_practice_timer(&[[start, i64::MAX]], 60_000, i64::MAX);
        assert_eq!(spend, 60_000, "荒谬片段的时长不能丢");
        for segment in &segments {
            assert!(
                ms_to_local(segment[0]).date_naive() > today,
                "荒谬片段被记到了今天: {segment:?}"
            );
        }
    }
}
