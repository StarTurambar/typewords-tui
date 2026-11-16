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
    /// 流程列表的滚动偏移（跟随 `flow_cursor`）。
    pub offset: usize,
    pub detail: Rect,
    pub footer: Rect,
}

pub fn layout(app: &App, content: Rect) -> FlowsLayout {
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
    let list = columns[0];
    let offset = widgets::list_offset(
        app.flow_cursor,
        app.flow_entry_count(),
        list.height as usize,
    );
    FlowsLayout {
        header,
        list,
        offset,
        detail: columns[1],
        footer: Rect::new(
            content.x,
            content.bottom().saturating_sub(1),
            content.width,
            1,
        ),
    }
}

/// 流程页的鼠标目标：流程列表每行一个。
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let layout = layout(app, content);
    let entries = app.flow_entries();
    let mut result = Vec::new();
    for (index, _) in entries.iter().enumerate().skip(layout.offset) {
        let y = layout.list.y + (index - layout.offset) as u16;
        if y >= layout.list.bottom() {
            break;
        }
        // 让出最右一列：滚动条画在那里，点它不该选中某个流程。
        result.push(Target::new(
            Rect::new(layout.list.x, y, layout.list.width.saturating_sub(1), 1),
            crate::app::Action::FlowMove(index as i32 - app.flow_cursor as i32),
        ));
    }
    result
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Home, |frame, content| {
        let layout = layout(app, content);
        render_header(app, frame, layout.header, theme);
        render_list(app, frame, layout.list, layout.offset, theme);
        render_detail(app, frame, layout.detail, theme);
        render_footer(app, frame, layout.footer, theme);
    });
}

fn render_header(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    // 「当前」与列表的 ● 只反映「下一轮练习实际会跑的流程」，因此与顶栏
    // 「模式」永远一致。已启用、但模式还没切到「自定义流程」的自建流程在
    // 这里补一句说明，不让它抢 ●。
    let entries = app.flow_entries();
    let active = entries.iter().find(|entry| entry.active);
    let mode = app.boot.settings.word_practice_mode;
    let custom_mode = mode == crate::domain::types::WordPracticeMode::Custom;
    let words_test = crate::domain::constants::is_words_test_mode(mode);
    let mut spans = vec![Span::styled(
        " 练习流程 ",
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    match active {
        Some(entry) => spans.push(Span::styled(
            format!("  当前：{}", entry.name),
            Style::default().fg(theme.muted),
        )),
        // 单词测试走 4 选 1 答题，任何流程都不会执行（`start_practice_with_mode`
        // 直接分流到 `start_word_test`）；这里明说，而不是让
        // `get_flow_id_for_mode` 的 `system` 兜底冒充「当前流程」。
        None if words_test => spans.push(Span::styled(
            format!(
                "  当前：{}（4 选 1 答题，不经过流程编排）",
                crate::domain::constants::practice_mode_name(mode)
            ),
            Style::default().fg(theme.muted),
        )),
        None => spans.push(Span::styled(
            "  当前：—（自定义流程模式尚未启用自建流程）",
            Style::default().fg(theme.warn),
        )),
    }
    if let Some(entry) = entries.iter().find(|entry| entry.enabled && !entry.active) {
        let hint = if custom_mode {
            format!(" · 「{}」的配置不可用，已回退到内置流程", entry.name)
        } else {
            format!(" · 已启用「{}」，切到「自定义流程」后生效", entry.name)
        };
        spans.push(Span::styled(hint, Style::default().fg(theme.dim)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}

fn render_list(app: &App, frame: &mut Frame, rect: Rect, offset: usize, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let entries = app.flow_entries();
    let row_width = rect.width.saturating_sub(1);
    for (index, entry) in entries.iter().enumerate().skip(offset) {
        let y = rect.y + (index - offset) as u16;
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
        // 「已启用」只是 flows.json 里的启用位：模式不是「自定义流程」时它
        // 不会生效，所以用文字标签说明，不占用只表示「实际生效」的 ●。
        let label = if entry.enabled && !entry.active {
            format!("{} （已启用）", entry.name)
        } else {
            entry.name.clone()
        };
        let row = Rect::new(rect.x, y, row_width, 1);
        if selected {
            widgets::paint_rect(frame, row, theme.selection);
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fit_text(&format!("{marker} {label}"), row_width as usize),
                style,
            ))),
            row,
        );
    }
    widgets::scrollbar(
        frame,
        Rect::new(rect.right().saturating_sub(1), rect.y, 1, rect.height),
        widgets::ScrollbarState::new(offset, entries.len(), rect.height as usize),
        theme,
    );
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
        " ] 换节点 · h/l 换步骤 · a 加步骤 · d 删步骤 · s 乱序 · g 组内巩固 · Esc 返回"
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
