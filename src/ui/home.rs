//! 主页：原版 Web Home 的信息层级 + opencode 画风。
//!
//! `layout` 与 `render` 共用同一组 Rect；鼠标命中检测只读 `layout`。

use chrono::Datelike;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Screen};
use crate::domain::timer::format_duration;
use crate::ui::insights;
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

#[derive(Clone, Debug)]
pub struct HomeLayout {
    pub hero: Rect,
    pub hero_inner: Rect,
    pub hero_left: Rect,
    pub hero_right: Rect,
    pub divider: Rect,
    pub primary: Rect,
    pub free: Option<Rect>,
    pub goal_minus: Option<Rect>,
    pub goal_plus: Option<Rect>,
    pub goal_short: bool,
    /// 练习模式提示（可点击打开模式选择）
    pub mode_row: Option<Rect>,
    pub stats: Option<Rect>,
    pub stats_tiles: Option<Rect>,
    pub week: Option<Rect>,
    pub dicts: Option<Rect>,
    pub dict_tiles: Option<Rect>,
    pub open_dicts: Option<Rect>,
    /// 「+ / d 管理」卡片（可点击打开词库页）
    pub manage_tile: Option<Rect>,
    pub recs: Option<Rect>,
    pub rec_list: Option<Rect>,
    pub open_more: Option<Rect>,
}

