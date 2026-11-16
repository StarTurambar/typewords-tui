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

/// 带光标的列表的滚动偏移：光标始终落在视口内。
///
/// 列表页（词库 / 词条 / 快捷键 / 流程 / 文章）共用这一条规则：光标在
/// 视口上方或下方时把视口拉回刚好露出光标，内容不超过一屏时偏移恒为 0
/// ——没有溢出就不该滚动。
pub fn list_offset(cursor: usize, total: usize, visible: usize) -> usize {
    if total == 0 || visible == 0 {
        return 0;
    }
    cursor
        .min(total.saturating_sub(1))
        .saturating_sub(visible.saturating_sub(1))
}

/// 竖向滚动条的状态：滚动偏移、内容总行数、视口行数。
///
/// 偏移以「行」为单位。`ScrollbarState::new` 会把越界的偏移夹回
/// `[0, total - visible]`——窗口变高、条目变少后旧偏移可能已经超出上限。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollbarState {
    pub offset: usize,
    pub total: usize,
    pub visible: usize,
}

impl ScrollbarState {
    pub fn new(offset: usize, total: usize, visible: usize) -> Self {
        Self {
            offset: offset.min(total.saturating_sub(visible)),
            total,
            visible,
        }
    }

    /// 内容是否超出视口；没有超出时不画滚动条、也不能滚动。
    pub fn has_overflow(self) -> bool {
        self.visible > 0 && self.total > self.visible
    }

    /// 最大滚动偏移。
    pub fn max_offset(self) -> usize {
        self.total.saturating_sub(self.visible)
    }

    /// 滑块在视口里的 `(起始行, 行数)`；没有溢出时返回 `None`。
    ///
    /// 滑块随内容比例变长（至少 1 行），`offset` 在两端时贴顶 / 贴底。
    /// 这里只算几何，不画——绘制与鼠标命中读到的是同一个结果。
    pub fn thumb(self) -> Option<(u16, u16)> {
        if !self.has_overflow() {
            return None;
        }
        let visible = self.visible;
        let length = (visible as u64 * visible as u64 / self.total as u64)
            .max(1)
            .min(visible as u64) as u16;
        let travel = visible - length as usize;
        let max_offset = self.max_offset();
        let start = if travel == 0 || max_offset == 0 {
            0
        } else {
            (self.offset.min(max_offset) as u64 * travel as u64 / max_offset as u64) as u16
        };
        Some((start, length))
    }
}

/// 在 `rect`（通常是最右 1 列）画竖向滚动条：轨道 `│` + 滑块 `█`。
///
/// 没有溢出、或矩形为空时什么都不画。颜色只用主题 token：轨道 `border`、
/// 滑块 `accent_alt`（与旧的 `↑` / `↓` 指示器同色）。滚动条画在页面
/// 上层，行内容、焦点高亮与命中区都必须先让出这一列。
pub fn scrollbar(frame: &mut Frame, rect: Rect, state: ScrollbarState, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let state = ScrollbarState::new(state.offset, state.total, rect.height as usize);
    let Some((start, length)) = state.thumb() else {
        return;
    };
    for row in 0..rect.height {
        let thumb = row >= start && row < start.saturating_add(length);
        let (symbol, color) = if thumb {
            ("█", theme.accent_alt)
        } else {
            ("│", theme.border)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(symbol, Style::default().fg(color))),
            Rect::new(rect.x, rect.y.saturating_add(row), 1, 1),
        );
    }
}

