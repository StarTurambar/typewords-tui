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

    // 练习中的帮助页整屏显示（不画侧栏），命中区也要按整屏算。
    if app.screen == Screen::Help && app.session.is_some() {
        let content = Rect::new(
            area.x + 2,
            area.y + 1,
            area.width.saturating_sub(4),
            area.height.saturating_sub(1),
        );
        return super::help::targets(app, content);
    }

    // 独占整屏的页面不画侧栏，也不能注册侧栏的命中区——
    // 否则点屏幕上根本没画东西的位置会触发「跳回主页」。
    if is_fullscreen(app.screen) {
        return match app.screen {
            Screen::Practice => super::practice::targets(app, area),
            Screen::ArticlePractice => super::articles::practice_targets(app, area),
            _ => Vec::new(),
        };
    }

    let shell = shell::layout(area, Nav::from_screen(app.screen));
    let mut targets = Vec::new();
    for (screen, rect) in &shell.nav_targets {
        targets.push(Target::new(*rect, Action::Navigate(*screen)));
    }
    if let Some(quit) = shell.quit_target {
        targets.push(Target::new(quit, Action::QuitRequest));
    }

    match app.screen {
        Screen::Home => targets.extend(super::home::targets(app, shell.content)),
        Screen::DictPicker => targets.extend(super::dict_picker::targets(app, shell.content)),
        Screen::Settings => targets.extend(super::settings::targets(app, shell.content)),
        Screen::Shortcuts => targets.extend(super::shortcuts::targets(app, shell.content)),
        Screen::Words => targets.extend(super::words::targets(app, shell.content)),
        // 帮助页正文整体可点返回，但要让出右侧的滚动指示器——
        // 早期实现两者共用同一块矩形，点 `↓` 反而退出了帮助页。
        Screen::Help => {
            let indicator = super::help::targets(app, shell.content);
            let content = Rect::new(
                shell.content.x,
                shell.content.y,
                shell.content.width.saturating_sub(1),
                shell.content.height,
            );
            // 指示器先注册：命中测试取第一个，指示器优先。
            targets.extend(indicator);
            targets.push(Target::new(content, Action::HelpBack));
        }
        Screen::Stats => targets.extend(super::stats::targets(app, shell.content)),
        Screen::Flows => targets.extend(super::flows::targets(app, shell.content)),
        Screen::Articles => targets.extend(super::articles::targets(app, shell.content)),
        Screen::WordTest => targets.extend(super::word_test::targets(app, shell.content)),
        Screen::Practice | Screen::ArticlePractice => {}
    }
    targets
}

/// 该页面是否独占整屏（不画侧栏 / 顶栏 / 底栏）。
fn is_fullscreen(screen: Screen) -> bool {
    matches!(screen, Screen::Practice | Screen::ArticlePractice)
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
        Screen::Words => Some(Action::WordsMove(delta)),
        Screen::Shortcuts => Some(Action::ShortcutMove(delta)),
        Screen::Flows => Some(Action::FlowMove(delta)),
        Screen::Articles => Some(Action::ArticleBookMove(delta)),
        _ => None,
    }
}
