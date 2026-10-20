//! 结算 / 输入 / 确认弹层。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::app::{Action, ConfirmState, EditState, SummaryData};
use crate::domain::i18n::tr;
use crate::domain::timer::format_duration;
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, fit_text};

#[derive(Clone, Debug)]
pub struct SummaryLayout {
    pub rect: Rect,
    pub inner: Rect,
    pub continue_btn: Rect,
    pub relearn_btn: Rect,
    pub home_btn: Rect,
}

#[derive(Clone, Debug)]
pub struct ConfirmLayout {
    pub rect: Rect,
    pub inner: Rect,
    pub accept: Rect,
    pub cancel: Rect,
}

/// 结算弹层在基础内容之外还需要多少行（错词 Top10 + 本周记录）。
fn summary_extra_lines(summary: &SummaryData) -> u16 {
    let mut extra = 0u16;
    if !summary.wrong_top.is_empty() {
        extra += 2 + summary.wrong_top.len().min(10) as u16;
    }
    extra += 2; // 空行 + 本周记录
    extra
}

/// 结算弹层布局：高度按内容自适应。
pub fn summary_layout(area: Rect, extra: u16) -> SummaryLayout {
    let rect = widgets::centered_rect(72, 12 + extra, area);
    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(2),
    );
    let button_y = inner.y + inner.height.saturating_sub(1);
    let gap = 1u16;
    let continue_width = 14.min(inner.width);
    let relearn_width = 11;
    let home_width = 10;
    let continue_btn = Rect::new(inner.x, button_y, continue_width, 1);
    let relearn_btn = Rect::new(inner.x + continue_width + gap, button_y, relearn_width, 1);
    let home_btn = Rect::new(
        inner.x + continue_width + gap + relearn_width + gap,
        button_y,
        home_width,
        1,
    );
    SummaryLayout {
        rect,
        inner,
        continue_btn,
        relearn_btn,
        home_btn,
    }
}

pub fn edit_layout(area: Rect) -> Rect {
    widgets::centered_rect(46, 5, area)
}

pub fn confirm_layout(area: Rect) -> ConfirmLayout {
    let rect = widgets::centered_rect(56, 7, area);
    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(2),
    );
    let button_y = inner.y + inner.height.saturating_sub(1);
    let accept = Rect::new(inner.x, button_y, 10, 1);
    let cancel = Rect::new(inner.x + 11, button_y, 10, 1);
    ConfirmLayout {
        rect,
        inner,
        accept,
        cancel,
    }
}

#[derive(Clone, Debug)]
pub struct GroupPickerLayout {
    pub rect: Rect,
    pub inner: Rect,
    /// (组号, 矩形)
    pub rows: Vec<(usize, Rect)>,
}

/// 跳组弹层：列出词库的所有学习组。
pub fn group_picker_layout(total_groups: usize, cursor: usize, area: Rect) -> GroupPickerLayout {
    let rect = widgets::centered_rect(44, 18, area);
    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(2),
    );
    // 顶部一行标题，底部一行提示，中间是列表
    let list = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(2),
    );
    let visible = list.height as usize;
    let offset = cursor.saturating_sub(visible.saturating_sub(1));
    let rows = (offset..total_groups.min(offset + visible))
        .map(|index| {
            (
                index,
                Rect::new(list.x, list.y + (index - offset) as u16, list.width, 1),
            )
        })
        .collect();
    GroupPickerLayout { rect, inner, rows }
}

pub fn render_group_picker(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    total_groups: usize,
    cursor: usize,
    current_group: usize,
) {
    let layout = group_picker_layout(total_groups, cursor, area);
    clear_modal_backdrop(frame, area, layout.rect, theme);
    widgets::paint_rect(frame, layout.rect, theme.panel);

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("跳到第几组（共 {total_groups} 组）"),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    for (index, rect) in layout.rows.iter() {
        let group = index + 1;
        let selected = group == cursor + 1;
        let current = group == current_group;
        let marker = if current { "●" } else { " " };
        let text = format!("{marker} 第 {group} 组");
        let color = if selected {
            theme.accent
        } else if current {
            theme.good
        } else {
            theme.fg
        };
        let style = if selected {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        let mut spans = vec![Span::styled(text, style)];
        if current {
            spans.push(Span::styled("  当前", Style::default().fg(theme.dim)));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), *rect);
    }
    let hint_y = layout.inner.bottom().saturating_sub(1);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ 选择 · Enter 跳转 · Esc 取消",
            Style::default().fg(theme.dim),
        ))),
        Rect::new(layout.inner.x, hint_y, layout.inner.width, 1),
    );
}

