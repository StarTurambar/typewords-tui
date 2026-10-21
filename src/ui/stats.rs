//! 统计页：累计概览 + 最近 14 天记录。
//!
//! 只读 App，不产生点击目标；列表滚动偏移由 app.stats_scroll 控制，
//! 渲染时再按可见行数收敛，避免越界。

use chrono::Datelike;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::domain::statistics::DayTotals;
use crate::domain::timer::format_duration;
use crate::ui::insights;
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

const RECENT_DAYS: usize = 14;
/// 一行放 6 个概览卡片所需的内容宽度。
const WIDE_TILE_WIDTH: u16 = 90;

struct StatsLayout {
    header: Rect,
    tiles: Rect,
    section: Rect,
    list: Rect,
    tile_columns: usize,
    tile_height: u16,
    visible: usize,
}

fn layout(_app: &App, content: Rect) -> StatsLayout {
    let header = Rect::new(content.x, content.y, content.width, content.height.min(1));
    let tile_columns = if content.width >= WIDE_TILE_WIDTH {
        6
    } else {
        3
    };
    let tile_rows = 6usize.div_ceil(tile_columns);
    let tile_height = if tile_columns == 6 { 3 } else { 2 };
    let gap: u16 = 1;
    let tiles_height = tile_rows as u16 * tile_height + (tile_rows as u16).saturating_sub(1) * gap;
    let tiles = Rect::new(
        content.x,
        header.bottom().saturating_add(1),
        content.width,
        tiles_height,
    );
    let section = Rect::new(
        content.x,
        tiles.bottom().saturating_add(1),
        content.width,
        1,
    );

    let mut list_height = content.bottom().saturating_sub(section.bottom());
    // 底部留一行放滚动提示
    if list_height > 1 {
        list_height -= 1;
    }
    let list = Rect::new(content.x, section.bottom(), content.width, list_height);

    StatsLayout {
        header,
        tiles,
        section,
        list,
        tile_columns,
        tile_height,
        visible: list_height as usize,
    }
}

/// 下方内容区的行数（滚动边界用）。
pub fn body_line_count(app: &App) -> usize {
    body_lines(app, 80, &app.theme()).len()
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Stats, |frame, content| {
        let layout = layout(app, content);
        render_header(app, frame, layout.header, theme);
        render_tiles(app, frame, &layout, theme);
        render_body(app, frame, &layout, theme);
    });
}

fn render_header(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.height == 0 || rect.width == 0 {
        return;
    }
    let view = if app.stats_by_dict {
        "按词典"
    } else {
        "日历"
    };
    frame.render_widget(
        Paragraph::new(widgets::section_header(
            "统计",
            &format!("{view} · v 切换"),
            rect.width as usize,
            theme,
        )),
        rect,
    );
}

/// 日历网格的几何：每月第一天是周几、这个月有几天。
///
/// 绘制与命中检测共用它——两处各算一遍就会让「点日期选一天」点不准。
struct CalendarGrid {
    /// 每月 1 号前面空几格（周一为一周第一天）。
    leading: usize,
    /// 这个月的最后一天。
    last_day: u32,
}

/// 日历里每格占 5 列（3 列日期 + 2 列记录点）。
const CALENDAR_CELL_WIDTH: u16 = 5;
/// 日历每行的日期数字占 3 列（右对齐）。
const CALENDAR_NUMBER_WIDTH: u16 = 3;
/// 日历块第一列前的缩进。
const CALENDAR_INDENT: u16 = 2;

fn calendar_grid(year: i32, month: u32) -> Option<CalendarGrid> {
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1)?;
    let leading = first.weekday().num_days_from_monday() as usize;
    let days_in_month = (first + chrono::Duration::days(31)).day();
    let last_day = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .and_then(|d| d.pred_opt())
    .map(|d| d.day())
    .unwrap_or(days_in_month);
    Some(CalendarGrid { leading, last_day })
}

