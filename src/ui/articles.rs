//! 文章背诵页：书本 / 文章列表，以及句子跟打练习。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::app::App;
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, fit_text};

/// 书本 / 文章列表页的布局。
pub struct ArticlesLayout {
    /// 左：书本列表
    pub books: Rect,
    /// 右：文章列表
    pub articles: Rect,
}

pub fn layout(content: Rect) -> ArticlesLayout {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Min(20)])
        .split(content);
    ArticlesLayout {
        books: columns[0],
        articles: columns[1],
    }
}

/// 书本 / 文章列表页的鼠标目标。
///
/// 书本列表每行一个目标（选书本），文章列表每行一个目标（选文章）。
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let layout = layout(content);
    let mut result = Vec::new();
    // List 组件的内容从边框内侧开始，每行一个条目。
    for (index, _) in app.article_books.iter().enumerate() {
        let y = layout.books.y + 1 + index as u16;
        if y >= layout.books.bottom().saturating_sub(1) {
            break;
        }
        result.push(Target::new(
            Rect::new(layout.books.x, y, layout.books.width, 1),
            crate::app::Action::ArticleBookMove(index as i32 - app.article_book_cursor as i32),
        ));
    }
    for (index, _) in app.article_list.iter().enumerate() {
        let y = layout.articles.y + 1 + index as u16;
        if y >= layout.articles.bottom().saturating_sub(1) {
            break;
        }
        result.push(Target::new(
            Rect::new(layout.articles.x, y, layout.articles.width, 1),
            crate::app::Action::ArticleMove(index as i32 - app.article_cursor as i32),
        ));
    }
    result
}

/// 书本 / 文章列表页。
pub fn render_articles(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Articles, |frame, content| {
        let layout = layout(content);
        let columns = [layout.books, layout.articles];

        // 左：书本列表
        let books: Vec<ListItem> = app
            .article_books
            .iter()
            .map(|book| {
                let selected = app.article_book_cursor
                    == app
                        .article_books
                        .iter()
                        .position(|item| item.en_name == book.en_name)
                        .unwrap_or(usize::MAX);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if selected { "▶ " } else { "  " },
                        Style::default().fg(if selected { theme.accent } else { theme.dim }),
                    ),
                    Span::styled(
                        book.name.clone(),
                        Style::default()
                            .fg(if selected { theme.fg } else { theme.muted })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(
                        format!("  {} 篇", book.length),
                        Style::default().fg(theme.dim),
                    ),
                ]))
            })
            .collect();
        frame.render_widget(
            List::new(books).block(practice_block(" 书本 ", theme)),
            columns[0],
        );

        // 右：文章列表
        let articles: Vec<ListItem> = app
            .article_list
            .iter()
            .enumerate()
            .map(|(index, article)| {
                let selected = index == app.article_cursor;
                let sentences: usize = article.sections.iter().map(|section| section.len()).sum();
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if selected { "▶ " } else { "  " },
                        Style::default().fg(if selected { theme.accent } else { theme.dim }),
                    ),
                    Span::styled(
                        article.title.clone(),
                        Style::default()
                            .fg(if selected { theme.fg } else { theme.muted })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(format!("  {sentences} 句"), Style::default().fg(theme.dim)),
                ]))
            })
            .collect();
        frame.render_widget(
            List::new(articles).block(practice_block(" 文章 ", theme)),
            columns[1],
        );
    });
}

/// 句子跟打练习页的鼠标目标。
///
/// 该页独占整屏，不画侧栏，因此没有导航目标——练习本身只能用键盘。
pub fn practice_targets(_app: &App, _area: Rect) -> Vec<crate::ui::target::Target> {
    Vec::new()
}

/// 句子跟打练习页。
pub fn render_article_practice(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    let Some(practice) = app.article_practice.as_ref() else {
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(area);

    let total = practice.sentences.len();
    let index = practice.index.min(total.saturating_sub(1));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", practice.title),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}/{} 句", index + 1, total),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!("   已完成 {} 句", practice.finished),
                Style::default().fg(theme.dim),
            ),
        ])),
        chunks[0],
    );

    let Some(sentence) = practice.sentences.get(index) else {
        return;
    };
    let expected: Vec<char> = sentence.text.chars().collect();
    let typed = practice.typing.typed.min(expected.len());
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (position, character) in expected.iter().enumerate() {
        let color = if position < typed {
            if practice.typing.wrong && position + 1 == typed {
                theme.bad
            } else {
                theme.good
            }
        } else if position == typed {
            theme.accent
        } else {
            theme.dim
        };
        spans.push(Span::styled(
            character.to_string(),
            Style::default().fg(color),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(practice_block(" 跟打 ", theme)),
        chunks[1],
    );

    let mut translate = vec![Line::from(Span::styled(
        "译文：",
        Style::default().fg(theme.muted),
    ))];
    translate.push(Line::from(Span::styled(
        sentence.translate.clone(),
        Style::default().fg(theme.fg),
    )));
    frame.render_widget(
        Paragraph::new(translate).block(practice_block(" 对照 ", theme)),
        chunks[2],
    );

    let hint = if practice.typing.wrong {
        " 有输错的字，按 Backspace 退格改正 · Ctrl+P 朗读 · Esc 返回"
    } else {
        " 直接照着输入整句 · Ctrl+P 朗读 · Esc 返回"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(if practice.typing.wrong {
                theme.warn
            } else {
                theme.dim
            }),
        ))),
        chunks[3],
    );
    let _ = fit_text("", 0);
    let _ = widgets::display_width("");
}

fn practice_block(title: &str, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.panel))
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(theme.dim),
        ))
}
