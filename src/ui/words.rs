//! 词条管理页：搜索、编辑、批量标记。
//!
//! 对齐上游 `app/pages/(words)/dict.vue` 与 `components/word/WordMarkPickList.vue`。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

/// 词条管理页布局。
pub struct WordsLayout {
    pub header: Rect,
    pub list: Rect,
    pub footer: Rect,
}

pub fn layout(app: &App, content: Rect) -> WordsLayout {
    let header = Rect::new(content.x, content.y, content.width, content.height.min(1));
    let footer = Rect::new(
        content.x,
        content.bottom().saturating_sub(1),
        content.width,
        1,
    );
    let list = Rect::new(
        content.x,
        header.bottom(),
        content.width,
        content.height.saturating_sub(2),
    );
    let _ = app;
    WordsLayout {
        header,
        list,
        footer,
    }
}

pub fn targets(app: &App, content: Rect) -> Vec<Target> {
    let layout = layout(app, content);
    let indices = app.filtered_word_indices();
    let mut result = Vec::new();
    for (row, index) in indices.iter().enumerate() {
        let y = layout.list.y + row as u16;
        if y >= layout.list.bottom() {
            break;
        }
        // 勾选行点一下是取消勾选，其余行是移动光标。
        // 早期实现只比较 `words_marked.first()`，第二个及之后的勾选行点不动。
        let action = if app.words_marked.contains(index) {
            crate::app::Action::WordsToggleMark
        } else {
            // 光标在**过滤后**的位置，位移也要按过滤后的行号算。
            crate::app::Action::WordsMove(row as i32 - app.words_cursor as i32)
        };
        result.push(Target::new(
            Rect::new(layout.list.x, y, layout.list.width, 1),
            action,
        ));
    }
    result
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Dicts, |frame, content| {
        let layout = layout(app, content);
        render_header(app, frame, layout.header, theme);
        render_list(app, frame, layout.list, theme);
        render_footer(app, frame, layout.footer, theme);
    });
}

fn render_header(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let dict = app.boot.base.study_dict();
    let name = dict.map(|dict| dict.name.clone()).unwrap_or_default();
    let total = dict.map(|dict| dict.words.len()).unwrap_or(0);
    let matched = app.filtered_word_indices().len();
    let editable = if app.words_is_system() {
        "（系统词典）"
    } else {
        "（改动写回词库文件）"
    };

    let mut spans = vec![
        Span::styled(
            format!(" {name} "),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{matched}/{total} 词{editable}  排序：{}",
                app.words_sort.label()
            ),
            Style::default().fg(theme.muted),
        ),
        Span::styled("    搜索 ", Style::default().fg(theme.dim)),
        Span::styled(
            format!("{}_", app.words_filter),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(cursor) = app
        .words_marked
        .first()
        .copied()
        .filter(|_| !app.words_marked.is_empty())
    {
        let _ = cursor;
        spans.push(Span::styled(
            format!("    已勾选 {} 个", app.words_marked.len()),
            Style::default().fg(theme.accent_alt),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}

fn render_list(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.height == 0 {
        return;
    }
    let Some(dict) = app.boot.base.study_dict() else {
        return;
    };
    let indices = app.filtered_word_indices();
    if indices.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  没有匹配的词条",
                Style::default().fg(theme.dim),
            )),
            rect,
        );
        return;
    }
    let width = rect.width as usize;
    for (row, index) in indices.iter().enumerate() {
        let y = rect.y + row as u16;
        if y >= rect.bottom() {
            break;
        }
        let Some(word) = dict.words.get(*index) else {
            continue;
        };
        let selected = *index == indices.get(app.words_cursor).copied().unwrap_or(usize::MAX);
        let marked = app.words_marked.contains(index);
        let row_rect = Rect::new(rect.x, y, rect.width, 1);
        if selected {
            widgets::paint_rect(frame, row_rect, theme.selection);
            widgets::paint_rect(
                frame,
                Rect::new(rect.x, rect.y + y - rect.y, 1, 1),
                theme.accent,
            );
        }
        let marker = if marked { "◉" } else { " " };
        let translation = word
            .trans
            .iter()
            .map(|item| format!("{}{}", item.pos, item.cn))
            .collect::<Vec<_>>()
            .join("；");
        let note = app
            .boot
            .base
            .note_data
            .get(&word.word)
            .map(|note| format!("  笔记：{note}"))
            .unwrap_or_default();
        let text = format!(
            "{marker} {:>4}. {:<24} {translation}{note}",
            index + 1,
            word.word
        );
        let style = if selected {
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)
        } else if marked {
            Style::default().fg(theme.accent_alt)
        } else {
            Style::default().fg(theme.fg)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(fit_text(&text, width), style))),
            row_rect,
        );
    }
    let _ = display_width("");
}

fn render_footer(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let hints = if app.words_marked.is_empty() {
        " / 搜索 · a 新增 · d 删除 · o 排序 · c 复制词典 · n 新建词典"
    } else {
        " 空格勾选 · 1 认识 · 2 已掌握 · 3 不认识 · Esc 返回"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hints,
            Style::default().fg(theme.dim),
        ))),
        rect,
    );
}