/// 日历块：一个月的网格，标出有记录的日子与选中的日子。
fn calendar_lines(app: &App, theme: &Theme) -> Vec<Line<'static>> {
    let (year, month) = app.stats_month;
    let today = insights::today();
    let studied = insights::studied_days(&app.boot.base);
    let Some(grid) = calendar_grid(year, month) else {
        return Vec::new();
    };
    let leading = grid.leading;
    let last_day = grid.last_day;
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default();

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {year} 年 {month} 月"),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("    ", Style::default().fg(theme.dim)),
        Span::styled("← → 选日期   , . 翻月", Style::default().fg(theme.dim)),
    ]));

    // 表头与日期列同宽（5 列），按显示宽度右对齐，避免中文表头错位
    let mut header_spans = vec![Span::raw("  ")];
    for label in ["一", "二", "三", "四", "五", "六", "日"] {
        let pad = 3usize.saturating_sub(display_width(label));
        header_spans.push(Span::styled(
            format!("{}{label}  ", " ".repeat(pad)),
            Style::default().fg(theme.dim),
        ));
    }
    lines.push(Line::from(header_spans));

    let mut cells: Vec<Option<u32>> = vec![None; leading];
    for day in 1..=last_day {
        cells.push(Some(day));
    }
    while !cells.len().is_multiple_of(7) {
        cells.push(None);
    }

    for week in cells.chunks(7) {
        let mut spans = vec![Span::raw("  ")];
        for cell in week {
            match cell {
                None => spans.push(Span::raw("     ")),
                Some(day) => {
                    let date = first.with_day(*day).unwrap_or(first);
                    let has_record = studied.contains(&date);
                    let selected = date == app.stats_day;
                    let is_today = date == today;
                    let text = format!("{day:>3}");
                    // 选中日用高亮底色 + 前景色区分。早期实现把号码画成
                    // `theme.bg` 前景，压在同为 `theme.bg` 的背景上，选中的
                    // 日期完全看不见。
                    let color = if selected {
                        theme.fg
                    } else if is_today {
                        theme.accent
                    } else if has_record {
                        theme.good
                    } else {
                        theme.dim
                    };
                    let style = if selected {
                        Style::default()
                            .fg(color)
                            .bg(theme.selection)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color)
                    };
                    spans.push(Span::styled(text, style));
                    let mut dot_style =
                        Style::default().fg(if has_record { theme.good } else { theme.dim });
                    if selected {
                        dot_style = dot_style.bg(theme.selection);
                    }
                    spans.push(Span::styled(
                        if has_record { "● " } else { "  " },
                        dot_style,
                    ));
                }
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// 选中日的各词典明细。
fn day_detail_lines(app: &App, theme: &Theme) -> Vec<Line<'static>> {
    let day = app.stats_day;
    let breakdown = insights::day_breakdown(&app.boot.base, day);
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("  {}", day.format("%Y-%m-%d")),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if breakdown.is_empty() {
                "  这一天没有学习记录".to_string()
            } else {
                format!("  {} 本词典有记录", breakdown.len())
            },
            Style::default().fg(theme.dim),
        ),
    ])];
    for (name, totals) in breakdown.iter().take(6) {
        lines.push(Line::from(vec![
            Span::styled(format!("    {name}"), Style::default().fg(theme.fg)),
            Span::styled(
                format!(
                    "  {}  新 {}  复习 {}  错 {}",
                    format_duration(totals.spend),
                    totals.new_words,
                    totals.review,
                    totals.wrong
                ),
                Style::default().fg(theme.muted),
            ),
        ]));
    }
    lines
}

/// 各词典累计。
fn dict_lines(app: &App, theme: &Theme) -> Vec<Line<'static>> {
    let totals = insights::dict_totals(&app.boot.base);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (name, value) in totals.iter() {
        if value.days == 0 && value.total == 0 {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(format!("  {name}"), Style::default().fg(theme.fg)),
            Span::styled(
                format!(
                    "  {}  天 {}  新 {}  复习 {}  错 {}",
                    format_duration(value.spend),
                    value.days,
                    value.new_words,
                    value.review,
                    value.wrong
                ),
                Style::default().fg(theme.muted),
            ),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  暂无学习记录",
            Style::default().fg(theme.dim),
        )));
    }
    lines
}

fn render_tiles(app: &App, frame: &mut Frame, layout: &StatsLayout, theme: &Theme) {
    if layout.tiles.height == 0 || layout.tiles.width == 0 {
        return;
    }
    let totals = insights::totals(&app.boot.base);
    let accuracy = insights::accuracy(&totals);
    let entries: [(&str, String, Option<String>); 6] = [
        ("累计时长", format_duration(totals.spend), None),
        ("学习天数", format!("{} 天", totals.days), None),
        (
            "新词",
            totals.new_words.to_string(),
            Some(format!("今日 {}", app.today_new)),
        ),
        (
            "复习",
            totals.review.to_string(),
            Some(format!("今日 {}", app.today_review)),
        ),
        (
            "错误",
            totals.wrong.to_string(),
            Some(format!("今日 {}", app.today_wrong)),
        ),
        ("正确率", format!("{accuracy}%"), None),
    ];

    let columns = layout.tile_columns as u16;
    let gap = 1u16;
    let tile_width = layout
        .tiles
        .width
        .saturating_sub(gap * columns.saturating_sub(1))
        / columns;
    if tile_width == 0 {
        return;
    }
    for (index, (label, value, sub)) in entries.iter().enumerate() {
        let column = index as u16 % columns;
        let row = index as u16 / columns;
        let y = layout.tiles.y + row * (layout.tile_height + gap);
        if y >= layout.tiles.bottom() {
            break;
        }
        let x = layout.tiles.x + column * (tile_width + gap);
        let height = layout
            .tile_height
            .min(layout.tiles.bottom().saturating_sub(y));
        render_tile(
            frame,
            Rect::new(x, y, tile_width, height),
            value,
            label,
            sub.as_deref(),
            theme,
        );
    }
}