/// 主页布局。
///
/// `per_day` 是每日词数——它决定「每日目标」行里数字占几位，进而决定
/// `[-]` / `[+]` 的位置，因此必须传进来。
///
/// `mode_name` 是当前练习模式的显示名——它决定「模式 X ▾」标签的宽度，
/// 同样必须传进来，否则标签会被截断。
///
/// 需要 `app` 是因为词典卡片的张数与位置取决于当前有哪些词库——
/// 「+ / d 管理」卡的命中矩形必须和绘制位置一致。
pub fn layout(app: &App, body: Rect, per_day: usize, mode_name: &str) -> HomeLayout {
    let (hero_h, stats_h, dicts_h, recs_h) = heights(body.height);
    let mut constraints = vec![
        Constraint::Length(hero_h),
        Constraint::Length(1),
        Constraint::Length(stats_h),
    ];
    let show_dicts = body.height >= hero_h + 1 + stats_h + 1 + dicts_h;
    if show_dicts {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(dicts_h));
    }
    let show_recs = show_dicts && body.height >= hero_h + 1 + stats_h + 1 + dicts_h + 1 + recs_h;
    if show_recs {
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Length(recs_h));
    }
    constraints.push(Constraint::Min(0));
    let chunks = Layout::vertical(constraints).split(body);

    let hero = chunks[0];
    let hero_inner = Rect::new(
        hero.x.saturating_add(2),
        hero.y.saturating_add(1),
        hero.width.saturating_sub(4),
        hero.height.saturating_sub(2),
    );
    let columns = Layout::horizontal([
        Constraint::Percentage(58),
        Constraint::Length(1),
        Constraint::Min(18),
    ])
    .split(hero_inner);
    let hero_left = columns[0];
    let divider = columns[1];
    let hero_right = columns[2];

    let button_y = hero_inner.y + hero_inner.height.saturating_sub(1);
    let primary_width = 22.min(hero_left.width.max(1));
    let primary = Rect::new(hero_left.x, button_y, primary_width, 1);
    let free_width = 13;
    let free = (hero_left.width >= primary_width + 1 + free_width)
        .then(|| Rect::new(hero_left.x + primary_width + 1, button_y, free_width, 1));

    let goal_short = hero_right.width < 24;
    // 每日目标行的命中矩形必须由**绘制用的同一行文本**算出来。
    // 早期实现用固定偏移（`hero_right.x + 9` / `+21`）估算位置，而渲染
    // 用的是「每日目标 [-] N 词 [+]」的实际排版，两者对不上：所有终端
    // 尺寸下 `[+]` 的命中区都偏 2 列，点 `+` 号没反应。
    let goal_line = goal_line_text(goal_short, per_day);
    let (goal_minus, goal_plus) = goal_buttons(&goal_line, hero_right);

    // 练习模式：显示在「今日任务」标题行的右端，点击打开模式选择。
    //
    // 宽度必须按**当前模式名**算，不能写死「学习」——早期实现把宽度固定
    // 成「模式 学习 ▾」，模式名一变长就被截断成「模式 随机…」，连 `▾`
    // 都被吃掉，用户看不出这是个可点的下拉。
    let mode_label = mode_label_text(mode_name);
    let mode_width = display_width(&mode_label) as u16;
    let mode_row = (hero_right.width >= mode_width + 12).then(|| {
        Rect::new(
            hero_right.right().saturating_sub(mode_width),
            hero_right.y,
            mode_width,
            1,
        )
    });

    let stats = Some(chunks[2]);
    let week_mode = body.width >= 60;
    let stats_left_width = if week_mode {
        (body.width * 58 / 100).max(24)
    } else {
        body.width
    };
    let stats_tiles = Some(Rect::new(
        chunks[2].x,
        chunks[2].y + 1,
        stats_left_width,
        chunks[2].height.saturating_sub(2),
    ));
    let week = week_mode.then(|| {
        Rect::new(
            chunks[2].x + stats_left_width + 2,
            chunks[2].y + 1,
            chunks[2].width.saturating_sub(stats_left_width + 2),
            chunks[2].height.saturating_sub(2),
        )
    });

    let dicts = show_dicts.then(|| chunks[4]);
    let dict_tiles = dicts.map(|rect| {
        Rect::new(
            rect.x,
            rect.y + 1,
            rect.width,
            rect.height.saturating_sub(2),
        )
    });
    let open_dicts = dicts.map(|rect| {
        let label = "d 词库";
        let width = display_width(label) as u16;
        Rect::new(rect.right().saturating_sub(width), rect.y, width, 1)
    });
    // 「+ / d 管理」卡的位置：与 `render_dicts` 共用 `dict_tile_layout`，
    // 否则两处各算一遍必然漂移，卡片上的「d」就点不到。
    let manage_tile = dict_tiles.and_then(|tiles| {
        let max_tiles = (tiles.width / 14).clamp(3, 5) as usize;
        let dicts = ordered_dicts(app, max_tiles);
        dict_tile_layout(tiles, &dicts, max_tiles).manage
    });

    let (recs, rec_list, open_more) = if show_recs {
        let rect = chunks[6];
        let list = Rect::new(
            rect.x,
            rect.y + 1,
            rect.width,
            rect.height.saturating_sub(2),
        );
        let label = "d 更多";
        let width = display_width(label) as u16;
        let more = Rect::new(rect.right().saturating_sub(width), rect.y, width, 1);
        (Some(rect), Some(list), Some(more))
    } else {
        (None, None, None)
    };

    let _ = Screen::Home;
    HomeLayout {
        mode_row,
        hero,
        hero_inner,
        hero_left,
        hero_right,
        divider,
        primary,
        free,
        goal_minus,
        goal_plus,
        goal_short,
        stats,
        stats_tiles,
        week,
        dicts,
        dict_tiles,
        open_dicts,
        manage_tile,
        recs,
        rec_list,
        open_more,
    }
}

/// 词典卡片的排布结果。
///
/// 绘制（`render_dicts`）与命中检测（`layout`）共用它——两边各算一遍
/// 必然漂移，「+ / d 管理」卡就会点不到。
struct DictTiles {
    /// 每张卡的矩形（按展示顺序）。
    tiles: Vec<Rect>,
    /// 「+ / d 管理」卡的位置；`None` 表示没画这张卡。
    manage: Option<Rect>,
}

