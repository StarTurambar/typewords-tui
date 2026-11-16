//! 文章背诵页：书本 / 文章列表，以及「整篇单页 + 逐句跟打」的练习页。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthChar;

use crate::app::buttons::ARTICLE_BUTTONS;
use crate::app::{App, ArticlePracticeState};
use crate::domain::article::TypingCell;
use crate::domain::timer::format_duration;
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::typing_buttons::{self, ButtonBar};
use crate::ui::widgets;

/// 书本 / 文章列表页的布局。
pub struct ArticlesLayout {
    /// 左：书本列表（带边框的整块面板 + 滚动信息）
    pub books_list: ListPane,
    /// 右：文章列表（带边框的整块面板 + 滚动信息）
    pub articles_list: ListPane,
}

/// 一栏带边框列表的几何：面板、行内容区、滚动条列与滚动偏移。
///
/// 行内容区与滚动条都让开边框；绘制、命中与焦点高亮都读这一份几何，
/// 行不会钻到滚动条下面，也不会和边框重叠。
pub struct ListPane {
    /// 带边框的整块面板
    pub rect: Rect,
    /// 行内容区（边框内侧、再让出最右 1 列滚动条）
    pub list: Rect,
    /// 滚动条所在的 1 列（边框内侧最右一列）
    pub scrollbar: Rect,
    /// 滚动偏移（行）
    pub offset: usize,
    /// 内容总行数
    pub total: usize,
    /// 可见行数
    pub visible: usize,
}

/// 左右两栏的矩形（只看终端尺寸，不看 App）。
fn columns(content: Rect) -> (Rect, Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Min(20)])
        .split(content);
    (columns[0], columns[1])
}

fn list_pane(rect: Rect, total: usize, cursor: usize) -> ListPane {
    let inner = Rect::new(
        rect.x.saturating_add(1),
        rect.y.saturating_add(1),
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    );
    let list = Rect::new(
        inner.x,
        inner.y,
        inner.width.saturating_sub(1),
        inner.height,
    );
    let scrollbar = Rect::new(inner.right().saturating_sub(1), inner.y, 1, inner.height);
    let visible = list.height as usize;
    ListPane {
        rect,
        list,
        scrollbar,
        offset: widgets::list_offset(cursor, total, visible),
        total,
        visible,
    }
}

pub fn layout(app: &App, content: Rect) -> ArticlesLayout {
    let (books, articles) = columns(content);
    ArticlesLayout {
        books_list: list_pane(books, app.article_books.len(), app.article_book_cursor),
        articles_list: list_pane(articles, app.article_list.len(), app.article_cursor),
    }
}

/// 书本 / 文章列表页的鼠标目标。
///
/// 每行一个目标（左栏选书本、右栏选文章）；行矩形来自 `ListPane::list`，
/// 与滚动偏移、滚动条同源，滚动后仍然画在哪就能点在哪。
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let layout = layout(app, content);
    let mut result = Vec::new();
    let books = &layout.books_list;
    for (index, _) in app.article_books.iter().enumerate().skip(books.offset) {
        let y = books.list.y + (index - books.offset) as u16;
        if y >= books.list.bottom() {
            break;
        }
        result.push(Target::new(
            Rect::new(books.list.x, y, books.list.width, 1),
            crate::app::Action::ArticleBookMove(index as i32 - app.article_book_cursor as i32),
        ));
    }
    let articles = &layout.articles_list;
    for (index, _) in app.article_list.iter().enumerate().skip(articles.offset) {
        let y = articles.list.y + (index - articles.offset) as u16;
        if y >= articles.list.bottom() {
            break;
        }
        result.push(Target::new(
            Rect::new(articles.list.x, y, articles.list.width, 1),
            crate::app::Action::ArticleMove(index as i32 - app.article_cursor as i32),
        ));
    }
    result
}

