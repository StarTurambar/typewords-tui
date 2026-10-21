//! 帮助页：快捷键总览。
//!
//! 宽屏分两栏（练习 / 导航），窄屏单栏并支持 ↑↓ 滚动；
//! 高度不足时优先显示前面的常用按键。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text, wrap_text};

/// 内容宽度达到此值时左练习、右导航分两栏展示。
const TWO_COLUMN_MIN_WIDTH: u16 = 78;

/// 拆成短行，避免窄屏按字符折行时把 Shift+拖拽 拆开。
const MOUSE_NOTES: &[&str] = &[
    "鼠标：",
    "侧栏/按钮/列表可点击，滚轮滚动；",
    "Shift+拖拽选择文本",
];

#[derive(Clone, Copy, Debug)]
enum HelpEntry {
    Section(&'static str),
    Key(&'static str, &'static str),
    Note(&'static str),
}

const PRACTICE: &[HelpEntry] = &[
    HelpEntry::Section("练习"),
    HelpEntry::Key("字母 / 数字", "输入单词"),
    HelpEntry::Key("空格", "确认 / 继续"),
    HelpEntry::Key("Esc", "显示完整单词"),
    HelpEntry::Key("Tab", "跳过当前词"),
    HelpEntry::Key("\u{60}", "标记 / 取消已掌握"),
    HelpEntry::Key("Enter", "收藏 / 取消收藏"),
    HelpEntry::Key("1 / 2 / 3", "自测：认识 / 不认识 / 已掌握"),
    HelpEntry::Key("Alt+1~4", "自测选择题 A~D"),
    HelpEntry::Key("Ctrl+I", "切换默写遮罩"),
    HelpEntry::Key("Ctrl+Z", "切换翻译"),
    HelpEntry::Key("Ctrl+,", "暂停 / 恢复计时"),
    HelpEntry::Key("Ctrl+P", "播放单词发音"),
    HelpEntry::Key("Ctrl+R", "随机默写当前词表"),
    HelpEntry::Key("Ctrl+B / Ctrl+L", "收起状态栏 / 词表"),
    HelpEntry::Key("Ctrl+E", "显示 / 隐藏例句与词源"),
    HelpEntry::Key("Shift+→ / Ctrl+J", "跳到下一阶段"),
    HelpEntry::Key("Ctrl+Enter", "重学本组"),
    HelpEntry::Key("Alt+← / Alt+→", "上一组 / 下一组"),
    HelpEntry::Key("Alt+Enter", "默写本组"),
    HelpEntry::Key("Shift+Enter", "收藏到指定词典"),
    HelpEntry::Key("Ctrl+G", "切换简洁模式"),
    HelpEntry::Key("Ctrl+Q", "切换主题"),
    HelpEntry::Key("Ctrl+C", "保存并返回主页"),
];

const NAVIGATION: &[HelpEntry] = &[
    HelpEntry::Section("导航/查看"),
    HelpEntry::Key("1 ~ 5", "切换页面"),
    HelpEntry::Key("?", "帮助"),
    HelpEntry::Key("q", "退出"),
    HelpEntry::Key("d / s / t", "词库 / 设置 / 统计"),
    HelpEntry::Key("m", "切换练习模式"),
    HelpEntry::Key("Enter", "开始学习"),
    HelpEntry::Key("f", "自由练习"),
    HelpEntry::Key("+ / -", "每日词数"),
    HelpEntry::Key("Ctrl+←", "上一个词"),
    HelpEntry::Key("Ctrl+→", "下一个词"),
    HelpEntry::Key("↑ / ↓", "列表与统计滚动"),
    HelpEntry::Key("鼠标点击", "侧栏 / 按钮 / 列表"),
    HelpEntry::Key("滚轮", "滚动列表"),
    HelpEntry::Key("Shift+拖拽", "选择文本"),
];

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    // 练习中进入帮助：整屏显示，不画侧栏。
    //
    // 练习中侧栏的导航是禁用的，画出来却点不动，用户会以为鼠标坏了。
    if app.session.is_some() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "帮助（Esc 返回练习）",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(area.x + 2, area.y, area.width.saturating_sub(4), 1),
        );
        let content = Rect::new(
            area.x + 2,
            area.y + 1,
            area.width.saturating_sub(4),
            area.height.saturating_sub(1),
        );
        if content.width >= TWO_COLUMN_MIN_WIDTH && content.height >= 20 {
            render_columns(frame, content, theme, app.help_scroll);
        } else {
            render_single(frame, content, theme, app.help_scroll);
        }
        return;
    }
    shell::render(app, frame, area, theme, Nav::Help, |frame, content| {
        if content.width >= TWO_COLUMN_MIN_WIDTH && content.height >= 20 {
            render_columns(frame, content, theme, app.help_scroll);
        } else {
            render_single(frame, content, theme, app.help_scroll);
        }
    });
}