fn render_tile(
    frame: &mut Frame,
    rect: Rect,
    value: &str,
    label: &str,
    sub: Option<&str>,
    theme: &Theme,
) {
    if rect.width < 2 || rect.height == 0 {
        return;
    }
    widgets::paint_rect(frame, rect, theme.element);
    let inner = Rect::new(
        rect.x.saturating_add(1),
        rect.y,
        rect.width.saturating_sub(2),
        rect.height,
    );
    let width = inner.width as usize;
    let mut lines = vec![Line::from(Span::styled(
        fit_text(value, width),
        Style::default()
            .fg(theme.accent_alt)
            .add_modifier(Modifier::BOLD),
    ))];
    if rect.height >= 2 {
        lines.push(Line::from(Span::styled(
            fit_text(label, width),
            Style::default().fg(theme.muted),
        )));
    }
    if rect.height >= 3 {
        match sub {
            Some(sub) => lines.push(Line::from(Span::styled(
                fit_text(sub, width),
                Style::default().fg(theme.dim),
            ))),
            None => lines.push(Line::from("")),
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// 统计页的鼠标目标。
///
/// - 顶栏的「v 切换」→ 切换日历 / 按词典视图
/// - 日历里的每一天 → 选中该日
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let layout = layout(app, content);
    let mut result = Vec::new();
    // 顶栏右侧的「v 切换」
    let toggle = widgets::section_header(
        "统计",
        &format!(
            "{} · v 切换",
            if app.stats_by_dict {
                "按词典"
            } else {
                "日历"
            }
        ),
        layout.header.width as usize,
        &app.theme(),
    );
    if let Some(rect) = header_right_rect(&toggle, layout.header) {
        result.push(Target::new(rect, crate::app::Action::StatsToggleView));
    }
    // 日历里的每一天
    if !app.stats_by_dict {
        result.extend(calendar_targets(app, &layout));
    }
    result
}

/// 顶栏里右侧提示文字的矩形。
fn header_right_rect(line: &Line<'_>, rect: Rect) -> Option<Rect> {
    let text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let marker = text.rfind("v 切换")?;
    let column = display_width(&text[..marker]) as u16;
    let width = display_width("v 切换") as u16;
    let target = Rect::new(rect.x + column, rect.y, width, 1);
    (target.right() <= rect.right()).then_some(target)
}

/// 日历里每一天的命中矩形。
fn calendar_targets(app: &App, layout: &StatsLayout) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let mut result = Vec::new();
    let (year, month) = app.stats_month;
    let Some(grid) = calendar_grid(year, month) else {
        return result;
    };
    // 日历在正文里：第 0 行是月份标题，第 1 行是星期表头，
    // 之后每一行是一周。
    let first_week_row = 2usize;
    let rows = grid.leading + grid.last_day as usize;
    let weeks = rows.div_ceil(7);
    for week in 0..weeks {
        for weekday in 0..7usize {
            let cell = week * 7 + weekday;
            if cell < grid.leading {
                continue;
            }
            let day = cell - grid.leading + 1;
            if day as u32 > grid.last_day {
                continue;
            }
            let line = first_week_row + week;
            // 滚动偏移
            let Some(visible_row) = line.checked_sub(app.stats_scroll as usize) else {
                continue;
            };
            if visible_row >= layout.visible {
                continue;
            }
            let y = layout.list.y + visible_row as u16;
            let x = layout.list.x
                + CALENDAR_INDENT
                + (weekday as u16) * CALENDAR_CELL_WIDTH
                + (CALENDAR_CELL_WIDTH - CALENDAR_NUMBER_WIDTH);
            let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day as u32) else {
                continue;
            };
            result.push(Target::new(
                Rect::new(x, y, CALENDAR_NUMBER_WIDTH, 1),
                crate::app::Action::StatsSelectDay(date),
            ));
        }
    }
    result
}

