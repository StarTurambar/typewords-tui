//! 文章背诵页：书本 / 文章列表，以及「整篇单页 + 逐句跟打」的练习页。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};
use unicode_width::UnicodeWidthChar;

use crate::app::{App, ArticlePracticeState};
use crate::domain::article::TypingCell;
use crate::domain::timer::format_duration;
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets;

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
                    saved_line(app, article.id, theme),
                ]))
            })
            .collect();
        frame.render_widget(
            List::new(articles).block(practice_block(" 文章 ", theme)),
            columns[1],
        );
    });
}

/// 列表行里「上次跟到这里」的标记：只有当前断点正好属于这一篇文章时才显示。
///
/// 续打是显式操作：看到标记后选中并按 Enter 才会从断点句子继续，
/// 而不是启动时自动跳进跟打页。
fn saved_line(app: &App, article_id: i64, theme: &Theme) -> Span<'static> {
    let Some(cache) = app
        .article_resume
        .as_ref()
        .filter(|cache| cache.article_id == article_id)
    else {
        return Span::raw("");
    };
    Span::styled(
        format!("  · 上次第 {} 句", cache.sentence_index + 1),
        Style::default().fg(theme.accent_alt),
    )
}

/// 跟打页的鼠标目标。
///
/// 该页独占整屏，不画侧栏，因此没有导航目标——练习本身只能用键盘。
pub fn practice_targets(_app: &App, _area: Rect) -> Vec<crate::ui::target::Target> {
    Vec::new()
}

/// 跟打页译文区的高度。
const TRANSLATE_HEIGHT: u16 = 4;

/// 折行后的一行：字符、所属句子下标、是否该句的首行。
struct ArticleLine {
    cells: Vec<TypingCell>,
    sentence: usize,
    first_of_sentence: bool,
}

/// 把整篇文章按句子折行成「同一页」的行列表。
fn article_lines(practice: &ArticlePracticeState, width: usize) -> Vec<ArticleLine> {
    let width = width.max(1);
    let mut lines: Vec<ArticleLine> = Vec::new();
    for (index, sentence) in practice.sentences.iter().enumerate() {
        // 段落之间空一行，保留原文的分段结构。
        if index > 0 && practice.section_starts.contains(&index) {
            lines.push(ArticleLine {
                cells: Vec::new(),
                sentence: index,
                first_of_sentence: false,
            });
        }
        let mut current: Vec<TypingCell> = Vec::new();
        let mut used = 0usize;
        let mut first = true;
        for cell in sentence.typing_cells() {
            let char_width = UnicodeWidthChar::width(cell.ch).unwrap_or(0);
            if used > 0 && used + char_width > width {
                lines.push(ArticleLine {
                    cells: std::mem::take(&mut current),
                    sentence: index,
                    first_of_sentence: first,
                });
                first = false;
                used = 0;
                // 折行处的空格丢掉，避免下一行以空格开头。
                if cell.ch == ' ' {
                    continue;
                }
            }
            used += char_width;
            current.push(cell);
        }
        lines.push(ArticleLine {
            cells: current,
            sentence: index,
            first_of_sentence: first,
        });
    }
    lines
}

/// 当前光标所在的行号（整篇折行结果里的下标）。
fn cursor_line(lines: &[ArticleLine], practice: &ArticlePracticeState) -> usize {
    let cursor = practice.typing.cursor();
    let mut fallback = 0usize;
    for (position, line) in lines.iter().enumerate() {
        if line.sentence != practice.index {
            continue;
        }
        fallback = position;
        if line.cells.iter().any(|cell| cell.target == Some(cursor)) {
            return position;
        }
    }
    fallback
}