fn render_columns(frame: &mut Frame, content: Rect, theme: &Theme, scroll: u16) {
    // 两栏之间：1 列分隔线 + 1 列留白。
    let left_width = content.width.saturating_sub(2) / 2;
    if left_width < 26 || content.height == 0 {
        render_single(frame, content, theme, scroll);
        return;
    }
    let left = Rect::new(content.x, content.y, left_width, content.height);
    let divider_x = content.x + left_width;
    let right = Rect::new(
        divider_x.saturating_add(2),
        content.y,
        content.width.saturating_sub(left_width + 2),
        content.height,
    );

    // 两栏各自滚动。
    //
    // 早期实现两栏都固定从第 0 行开始画，且不响应 `help_scroll`：
    // 终端高度不够时，最后几行（Ctrl+C / Ctrl+Q 等）被裁掉且滚不出来。
    let visible = content.height as usize;
    let left_max = PRACTICE.len().saturating_sub(visible);
    let right_entries_len = NAVIGATION.len() + 4; // 含末尾的鼠标说明
    let right_max = right_entries_len.saturating_sub(visible);
    let (left_offset, right_offset) = if left_max.max(right_max) == 0 {
        (0, 0)
    } else {
        // 滚动量按两栏里更长的一栏分配，两栏同步前进。
        let offset = (scroll as usize).min(left_max.max(right_max));
        (offset.min(left_max), offset.min(right_max))
    };
    render_entries(frame, left, PRACTICE, theme, left_offset);
    let mut right_entries: Vec<HelpEntry> = NAVIGATION.to_vec();
    push_mouse_notes(&mut right_entries);
    render_entries(frame, right, &right_entries, theme, right_offset);

    for y in content.y..content.bottom() {
        frame.render_widget(
            Paragraph::new(Span::styled("│", Style::default().fg(theme.border))),
            Rect::new(divider_x, y, 1, 1),
        );
    }
    render_scroll_indicators(
        frame,
        content,
        theme,
        left_offset > 0 || right_offset > 0,
        left_offset < left_max || right_offset < right_max,
    );
}

fn render_single(frame: &mut Frame, content: Rect, theme: &Theme, scroll: u16) {
    let mut entries: Vec<HelpEntry> = Vec::new();
    entries.extend_from_slice(PRACTICE);
    entries.extend_from_slice(NAVIGATION);
    push_mouse_notes(&mut entries);
    let list = Rect::new(
        content.x,
        content.y,
        content.width.saturating_sub(2).max(1),
        content.height,
    );
    let visible = list.height as usize;
    let max_offset = entries.len().saturating_sub(visible);
    let offset = (scroll as usize).min(max_offset);
    render_entries(frame, list, &entries, theme, offset);
    render_scroll_indicators(frame, content, theme, offset > 0, offset < max_offset);
}

