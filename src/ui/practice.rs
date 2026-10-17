//! 练习页渲染：大字单词 + 信息卡片。
//!
//! 视觉层次：顶部状态行（进度条/错词/计时）→ 居中大字单词 → 释义/例句/词源卡片 → 阶段与按键条。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::App;
use crate::domain::flow::{FlowDisplayState, FlowProgressStage, display_state};
use crate::domain::i18n::tr;
use crate::domain::session::PracticeSession;
use crate::domain::timer::format_duration;
use crate::domain::types::WordPracticeType;
use crate::ui::bigtext::{self, BigLetter, GLYPH_WIDTH};
use crate::ui::shell::toast_line;
use crate::ui::theme::Theme;
use crate::ui::widgets::{bar_spans, wrap_text};

pub(super) fn render_practice(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    let Some(session) = app.session.as_ref() else {
        return;
    };
    let display = display_state(
        session.flow(),
        session.cursor(),
        session.data.index,
        session.data.words.len(),
    );

    let constraints = if app.show_toolbar {
        vec![
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(6),
        ]
    } else {
        vec![
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
        ]
    };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    frame.render_widget(
        Paragraph::new(header(app, session, &display, theme)),
        outer[0],
    );

    let body = if app.show_panel {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(30), Constraint::Length(28)])
            .split(outer[1]);
        render_word_list(app, session, frame, horizontal[1], theme);
        horizontal[0]
    } else {
        outer[1]
    };

    let inner_width = body.width.saturating_sub(4) as usize;
    // 以本轮最长的单词决定是否 2 倍放大，避免同一轮里字号忽大忽小
    let longest = session
        .data
        .words
        .iter()
        .map(|word| word.word.chars().count())
        .max()
        .unwrap_or(0);
    let max_letters_for_2x = (inner_width + 2) / (GLYPH_WIDTH * 2 + 2);
    let use_big = app.boot.settings.tui_big_word
        && longest > 0
        && longest <= max_letters_for_2x
        && body.height >= 20;
    let max_big_height = if use_big {
        body.height.saturating_sub(9) as usize
    } else {
        0
    };
    let big = word_lines(app, session, inner_width, max_big_height, theme);
    let specs = card_specs(app, session, inner_width, theme);
    let cards_height: u16 = specs
        .iter()
        .map(|(_, lines, max_lines)| card_height(inner_width, lines, *max_lines))
        .sum();
    let word_height = (big.len() as u16 + 5)
        .min(body.height.saturating_sub(6))
        .max(6);
    let cards_height = cards_height
        .min(body.height.saturating_sub(word_height + 1))
        .max(3);
    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(word_height),
            Constraint::Length(cards_height),
        ])
        .flex(Flex::Center)
        .split(body);
    word_area(app, session, frame, content[0], theme, &big);
    render_cards(frame, content[1], &specs, inner_width, theme);

    if app.show_toolbar {
        render_stage_progress(app, session, frame, outer[2], theme, &display.stages);
    } else if outer[2].height > 0 {
        frame.render_widget(Paragraph::new(toast_line(app, theme)), outer[2]);
    }
}

fn header(
    app: &App,
    session: &PracticeSession,
    display: &FlowDisplayState,
    theme: &Theme,
) -> Line<'static> {
    let dict_name = app
        .boot
        .study_dict()
        .map(|dict| dict.name.clone())
        .unwrap_or_default();
    let total = session.data.words.len();
    let index = (session.data.index + 1).min(total.max(1));
    let percent = if total == 0 {
        0
    } else {
        ((index as f64 / total as f64) * 100.0).round() as u32
    };
    let timer = if app.stat.timer_paused {
        "已暂停".to_string()
    } else {
        format_duration(app.stat.spend)
    };
    let wrong = session.data.wrong_words.len();

    let mut spans = vec![Span::styled(
        format!(" {dict_name} "),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )];
    if !display.status.is_empty() {
        spans.push(Span::styled("· ", Style::default().fg(theme.dim)));
        spans.push(Span::styled(
            display.status.clone(),
            Style::default().fg(theme.fg),
        ));
    }
    spans.push(Span::raw("   "));
    spans.extend(bar(percent, 16, theme.good, theme.dim));
    spans.push(Span::styled(
        format!(" {index}/{total}"),
        Style::default().fg(theme.fg),
    ));
    spans.push(Span::styled("   · ", Style::default().fg(theme.dim)));
    spans.push(Span::styled(
        format!("错词 {wrong}"),
        Style::default().fg(if wrong == 0 { theme.dim } else { theme.bad }),
    ));
    spans.push(Span::styled("   · ", Style::default().fg(theme.dim)));
    spans.push(Span::styled(
        timer,
        Style::default().fg(if app.stat.timer_paused {
            theme.warn
        } else {
            theme.dim
        }),
    ));
    Line::from(spans)
}

