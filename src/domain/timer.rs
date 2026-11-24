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

/// DST 拨快导致本地午夜不存在时，向后最多找多久的「次日第一个可表示时刻」。
///
/// 现实中的夏令时跳变是 30 分钟或 1 小时（历史上偶有 2 小时），3 小时足够
/// 覆盖，又不会在异常时区里空转。
const MAX_DST_GAP_MINUTES: i64 = 180;

/// 单个片段最多拆成多少个自然日。
///
/// 正常练习不会跨年，这条上限只用来兜住手改过的断点缓存（例如终点被写成
/// 几千年后）：没有它，按天循环会在这里空转很久，进而给每一天写一条统计。
/// 超出上限的部分不再按天拆，整段记到当前日——宁可少拆一天，也不丢时长。
const MAX_SPLIT_DAYS: usize = 3660;

/// 本地时刻 → 可表示的瞬时；超范围时间戳（手改存档）返回 `None`。
fn local_datetime<Tz: TimeZone>(tz: &Tz, ms: i64) -> Option<chrono::DateTime<Tz>> {
    tz.timestamp_millis_opt(ms).single()
}

/// 某个自然日结束、次日开始的时刻。
///
/// 普通日与「重复午夜」（DST 回拨）取最早一次——回拨后重复的 00:00 仍然
/// 属于次日，取最晚一次会把多出来的一小时错记到前一天。DST 拨快跳过本地
/// 午夜时（如 America/Santiago 在 9 月把 00:00 直接跳到 01:00），本地午夜
/// 根本不存在，改为向后找次日的第一个可表示时刻。早期实现在 `.single()`
/// 失败时回退成片段终点，于是跨那个午夜的整段都记在开始日，`session_role`
/// 的 start / middle / end 少一天。
fn next_day_start<Tz: TimeZone>(tz: &Tz, day: chrono::NaiveDate) -> Option<i64> {
    let midnight = (day + chrono::Days::new(1)).and_hms_opt(0, 0, 0)?;
    if let Some(dt) = tz.from_local_datetime(&midnight).earliest() {
        return Some(dt.timestamp_millis());
    }
    for minutes in 1..=MAX_DST_GAP_MINUTES {
        let candidate = midnight.checked_add_signed(chrono::TimeDelta::minutes(minutes))?;
        if let Some(dt) = tz.from_local_datetime(&candidate).earliest() {
            return Some(dt.timestamp_millis());
        }
    }
    None
}

/// 把跨天的时间片段按自然日拆分。
fn split_segment_by_day<Tz: TimeZone>(tz: &Tz, start: i64, end: i64) -> Vec<[i64; 2]> {
    let mut result = Vec::new();
    let mut cursor = start;
    while cursor < end && result.len() < MAX_SPLIT_DAYS {
        // 超范围时间戳无法归到任何自然日：不再回退成「现在」（那会把坏时间
        // 记到今天），剩下的部分整段保留。
        let Some(start_dt) = local_datetime(tz, cursor) else {
            break;
        };
        let next_midnight = next_day_start(tz, start_dt.date_naive()).unwrap_or(end);
        let part_end = end.min(next_midnight);
        if part_end <= cursor {
            // 无法推进（例如整天被跳过的时区）：不再继续拆。
            break;
        }
        result.push([cursor, part_end]);
        cursor = part_end;
    }
    if cursor < end {
        // 拆天上限或异常时区让循环提前收手：剩余部分整段记到当前日。
        // 宁可少拆一天，也不能像早期实现那样把剩余时长直接丢掉——
        // `normalize_practice_timer` 会用拆完的和覆盖 spend。
        result.push([cursor, end]);
    }
    result
}

/// 清理异常时间片段并返回（片段, spend）。
pub fn normalize_practice_timer(
    segments: &[[i64; 2]],
    spend: i64,
    now: i64,
) -> (Vec<[i64; 2]>, i64) {
    normalize_practice_timer_in(&Local, segments, spend, now)
}