/// 计算词典卡片的排布。
///
/// `dicts` 是要展示的词库（已把当前词库换到前面），`max_tiles` 是最多
/// 展示几张卡。
fn dict_tile_layout(
    tiles_area: Rect,
    dicts: &[&crate::domain::types::Dict],
    max_tiles: usize,
) -> DictTiles {
    let show_plus = dicts.len() < max_tiles;
    let tile_count = dicts
        .len()
        .min(max_tiles - usize::from(show_plus && max_tiles > 1));
    let tile_count = tile_count.max(1);
    let slots = tile_count + usize::from(show_plus && tile_count < max_tiles);
    let gap = 1u16;
    let tile_width = (tiles_area.width.saturating_sub(gap * (slots as u16 - 1))) / slots as u16;
    if tile_width == 0 {
        return DictTiles {
            tiles: Vec::new(),
            manage: None,
        };
    }
    let mut tiles = Vec::with_capacity(tile_count);
    for index in 0..tile_count {
        let x = tiles_area.x + index as u16 * (tile_width + gap);
        tiles.push(Rect::new(x, tiles_area.y, tile_width, tiles_area.height));
    }
    let manage = (show_plus && tile_count < max_tiles).then(|| {
        let x = tiles_area.x + tile_count as u16 * (tile_width + gap);
        Rect::new(x, tiles_area.y, tile_width, tiles_area.height)
    });
    DictTiles { tiles, manage }
}

/// 词典卡片的展示顺序：当前词库排第一，避免被系统词典挤掉。
///
/// 返回的引用顺序与 `book_list` 一致（除交换外），便于调用方按索引取。
fn ordered_dicts(app: &App, max_tiles: usize) -> Vec<&crate::domain::types::Dict> {
    let mut dicts: Vec<_> = app.boot.base.word.book_list.iter().collect();
    if let Some(study) = app.boot.base.study_dict()
        && let Some(position) = dicts.iter().position(|dict| {
            dict.system == study.system && (dict.id == study.id || dict.en_name == study.en_name)
        })
        && position >= max_tiles
        && !dicts.is_empty()
    {
        // 窄屏优先展示当前词库，避免它被系统词典挤掉。
        dicts.swap(0, position);
    }
    dicts
}

/// 「模式 X ▾」标签的文本。
///
/// 命中矩形与渲染共用它——宽度按真实模式名算，模式名再长也不会被截断。
pub fn mode_label_text(mode_name: &str) -> String {
    format!("模式 {mode_name} ▾")
}

/// 每日目标行的文本。
///
/// 命中检测与渲染共用它——两边各自排版就会算出不同的按钮位置。
/// 拼接结果必须与 `goal_line_spans` 的内容一致。
pub fn goal_line_text(goal_short: bool, per_day: usize) -> String {
    if goal_short {
        format!("目标 [-] {per_day} [+] ")
    } else {
        format!("每日目标 [-] {per_day:>2} 词 [+]")
    }
}

/// 每日目标行的分段（带颜色）。
///
/// 拼接后的文本与 `goal_line_text` 相同——命中检测按后者算位置。
fn goal_line_spans(goal_short: bool, per_day: usize, theme: &Theme) -> Vec<Span<'static>> {
    if goal_short {
        vec![
            Span::styled("目标 ", Style::default().fg(theme.dim)),
            Span::styled("[-]", Style::default().fg(theme.accent_alt)),
            Span::styled(format!(" {per_day}"), Style::default().fg(theme.fg)),
            Span::styled(" [+] ", Style::default().fg(theme.accent_alt)),
        ]
    } else {
        vec![
            Span::styled("每日目标 ", Style::default().fg(theme.dim)),
            Span::styled("[-]", Style::default().fg(theme.accent_alt)),
            Span::styled(
                format!(" {per_day:>2} "),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled("词 ", Style::default().fg(theme.muted)),
            Span::styled("[+]", Style::default().fg(theme.accent_alt)),
        ]
    }
}

/// 每日目标行里 `[-]` 与 `[+]` 的命中矩形。
///
/// 按文本里的实际字符位置计算，不做任何偏移估算。
fn goal_buttons(text: &str, hero_right: Rect) -> (Option<Rect>, Option<Rect>) {
    let row = Rect::new(hero_right.x, hero_right.y + 1, hero_right.width, 1);
    let find = |needle: &str| -> Option<Rect> {
        let offset = text.find(needle)?;
        // 文本是 ASCII + CJK 混合，用显示宽度算列号。
        let column = display_width(&text[..offset]) as u16;
        let width = display_width(needle) as u16;
        let rect = Rect::new(row.x + column, row.y, width, 1);
        (rect.right() <= row.right()).then_some(rect)
    };
    (find("[-]"), find("[+]"))
}

