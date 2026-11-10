//! ratatui 渲染层。
//!
//! 分层：`theme`（颜色映射）/ `shell` 与各页面（纯布局 + 渲染）/
//! `hit`（把布局命中转换成 Action）。渲染只读 App，不修改状态。

pub mod articles;
pub mod bigtext;
pub mod dict_picker;
pub mod flows;
pub mod focus;
pub mod help;
pub mod hit;
pub mod home;
pub mod insights;
pub mod overlays;
pub mod practice;
pub mod settings;
pub mod shell;
pub mod shortcuts;
pub mod stats;
pub mod target;
pub mod theme;
pub mod widgets;
pub mod word_test;
pub mod words;

pub use theme::Theme;

use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Block;

use crate::app::{App, Screen};

/// 可交互的最小终端尺寸；低于它只画「终端太小」。
///
/// 键盘 / 鼠标分发也按它判断（`App::interactive`），保证「画不出来的界面
/// 不会被看不见的按键改到」。
pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 20;

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let theme = app.theme();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        shell::render_too_small(frame, area, &theme);
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg).fg(theme.fg)),
        area,
    );

    match app.screen {
        Screen::Home => home::render_home(app, frame, area, &theme),
        Screen::Practice => practice::render_practice(app, frame, area, &theme),
        Screen::WordTest => word_test::render(app, frame, area, &theme),
        Screen::Words => words::render(app, frame, area, &theme),
        Screen::Flows => flows::render(app, frame, area, &theme),
        Screen::Articles => articles::render_articles(app, frame, area, &theme),
        Screen::ArticlePractice => articles::render_article_practice(app, frame, area, &theme),
        Screen::DictPicker => dict_picker::render(app, frame, area, &theme),
        Screen::Stats => stats::render(app, frame, area, &theme),
        Screen::Settings => settings::render(app, frame, area, &theme),
        Screen::Shortcuts => shortcuts::render(app, frame, area, &theme),
        Screen::Help => help::render(app, frame, area, &theme),
    }

    // 键盘焦点高亮画在页面内容之上、弹层遮罩之下；弹层打开时
    // `focus::targets` 返回空列表，不会留下静止的高亮。
    if let Some(rect) = focus::focused_rect(app, area) {
        focus::paint(frame, rect, &theme);
    }

    let modal_open = app.summary.is_some()
        || app.edit.is_some()
        || app.confirm.is_some()
        || app.word_edit.is_some()
        || app.mode_picker.is_some()
        || app.group_picker.is_some()
        || app.collect_picker.is_some();
    if modal_open {
        // 无 alpha 的终端里用 DIM 模拟遮罩；弹层自身会在 Clear 后恢复高亮。
        for cell in frame.buffer_mut().content.iter_mut() {
            cell.modifier.insert(Modifier::DIM);
        }
    }

    if let Some(summary) = &app.summary {
        overlays::render_summary(frame, area, &theme, summary);
    }
    if let Some(edit) = &app.edit {
        overlays::render_edit(frame, area, &theme, edit);
    }
    if let Some(confirm) = &app.confirm {
        overlays::render_confirm(frame, area, &theme, confirm);
    }
    if let Some(edit) = &app.word_edit {
        overlays::render_word_edit(frame, area, &theme, edit);
    }
    if let Some(picker) = &app.mode_picker {
        overlays::render_mode_picker(
            frame,
            area,
            &theme,
            crate::domain::constants::PRACTICE_MODE_NAMES,
            picker.cursor,
            app.boot.settings.word_practice_mode,
        );
    }
    if let Some(picker) = &app.group_picker {
        overlays::render_group_picker(
            frame,
            area,
            &theme,
            app.total_groups(),
            picker.cursor,
            app.current_group(),
        );
    }
    if let Some(picker) = &app.collect_picker {
        overlays::render_collect_picker(
            frame,
            area,
            &theme,
            &app.collectible_dicts(),
            picker.cursor,
        );
    }
}
