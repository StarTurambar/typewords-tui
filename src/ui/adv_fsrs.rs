//! 高级 FSRS 页：目标留存率、最大间隔、调度开关与 21 个权重。
//!
//! layout 与 render 共用同一组条目行；targets 只读 layout。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{Action, AdvFsrsRow, App, weight_text};
use crate::domain::i18n::tr;
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

/// 分区起点：行下标 -> 标题 key。顺序即展示顺序。
///
/// 与 `src/app/mod.rs` 的 `AdvFsrsRow::ALL` 顺序绑定：前 4 行是数值与
/// 开关，中间 21 行是权重，最后一行是「恢复默认」。
const SECTIONS: &[(usize, &str)] = &[
    (0, "fsrs_retention_interval"),
    (2, "fsrs_schedule_switch"),
    (4, "fsrs_weights"),
    (25, "fsrs_reset"),
];

#[derive(Clone, Debug)]
pub struct AdvFsrsLayout {
    /// 列表上方的标题 + 操作提示行
    pub hint: Rect,
    pub offset: usize,
    /// (行下标, 矩形)；行下标为 `None` 的是分区标题行。
    pub rows: Vec<(Option<usize>, Rect)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Section(&'static str),
    Item(usize),
}

fn entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut next_section = 0usize;
    for index in 0..AdvFsrsRow::ALL.len() {
        if let Some((start, key)) = SECTIONS.get(next_section).copied()
            && index == start
        {
            entries.push(Entry::Section(key));
            next_section += 1;
        }
        entries.push(Entry::Item(index));
    }
    entries
}

pub fn layout(app: &App, content: Rect) -> AdvFsrsLayout {
    // 顶栏只有外壳的「设置」标题：子屏自己的名字画在内容区第一行，
    // 否则用户在 26 行参数里看不出这是「高级 FSRS」。
    let hint = Rect::new(content.x, content.y, content.width, 1);
    let list = Rect::new(
        content.x,
        content.y.saturating_add(1),
        content.width,
        content.height.saturating_sub(1),
    );
    let entries = entries();
    let visible = list.height as usize;
    let cursor_entry = entries
        .iter()
        .position(|entry| matches!(entry, Entry::Item(index) if *index == app.adv_fsrs_cursor))
        .unwrap_or(0);

    let mut offset = widgets::list_offset(cursor_entry, entries.len(), visible);
    if offset > 0 {
        // 尽量让分区标题跟着当前条目一起滚入视口。
        let start = entries[..=offset]
            .iter()
            .rposition(|entry| matches!(entry, Entry::Section(_)))
            .unwrap_or(0);
        if cursor_entry - start < visible {
            offset = start;
        }
    }

    let rows = entries
        .iter()
        .skip(offset)
        .take(visible)
        .enumerate()
        .map(|(position, entry)| {
            let item = match entry {
                Entry::Item(index) => Some(*index),
                Entry::Section(_) => None,
            };
            (
                item,
                Rect::new(
                    list.x,
                    list.y + position as u16,
                    // 最右 1 列留给滚动条。
                    list.width.saturating_sub(1),
                    1,
                ),
            )
        })
        .collect();

    AdvFsrsLayout { hint, offset, rows }
}

pub fn targets(app: &App, content: Rect) -> Vec<Target> {
    let layout = layout(app, content);
    layout
        .rows
        .iter()
        .filter_map(|(entry, rect)| {
            let index = (*entry)?;
            let action = if index == app.adv_fsrs_cursor {
                Action::AdvFsrsActivate(index)
            } else {
                Action::AdvFsrsSelect(index)
            };
            // 行矩形已在布局里让出最右一列（滚动条）。
            Some(Target::new(*rect, action))
        })
        .collect()
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Settings, |frame, content| {
        let layout = layout(app, content);
        render_hint(frame, layout.hint, theme);
        let entries = entries();
        for (position, (_, rect)) in layout.rows.iter().enumerate() {
            match entries.get(layout.offset + position) {
                Some(Entry::Section(key)) => {
                    render_section(frame, *rect, &tr(key), theme);
                }
                Some(Entry::Item(index)) => {
                    render_row(app, frame, *rect, *index, theme);
                }
                None => {}
            }
        }

        // 滚动条画在列表区（标题行下方），不能占标题行。
        let list_top = content.y.saturating_add(1);
        let list_height = content.height.saturating_sub(1);
        widgets::scrollbar(
            frame,
            Rect::new(content.right().saturating_sub(1), list_top, 1, list_height),
            widgets::ScrollbarState::new(layout.offset, entries.len(), list_height as usize),
            theme,
        );
    });
}