/// 「收藏到指定词典」弹层：列出可收藏的词典。
pub fn render_collect_picker(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    dicts: &[(usize, String)],
    cursor: usize,
) {
    let rect = widgets::centered_rect(44, 12, area);
    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(2),
    );
    clear_modal_backdrop(frame, area, rect, theme);
    widgets::paint_rect(frame, rect, theme.panel);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "收藏到哪个词典",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let list_y = inner.y + 1;
    for (offset, (_, name)) in dicts.iter().enumerate() {
        if offset as u16 + 1 >= inner.height.saturating_sub(1) {
            break;
        }
        let selected = offset == cursor;
        let style = if selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.fg)
        };
        let marker = if selected { "▶ " } else { "  " };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!("{marker}{name}"), style))),
            Rect::new(inner.x, list_y + offset as u16, inner.width, 1),
        );
    }

    let hint_y = inner.bottom().saturating_sub(1);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓ 选择 · Enter 收藏 · Esc 取消",
            Style::default().fg(theme.dim),
        ))),
        Rect::new(inner.x, hint_y, inner.width, 1),
    );
}

fn clear_modal_backdrop(frame: &mut Frame, area: Rect, rect: Rect, theme: &Theme) {
    let margin = 2u16;
    let left = rect.x.saturating_sub(margin).max(area.x);
    let top = rect.y.saturating_sub(margin).max(area.y);
    let right = rect.right().saturating_add(margin).min(area.right());
    let bottom = rect.bottom().saturating_add(margin).min(area.bottom());
    let backdrop = Rect::new(
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    );
    frame.render_widget(Clear, backdrop);
    widgets::paint_rect(frame, backdrop, theme.bg);
}

pub fn render_summary(frame: &mut Frame, area: Rect, theme: &Theme, summary: &SummaryData) {
    let layout = summary_layout(area, summary_extra_lines(summary));
    clear_modal_backdrop(frame, area, layout.rect, theme);
    widgets::paint_rect(frame, layout.rect, theme.panel);
    let percent = if summary.length == 0 {
        0
    } else {
        ((summary.last_learn_index.min(summary.length) as f64 / summary.length as f64) * 100.0)
            .round() as u32
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "今日任务完成",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled("学习报告", Style::default().fg(theme.dim))),
        Line::from(""),
        Line::from(vec![
            Span::styled("学习时长 ", Style::default().fg(theme.muted)),
            Span::styled(
                format_duration(summary.spend_ms),
                Style::default().fg(theme.accent_alt),
            ),
            Span::styled("    正确率 ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{}%", summary.accuracy),
                Style::default().fg(theme.accent_alt),
            ),
        ]),
        Line::from(vec![
            Span::styled("新词 ", Style::default().fg(theme.muted)),
            Span::styled(summary.new_words.to_string(), Style::default().fg(theme.fg)),
            Span::styled("    复习 ", Style::default().fg(theme.muted)),
            Span::styled(summary.review.to_string(), Style::default().fg(theme.fg)),
            Span::styled("    错词 ", Style::default().fg(theme.muted)),
            Span::styled(summary.wrong.to_string(), Style::default().fg(theme.bad)),
        ]),
        Line::from(vec![
            Span::styled("学习进度 ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{} / {}", summary.last_learn_index, summary.length),
                Style::default().fg(theme.fg),
            ),
            Span::styled(
                if summary.complete { "  已完成" } else { "" },
                Style::default().fg(theme.good),
            ),
        ]),
        Line::from(""),
        Line::from(widgets::thin_bar_spans(
            percent,
            layout.inner.width as usize,
            if summary.complete {
                theme.good
            } else {
                theme.accent_alt
            },
            theme.dim,
        )),
    ];

    // 错词 Top10（上游结算报告有这一项）
    if !summary.wrong_top.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "错词 Top10",
            Style::default().fg(theme.dim),
        )));
        for (word, times) in &summary.wrong_top {
            if lines.len() as u16 + 2 >= layout.inner.height {
                break;
            }
            lines.push(Line::from(vec![
                Span::styled(format!("  {word}"), Style::default().fg(theme.fg)),
                Span::styled(format!("  ×{times}"), Style::default().fg(theme.bad)),
            ]));
        }
    }

    // 本周记录：只统计当前词典，与上游结算报告口径一致
    if lines.len() as u16 + 2 < layout.inner.height {
        lines.push(Line::from(""));
        lines.push(week_line(&summary.week_days, theme));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme.panel))
            .block(widgets::modal_block(" 结算 ", theme)),
        layout.rect,
    );
    widgets::draw_button(
        frame,
        layout.continue_btn,
        "Enter 继续",
        theme.accent,
        theme.bg,
        true,
    );
    widgets::draw_button(
        frame,
        layout.relearn_btn,
        "r 重学",
        theme.element,
        theme.fg,
        false,
    );
    widgets::draw_button(
        frame,
        layout.home_btn,
        "q 主页",
        theme.element,
        theme.fg,
        false,
    );
}