/// 文章页的滚轮：指针落在哪一栏就滚哪一栏。
///
/// 左栏（书本）的滚动就是「换书」——换书后文章光标随新书回到第一篇；右栏
/// （文章）只移动文章光标，绝不碰书本。两栏之外（导航栏 / 顶栏 / 底栏）返回
/// `None`。早期实现不看指针位置、一律发 `ArticleBookMove`：在右侧文章列表
/// 上滚一格，整本书被换掉，文章光标也被重置。
pub fn wheel_action(
    content: Rect,
    column: u16,
    row: u16,
    delta: i32,
) -> Option<crate::app::Action> {
    let (books, articles) = columns(content);
    if inside(books, column, row) {
        Some(crate::app::Action::ArticleBookMove(delta))
    } else if inside(articles, column, row) {
        Some(crate::app::Action::ArticleMove(delta))
    } else {
        None
    }
}

/// 终端坐标是否落在矩形里。
///
/// 与 `Target::contains` 同一套开区间语义（右边 / 下边不含），命中区不会因为
/// 边界上的一个格子互相重叠。
fn inside(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.right() && row >= rect.y && row < rect.bottom()
}

/// 书本 / 文章列表页。
pub fn render_articles(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Articles, |frame, content| {
        let layout = layout(app, content);

        // 左：书本列表
        let books: Vec<Line<'static>> = app
            .article_books
            .iter()
            .map(|book| {
                let selected = app.article_book_cursor
                    == app
                        .article_books
                        .iter()
                        .position(|item| item.en_name == book.en_name)
                        .unwrap_or(usize::MAX);
                Line::from(vec![
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
                    // 篇数是 `load_books` 从文章文件里数出来的实际篇数，不采信
                    // 目录里的登记值：写「N 篇」就必须能列出 N 条，两个数字来自
                    // 同一份资源（详见 `domain::article::load_books`）。文件读不到
                    // 时篇数未知，如实显示，不拿没核对过的登记值充数。
                    match book.length {
                        Some(length) => {
                            Span::styled(format!("  {length} 篇"), Style::default().fg(theme.dim))
                        }
                        None => Span::styled("  篇数未知", Style::default().fg(theme.dim)),
                    },
                ])
            })
            .collect();
        render_pane(frame, &layout.books_list, " 书本 ", books, theme);

        // 右：文章列表
        let articles: Vec<Line<'static>> = app
            .article_list
            .iter()
            .enumerate()
            .map(|(index, article)| {
                let selected = index == app.article_cursor;
                let sentences: usize = article.sections.iter().map(|section| section.len()).sum();
                Line::from(vec![
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
                    saved_line(app, article.id, sentences, theme),
                ])
            })
            .collect();
        render_pane(frame, &layout.articles_list, " 文章 ", articles, theme);
    });
}

/// 画一栏带边框列表：边框 + 从滚动偏移开始的可见行 + 右侧滚动条。
///
/// 行按值传入：只把可见的那一段 move 进一个 `Paragraph`，避免逐行 clone
/// 已构造好的 `Line`，也避免每行一个 widget。
fn render_pane(
    frame: &mut Frame,
    pane: &ListPane,
    title: &str,
    lines: Vec<Line<'static>>,
    theme: &Theme,
) {
    if pane.rect.width == 0 || pane.rect.height == 0 {
        return;
    }
    frame.render_widget(practice_block(title, theme), pane.rect);
    if pane.list.width > 0 && pane.list.height > 0 {
        let visible: Vec<Line<'static>> = lines
            .into_iter()
            .skip(pane.offset)
            .take(pane.visible)
            .collect();
        if !visible.is_empty() {
            frame.render_widget(Paragraph::new(visible), pane.list);
        }
    }
    widgets::scrollbar(
        frame,
        pane.scrollbar,
        widgets::ScrollbarState::new(pane.offset, pane.total, pane.visible),
        theme,
    );
}

