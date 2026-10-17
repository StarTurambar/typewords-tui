//! 通用小部件与文本工具：所有自定义行宽计算都走 unicode-width。

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::ui::theme::Theme;

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// 按显示宽度截断并加 …（CJK 不会被截成半个）。
pub fn fit_text(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let text = text.replace('\n', " ");
    if display_width(&text) <= max_width {
        return text;
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut result = String::new();
    let mut used = 0usize;
    for character in text.chars() {
        let width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + width + 1 > max_width {
            break;
        }
        result.push(character);
        used += width;
    }
    result.push('…');
    result
}

/// 按显示宽度折行；保留原文换行。
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for character in text.chars() {
        if character == '\n' {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
            continue;
        }
        let char_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if current_width > 0 && current_width + char_width > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(character);
        current_width += char_width;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn rule(width: usize, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(theme.border),
    ))
}

/// 左侧标题 + 右侧操作文案，中间自动填满。
pub fn section_header(title: &str, right: &str, width: usize, theme: &Theme) -> Line<'static> {
    let title = fit_text(title, width);
    let title_width = display_width(&title);
    let mut spans = vec![Span::styled(
        title,
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
    )];
    if !right.is_empty() && width > title_width + 2 {
        let available = width - title_width;
        let right = fit_text(right, available.saturating_sub(1));
        let right_width = display_width(&right);
        let gap = width.saturating_sub(title_width + right_width);
        if gap > 0 {
            spans.push(Span::raw(" ".repeat(gap)));
        }
        spans.push(Span::styled(right, Style::default().fg(theme.accent_alt)));
    }
    Line::from(spans)
}

pub fn key_hint(key: &str, label: &str, theme: &Theme) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            key.to_string(),
            Style::default()
                .fg(theme.accent_alt)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(label.to_string(), Style::default().fg(theme.muted)),
    ]
}

/// 底部按键提示；超宽时从尾部丢弃，并以 … 结束。
pub fn footer_hints(hints: &[(&str, &str)], width: usize, theme: &Theme) -> Vec<Span<'static>> {
    let separator = "  ";
    let sep_width = display_width(separator);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;
    for (index, (key, label)) in hints.iter().enumerate() {
        let key = fit_text(key, width);
        let key_width = display_width(&key);
        let label = fit_text(label, width.saturating_sub(key_width + 1));
        let label_width = display_width(&label);
        let prefix = if index == 0 { 0 } else { sep_width };
        let needed = prefix + key_width + 1 + label_width;
        if used + needed > width {
            truncated = true;
            break;
        }
        if prefix > 0 {
            spans.push(Span::styled(separator, Style::default().fg(theme.dim)));
            used += prefix;
        }
        spans.push(Span::styled(
            key,
            Style::default()
                .fg(theme.accent_alt)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label, Style::default().fg(theme.muted)));
        used += key_width + 1 + label_width;
    }
    if truncated && used < width {
        spans.push(Span::styled("…", Style::default().fg(theme.dim)));
    }
    spans
}

/// 实心块进度条（练习页沿用）。
pub fn bar_spans(percent: u32, width: usize, filled: Color, empty: Color) -> Vec<Span<'static>> {
    let count = ((percent as usize * width) / 100).min(width);
    vec![
        Span::styled("█".repeat(count), Style::default().fg(filled)),
        Span::styled(
            "░".repeat(width.saturating_sub(count)),
            Style::default().fg(empty),
        ),
    ]
}

/// 细线进度条（主页/统计）。
pub fn thin_bar_spans(
    percent: u32,
    width: usize,
    filled: Color,
    empty: Color,
) -> Vec<Span<'static>> {
    let count = ((percent as usize * width) / 100).min(width);
    vec![
        Span::styled("━".repeat(count), Style::default().fg(filled)),
        Span::styled(
            "─".repeat(width.saturating_sub(count)),
            Style::default().fg(empty),
        ),
    ]
}

pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let vertical = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Length((area.height.saturating_sub(height)) / 2),
        ratatui::layout::Constraint::Length(height),
        ratatui::layout::Constraint::Min(0),
    ])
    .split(area);
    let horizontal = ratatui::layout::Layout::horizontal([
        ratatui::layout::Constraint::Length((area.width.saturating_sub(width)) / 2),
        ratatui::layout::Constraint::Length(width),
        ratatui::layout::Constraint::Min(0),
    ])
    .split(vertical[1]);
    horizontal[1]
}

pub fn modal_block(title: &str, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
}

pub fn paint_rect(frame: &mut Frame, rect: Rect, color: Color) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    frame.render_widget(Block::default().style(Style::default().bg(color)), rect);
}

pub fn draw_button(frame: &mut Frame, rect: Rect, label: &str, bg: Color, fg: Color, bold: bool) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let mut style = Style::default().bg(bg).fg(fg);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    let label = fit_text(label, rect.width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label, style)))
            .style(Style::default().bg(bg))
            .alignment(Alignment::Center),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_text_respects_display_width() {
        assert_eq!(fit_text("abcdef", 4), "abc…");
        assert_eq!(display_width(&fit_text("测试词库", 5)), 5);
        assert_eq!(fit_text("中文", 4), "中文");
        assert_eq!(fit_text("anything", 0), "");
    }

    #[test]
    fn wrap_text_keeps_cjk_width() {
        let lines = wrap_text("这是测试词库", 6);
        assert!(lines.iter().all(|line| display_width(line) <= 6));
        assert!(lines.len() > 1);
    }

    #[test]
    fn footer_hints_truncate_without_overflow() {
        let theme = crate::ui::theme::Theme::from_spec(&crate::theme::default_dark());
        let hints = [
            ("enter", "开始学习"),
            ("f", "自由练习"),
            ("d", "词库"),
            ("s", "设置"),
            ("t", "统计"),
            ("?", "帮助"),
            ("q", "退出"),
        ];
        for width in 0..60 {
            let spans = footer_hints(&hints, width, &theme);
            let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
            assert!(display_width(&text) <= width, "width {width}: {text:?}");
        }
    }
}
