//! 快捷键编辑页：列出全部可配置动作与当前键位。
//!
//! layout 与 render 共用同一组条目行；targets 只读 layout。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{Action, App};
use crate::domain::shortcut::ShortcutAction;
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

#[derive(Clone, Debug)]
pub struct ShortcutsLayout {
    /// 列表上方的提示行
    pub hint: Rect,
    pub offset: usize,
    /// (动作序号, 矩形)
    pub rows: Vec<(usize, Rect)>,
}

pub fn layout(app: &App, content: Rect) -> ShortcutsLayout {
    let hint = Rect::new(content.x, content.y, content.width, 1);
    let list_area = Rect::new(
        content.x,
        content.y + 1,
        content.width,
        content.height.saturating_sub(1),
    );
    let visible = list_area.height as usize;
    let total = ShortcutAction::ALL.len();
    let offset = app
        .shortcut_cursor
        .saturating_sub(visible.saturating_sub(1));

    let rows = (offset..total.min(offset + visible))
        .map(|index| {
            (
                index,
                Rect::new(
                    list_area.x,
                    list_area.y + (index - offset) as u16,
                    list_area.width,
                    1,
                ),
            )
        })
        .collect();

    ShortcutsLayout { hint, offset, rows }
}

pub fn targets(app: &App, content: Rect) -> Vec<Target> {
    let layout = layout(app, content);
    layout
        .rows
        .iter()
        .map(|(index, rect)| {
            let action = if *index == app.shortcut_cursor {
                Action::ShortcutCapture
            } else {
                Action::ShortcutMove((*index as i32) - (app.shortcut_cursor as i32))
            };
            Target::new(*rect, action)
        })
        .collect()
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Settings, |frame, content| {
        let layout = layout(app, content);
        render_hint(app, frame, layout.hint, theme);
        for (index, rect) in layout.rows.iter() {
            let Some(action) = ShortcutAction::ALL.get(*index).copied() else {
                continue;
            };
            render_row(app, frame, *rect, action, theme);
        }
    });
}

fn render_hint(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let text = if app.shortcut_capturing {
        "  按下新的键位（Esc 取消）"
    } else {
        "  ↑↓ 选择 · Enter 改绑 · Delete 清除 · r 恢复默认 · R 全部恢复 · Esc 返回"
    };
    let style = Style::default().fg(if app.shortcut_capturing {
        theme.accent
    } else {
        theme.dim
    });
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
}

fn render_row(app: &App, frame: &mut Frame, rect: Rect, action: ShortcutAction, theme: &Theme) {
    if rect.width < 12 || rect.height == 0 {
        return;
    }
    let selected = app.shortcut_cursor
        == ShortcutAction::ALL
            .iter()
            .position(|a| *a == action)
            .unwrap_or(usize::MAX);
    if selected {
        widgets::paint_rect(frame, rect, theme.selection);
        widgets::paint_rect(
            frame,
            Rect::new(rect.x, rect.y, 1, rect.height),
            theme.accent,
        );
    }

    let inner_x = rect.x.saturating_add(2);
    let inner_width = rect.width.saturating_sub(4) as usize;
    if inner_width == 0 {
        return;
    }

    let mut label = action.label().to_string();
    if !action.is_available() {
        label.push_str("（暂不可用）");
    }
    let binding = app.shortcuts.key_for(action).to_upstream();

    let binding_width = display_width(&binding);
    let label = fit_text(&label, inner_width.saturating_sub(binding_width + 1));
    let label_width = display_width(&label);
    let gap = inner_width.saturating_sub(label_width + binding_width);

    let (label_color, binding_color) = if !action.is_available() {
        (theme.dim, theme.dim)
    } else if selected {
        (theme.fg, theme.accent_alt)
    } else {
        (theme.muted, theme.fg)
    };

    let label_style = if selected {
        Style::default()
            .fg(label_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(label_color)
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(label, label_style),
            Span::raw(" ".repeat(gap)),
            Span::styled(binding, Style::default().fg(binding_color)),
        ])),
        Rect::new(inner_x, rect.y, inner_width as u16, 1),
    );
}

/// 当前行的键位文本（供帮助页与提示复用）。
pub fn binding_text(app: &App, action: ShortcutAction) -> String {
    app.shortcuts.key_for(action).to_upstream()
}

/// 冲突提示色。
pub fn conflict_color(theme: &Theme) -> Color {
    theme.warn
}
