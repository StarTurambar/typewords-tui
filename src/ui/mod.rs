//! ratatui 渲染层。
//!
//! 分层：`theme`（颜色映射）/ `shell` 与各页面（纯布局 + 渲染）/
//! `hit`（把布局命中转换成 Action）。渲染只读 App，不修改状态。

pub mod bigtext;
pub mod dict_picker;
pub mod help;
pub mod hit;
pub mod home;
pub mod insights;
pub mod overlays;
pub mod practice;
pub mod settings;
pub mod shell;
pub mod stats;
pub mod target;
pub mod theme;
pub mod widgets;

pub use theme::Theme;

use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Block;

use crate::app::{App, Screen};

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let theme = app.theme();
    if area.width < 60 || area.height < 20 {
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
        Screen::DictPicker => dict_picker::render(app, frame, area, &theme),
        Screen::Stats => stats::render(app, frame, area, &theme),
        Screen::Settings => settings::render(app, frame, area, &theme),
        Screen::Help => help::render(app, frame, area, &theme),
    }

    let modal_open = app.summary.is_some() || app.edit.is_some() || app.confirm.is_some();
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
}