/// 句子跟打页：整篇文章在同一页显示，但仍一句一句地跟打。
///
/// 纵向视口自动跟随当前光标（渲染期计算滚动位置，不写状态）；已经打完的
/// 句子用 `muted`、未来句用 `fg`，当前句的目标字符（字母、数字与词间
/// 空格）按「绿 = 打对了 / 红 = 打错了（显示实际按下的字符）/ selection 底
/// = 光标」着色，标点与多余空白用 `dim` 表示不用输入。
pub fn render_article_practice(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    let Some(practice) = app.article_practice.as_ref() else {
        // 防御：正常路径不会进入这个组合（跟打页只在状态存在时进入）；
        // 万一将来状态错位，也留一句可操作的提示，而不是整屏只剩背景色。
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "没有进行中的文章练习",
                Style::default().fg(theme.muted),
            )),
            Line::from(Span::styled(
                "Esc 返回文章列表",
                Style::default().fg(theme.dim),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(TRANSLATE_HEIGHT),
            Constraint::Length(2),
        ])
        .split(area);

    let total = practice.sentences.len();
    let index = practice.index.min(total.saturating_sub(1));

    // 顶部状态行
    let mut header: Vec<Span<'static>> = vec![Span::styled(
        format!(" {} ", practice.title),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    header.push(Span::styled(
        format!("  {}/{} 句", index + 1, total.max(1)),
        Style::default().fg(theme.muted),
    ));
    header.push(Span::styled(
        format!("   已完成 {}", practice.finished),
        Style::default().fg(theme.dim),
    ));
    // 当前句一出现错误就计入显示，不必等整句打完——红色标记当场就有反馈。
    let wrong_sentences = practice.wrong_sentences + usize::from(practice.typing.wrong);
    header.push(Span::styled(
        format!("   错误 {wrong_sentences}"),
        Style::default().fg(if wrong_sentences == 0 {
            theme.dim
        } else {
            theme.bad
        }),
    ));
    header.push(Span::styled(
        format!("   用时 {}", format_duration(practice.spend)),
        Style::default().fg(theme.dim),
    ));
    frame.render_widget(Paragraph::new(Line::from(header)), chunks[0]);

    // 正文：整篇文章一页显示，视口跟着光标走
    let inner_width = chunks[1].width.saturating_sub(2) as usize;
    let viewport = chunks[1].height.saturating_sub(2) as usize;
    let lines = article_lines(practice, inner_width.saturating_sub(2));
    let offset = cursor_line(&lines, practice)
        .saturating_sub(1)
        .min(lines.len().saturating_sub(viewport));
    let current_target: Vec<char> = practice
        .sentences
        .get(index)
        .map(|sentence| sentence.typing_target())
        .unwrap_or_default();
    let ignore_case = app.boot.settings.ignore_case;
    let rendered: Vec<Line<'static>> = lines
        .iter()
        .skip(offset)
        .take(viewport)
        .map(|line| article_line(line, practice, &current_target, ignore_case, theme))
        .collect();
    frame.render_widget(
        Paragraph::new(rendered).block(practice_block(" 跟打 ", theme)),
        chunks[1],
    );

    // 当前句译文（整篇文章同页时，译文只跟随当前句）
    let translate = practice
        .sentences
        .get(index)
        .map(|sentence| sentence.translate.clone())
        .unwrap_or_default();
    let translate_width = chunks[2].width.saturating_sub(3) as usize;
    let mut translate_lines: Vec<Line<'static>> = Vec::new();
    if translate.trim().is_empty() {
        translate_lines.push(Line::from(Span::styled(
            " （这句没有译文）",
            Style::default().fg(theme.dim),
        )));
    } else {
        for text in widgets::wrap_text(translate.trim(), translate_width)
            .into_iter()
            .take(2)
        {
            translate_lines.push(Line::from(Span::styled(
                format!(" {text}"),
                Style::default().fg(theme.fg),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(translate_lines).block(practice_block(" 当前句译文 ", theme)),
        chunks[2],
    );

    // 底部提示
    let (hint, color) = if practice.typing.wrong {
        (
            " 红色 = 打错的字符，按 Backspace 退格改正 · 打完一整句自动进入下一句",
            theme.warn,
        )
    } else {
        (
            " 字母、数字与词间空格照打 · 标点不用打 · Backspace 退格",
            theme.dim,
        )
    };
    let hints = vec![
        Line::from(Span::styled(hint, Style::default().fg(color))),
        Line::from(Span::styled(
            " Ctrl+P 朗读当前句 · Esc 返回文章列表",
            Style::default().fg(theme.dim),
        )),
    ];
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

/// 渲染正文的一行：行首 gutter + 按跟打进度逐字符着色。
fn article_line(
    line: &ArticleLine,
    practice: &ArticlePracticeState,
    target: &[char],
    ignore_case: bool,
    theme: &Theme,
) -> Line<'static> {
    if line.cells.is_empty() {
        return Line::from("");
    }
    let marker = if line.first_of_sentence && line.sentence == practice.index {
        Span::styled(
            "▶ ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  ")
    };
    let mut spans: Vec<Span<'static>> = vec![marker];
    let cursor = practice.typing.cursor();
    for cell in &line.cells {
        let (text, style) = match cell.target {
            // 标点 / 符号 / 多余空白：当前句变暗表示不用输入，其它句子跟随正文颜色。
            None => {
                let color = match line.sentence.cmp(&practice.index) {
                    std::cmp::Ordering::Less => theme.muted,
                    std::cmp::Ordering::Equal => theme.dim,
                    std::cmp::Ordering::Greater => theme.fg,
                };
                (cell.ch.to_string(), Style::default().fg(color))
            }
            // 已完成的句子只显示原文，不保留当时的错字。
            _ if line.sentence < practice.index => {
                (cell.ch.to_string(), Style::default().fg(theme.muted))
            }
            Some(position) if line.sentence == practice.index => {
                if position < cursor {
                    let typed = practice.typing.typed_char(position).unwrap_or(cell.ch);
                    let color = if practice.typing.is_mismatch(position, target, ignore_case) {
                        theme.bad
                    } else {
                        theme.good
                    };
                    (typed.to_string(), Style::default().fg(color))
                } else if position == cursor {
                    (
                        cell.ch.to_string(),
                        Style::default()
                            .fg(theme.fg)
                            .bg(theme.selection)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    (cell.ch.to_string(), Style::default().fg(theme.fg))
                }
            }
            Some(_) => (cell.ch.to_string(), Style::default().fg(theme.fg)),
        };
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
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
