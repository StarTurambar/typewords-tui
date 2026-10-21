//! 鼠标命中检测：只读布局，返回 Action。

use ratatui::layout::Rect;

use crate::app::{Action, App, Screen};
use crate::ui::shell::{self, Nav};

pub use super::target::Target;

/// 是否有弹层打开。
///
/// 必须与 `ui::mod` 的 `modal_open` 判定保持一致——漏掉一个弹层，鼠标点击
/// 就会穿透到下层页面，把弹层里的编辑对象和下层光标搅乱。
fn modal_open(app: &App) -> bool {
    app.confirm.is_some()
        || app.edit.is_some()
        || app.summary.is_some()
        || app.word_edit.is_some()
        || app.mode_picker.is_some()
        || app.group_picker.is_some()
        || app.collect_picker.is_some()
}

pub fn targets(app: &App, area: Rect) -> Vec<Target> {
    if modal_open(app) {
        return super::overlays::targets(app, area);
    }

    let shell = shell::layout(area, Nav::from_screen(app.screen));
    let mut targets = Vec::new();
    if app.session.is_none() {
        for (screen, rect) in &shell.nav_targets {
            targets.push(Target::new(*rect, Action::Navigate(*screen)));
        }
    }
    if app.session.is_none()
        && let Some(quit) = shell.quit_target
    {
        targets.push(Target::new(quit, Action::QuitRequest));
    }

    match app.screen {
        Screen::Home => targets.extend(super::home::targets(app, shell.content)),
        // 练习页独占整屏，不走 shell 布局
        Screen::Practice => targets.extend(super::practice::targets(app, area)),
        Screen::DictPicker => targets.extend(super::dict_picker::targets(app, shell.content)),
        Screen::Settings => targets.extend(super::settings::targets(app, shell.content)),
        Screen::Shortcuts => targets.extend(super::shortcuts::targets(app, shell.content)),
        Screen::Words => targets.extend(super::words::targets(app, shell.content)),
        Screen::Help => targets.push(Target::new(shell.content, Action::HelpBack)),
        _ => {}
    }
    targets
}

pub fn action_at(app: &App, area: Rect, column: u16, row: u16) -> Option<Action> {
    if modal_open(app) {
        return super::overlays::target_action(app, area, column, row);
    }
    targets(app, area)
        .into_iter()
        .find(|target| target.contains(column, row))
        .map(|target| target.action)
}

pub fn wheel_action(app: &App, _area: Rect, _column: u16, _row: u16, delta: i32) -> Option<Action> {
    if modal_open(app) {
        return None;
    }
    match app.screen {
        Screen::DictPicker => Some(Action::DictMove(delta)),
        Screen::Settings => Some(Action::SettingsMove(delta)),
        Screen::Stats => Some(Action::StatsScroll(delta)),
        Screen::Help => Some(Action::HelpScroll(delta)),
        _ => None,
    }
}