fn heights(body_height: u16) -> (u16, u16, u16, u16) {
    if body_height >= 34 {
        (11, 7, 7, 6)
    } else if body_height >= 28 {
        (10, 6, 6, 5)
    } else if body_height >= 23 {
        (9, 5, 5, 4)
    } else {
        // 80×24 左右：收紧 hero，让“我的词典”仍能看到。
        (8, 5, 4, 3)
    }
}

pub fn render_home(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    shell::render(app, frame, area, theme, Nav::Home, |frame, content| {
        let per_day = app
            .boot
            .study_dict()
            .map(|dict| dict.per_day())
            .unwrap_or(0);
        let mode_name =
            crate::domain::constants::practice_mode_name(app.boot.settings.word_practice_mode);
        let layout = layout(app, content, per_day, mode_name);
        render_hero(app, frame, &layout, theme);
        if let Some(stats) = layout.stats {
            render_stats(app, frame, stats, layout.stats_tiles, layout.week, theme);
        }
        if let Some(dicts) = layout.dicts {
            render_dicts(
                app,
                frame,
                dicts,
                layout.dict_tiles,
                layout.open_dicts,
                theme,
            );
        }
        if let Some(recs) = layout.recs {
            render_recommendations(app, frame, recs, layout.rec_list, layout.open_more, theme);
        }
    });
}