/// 列表行里「上次跟到这里」的标记：只有当前断点正好属于这一篇文章时才显示。
///
/// 续打是显式操作：看到标记后选中并按 Enter 才会从断点句子继续，
/// 而不是启动时自动跳进跟打页。
///
/// 存档里的 `sentence_index` 是保存时那篇文章的下标，资源更新后文章可能变短，
/// 旧断点会指到已经不存在的句子。这里用本行已经算出的 `sentences`（当前文章
/// 的实际句数）校验：超出时 clamp 到最后一句，与 `start_article_practice`
/// 续打时把下标 clamp 到当前句数内的行为一致——标记上的句子必须按当前句数
/// 夹取，不能把存档里的旧数字原样搬上屏幕（同书本行「72 篇」那一类
/// 「显示存档数字、不与实际数据对照」的缺陷）。文章连一句都没有时
/// 无可续打的内容，不显示标记。
fn saved_line(app: &App, article_id: i64, sentences: usize, theme: &Theme) -> Span<'static> {
    let Some(cache) = app
        .article_resume
        .as_ref()
        .filter(|cache| cache.article_id == article_id)
    else {
        return Span::raw("");
    };
    // 句数来自当前文章文件；0 句时没有任何可续打的句子。
    let Some(last) = sentences.checked_sub(1) else {
        return Span::raw("");
    };
    let index = cache.sentence_index.min(last);
    Span::styled(
        format!("  · 上次第 {} 句", index + 1),
        Style::default().fg(theme.accent_alt),
    )
}

/// 跟打页译文区的高度。
const TRANSLATE_HEIGHT: u16 = 4;

/// 跟打页底部的高度：规则一行 + 按钮条四行（两行按钮 + 留白 + 提示）。
const HINT_HEIGHT: u16 = 5;

/// 跟打页布局：标题行 / 正文 / 译文 / 底部规则 + 动作按钮。
///
/// 绘制与鼠标命中共用这一份矩形，按钮条不会和画出来的位置漂移。
pub struct PracticeLayout {
    pub header: Rect,
    pub body: Rect,
    /// 当前句译文区（关掉译文时为 `None`，这块高度还给正文）
    pub translate: Option<Rect>,
    /// 底部规则行（当前输入规则 / 打错提示）
    pub rule: Rect,
    /// 底部动作按钮条
    pub button_bar: ButtonBar,
}

pub fn practice_layout(practice: &ArticlePracticeState, area: Rect) -> PracticeLayout {
    let translate_height = if practice.show_translate {
        TRANSLATE_HEIGHT
    } else {
        0
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(translate_height),
            Constraint::Length(HINT_HEIGHT),
        ])
        .split(area);
    let footer = chunks[3];
    PracticeLayout {
        header: chunks[0],
        body: chunks[1],
        translate: (translate_height > 0).then(|| chunks[2]),
        rule: Rect::new(footer.x, footer.y, footer.width, 1),
        button_bar: typing_buttons::layout(
            Rect::new(footer.x, footer.y + 1, footer.width, typing_buttons::HEIGHT),
            ARTICLE_BUTTONS.len(),
        ),
    }
}

/// 跟打页的鼠标目标：底部动作按钮。
///
/// 该页独占整屏、不画侧栏，因此没有导航目标；点击按钮与键盘 `Enter` 走同一个
/// `Action::TypingButton`，点哪个按钮就执行哪个。
pub fn practice_targets(app: &App, area: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let Some(practice) = app.article_practice.as_ref() else {
        return Vec::new();
    };
    let layout = practice_layout(practice, area);
    layout
        .button_bar
        .buttons
        .iter()
        .enumerate()
        .map(|(index, rect)| Target::new(*rect, crate::app::Action::TypingButton(index)))
        .collect()
}

/// 折行后的一行：字符、所属句子下标、是否该句的首行。
struct ArticleLine {
    cells: Vec<TypingCell>,
    sentence: usize,
    first_of_sentence: bool,
}