fn render_hint(frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let text = fit_text(
        "  高级 FSRS · ↑↓ 选择 · Enter 修改 · Esc 保存返回",
        rect.width as usize,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(theme.dim),
        ))),
        rect,
    );
}

fn render_section(frame: &mut Frame, rect: Rect, title: &str, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let prefix = fit_text(&format!("▎ {title} "), rect.width as usize);
    let prefix_width = display_width(&prefix);
    let rule_width = (rect.width as usize).saturating_sub(prefix_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(rule_width), Style::default().fg(theme.border)),
        ])),
        rect,
    );
}

fn render_row(app: &App, frame: &mut Frame, rect: Rect, index: usize, theme: &Theme) {
    let Some(row) = AdvFsrsRow::ALL.get(index).copied() else {
        return;
    };
    if rect.width < 8 || rect.height == 0 {
        return;
    }
    let selected = index == app.adv_fsrs_cursor;
    if selected {
        widgets::paint_rect(frame, rect, theme.selection);
        widgets::paint_rect(
            frame,
            Rect::new(rect.x, rect.y, 1, rect.height),
            theme.accent,
        );
    }

    let inner_x = rect.x.saturating_add(2);
    // 行矩形已扣掉最右一列滚动条，再留 4 列空白，避免数值贴着滚动条。
    let inner_width = rect.width.saturating_sub(4) as usize;
    if inner_width == 0 {
        return;
    }

    let label_text = row.label();
    let (value_text, value_color, range_text) = row_value(app, row, theme);
    // 窄屏时先丢掉区间列：行名与当前值永远可读。
    let range_text = range_text.filter(|text| {
        !text.is_empty() && display_width(text) + display_width(&value_text) + 6 < inner_width
    });
    let range_width = range_text.as_ref().map(|t| display_width(t)).unwrap_or(0);
    let range_gap = if range_width > 0 { 3 } else { 0 };
    let value = fit_text(
        &value_text,
        inner_width.saturating_sub(range_width + range_gap).max(1),
    );
    let value_width = display_width(&value);
    let label_budget = inner_width
        .saturating_sub(value_width + range_width + range_gap + 1)
        .max(1);
    let label = fit_text(&label_text, label_budget);
    let label_width = display_width(&label);
    let gap = inner_width.saturating_sub(label_width + value_width + range_width + range_gap);

    let label_style = if selected {
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    let mut spans = vec![
        Span::styled(label, label_style),
        Span::raw(" ".repeat(gap)),
        Span::styled(value, Style::default().fg(value_color)),
    ];
    if let Some(range) = range_text {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(range, Style::default().fg(theme.dim)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(inner_x, rect.y, inner_width as u16, 1),
    );
}

/// 一行的右侧内容：当前值 + 颜色 + 可选的合法区间（权重行才有）。
fn row_value(app: &App, row: AdvFsrsRow, theme: &Theme) -> (String, Color, Option<String>) {
    let parameters = &app.boot.settings.fsrs_parameters;
    match row {
        AdvFsrsRow::Retention => (
            format!("{}%", (app.fsrs_request_retention() * 100.0).round() as i64),
            theme.fg,
            Some("50% ~ 99%".to_string()),
        ),
        AdvFsrsRow::MaximumInterval => (
            format!("{} 天", app.fsrs_maximum_interval().round() as i64),
            theme.fg,
            Some("1 ~ 36500".to_string()),
        ),
        AdvFsrsRow::EnableFuzz => bool_value(parameters.enable_fuzz, theme),
        AdvFsrsRow::EnableShortTerm => bool_value(parameters.enable_short_term, theme),
        AdvFsrsRow::Weight(index) => {
            let bounds = crate::domain::fsrs::weight_bounds(parameters, index);
            (
                weight_text(app.adv_fsrs_weight(index)),
                theme.fg,
                Some(format!(
                    "{} ~ {}",
                    weight_text(bounds.0),
                    weight_text(bounds.1)
                )),
            )
        }
        AdvFsrsRow::Reset => ("按 Enter 恢复".to_string(), theme.dim, None),
    }
}

fn bool_value(enabled: bool, theme: &Theme) -> (String, Color, Option<String>) {
    (
        if enabled { "开启" } else { "关闭" }.to_string(),
        if enabled { theme.good } else { theme.muted },
        None,
    )
}