fn render_hero(app: &App, frame: &mut Frame, layout: &HomeLayout, theme: &Theme) {
    widgets::paint_rect(frame, layout.hero, theme.panel);
    widgets::paint_rect(
        frame,
        Rect::new(layout.hero.x, layout.hero.y, 1, layout.hero.height),
        theme.accent,
    );
    widgets::paint_rect(frame, layout.divider, theme.border);

    let dict = app.boot.study_dict();
    let left = layout.hero_left;
    let row = |offset: u16| Rect::new(left.x, left.y + offset, left.width, 1);

    let name = dict
        .map(|dict| dict.name.clone())
        .unwrap_or_else(|| "尚未选择词库".to_string());
    frame.render_widget(
        Paragraph::new(Span::styled(
            fit_text(&name, left.width as usize),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        row(0),
    );

    let date_line = match dict {
        Some(dict) if dict.complete => Line::from(Span::styled(
            "已学完 · 总复习阶段",
            Style::default().fg(theme.good),
        )),
        Some(dict) => match insights::estimated_finish(dict, insights::today()) {
            Some(date) => Line::from(vec![
                Span::styled("预计完成日期 ", Style::default().fg(theme.dim)),
                Span::styled(
                    insights::format_date(date),
                    Style::default().fg(theme.muted),
                ),
            ]),
            None => Line::from(Span::styled("已完成", Style::default().fg(theme.good))),
        },
        None => Line::from(vec![
            Span::styled("按 ", Style::default().fg(theme.dim)),
            Span::styled("d", Style::default().fg(theme.accent_alt)),
            Span::styled(" 选择词库", Style::default().fg(theme.dim)),
        ]),
    };
    frame.render_widget(Paragraph::new(date_line), row(1));

    let (percent, last, length) = dict
        .map(|dict| {
            (
                dict.progress_percent(),
                dict.last_learn_index,
                dict.effective_length(),
            )
        })
        .unwrap_or((0, 0, 0));
    let percent_text = format!("{percent}%");
    let bar_width = (left.width as usize)
        .saturating_sub(display_width(&percent_text) + 2)
        .max(6);
    let mut progress = widgets::thin_bar_spans(
        percent,
        bar_width,
        if dict.map(|dict| dict.complete).unwrap_or(false) {
            theme.good
        } else {
            theme.accent_alt
        },
        theme.dim,
    );
    progress.push(Span::raw(" "));
    progress.push(Span::styled(percent_text, Style::default().fg(theme.muted)));
    frame.render_widget(Paragraph::new(Line::from(progress)), row(2));

    let left_text = if dict.map(|dict| dict.complete).unwrap_or(false) {
        "已学完，进入总复习阶段".to_string()
    } else {
        format!("当前进度：已学 {percent}%")
    };
    frame.render_widget(
        Paragraph::new(two_part_line(
            Span::styled(left_text, Style::default().fg(theme.dim)),
            Span::styled(
                format!("{last} / {length} 词"),
                Style::default().fg(theme.muted),
            ),
            left.width as usize,
        )),
        row(3),
    );

    // 按钮文案：词典学到一半、或有未完成的一轮，都显示「继续」。
    // 词典进度用 `last_learn_index` 判断——它是「学过多少」的累计值；
    // 断点缓存则是「有一轮没练完」，两者是不同的语义，可以同时成立。
    let continue_learning = dict
        .map(|dict| dict.last_learn_index > 0 || dict.complete)
        .unwrap_or(false)
        || app.has_saved_practice();
    let primary_label = if continue_learning {
        "Enter  继续学习 →"
    } else {
        "Enter  开始学习 →"
    };
    widgets::draw_button(
        frame,
        layout.primary,
        primary_label,
        theme.accent,
        theme.bg,
        true,
    );
    if let Some(free) = layout.free {
        widgets::draw_button(frame, free, "f  自由练习", theme.element, theme.fg, false);
    }

    render_hero_right(app, frame, layout, theme);
}

fn render_hero_right(app: &App, frame: &mut Frame, layout: &HomeLayout, theme: &Theme) {
    let right = layout.hero_right;
    if right.width < 8 || right.height < 4 {
        return;
    }
    // 有未完成的练习缓存时，标题从「今日任务」变成「上次任务」——
    // 对齐上游 `words.vue` 的 `isSaveData ? '上次任务' : '今日任务'`：
    // 数字卡里显示的是缓存里那份任务，不是新生成的今日任务。
    let title = if app.has_saved_practice() {
        "上次任务"
    } else {
        "今日任务"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◆ ", Style::default().fg(theme.warn)),
            Span::styled(
                title,
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect::new(right.x, right.y, right.width, 1),
    );

    // 练习模式：可点击打开模式选择
    if let Some(mode_rect) = layout.mode_row {
        let mode_name =
            crate::domain::constants::practice_mode_name(app.boot.settings.word_practice_mode);
        let text = mode_label_text(mode_name);
        frame.render_widget(
            Paragraph::new(Span::styled(
                fit_text(&text, mode_rect.width as usize),
                Style::default().fg(theme.accent_alt),
            )),
            mode_rect,
        );
    }

    let dict = app.boot.study_dict();
    let per_day = dict.map(|dict| dict.per_day()).unwrap_or(0);
    frame.render_widget(
        Paragraph::new(Line::from(goal_line_spans(
            layout.goal_short,
            per_day,
            theme,
        ))),
        Rect::new(right.x, right.y + 1, right.width, 1),
    );

    let (new_count, review_count, due) = app.task_preview.unwrap_or((0, 0, 0));
    let tile_y = right.y + 2;
    let tile_height = 3.min(right.height.saturating_sub(3)).max(1);
    let tile_width = right.width.saturating_sub(1) / 2;
    render_number_tile(
        frame,
        Rect::new(right.x, tile_y, tile_width, tile_height),
        &new_count.to_string(),
        "新词",
        "",
        theme,
    );
    render_number_tile(
        frame,
        Rect::new(right.x + tile_width + 1, tile_y, tile_width, tile_height),
        &review_count.to_string(),
        "复习",
        if review_count == 0 && due == 0 {
            "无到期"
        } else {
            ""
        },
        theme,
    );
}

fn render_number_tile(
    frame: &mut Frame,
    rect: Rect,
    number: &str,
    label: &str,
    sub: &str,
    theme: &Theme,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    widgets::paint_rect(frame, rect, theme.element);
    widgets::paint_rect(
        frame,
        Rect::new(rect.x, rect.y, 1, rect.height),
        theme.accent_alt,
    );
    let inner = Rect::new(
        rect.x + 2,
        rect.y,
        rect.width.saturating_sub(3),
        rect.height,
    );
    let mut lines = vec![Line::from(Span::styled(
        fit_text(number, inner.width as usize),
        Style::default()
            .fg(theme.accent_alt)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(Span::styled(
        fit_text(label, inner.width as usize),
        Style::default().fg(theme.muted),
    )));
    if rect.height >= 3 && !sub.is_empty() {
        lines.push(Line::from(Span::styled(
            fit_text(sub, inner.width as usize),
            Style::default().fg(theme.dim),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_stats(
    app: &App,
    frame: &mut Frame,
    rect: Rect,
    tiles: Option<Rect>,
    week: Option<Rect>,
    theme: &Theme,
) {
    frame.render_widget(
        Paragraph::new(widgets::section_header(
            "统计",
            if week.is_some() {
                "本周学习记录"
            } else {
                ""
            },
            rect.width as usize,
            theme,
        )),
        Rect::new(rect.x, rect.y, rect.width, 1),
    );
    let Some(tiles) = tiles else {
        return;
    };
    let gap = 1u16;
    let tile_width = (tiles.width.saturating_sub(gap * 2)) / 3;
    if tile_width == 0 {
        return;
    }
    let tile_height = tiles.height.max(1);
    let totals = insights::totals(&app.boot.base);
    let compact = tile_width < 15;
    let entries = [
        (
            format_duration(app.today_spend),
            if compact {
                "今日时长"
            } else {
                "今日学习时长"
            },
        ),
        (
            totals.days.to_string(),
            if compact {
                "总天数"
            } else {
                "总学习天数"
            },
        ),
        (
            format_duration(totals.spend),
            if compact {
                "总时长"
            } else {
                "总学习时长"
            },
        ),
    ];
    for (index, (value, label)) in entries.iter().enumerate() {
        let x = tiles.x + index as u16 * (tile_width + gap);
        let tile = Rect::new(x, tiles.y, tile_width, tile_height);
        widgets::paint_rect(frame, tile, theme.element);
        let inner = Rect::new(
            tile.x + 1,
            tile.y,
            tile.width.saturating_sub(2),
            tile.height,
        );
        let mut lines = vec![Line::from(Span::styled(
            fit_text(value, inner.width as usize),
            Style::default()
                .fg(theme.accent_alt)
                .add_modifier(Modifier::BOLD),
        ))];
        if tile.height >= 2 {
            lines.push(Line::from(Span::styled(
                fit_text(label, inner.width as usize),
                Style::default().fg(theme.muted),
            )));
        }
        let text_height = lines.len() as u16;
        let text_y = inner.y + inner.height.saturating_sub(text_height) / 2;
        frame.render_widget(
            Paragraph::new(lines),
            Rect::new(inner.x, text_y, inner.width, text_height),
        );
    }
    if let Some(week) = week {
        render_week(app, frame, week, theme);
    }
}

fn render_week(app: &App, frame: &mut Frame, rect: Rect, theme: &Theme) {
    if rect.width == 0 || rect.height < 2 {
        return;
    }
    let today = insights::today();
    let days = insights::week_days(&app.boot.base, today);
    let column = (rect.width / 7).max(1);
    let monday = today - chrono::Days::new(u64::from(today.weekday().num_days_from_monday()));
    for (index, day) in days.iter().enumerate() {
        let date = monday + chrono::Days::new(index as u64);
        let x = rect.x + index as u16 * column;
        let width = if index == 6 {
            rect.right().saturating_sub(x)
        } else {
            column
        };
        let is_today = date == today;
        let studied = day.is_some();
        frame.render_widget(
            Paragraph::new(Span::styled(
                insights::weekday_label(index),
                Style::default().fg(theme.dim),
            ))
            .alignment(ratatui::layout::Alignment::Center),
            Rect::new(x, rect.y, width, 1),
        );
        let date_rect = Rect::new(x, rect.y + 1, width, 1);
        if is_today {
            widgets::paint_rect(frame, date_rect, theme.accent);
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                insights::day_number(date).to_string(),
                Style::default().fg(if is_today {
                    theme.bg
                } else if studied {
                    theme.accent_alt
                } else {
                    theme.dim
                }),
            ))
            .style(if is_today {
                Style::default().bg(theme.accent)
            } else {
                Style::default()
            })
            .alignment(ratatui::layout::Alignment::Center),
            date_rect,
        );
        if rect.height >= 3 {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    if studied { "●" } else { "·" },
                    Style::default().fg(if studied { theme.accent_alt } else { theme.dim }),
                ))
                .alignment(ratatui::layout::Alignment::Center),
                Rect::new(x, rect.y + 2, width, 1),
            );
        }
    }
}

fn render_dicts(
    app: &App,
    frame: &mut Frame,
    rect: Rect,
    tiles: Option<Rect>,
    open_dicts: Option<Rect>,
    theme: &Theme,
) {
    frame.render_widget(
        Paragraph::new(widgets::section_header(
            "我的词典",
            "d 词库",
            rect.width as usize,
            theme,
        )),
        Rect::new(rect.x, rect.y, rect.width, 1),
    );
    let _ = open_dicts;
    let Some(tiles) = tiles else {
        return;
    };
    if tiles.width == 0 || tiles.height == 0 {
        return;
    }
    let max_tiles = (tiles.width / 14).clamp(3, 5) as usize;
    let dicts = ordered_dicts(app, max_tiles);
    let arrangement = dict_tile_layout(tiles, &dicts, max_tiles);
    if arrangement.tiles.is_empty() {
        return;
    }
    let study = app.boot.base.study_dict();
    for (index, dict) in dicts.iter().enumerate() {
        let Some(tile) = arrangement.tiles.get(index).copied() else {
            break;
        };
        widgets::paint_rect(frame, tile, theme.element);
        let current = study
            .map(|study| study.matches_id(&dict.id) || study.matches_id(&dict.en_name))
            .unwrap_or(false);
        let badge = if dict.system {
            "内置"
        } else if current {
            "当前"
        } else {
            "词库"
        };
        let count = dict.effective_length();
        let inner = Rect::new(
            tile.x + 1,
            tile.y + 1,
            tile.width.saturating_sub(2),
            tile.height.saturating_sub(1),
        );
        let name_y = if tile.height >= 5 { tile.y + 1 } else { tile.y };
        frame.render_widget(
            Paragraph::new(Span::styled(
                fit_text(&dict.name, inner.width as usize),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            )),
            Rect::new(inner.x, name_y, inner.width, 1),
        );
        if tile.height >= 2 {
            let badge = Span::styled(
                badge.to_string(),
                Style::default().fg(if current {
                    theme.accent
                } else {
                    theme.accent_alt
                }),
            );
            let count = Span::styled(format!("{count}词"), Style::default().fg(theme.muted));
            frame.render_widget(
                Paragraph::new(two_part_line(badge, count, inner.width as usize)),
                Rect::new(inner.x, tile.bottom().saturating_sub(1), inner.width, 1),
            );
        }
    }
    if let Some(tile) = arrangement.manage {
        widgets::paint_rect(frame, tile, theme.element);
        let mut lines = vec![Line::from(Span::styled(
            "+",
            Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
        ))];
        if tile.height >= 2 {
            lines.push(Line::from(Span::styled(
                "d 管理",
                Style::default().fg(theme.dim),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
            tile,
        );
    }
}

fn render_recommendations(
    app: &App,
    frame: &mut Frame,
    rect: Rect,
    list: Option<Rect>,
    open_more: Option<Rect>,
    theme: &Theme,
) {
    frame.render_widget(
        Paragraph::new(widgets::section_header(
            "推荐",
            "d 更多",
            rect.width as usize,
            theme,
        )),
        Rect::new(rect.x, rect.y, rect.width, 1),
    );
    let _ = open_more;
    let Some(list) = list else {
        return;
    };
    let current = app.boot.study_dict();
    let owned: Vec<String> = app
        .boot
        .base
        .word
        .book_list
        .iter()
        .flat_map(|dict| dict.identity_list())
        .collect();
    let recommended: Vec<_> = app
        .boot
        .catalog
        .resources()
        .iter()
        .filter(|resource| {
            let id = resource.id_string();
            !current
                .map(|dict| dict.matches_id(&id) || dict.matches_id(&resource.en_name))
                .unwrap_or(false)
                && !owned
                    .iter()
                    .any(|owned| owned == &id || owned == &resource.en_name)
        })
        .take(list.height.max(1) as usize)
        .collect();
    if recommended.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "暂无推荐 · d 浏览词库",
                Style::default().fg(theme.dim),
            )),
            Rect::new(list.x, list.y, list.width, 1),
        );
        return;
    }
    for (index, resource) in recommended.iter().enumerate() {
        let y = list.y + index as u16;
        if y >= list.bottom() {
            break;
        }
        let name = fit_text(&resource.name, 18);
        let description = fit_text(&resource.description, 22);
        let count = format!("{} 词", resource.length);
        let name_width = display_width(&name);
        let desc_width = display_width(&description);
        let count_width = display_width(&count);
        let gap = (list.width as usize)
            .saturating_sub(name_width + desc_width + count_width + 4)
            .max(1);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(name, Style::default().fg(theme.fg)),
                Span::raw(" ".repeat(2)),
                Span::styled(description, Style::default().fg(theme.muted)),
                Span::raw(" ".repeat(gap)),
                Span::styled(count, Style::default().fg(theme.dim)),
            ])),
            Rect::new(list.x, y, list.width, 1),
        );
    }
}

