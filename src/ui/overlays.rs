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

pub fn summary_layout(area: Rect) -> SummaryLayout {
    let rect = widgets::centered_rect(64, 14, area);
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

pub fn render_summary(frame: &mut Frame, area: Rect, theme: &Theme, summary: &SummaryData) {
    let layout = summary_layout(area);
    frame.render_widget(Clear, layout.rect);
    widgets::paint_rect(frame, layout.rect, theme.panel);
    let percent = if summary.length == 0 {
        0
    } else {
        ((summary.last_learn_index.min(summary.length) as f64 / summary.length as f64) * 100.0)
            .round() as u32
    };
    let lines = vec![
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

pub fn render_edit(frame: &mut Frame, area: Rect, theme: &Theme, edit: &EditState) {
    let rect = edit_layout(area);
    frame.render_widget(Clear, rect);
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
    frame.render_widget(Clear, layout.rect);
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
        crate::app::EditTarget::WaitMs => tr("wait_time"),
        crate::app::EditTarget::SpaceCooldown => tr("space_cooldown"),
        crate::app::EditTarget::EasyLimit => tr("fsrs_easy_limit"),
        crate::app::EditTarget::GoodLimit => tr("fsrs_good_limit"),
        crate::app::EditTarget::HardLimit => tr("fsrs_hard_limit"),
        crate::app::EditTarget::Retention => tr("request_retention"),
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
    if app.summary.is_some() {
        let layout = summary_layout(area);
        return vec![
            Target::new(layout.continue_btn, Action::SummaryContinue),
            Target::new(layout.relearn_btn, Action::SummaryRelearn),
            Target::new(layout.home_btn, Action::SummaryHome),
        ];
    }
    Vec::new()
}

pub fn target_action(app: &crate::app::App, area: Rect, column: u16, row: u16) -> Option<Action> {
    let hits = |rect: Rect| {
        column >= rect.x && column < rect.right() && row >= rect.y && row < rect.bottom()
    };
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
    if app.summary.is_some() {
        let layout = summary_layout(area);
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
        let layout = summary_layout(Rect::new(0, 0, 60, 20));
        assert!(layout.rect.width <= 60);
        assert!(layout.rect.height <= 20);
        assert!(layout.continue_btn.right() <= layout.inner.right());
    }

    #[test]
    fn confirm_buttons_are_inside_modal() {
        let layout = confirm_layout(Rect::new(0, 0, 80, 24));
        assert!(layout.accept.right() <= layout.inner.right());
        assert!(layout.cancel.right() <= layout.inner.right());
    }
}