/// 本周七天的打卡点：有记录用实心点，没有用空心点。
fn week_line(
    days: &[Option<crate::domain::statistics::DayTotals>; 7],
    theme: &Theme,
) -> Line<'static> {
    let mut spans = vec![Span::styled("本周 ", Style::default().fg(theme.muted))];
    for (index, day) in days.iter().enumerate() {
        let studied = day.is_some();
        spans.push(Span::styled(
            crate::ui::insights::weekday_label(index),
            Style::default().fg(if studied { theme.accent_alt } else { theme.dim }),
        ));
        spans.push(Span::styled(
            if studied { "● " } else { "· " },
            Style::default().fg(if studied { theme.good } else { theme.dim }),
        ));
    }
    Line::from(spans)
}

pub fn render_edit(frame: &mut Frame, area: Rect, theme: &Theme, edit: &EditState) {
    let rect = edit_layout(area);
    clear_modal_backdrop(frame, area, rect, theme);
    widgets::paint_rect(frame, rect, theme.panel);
    let label = format!("修改 {}（Enter 保存 · Esc 取消）", edit_label(edit));
    let lines = vec![
        Line::from(Span::styled(label, Style::default().fg(theme.muted))),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.accent)),
            Span::styled(
                format!("{}_", edit.buffer),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme.panel))
            .block(widgets::modal_block(" 输入 ", theme)),
        rect,
    );
}

pub fn render_confirm(frame: &mut Frame, area: Rect, theme: &Theme, confirm: &ConfirmState) {
    let layout = confirm_layout(area);
    clear_modal_backdrop(frame, area, layout.rect, theme);
    widgets::paint_rect(frame, layout.rect, theme.panel);
    let message = fit_text(&confirm.message, layout.inner.width as usize);
    let lines = vec![
        Line::from(Span::styled(message, Style::default().fg(theme.fg))),
        Line::from(""),
        Line::from(Span::styled(
            "Enter/y 确认 · 其他键取消",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme.panel))
            .block(widgets::modal_block(" 确认 ", theme)),
        layout.rect,
    );
    widgets::draw_button(frame, layout.accept, "确认", theme.accent, theme.bg, true);
    widgets::draw_button(frame, layout.cancel, "取消", theme.element, theme.fg, false);
}