/// 右侧的 `↑` / `↓` 滚动指示器。
fn render_scroll_indicators(
    frame: &mut Frame,
    content: Rect,
    theme: &Theme,
    can_scroll_up: bool,
    can_scroll_down: bool,
) {
    if content.width == 0 {
        return;
    }
    if can_scroll_up {
        frame.render_widget(
            Paragraph::new(Span::styled("↑", Style::default().fg(theme.accent_alt))),
            Rect::new(content.right().saturating_sub(1), content.y, 1, 1),
        );
    }
    if can_scroll_down {
        frame.render_widget(
            Paragraph::new(Span::styled("↓", Style::default().fg(theme.accent_alt))),
            Rect::new(
                content.right().saturating_sub(1),
                content.bottom().saturating_sub(1),
                1,
                1,
            ),
        );
    }
}

/// 帮助页的鼠标目标：滚动指示器。
///
/// 正文区整体是「点击返回上一层」，但滚动指示器要能滚动——早期实现
/// 两者共用同一块矩形，点 `↓` 反而退出了帮助页。
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let mut result = Vec::new();
    let entries = help_entry_count();
    let visible = content.height as usize;
    let max_offset = entries.saturating_sub(visible);
    let offset = (app.help_scroll as usize).min(max_offset);
    if content.width == 0 {
        return result;
    }
    if offset > 0 {
        result.push(Target::new(
            Rect::new(content.right().saturating_sub(1), content.y, 1, 1),
            crate::app::Action::HelpScroll(-1),
        ));
    }
    if offset < max_offset {
        result.push(Target::new(
            Rect::new(
                content.right().saturating_sub(1),
                content.bottom().saturating_sub(1),
                1,
                1,
            ),
            crate::app::Action::HelpScroll(1),
        ));
    }
    result
}

/// 帮助页的最大滚动量。
///
/// 键位表条目会随版本增减，写死一个上限迟早会截断或滚过头。
pub fn max_scroll() -> u16 {
    help_entry_count().saturating_sub(1) as u16
}

/// 帮助页的条目总数（两栏 / 单栏共用）。
fn help_entry_count() -> usize {
    let mut entries: Vec<HelpEntry> = Vec::new();
    entries.extend_from_slice(PRACTICE);
    entries.extend_from_slice(NAVIGATION);
    push_mouse_notes(&mut entries);
    entries.len()
}

fn push_mouse_notes(entries: &mut Vec<HelpEntry>) {
    entries.push(HelpEntry::Note(""));
    for line in MOUSE_NOTES {
        entries.push(HelpEntry::Note(line));
    }
}

fn render_entries(
    frame: &mut Frame,
    area: Rect,
    entries: &[HelpEntry],
    theme: &Theme,
    offset: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = area.width as usize;
    let mut y = area.y;
    for entry in entries.get(offset..).unwrap_or(&[]) {
        if y >= area.bottom() {
            break;
        }
        match entry {
            HelpEntry::Section(title) => {
                render_section(frame, Rect::new(area.x, y, area.width, 1), title, theme);
                y += 1;
            }
            HelpEntry::Key(key, label) => {
                render_key(
                    frame,
                    Rect::new(area.x, y, area.width, 1),
                    key,
                    label,
                    theme,
                );
                y += 1;
            }
            HelpEntry::Note(text) => {
                for line in wrap_text(text, width.saturating_sub(2)) {
                    if y >= area.bottom() {
                        break;
                    }
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!(" {line}"),
                            Style::default().fg(theme.dim),
                        )),
                        Rect::new(area.x, y, area.width, 1),
                    );
                    y += 1;
                }
            }
        }
    }
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

fn render_key(frame: &mut Frame, rect: Rect, key: &str, label: &str, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let width = rect.width as usize;
    let indent = usize::from(width > 2);
    let available = width.saturating_sub(indent);
    let key = fit_text(key, available);
    let key_width = display_width(&key);
    let label = fit_text(label, available.saturating_sub(key_width + 1));
    frame.render_widget(
        Paragraph::new(Line::from(widgets::key_hint(&key, &label, theme))),
        Rect::new(
            rect.x.saturating_add(indent as u16),
            rect.y,
            available as u16,
            1,
        ),
    );
}
