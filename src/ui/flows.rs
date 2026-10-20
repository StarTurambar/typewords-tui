//! 自定义练习流程页：流程列表与步骤编辑。
//!
//! 对齐上游 `practice-flow-runtime.ts` 的 `PracticeFlowV2`：用户可以基于内置
//! 流程新建副本，调整节点里的步骤顺序，保存后成为当前练习使用的流程。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::domain::flow::{PracticeStepTemplateId, PracticeWordsSource};
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, fit_text};

/// 流程页布局。
pub struct FlowsLayout {
    pub header: Rect,
    pub list: Rect,
    pub detail: Rect,
    pub footer: Rect,
}

pub fn layout(_app: &App, content: Rect) -> FlowsLayout {
    let header = Rect::new(content.x, content.y, content.width, content.height.min(1));
    let columns = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(42),
            ratatui::layout::Constraint::Min(24),
        ])
        .split(Rect::new(
            content.x,
            header.bottom(),
            content.width,
            content.height.saturating_sub(2),
        ));
    FlowsLayout {
        header,
        list: columns[0],
        detail: columns[1],
        footer: Rect::new(
            content.x,
            content.bottom().saturating_sub(1),
            content.width,
            1,
        ),
    }
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Home, |frame, content| {
        let layout = layout(app, content);
        render_header(app, frame, layout.header, theme);
        render_list(app, frame, layout.list, theme);
        render_detail(app, frame, layout.detail, theme);
        render_footer(app, frame, layout.footer, theme);
    });
}

fn render_header(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let active = app
        .flow_store
        .active()
        .map(|flow| flow.name.clone())
        .unwrap_or_else(|| "内置流程".to_string());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " 练习流程 ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  当前：{active}"),
                Style::default().fg(theme.muted),
            ),
        ])),
        rect,
    );
}

fn render_list(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let entries = app.flow_entries();
    for (index, entry) in entries.iter().enumerate() {
        let y = rect.y + index as u16;
        if y >= rect.bottom() {
            break;
        }
        let selected = index == app.flow_cursor;
        let active = entry.active;
        let marker = if active {
            "●"
        } else if selected {
            "▶"
        } else {
            " "
        };
        let color = if active {
            theme.good
        } else if selected {
            theme.accent
        } else {
            theme.muted
        };
        let style = if selected {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        let row = Rect::new(rect.x, y, rect.width, 1);
        if selected {
            widgets::paint_rect(frame, row, theme.selection);
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fit_text(&format!("{marker} {}", entry.name), rect.width as usize),
                style,
            ))),
            row,
        );
    }
}

fn render_detail(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let Some(entry) = app.flow_entries().get(app.flow_cursor).cloned() else {
        return;
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" {} ", entry.name),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));
    for (node_index, node) in entry.config.nodes.iter().enumerate() {
        let node_selected = app.flow_editing && node_index == app.flow_node_cursor;
        let marker = if node_selected { "▶" } else { " " };
        lines.push(Line::from(Span::styled(
            format!(
                "{marker} {}（{}）",
                crate::domain::i18n::tr(&node.label),
                source_label(node.source)
            ),
            Style::default().fg(if node_selected {
                theme.accent
            } else {
                theme.muted
            }),
        )));
        for (step_index, step) in node.steps.iter().enumerate() {
            let step_selected = node_selected && step_index == app.flow_step_cursor;
            let step_marker = if step_selected { "·" } else { " " };
            let mut text = format!(
                "  {step_marker} {}",
                crate::domain::i18n::tr(template_label(step.template_id))
            );
            if matches!(
                step.word_advance,
                crate::domain::flow::WordAdvance::WordLoop { .. }
            ) {
                text.push_str("  〔组内巩固〕");
            }
            if step.shuffle_on_enter {
                text.push_str("  〔乱序〕");
            }
            lines.push(Line::from(Span::styled(
                text,
                Style::default().fg(if step_selected { theme.fg } else { theme.dim }),
            )));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(block(" 结构 ", theme)), rect);
}

fn render_footer(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let text = if app.flow_editing {
        " Tab 换节点 · a 加步骤 · d 删步骤 · s 乱序 · g 组内巩固 · Esc 返回"
    } else {
        " ↑↓ 选择 · Enter 启用 · n 新建副本 · e 编辑 · Delete 删除 · Esc 返回"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(theme.dim),
        ))),
        rect,
    );
}

fn block(title: &str, theme: &Theme) -> ratatui::widgets::Block<'static> {
    ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(theme.dim),
        ))
}

/// 步骤模板的中文名。
pub fn template_label(template: PracticeStepTemplateId) -> &'static str {
    template.label()
}

/// 词表来源的中文名。
pub fn source_label(source: PracticeWordsSource) -> &'static str {
    match source {
        PracticeWordsSource::TaskNew => "今日新词",
        PracticeWordsSource::TaskReview => "今日复习",
        PracticeWordsSource::Current => "当前词表",
        PracticeWordsSource::WrongWords => "错词",
    }
}