fn edit_label(edit: &EditState) -> String {
    match edit.target {
        crate::app::EditTarget::PerDay => tr("daily_words"),
        crate::app::EditTarget::Ratio => tr("word_review_ratio"),
        crate::app::EditTarget::Repeat => tr("repeat_count"),
        crate::app::EditTarget::RepeatCustom => tr("repeat_custom_count"),
        crate::app::EditTarget::WaitMs => tr("wait_time"),
        crate::app::EditTarget::SpaceCooldown => tr("space_cooldown"),
        crate::app::EditTarget::EasyLimit => tr("fsrs_easy_limit"),
        crate::app::EditTarget::GoodLimit => tr("fsrs_good_limit"),
        crate::app::EditTarget::HardLimit => tr("fsrs_hard_limit"),
        crate::app::EditTarget::Retention => tr("request_retention"),
        crate::app::EditTarget::Note => "单词笔记".to_string(),
    }
}

pub fn targets(app: &crate::app::App, area: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    if app.confirm.is_some() {
        let layout = confirm_layout(area);
        return vec![
            Target::new(layout.accept, Action::ConfirmAccept),
            Target::new(layout.cancel, Action::ConfirmCancel),
        ];
    }
    if let Some(summary) = &app.summary {
        let layout = summary_layout(area, summary_extra_lines(summary));
        return vec![
            Target::new(layout.continue_btn, Action::SummaryContinue),
            Target::new(layout.relearn_btn, Action::SummaryRelearn),
            Target::new(layout.home_btn, Action::SummaryHome),
        ];
    }
    if app.collect_picker.is_some() {
        return Vec::new();
    }
    if let Some(picker) = &app.group_picker {
        let layout = group_picker_layout(app.total_groups(), picker.cursor, area);
        return layout
            .rows
            .iter()
            .map(|(index, rect)| {
                Target::new(
                    *rect,
                    Action::GroupMove(*index as i32 - picker.cursor as i32),
                )
            })
            .collect();
    }
    Vec::new()
}

pub fn target_action(app: &crate::app::App, area: Rect, column: u16, row: u16) -> Option<Action> {
    let hits = |rect: Rect| {
        column >= rect.x && column < rect.right() && row >= rect.y && row < rect.bottom()
    };
    if app.collect_picker.is_some() {
        return None;
    }
    if let Some(picker) = &app.group_picker {
        let layout = group_picker_layout(app.total_groups(), picker.cursor, area);
        for (index, rect) in layout.rows.iter() {
            if hits(*rect) {
                return Some(Action::GroupMove(*index as i32 - picker.cursor as i32));
            }
        }
        if !hits(layout.rect) {
            return Some(Action::GroupCancel);
        }
        return None;
    }
    if app.confirm.is_some() {
        let layout = confirm_layout(area);
        if hits(layout.accept) {
            return Some(Action::ConfirmAccept);
        }
        if hits(layout.cancel) {
            return Some(Action::ConfirmCancel);
        }
        if !hits(layout.rect) {
            return Some(Action::ConfirmCancel);
        }
        return None;
    }
    if app.edit.is_some() {
        let rect = edit_layout(area);
        if !hits(rect) {
            return Some(Action::EditCancel);
        }
        return None;
    }
    if let Some(summary) = &app.summary {
        let layout = summary_layout(area, summary_extra_lines(summary));
        if hits(layout.continue_btn) {
            return Some(Action::SummaryContinue);
        }
        if hits(layout.relearn_btn) {
            return Some(Action::SummaryRelearn);
        }
        if hits(layout.home_btn) {
            return Some(Action::SummaryHome);
        }
        return None;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_layout_fits_min_terminal() {
        // 最小终端 60x20：带满额错词与本周记录时也不能溢出
        for extra in [0u16, 2, 14] {
            let layout = summary_layout(Rect::new(0, 0, 60, 20), extra);
            assert!(layout.rect.width <= 60);
            assert!(layout.rect.height <= 20);
            assert!(layout.continue_btn.right() <= layout.inner.right());
            assert!(layout.continue_btn.bottom() <= layout.rect.bottom());
        }
    }

    #[test]
    fn confirm_buttons_are_inside_modal() {
        let layout = confirm_layout(Rect::new(0, 0, 80, 24));
        assert!(layout.accept.right() <= layout.inner.right());
        assert!(layout.cancel.right() <= layout.inner.right());
    }
}