fn bar(percent: u32, width: usize, filled: Color, empty: Color) -> Vec<Span<'static>> {
    let count = ((percent as usize * width) / 100).min(width);
    vec![
        Span::styled("█".repeat(count), Style::default().fg(filled)),
        Span::styled(
            "░".repeat(width.saturating_sub(count)),
            Style::default().fg(empty),
        ),
    ]
}

fn chars_match(input: char, target: char, ignore_case: bool) -> bool {
    if input == target {
        return true;
    }
    ignore_case && input.to_lowercase().eq(target.to_lowercase())
}

fn word_lines(
    app: &App,
    session: &PracticeSession,
    max_width: usize,
    max_height: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let word = session.current_word();
    let view = session.view_state();
    let identify_hidden = session.practice_type() == WordPracticeType::Identify
        && !session.display.show_word_result
        && !view.reveal_all;
    let masked = view.is_word_masked || identify_hidden;
    let typed: Vec<char> = app.typing.input.chars().collect();
    let target: Vec<char> = word.word.chars().collect();
    let ignore_case = app.boot.settings.ignore_case;

    let mut letters = Vec::with_capacity(target.len());
    for (index, character) in target.iter().enumerate() {
        let color = if index < typed.len() {
            if chars_match(typed[index], *character, ignore_case) {
                theme.good
            } else {
                theme.bad
            }
        } else if masked {
            theme.dim
        } else if index == typed.len() {
            theme.accent
        } else {
            theme.fg
        };
        letters.push(BigLetter::new(color, masked && index >= typed.len()));
    }

    if let Some(lines) = bigtext::render_word(&word.word, &letters, max_width, max_height) {
        return lines;
    }

    // 大字字体不支持的字符（如带重音的单词）回退为普通文本
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (index, character) in target.iter().enumerate() {
        let (text, color, bold) = if index < typed.len() {
            (
                typed[index].to_string(),
                if chars_match(typed[index], *character, ignore_case) {
                    theme.good
                } else {
                    theme.bad
                },
                true,
            )
        } else if masked {
            ("_".to_string(), theme.dim, false)
        } else if index == typed.len() {
            (character.to_string(), theme.accent, true)
        } else {
            (character.to_string(), theme.fg, false)
        };
        let mut style = Style::default().fg(color);
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(format!("{text} "), style));
    }
    vec![Line::from(spans)]
}

fn practice_hint(app: &App, session: &PracticeSession) -> String {
    if session.display.show_word_result
        && app
            .typing
            .is_word_correct(&crate::domain::typing::TypingContext {
                target: &session.current_word().word,
                practice_type: session.practice_type(),
                is_word_masked: true,
                show_word_result: true,
                settings: &PracticeSession::typing_settings(&app.boot.settings),
            })
    {
        return tr("press_space_continue");
    }
    match session.practice_type() {
        WordPracticeType::Identify => tr("choose_or_spell"),
        WordPracticeType::Dictation => {
            if session.display.show_word_result {
                tr("press_space_continue")
            } else {
                tr("press_space_dictation")
            }
        }
        WordPracticeType::Listen => {
            if session.display.show_word_result {
                tr("press_space_continue")
            } else {
                tr("type_word_hint")
            }
        }
        _ => tr("type_word_hint"),
    }
}