fn two_part_line(left: Span<'static>, right: Span<'static>, width: usize) -> Line<'static> {
    let left_width = display_width(left.content.as_ref());
    let right_text = fit_text(right.content.as_ref(), width.saturating_sub(left_width + 1));
    let right_width = display_width(&right_text);
    let gap = width.saturating_sub(left_width + right_width);
    Line::from(vec![
        left,
        Span::raw(" ".repeat(gap)),
        Span::styled(right_text, right.style),
    ])
}

/// 鼠标命中检测用的目标（主页）。
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let per_day = app
        .boot
        .study_dict()
        .map(|dict| dict.per_day())
        .unwrap_or(0);
    let mode_name =
        crate::domain::constants::practice_mode_name(app.boot.settings.word_practice_mode);
    let layout = layout(app, content, per_day, mode_name);
    let mut targets = Vec::new();
    targets.push(Target::new(
        layout.primary,
        crate::app::Action::StartPractice,
    ));
    if let Some(free) = layout.free {
        targets.push(Target::new(free, crate::app::Action::StartFreePractice));
    }
    if let Some(minus) = layout.goal_minus {
        targets.push(Target::new(minus, crate::app::Action::AdjustDaily(-5)));
    }
    if let Some(plus) = layout.goal_plus {
        targets.push(Target::new(plus, crate::app::Action::AdjustDaily(5)));
    }
    if let Some(mode) = layout.mode_row {
        targets.push(Target::new(mode, crate::app::Action::OpenModePicker));
    }
    if let Some(open) = layout.open_dicts {
        targets.push(Target::new(
            open,
            crate::app::Action::Navigate(Screen::DictPicker),
        ));
    }
    // 「+ / d 管理」卡也是词库入口：卡片上写着「d」，点它就该去词库页。
    if let Some(manage) = layout.manage_tile {
        targets.push(Target::new(
            manage,
            crate::app::Action::Navigate(Screen::DictPicker),
        ));
    }
    if let Some(open) = layout.open_more {
        targets.push(Target::new(
            open,
            crate::app::Action::Navigate(Screen::DictPicker),
        ));
    }
    targets
}