/// 下方内容区：日历视图渲染「日历 + 选中日明细 + 最近 14 天」，
/// 词典视图渲染各词典累计。整体按行滚动。
fn render_body(app: &App, frame: &mut Frame, layout: &StatsLayout, theme: &Theme) {
    let lines = body_lines(app, layout.list.width as usize, theme);
    if layout.section.height > 0 {
        let title = if app.stats_by_dict {
            "按词典统计"
        } else {
            "学习日历"
        };
        frame.render_widget(
            Paragraph::new(widgets::section_header(
                title,
                "",
                layout.section.width as usize,
                theme,
            )),
            layout.section,
        );
    }
    if layout.list.height == 0 {
        return;
    }
    let total = lines.len();
    let offset = (app.stats_scroll as usize).min(total.saturating_sub(1));
    let end = (offset + layout.visible).min(total);
    for (row, line) in lines[offset..end].iter().enumerate() {
        frame.render_widget(
            Paragraph::new(line.clone()),
            Rect::new(
                layout.list.x,
                layout.list.y + row as u16,
                layout.list.width,
                1,
            ),
        );
    }
    if total > layout.visible {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    "↓ 还有 {} 行",
                    total.saturating_sub(offset + layout.visible)
                ),
                Style::default().fg(theme.dim),
            )),
            Rect::new(
                layout.list.x,
                layout.list.bottom().saturating_sub(1),
                layout.list.width,
                1,
            ),
        );
    }
}

/// 下方内容区的全部行。
fn body_lines(app: &App, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    if app.stats_by_dict {
        lines.extend(dict_lines(app, theme));
        return lines;
    }
    lines.extend(calendar_lines(app, theme));
    lines.push(Line::from(""));
    lines.extend(day_detail_lines(app, theme));
    lines.push(Line::from(""));

    let days = insights::recent_days(&app.boot.base, RECENT_DAYS);
    lines.push(Line::from(Span::styled(
        format!("  最近 {RECENT_DAYS} 天（{} 天有记录）", days.len()),
        Style::default().fg(theme.muted),
    )));
    if days.is_empty() {
        lines.push(Line::from(Span::styled(
            "  暂无学习记录",
            Style::default().fg(theme.dim),
        )));
        return lines;
    }
    let max_spend = days.iter().map(|(_, day)| day.spend).max().unwrap_or(0);
    let today = insights::today();
    for (date, day) in days.iter() {
        lines.push(day_line(
            *date,
            day,
            max_spend,
            *date == today,
            width,
            theme,
        ));
    }
    lines
}

fn day_line(
    date: chrono::NaiveDate,
    day: &DayTotals,
    max_spend: i64,
    is_today: bool,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    if width < 28 {
        return Line::from("");
    }
    let weekday = format!(
        "周{}",
        insights::weekday_label(date.weekday().num_days_from_monday() as usize)
    );
    let date_text = format!(
        "{} {}",
        date.format("%m-%d"),
        if is_today { "今天" } else { weekday.as_str() }
    );
    let date_width = display_width(&date_text);
    let duration = fit_text(&format_duration(day.spend), (width / 4).max(4));
    let base_stats = fit_text(
        &format!("新 {} 复 {}", day.new_words, day.review),
        (width / 4).max(6),
    );
    let wrong_stats = fit_text(&format!("错 {}", day.wrong), 8);
    let duration_width = display_width(&duration);
    let base_width = display_width(&base_stats);
    let wrong_width = display_width(&wrong_stats);
    let stats_width = base_width + 1 + wrong_width;
    let fixed = date_width + 2 + duration_width + stats_width;
    let mut bar_width = width.saturating_sub(fixed).min(24);
    let mut gap = width.saturating_sub(date_width + bar_width + duration_width + stats_width + 2);
    if gap == 0 && bar_width > 1 {
        bar_width -= 1;
        gap = 1;
    }
    let percent = if max_spend > 0 {
        ((day.spend.max(0) as f64 / max_spend as f64) * 100.0).round() as u32
    } else {
        0
    };

    let date_style = if is_today {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };
    let mut spans: Vec<Span<'static>> = vec![Span::styled(date_text, date_style), Span::raw(" ")];
    spans.extend(widgets::thin_bar_spans(
        percent.min(100),
        bar_width,
        if is_today {
            theme.good
        } else {
            theme.accent_alt
        },
        theme.border,
    ));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        duration,
        Style::default().fg(if is_today { theme.fg } else { theme.muted }),
    ));
    spans.push(Span::raw(" ".repeat(gap)));
    spans.push(Span::styled(base_stats, Style::default().fg(theme.muted)));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        wrong_stats,
        Style::default().fg(if day.wrong > 0 { theme.bad } else { theme.dim }),
    ));

    Line::from(spans)
}