fn word_area(
    app: &App,
    session: &PracticeSession,
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    big: &[Line<'static>],
) {
    let word = session.current_word();
    let view = session.view_state();
    let mut lines: Vec<Line<'static>> = Vec::new();

    let phonetic = word.phonetic(app.boot.settings.sound_type == "uk");
    if !phonetic.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("/ {phonetic} /"),
            Style::default().fg(theme.dim),
        )));
        lines.push(Line::from(""));
    }
    lines.extend(big.iter().cloned());
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        practice_hint(app, session),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )));

    if app.boot.settings.show_near_word && !view.is_word_masked {
        let previous = session
            .data
            .words
            .get(session.data.index.wrapping_sub(1))
            .filter(|_| session.data.index > 0)
            .map(|item| item.word.as_str())
            .unwrap_or("");
        let next = session
            .data
            .words
            .get(session.data.index + 1)
            .map(|item| item.word.as_str())
            .unwrap_or("");
        let mut parts: Vec<String> = Vec::new();
        if !previous.is_empty() {
            parts.push(format!("← {previous}"));
        }
        if !next.is_empty() {
            parts.push(format!("{next} →"));
        }
        if !parts.is_empty() {
            lines.push(Line::from(Span::styled(
                parts.join("        "),
                Style::default().fg(theme.dim),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

type CardSpec = (String, Vec<(String, Color)>, usize);

fn card_specs(
    app: &App,
    session: &PracticeSession,
    inner_width: usize,
    theme: &Theme,
) -> Vec<CardSpec> {
    if session.practice_type() == WordPracticeType::Identify && !session.display.show_word_result {
        return identify_specs(app, session, theme);
    }

    let word = session.current_word();
    let view = session.view_state();
    let mut cards: Vec<CardSpec> = Vec::new();

    if view.is_show_translate && !word.trans.is_empty() {
        let text = word
            .trans
            .iter()
            .map(|item| format!("{}{}", item.pos, item.cn))
            .collect::<Vec<_>>()
            .join("；");
        let content = wrap_text(&text, inner_width)
            .into_iter()
            .map(|line| (line, theme.fg))
            .collect();
        cards.push((" 释义 ".to_string(), content, 4));
    }

    if app.show_meta && !word.sentences.is_empty() {
        let mut content: Vec<(String, Color)> = Vec::new();
        for sentence in word.sentences.iter().take(2) {
            content.push((sentence.c.clone(), theme.fg));
            if !sentence.cn.is_empty() {
                content.push((sentence.cn.clone(), theme.dim));
            }
        }
        cards.push((" 例句 ".to_string(), content, 4));
    }

    if app.show_meta && app.boot.settings.show_etymology_and_rel_words {
        let mut content: Vec<(String, Color)> = Vec::new();
        if let Some(etymology) = word.etymology.first() {
            content.push((format!("{}：{}", etymology.t, etymology.d), theme.dim));
        }
        if !word.rel_words.root.is_empty() {
            content.push((format!("词根：{}", word.rel_words.root), theme.dim));
        }
        if !content.is_empty() {
            cards.push((" 词源 ".to_string(), content, 3));
        }
    }

    cards
}

fn render_cards(
    frame: &mut Frame,
    area: Rect,
    specs: &[CardSpec],
    inner_width: usize,
    theme: &Theme,
) {
    let mut y = area.y;
    let mut remaining = area.height;
    for (title, content, max_lines) in specs {
        if remaining < 3 {
            break;
        }
        let height = card_height(inner_width, content, *max_lines)
            .min(remaining)
            .max(3);
        let rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height,
        };
        render_card(frame, rect, title, content, *max_lines, theme);
        y += height;
        remaining = remaining.saturating_sub(height);
    }
}

fn identify_specs(app: &App, session: &PracticeSession, theme: &Theme) -> Vec<CardSpec> {
    let Some(question) = session.question() else {
        return Vec::new();
    };
    let mut content: Vec<(String, Color)> = Vec::new();
    if app.boot.settings.show_word_question {
        for (index, candidate) in question.candidates.iter().enumerate() {
            let marker = ["A", "B", "C", "D"].get(index).copied().unwrap_or("?");
            let translation = candidate
                .word
                .trans
                .iter()
                .take(2)
                .map(|item| format!("{}{}", item.pos, item.cn))
                .collect::<Vec<_>>()
                .join("；");
            let selected = app.identify_select == Some(index);
            let color = if selected {
                if question.correct_index == index {
                    theme.good
                } else {
                    theme.bad
                }
            } else {
                theme.fg
            };
            let shown = if app.identify_select.is_some() {
                format!("    {}", candidate.word.word)
            } else {
                String::new()
            };
            content.push((format!("{marker}   {translation}{shown}"), color));
        }
    } else {
        content.push((
            "1 我认识    2 不认识    3 已掌握    或直接拼写".to_string(),
            theme.dim,
        ));
    }
    vec![(" 选择答案 ".to_string(), content, 4)]
}

fn card_height(inner_width: usize, content: &[(String, Color)], max_lines: usize) -> u16 {
    let total: usize = content
        .iter()
        .map(|(text, _)| wrap_text(text, inner_width).len())
        .sum();
    (total.min(max_lines) + 2) as u16
}

fn render_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    content: &[(String, Color)],
    max_lines: usize,
    theme: &Theme,
) {
    let inner_width = area.width.saturating_sub(3) as usize;
    // 同时受内容行数上限与卡片实际高度限制
    let capacity = max_lines.min(area.height.saturating_sub(2) as usize);
    let mut rendered: Vec<(String, Color)> = Vec::new();
    let mut total = 0usize;
    for (text, color) in content {
        for line in wrap_text(text, inner_width) {
            total += 1;
            if rendered.len() < capacity {
                rendered.push((format!(" {line}"), *color));
            }
        }
    }
    if total > rendered.len()
        && let Some(last) = rendered.last_mut()
    {
        last.0.push('…');
    }

    let lines: Vec<Line<'static>> = rendered
        .into_iter()
        .map(|(text, color)| Line::from(Span::styled(text, Style::default().fg(color))))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(card_block(title, theme)), area);
}

fn card_block(title: &str, theme: &Theme) -> Block<'static> {
    practice_block(title, theme)
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

fn hint_line(hints: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        hints.to_string(),
        Style::default().fg(theme.dim),
    ))
}

