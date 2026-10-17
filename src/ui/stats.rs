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
    hint: Option<Rect>,
    tile_columns: usize,
    tile_height: u16,
    offset: usize,
    visible: usize,
}

fn layout(app: &App, content: Rect) -> StatsLayout {
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

    let total = insights::recent_days(&app.boot.base, RECENT_DAYS).len();
    let mut list_height = content.bottom().saturating_sub(section.bottom());
    if total > list_height as usize {
        list_height = list_height.saturating_sub(1);
    }
    let list = Rect::new(content.x, section.bottom(), content.width, list_height);
    let hint = (total > list_height as usize)
        .then(|| Rect::new(content.x, list.bottom(), content.width, 1));
    let visible = list_height as usize;
    let offset = (app.stats_scroll as usize).min(total.saturating_sub(visible));

    StatsLayout {
        header,
        tiles,
        section,
        list,
        hint,
        tile_columns,
        tile_height,
        offset,
        visible,
    }
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Stats, |frame, content| {
        let layout = layout(app, content);
        render_header(frame, layout.header, theme);
        render_tiles(app, frame, &layout, theme);
        render_recent(app, frame, &layout, theme);
    });
}

fn render_header(frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.height == 0 || rect.width == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(widgets::section_header(
            "统计",
            "总览",
            rect.width as usize,
            theme,
        )),
        rect,
    );
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

fn render_recent(app: &App, frame: &mut Frame, layout: &StatsLayout, theme: &Theme) {
    let days = insights::recent_days(&app.boot.base, RECENT_DAYS);
    if layout.section.height > 0 {
        frame.render_widget(
            Paragraph::new(widgets::section_header(
                "最近 14 天",
                &format!("{} 天有记录", days.len()),
                layout.section.width as usize,
                theme,
            )),
            layout.section,
        );
    }
    if layout.list.height == 0 {
        return;
    }
    if days.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("暂无学习记录", Style::default().fg(theme.dim))),
            Rect::new(
                layout.list.x,
                layout.list.y,
                layout.list.width,
                layout.list.height.min(1),
            ),
        );
        return;
    }

    let max_spend = days.iter().map(|(_, day)| day.spend).max().unwrap_or(0);
    let today = insights::today();
    let end = (layout.offset + layout.visible).min(days.len());
    for (row, (date, day)) in days[layout.offset..end].iter().enumerate() {
        let rect = Rect::new(
            layout.list.x,
            layout.list.y + row as u16,
            layout.list.width,
            1,
        );
        render_day(frame, rect, *date, day, max_spend, *date == today, theme);
    }

    if let Some(hint) = layout.hint {
        let more = layout.offset + layout.visible < days.len();
        let mut spans: Vec<Span<'static>> = Vec::new();
        if layout.offset > 0 {
            spans.push(Span::styled("↑ 向上滚动  ", Style::default().fg(theme.dim)));
        }
        if more {
            spans.push(Span::styled("↓ 继续滚动", Style::default().fg(theme.dim)));
        } else if layout.offset > 0 {
            spans.push(Span::styled("↓ 已到底部", Style::default().fg(theme.dim)));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), hint);
    }
}

fn render_day(
    frame: &mut Frame,
    rect: Rect,
    date: chrono::NaiveDate,
    day: &DayTotals,
    max_spend: i64,
    is_today: bool,
    theme: &Theme,
) {
    if rect.width < 28 || rect.height == 0 {
        return;
    }
    let width = rect.width as usize;
    if is_today {
        widgets::paint_rect(frame, rect, theme.selection);
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

    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(rect.x, rect.y, rect.width, 1),
    );
}