/// 滚动条上可点按的两段：滑块上方（`-1` 上滚）与下方（`+1` 下滚）。
///
/// 滑块本身不产生动作（终端鼠标没有拖动语义）；点轨道一次滚一行，与原来
/// `↑` / `↓` 指示器的行为一致。没有溢出时返回空列表——不能滚，也没有可
/// 点的轨道。
pub fn scrollbar_click_rows(state: ScrollbarState, rect: Rect) -> Vec<(Rect, i32)> {
    if rect.width == 0 || rect.height == 0 {
        return Vec::new();
    }
    let state = ScrollbarState::new(state.offset, state.total, rect.height as usize);
    let Some((start, length)) = state.thumb() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if start > 0 {
        rows.push((Rect::new(rect.x, rect.y, rect.width, start), -1));
    }
    let below = rect.height.saturating_sub(start.saturating_add(length));
    if below > 0 {
        rows.push((
            Rect::new(rect.x, rect.y + start + length, rect.width, below),
            1,
        ));
    }
    rows
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
    fn scrollbar_hides_when_content_fits() {
        // 没有溢出：不画滚动条（thumb 为 None），点击区也为空。
        let fits = ScrollbarState::new(0, 5, 20);
        assert!(!fits.has_overflow());
        assert_eq!(fits.thumb(), None);
        assert_eq!(
            scrollbar_click_rows(fits, Rect::new(0, 0, 1, 20)),
            Vec::new()
        );
        // 空内容 / 零高度同样不该出现滚动条。
        assert_eq!(ScrollbarState::new(0, 0, 0).thumb(), None);
        assert_eq!(ScrollbarState::new(0, 0, 10).thumb(), None);
    }

    #[test]
    fn scrollbar_clamps_offset_and_pins_thumb_to_ends() {
        let state = ScrollbarState::new(100, 100, 10);
        assert_eq!(state.offset, 90, "偏移应夹到 total - visible");
        assert_eq!(state.max_offset(), 90);

        let top = ScrollbarState::new(0, 100, 10).thumb().expect("应有滑块");
        assert_eq!(top.0, 0, "滚到顶时滑块贴顶");
        let bottom = ScrollbarState::new(90, 100, 10).thumb().expect("应有滑块");
        assert_eq!(bottom.0 + bottom.1, 10, "滚到底时滑块贴底: {bottom:?}");
        // 滑块至少 1 行，且永远落在视口里。
        let tiny = ScrollbarState::new(5000, 10_000, 7).thumb().unwrap();
        assert!(tiny.1 >= 1);
        assert!(tiny.0 as usize + tiny.1 as usize <= 7);
    }

    #[test]
    fn scrollbar_click_rows_skip_the_thumb() {
        // 视口 10 行、内容 100 行：滑块 1 行；滚到中间时上下都可点。
        let rect = Rect::new(4, 2, 1, 10);
        let state = ScrollbarState::new(45, 100, 10);
        let (start, length) = state.thumb().expect("应有滑块");
        let rows = scrollbar_click_rows(state, rect);
        assert_eq!(rows.len(), 2, "滑块上下各有一段轨道: {rows:?}");
        assert_eq!(rows[0].0.y, rect.y);
        assert_eq!(rows[0].0.height, start);
        assert_eq!(rows[0].1, -1);
        assert_eq!(rows[1].0.y, rect.y + start + length);
        assert_eq!(rows[1].0.height, rect.height - start - length);
        assert_eq!(rows[1].1, 1);

        // 滚到顶：上方没有轨道，只有下滚一段。
        let top = scrollbar_click_rows(ScrollbarState::new(0, 100, 10), rect);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].1, 1);
    }

    #[test]
    fn list_offset_keeps_the_cursor_visible() {
        // 内容不足一屏：偏移恒为 0（不能滚动）。
        assert_eq!(list_offset(0, 5, 10), 0);
        assert_eq!(list_offset(4, 5, 10), 0);
        // 光标越过视口底部：视口刚好把光标露出来。
        assert_eq!(list_offset(6, 20, 5), 2);
        assert_eq!(list_offset(19, 20, 5), 15);
        // 光标还在首屏里移动时不滚。
        assert_eq!(list_offset(3, 20, 5), 0);
        assert_eq!(list_offset(0, 20, 5), 0);
        assert_eq!(list_offset(0, 0, 5), 0);
        assert_eq!(list_offset(3, 20, 0), 0);
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