fn render_stage_progress(
    app: &App,
    session: &PracticeSession,
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    stages: &[FlowProgressStage],
) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let mut stage_spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    if stages.is_empty() {
        stage_spans.push(Span::styled("练习", Style::default().fg(theme.dim)));
    }
    for stage in stages {
        let color = if stage.active {
            theme.accent
        } else if stage.percentage >= 100 {
            theme.good
        } else {
            theme.dim
        };
        let name = if stage.name.is_empty() {
            String::from("练习")
        } else {
            stage.name.clone()
        };
        stage_spans.push(Span::styled(format!("{name} "), Style::default().fg(color)));
        stage_spans.extend(bar_spans(stage.percentage, 8, color, theme.dim));
        stage_spans.push(Span::styled(
            format!(" {}%   ", stage.percentage),
            Style::default().fg(theme.dim),
        ));
    }
    lines.push(Line::from(stage_spans));

    lines.push(hint_line(
        " 空格 继续 · Esc 显示 · Tab 跳过 · 反引号 掌握 · Enter 收藏 · ? 帮助",
        theme,
    ));
    lines.push(hint_line(
        " Ctrl+Z 翻译 · Ctrl+I 默写 · Ctrl+P 暂停 · Ctrl+L 词表 · Ctrl+E 例句 · Ctrl+C 返回",
        theme,
    ));
    if area.height >= 6 {
        lines.push(toast_line(app, theme));
    }
    let _ = session;
    frame.render_widget(
        Paragraph::new(lines).block(practice_block(" 阶段 ", theme)),
        area,
    );
}

fn render_word_list(
    app: &App,
    session: &PracticeSession,
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
) {
    let items: Vec<ratatui::widgets::ListItem> = session
        .data
        .words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let active = index == session.data.index;
            let excluded = session.data.exclude_words.contains(&word.word);
            let wrong = session
                .data
                .wrong_words
                .iter()
                .any(|item| item.word == word.word);
            let color = if active {
                theme.accent
            } else if wrong {
                theme.bad
            } else if excluded {
                theme.dim
            } else {
                theme.fg
            };
            let marker = if active { "▶" } else { " " };
            ratatui::widgets::ListItem::new(Line::from(Span::styled(
                format!("{marker} {:>3}. {}", index + 1, word.word),
                Style::default().fg(color),
            )))
        })
        .collect();
    let _ = app;
    frame.render_widget(
        ratatui::widgets::List::new(items).block(practice_block(" 词表 ", theme)),
        area,
    );
}