/// [`normalize_practice_timer`] 的实现，时区可注入（单测用假时区模拟 DST
/// 午夜不存在，不必依赖运行机器的 TZ）。
fn normalize_practice_timer_in<Tz: TimeZone>(
    tz: &Tz,
    segments: &[[i64; 2]],
    spend: i64,
    now: i64,
) -> (Vec<[i64; 2]>, i64) {
    let mut valid: Vec<[i64; 2]> = segments
        .iter()
        .filter(|segment| segment[1] > 0)
        .map(|segment| [segment[0], segment[1].min(now.max(segment[1]))])
        .filter(|segment| segment[0] >= 0 && segment[1] > segment[0])
        // 超范围时间戳（手改存档）与按天统计同一策略：跳过。两端都要可表示，
        // 否则拆天循环会在一个无法归日的终点上停不下来。
        .filter(|segment| {
            local_datetime(tz, segment[0]).is_some() && local_datetime(tz, segment[1]).is_some()
        })
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
        normalized.extend(split_segment_by_day(tz, segment[0], segment[1]));
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
    use chrono::{FixedOffset, LocalResult, NaiveDate, NaiveDateTime, TimeZone};

    /// 假时区：模拟 America/Santiago 2026-09-06 的春季拨快——本地 00:00 不存在
    /// （时钟从 00:00 直接跳到 01:00），UTC 偏移从 -04:00 变成 -03:00。
    #[derive(Clone, Debug)]
    struct GapAtMidnight;

    fn gap_start() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, 6)
            .expect("合法日期")
            .and_hms_opt(0, 0, 0)
            .expect("合法时刻")
    }

    fn winter() -> FixedOffset {
        FixedOffset::east_opt(-4 * 3600).expect("合法偏移")
    }

    fn summer() -> FixedOffset {
        FixedOffset::east_opt(-3 * 3600).expect("合法偏移")
    }

    impl TimeZone for GapAtMidnight {
        type Offset = FixedOffset;

        fn from_offset(_offset: &Self::Offset) -> Self {
            Self
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<Self::Offset> {
            self.offset_from_local_datetime(&local.and_hms_opt(0, 0, 0).expect("合法时刻"))
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
            let gap = gap_start();
            if *local >= gap && *local < gap + chrono::TimeDelta::hours(1) {
                LocalResult::None
            } else if *local < gap {
                LocalResult::Single(winter())
            } else {
                LocalResult::Single(summer())
            }
        }

        fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset {
            self.offset_from_utc_datetime(&utc.and_hms_opt(0, 0, 0).expect("合法时刻"))
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
            // 拨快发生在本地 00:00 -04:00，也就是 UTC 04:00。
            let transition = gap_start() + chrono::TimeDelta::hours(4);
            if *utc < transition {
                winter()
            } else {
                summer()
            }
        }
    }

    fn local_ms(
        tz: &GapAtMidnight,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> i64 {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .expect("合法日期")
            .and_hms_opt(hour, minute, 0)
            .expect("合法时刻");
        tz.from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| panic!("{naive} 应是可表示的本地时刻"))
            .timestamp_millis()
    }

    /// 跨过「本地午夜不存在」的那一天：必须从次日的第一个可表示时刻拆开。
    ///
    /// 早期实现在 `.single()` 失败时回退成片段终点，整段都记在开始日，
    /// 按天统计与 session_role 的 start / middle / end 因此少一天。
    #[test]
    fn a_segment_crossing_a_dst_midnight_gap_is_split_at_the_next_representable_instant() {
        let tz = GapAtMidnight;
        let start = local_ms(&tz, 2026, 9, 5, 22, 0);
        let end = local_ms(&tz, 2026, 9, 7, 0, 30);

        let (segments, spend) = normalize_practice_timer_in(&tz, &[[start, end]], end - start, end);

        assert_eq!(
            segments.len(),
            3,
            "跨过被跳过的午夜应拆成三天：{segments:?}"
        );
        let first_of_sixth = local_ms(&tz, 2026, 9, 6, 1, 0);
        let first_of_seventh = local_ms(&tz, 2026, 9, 7, 0, 0);
        assert_eq!(segments[0], [start, first_of_sixth]);
        assert_eq!(segments[1], [first_of_sixth, first_of_seventh]);
        assert_eq!(segments[2], [first_of_seventh, end]);
        // 中间那天是完整的 23 小时（拨快少一小时），没有被整段算到前一天。
        assert_eq!(segments[1][1] - segments[1][0], 23 * 3600 * 1000);
        assert_eq!(spend, end - start);
    }

    /// 假时区：模拟「回拨落在午夜」——2026-04-05 本地 01:00 拨回 00:00，
    /// 本地 00:00–01:00 出现两次（歧义午夜），偏移从 -03:00 变成 -04:00。
    #[derive(Clone, Debug)]
    struct FallBackAtMidnight;

    fn fall_back_start() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 4, 5)
            .expect("合法日期")
            .and_hms_opt(0, 0, 0)
            .expect("合法时刻")
    }

    fn fall_back_end() -> NaiveDateTime {
        fall_back_start() + chrono::TimeDelta::hours(1)
    }

    impl TimeZone for FallBackAtMidnight {
        type Offset = FixedOffset;

        fn from_offset(_offset: &Self::Offset) -> Self {
            Self
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<Self::Offset> {
            self.offset_from_local_datetime(&local.and_hms_opt(0, 0, 0).expect("合法时刻"))
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
            if *local < fall_back_start() {
                LocalResult::Single(summer())
            } else if *local < fall_back_end() {
                // 这一小时发生两次：earliest 是拨回前，latest 是拨回后。
                LocalResult::Ambiguous(summer(), winter())
            } else {
                LocalResult::Single(winter())
            }
        }

        fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset {
            self.offset_from_utc_datetime(&utc.and_hms_opt(0, 0, 0).expect("合法时刻"))
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
            // 回拨发生在本地 01:00 -03:00，也就是 UTC 04:00。
            let transition = fall_back_start() + chrono::TimeDelta::hours(4);
            if *utc < transition {
                summer()
            } else {
                winter()
            }
        }
    }

    fn local_ms_at(
        tz: &FallBackAtMidnight,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        latest: bool,
    ) -> i64 {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .expect("合法日期")
            .and_hms_opt(hour, minute, 0)
            .expect("合法时刻");
        let resolved = tz.from_local_datetime(&naive);
        let dt = if latest {
            resolved.latest()
        } else {
            resolved.earliest()
        };
        dt.unwrap_or_else(|| panic!("{naive} 应是可表示的本地时刻"))
            .timestamp_millis()
    }

    /// 回拨让本地午夜出现两次：拆分必须落在第一次午夜上，且游标不能原地打转
    /// 把剩余时长丢掉——PR #8 的同类实现正是在这里截断片段，再由
    /// `normalize_practice_timer` 用截断后的和覆盖 spend。
    #[test]
    fn a_segment_crossing_an_ambiguous_midnight_keeps_every_millisecond() {
        let tz = FallBackAtMidnight;
        let start = local_ms_at(&tz, 2026, 4, 4, 22, 0, false);
        let end = local_ms_at(&tz, 2026, 4, 5, 0, 30, true);
        let first_midnight = local_ms_at(&tz, 2026, 4, 5, 0, 0, false);

        let (segments, spend) = normalize_practice_timer_in(&tz, &[[start, end]], end - start, end);

        assert_eq!(segments.len(), 2, "歧义午夜只拆一次：{segments:?}");
        assert_eq!(segments[0], [start, first_midnight]);
        assert_eq!(segments[1], [first_midnight, end]);
        let total: i64 = segments.iter().map(|segment| segment[1] - segment[0]).sum();
        assert_eq!(total, end - start, "拆天不能丢时长");
        assert_eq!(spend, end - start);
    }

    /// 没有 DST 的时区：普通片段仍是一段，不因新逻辑被切开。
    #[test]
    fn a_segment_inside_one_day_stays_whole() {
        let tz = FixedOffset::east_opt(8 * 3600).expect("合法偏移");
        let start = tz
            .with_ymd_and_hms(2026, 3, 1, 10, 0, 0)
            .single()
            .expect("合法时刻")
            .timestamp_millis();
        let (segments, _) =
            normalize_practice_timer_in(&tz, &[[start, start + 5_000]], 5_000, start + 5_000);
        assert_eq!(segments, vec![[start, start + 5_000]]);
    }

    /// 超范围时间戳（手改存档）与按天统计同一策略：跳过，不回退成「现在」。
    #[test]
    fn out_of_range_segments_are_dropped_instead_of_being_attributed_to_today() {
        let now = Local::now().timestamp_millis();
        let valid = [now - 5_000, now];
        let (segments, spend) =
            normalize_practice_timer(&[[i64::MIN, i64::MIN + 5_000], valid], 10_000, now);
        assert_eq!(segments, vec![valid], "不可表示的片段应被跳过");
        assert_eq!(spend, 5_000, "跳过后 spend 只累计可表示的时长");
    }
}
