//! 单词测试页：一题 4 选 1。
//!
//! 对齐上游 `app/pages/(words)/words-test/[id].vue`：题面给出四个中文释义选项，
//! 答错时把正确答案放进错词本，全部答完给结算。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, WordTestState};
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets::fit_text;

/// 单词测试页布局。
pub struct WordTestLayout {
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
    /// 每个选项一行，与 `question.candidates` 的顺序一致。
    pub option_rows: Vec<Rect>,
}

pub fn layout(area: Rect, options: usize) -> WordTestLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);
    // 每个选项占两行（选项文字 + 间隔）
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            (0..options)
                .map(|_| Constraint::Length(2))
                .collect::<Vec<_>>(),
        )
        .split(chunks[1]);
    WordTestLayout {
        header: chunks[0],
        body: chunks[1],
        footer: chunks[2],
        option_rows: rows.to_vec(),
    }
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    let Some(state) = app.word_test.as_ref() else {
        return;
    };
    shell::render(app, frame, area, theme, Nav::Home, |frame, content| {
        let options = state
            .question
            .as_ref()
            .map(|question| question.candidates.len())
            .unwrap_or(0);
        let layout = layout(content, options);
        render_header(state, frame, layout.header, theme);
        render_options(state, frame, layout.body, &layout.option_rows, theme);
        render_footer(state, frame, layout.footer, theme);
    });
}

/// 单词测试页的鼠标目标：四个选项各一个。
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let Some(state) = app.word_test.as_ref() else {
        return Vec::new();
    };
    let options = state
        .question
        .as_ref()
        .map(|question| question.candidates.len())
        .unwrap_or(0);
    let layout = layout(content, options);
    layout
        .option_rows
        .iter()
        .enumerate()
        .map(|(index, rect)| Target::new(*rect, crate::app::Action::WordTestChoose(index)))
        .collect()
}

fn render_header(state: &WordTestState, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let total = state.words.len();
    let index = state.index + 1;
    let spans = vec![
        Span::styled(
            " 单词测试 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{index}/{total}"), Style::default().fg(theme.fg)),
        Span::styled("   ·   ", Style::default().fg(theme.dim)),
        Span::styled("对 ", Style::default().fg(theme.muted)),
        Span::styled(state.correct.to_string(), Style::default().fg(theme.good)),
        Span::styled(" 错 ", Style::default().fg(theme.muted)),
        Span::styled(state.wrong.to_string(), Style::default().fg(theme.bad)),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}

fn render_options(
    state: &WordTestState,
    frame: &mut Frame,
    rect: Rect,
    option_rows: &[Rect],
    theme: &Theme,
) {
    let Some(question) = state.question.as_ref() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " 题目生成失败，换一个词库试试",
                Style::default().fg(theme.warn),
            ))),
            rect,
        );
        return;
    };
    let answered = state.selected.is_some();

    for (index, candidate) in question.candidates.iter().enumerate() {
        let Some(row) = option_rows.get(index).copied() else {
            break;
        };
        if row.height == 0 {
            break;
        }
        let marker = ["A", "B", "C", "D"][index];
        let translation = candidate
            .word
            .trans
            .iter()
            .take(2)
            .map(|item| format!("{}{}", item.pos, item.cn))
            .collect::<Vec<_>>()
            .join("；");

        let selected = state.selected == Some(index);
        let correct = index == question.correct_index;
        let (color, bold) = if answered && correct {
            (theme.good, true)
        } else if answered && selected {
            (theme.bad, true)
        } else if selected {
            (theme.accent, true)
        } else {
            (theme.fg, false)
        };
        let mut style = Style::default().fg(color);
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        let mut text = format!("{marker}   {translation}");
        // 答完后显示单词：正确答案始终显示，选中的错误项也显示
        if answered && (correct || selected) {
            text.push_str(&format!("   {}", candidate.word.word));
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fit_text(&text, row.width as usize),
                style,
            ))),
            row,
        );
    }
}

fn render_footer(state: &WordTestState, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let text = if state.selected.is_some() {
        " 空格 下一题 · Esc 退出"
    } else {
        " 1 / 2 / 3 / 4 选择答案 · Esc 退出"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(theme.dim),
        ))),
        Rect::new(rect.x, rect.y, rect.width, 1),
    );
    let tip = "答错的单词会自动进入错词本";
    if rect.height >= 2 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                tip,
                Style::default().fg(theme.dim),
            ))),
            Rect::new(rect.x, rect.y + 1, rect.width, 1),
        );
    }
}