/// 把整篇文章按句子折行成「同一页」的行列表。
///
/// 折行恰好落在词间空格上时，这个空格直接丢掉，避免下一行以空格开头；
/// 空格可以省略且总按正确输入显示（见 `SentenceTyping::input`），折掉不会
/// 藏起错误标记——错误只可能落在字母 / 数字位置上。
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
                // 折行处的空白直接丢掉，避免下一行以空格开头。
                if cell.ch.is_whitespace() {
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
/// = 光标」着色，标点与多余空白用 `dim` 表示不用输入。词间空格可以省略：
/// 省略时它仍按正确输入显示，错误只会出现在字母 / 数字位置（红色显示实际
/// 按下的字符），不会落在空格上。
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
    // 译文区跟着 `Ctrl+Z`（与练习页「切换翻译」同一个键）走：
    // 关掉时把它占的行数还给正文，而不是留一块空面板。
    let layout = practice_layout(practice, area);

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
    // 暂停中的提示与练习页状态行的「已暂停」同义（键位也是同一个 `Ctrl+,`）。
    if practice.paused {
        header.push(Span::styled("   已暂停", Style::default().fg(theme.warn)));
    }
    frame.render_widget(Paragraph::new(Line::from(header)), layout.header);

    // 正文：整篇文章一页显示，视口跟着光标走
    let inner_width = layout.body.width.saturating_sub(2) as usize;
    let viewport = layout.body.height.saturating_sub(2) as usize;
    let ignore_case = app.boot.settings.ignore_case;
    let lines = article_lines(practice, inner_width.saturating_sub(2));
    let offset = cursor_line(&lines, practice)
        .saturating_sub(1)
        .min(lines.len().saturating_sub(viewport));
    let current_target: Vec<char> = practice
        .sentences
        .get(index)
        .map(|sentence| sentence.typing_target())
        .unwrap_or_default();
    let rendered: Vec<Line<'static>> = lines
        .iter()
        .skip(offset)
        .take(viewport)
        .map(|line| article_line(line, practice, &current_target, ignore_case, theme))
        .collect();
    frame.render_widget(
        Paragraph::new(rendered).block(practice_block(" 跟打 ", theme)),
        layout.body,
    );

    // 当前句译文（整篇文章同页时，译文只跟随当前句）
    if let Some(area) = layout.translate {
        let translate = practice
            .sentences
            .get(index)
            .map(|sentence| sentence.translate.clone())
            .unwrap_or_default();
        let translate_width = area.width.saturating_sub(3) as usize;
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
            area,
        );
    }

    // 底部：一行输入规则 + 动作按钮条（两行按钮 + 一行提示）。
    //
    // 键名不再写死：按钮上的键名从键位表取，改绑「朗读 / 翻译 / 主题」后
    // 跟打页的按钮跟着变，和练习页保持同源。
    let (hint, color) = if practice.typing.wrong {
        (" 红色 = 打错的字符 · Backspace 改正", theme.warn)
    } else {
        (
            " 字母、数字照打 · 空格可省略 · 标点不用打 · Backspace 退格",
            theme.dim,
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, Style::default().fg(color)))),
        layout.rule,
    );
    typing_buttons::render(
        frame,
        app,
        theme,
        &layout.button_bar,
        app.typing_buttons(),
        app.typing_button,
        "Esc 返回列表",
    );
}

/// 渲染正文的一行：行首 gutter + 按跟打进度逐字符着色。
///
/// 当前句已输入的位置显示实际按下的字符：打对用 `good`，打错用 `bad` 并
/// 显示按下的那个字符。词间空格可以省略（省略时仍按正确输入显示），所以
/// 错误只会出现在字母 / 数字位置，不需要额外的下划线样式。
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
                    let mismatch = practice.typing.is_mismatch(position, target, ignore_case);
                    let color = if mismatch { theme.bad } else { theme.good };
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
