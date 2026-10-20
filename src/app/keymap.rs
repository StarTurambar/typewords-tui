//! 键位映射：非练习页与弹层只产出 Action，不直接改状态。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, App, Screen};
use crate::domain::shortcut::{BaseKey, ShortcutKey};

/// 把终端按键事件转成领域层的键位表示。
///
/// 无法映射的按键（功能键以外的修饰键、媒体键等）返回 `None`。
pub fn to_shortcut_key(key: KeyEvent) -> Option<ShortcutKey> {
    let base = match key.code {
        KeyCode::Char(' ') => BaseKey::Space,
        // 上游 `getShortcutKey` 把字母统一大写，键位表沿用同一约定
        KeyCode::Char(character) => BaseKey::Char(character.to_ascii_uppercase()),
        KeyCode::Enter => BaseKey::Enter,
        KeyCode::Esc => BaseKey::Esc,
        KeyCode::Tab => BaseKey::Tab,
        KeyCode::Backspace => BaseKey::Backspace,
        KeyCode::Delete => BaseKey::Delete,
        KeyCode::Insert => BaseKey::Insert,
        KeyCode::Left => BaseKey::Left,
        KeyCode::Right => BaseKey::Right,
        KeyCode::Up => BaseKey::Up,
        KeyCode::Down => BaseKey::Down,
        KeyCode::Home => BaseKey::Home,
        KeyCode::End => BaseKey::End,
        KeyCode::PageUp => BaseKey::PageUp,
        KeyCode::PageDown => BaseKey::PageDown,
        KeyCode::F(number) => BaseKey::F(number),
        _ => return None,
    };
    Some(ShortcutKey {
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
        key: base,
    })
}

pub fn overlay_action(app: &App, key: KeyEvent) -> Option<Action> {
    if app.edit.is_some() {
        return edit_action(key);
    }
    if app.confirm.is_some() {
        return confirm_action(key);
    }
    if app.summary.is_some() {
        return summary_action(key);
    }
    if app.group_picker.is_some() {
        return group_action(key);
    }
    if app.collect_picker.is_some() {
        return collect_action(key);
    }
    None
}

fn collect_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CollectCancel),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::CollectMove(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::CollectMove(1)),
        KeyCode::Enter => Some(Action::CollectSelect),
        _ => None,
    }
}

fn group_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::GroupCancel),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::GroupMove(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::GroupMove(1)),
        KeyCode::PageUp => Some(Action::GroupMove(-10)),
        KeyCode::PageDown => Some(Action::GroupMove(10)),
        KeyCode::Home => Some(Action::GroupMove(-1000)),
        KeyCode::End => Some(Action::GroupMove(1000)),
        KeyCode::Enter => Some(Action::GroupSelect),
        _ => None,
    }
}

fn edit_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::EditCancel),
        KeyCode::Enter => Some(Action::EditSave),
        KeyCode::Backspace => Some(Action::EditBackspace),
        KeyCode::Char(character) if character.is_ascii_digit() => Some(Action::EditChar(character)),
        _ => None,
    }
}

fn confirm_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => Some(Action::ConfirmAccept),
        _ => Some(Action::ConfirmCancel),
    }
}

fn summary_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter | KeyCode::Char(' ') => Some(Action::SummaryContinue),
        KeyCode::Char('r') => Some(Action::SummaryRelearn),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::SummaryHome),
        _ => None,
    }
}

pub fn screen_action(app: &App, key: KeyEvent) -> Option<Action> {
    if app.screen == Screen::Help {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(Action::HelpScroll(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::HelpScroll(1)),
            KeyCode::PageUp => Some(Action::HelpScroll(-5)),
            KeyCode::PageDown => Some(Action::HelpScroll(5)),
            _ => Some(Action::HelpBack),
        };
    }
    if app.session.is_none()
        && key.modifiers.is_empty()
        && let KeyCode::Char(character @ '1'..='5') = key.code
    {
        let screen = match character {
            '1' => Screen::Home,
            '2' => Screen::DictPicker,
            '3' => Screen::Stats,
            '4' => Screen::Settings,
            _ => Screen::Help,
        };
        return Some(Action::Navigate(screen));
    }

    match app.screen {
        Screen::Home => match key.code {
            KeyCode::Enter => Some(Action::StartPractice),
            KeyCode::Char('f') => Some(Action::StartFreePractice),
            KeyCode::Char('d') => Some(Action::Navigate(Screen::DictPicker)),
            KeyCode::Char('s') => Some(Action::Navigate(Screen::Settings)),
            KeyCode::Char('t') => Some(Action::Navigate(Screen::Stats)),
            KeyCode::Char('m') => Some(Action::CycleMode),
            KeyCode::Char('?') => Some(Action::Navigate(Screen::Help)),
            KeyCode::Char('q') => Some(Action::QuitRequest),
            KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::AdjustDaily(5)),
            KeyCode::Char('-') => Some(Action::AdjustDaily(-5)),
            _ => None,
        },
        Screen::DictPicker => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::Back),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::DictMove(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::DictMove(1)),
            KeyCode::Enter => Some(Action::ActivateDict(app.dict_cursor)),
            KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::AdjustDaily(5)),
            KeyCode::Char('-') => Some(Action::AdjustDaily(-5)),
            _ => None,
        },
        Screen::Stats => match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => Some(Action::Back),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::StatsScroll(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::StatsScroll(1)),
            KeyCode::PageUp => Some(Action::StatsScroll(-5)),
            KeyCode::PageDown => Some(Action::StatsScroll(5)),
            _ => None,
        },
        Screen::Settings => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::Back),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::SettingsMove(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::SettingsMove(1)),
            KeyCode::Enter | KeyCode::Char(' ') => {
                Some(Action::ActivateSetting(app.settings_cursor))
            }
            _ => None,
        },
        Screen::Shortcuts => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::Back),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::ShortcutMove(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::ShortcutMove(1)),
            KeyCode::PageUp => Some(Action::ShortcutMove(-10)),
            KeyCode::PageDown => Some(Action::ShortcutMove(10)),
            KeyCode::Home => Some(Action::ShortcutMove(-100)),
            KeyCode::End => Some(Action::ShortcutMove(100)),
            KeyCode::Enter | KeyCode::Char(' ') => Some(Action::ShortcutCapture),
            KeyCode::Delete => Some(Action::ShortcutClear),
            KeyCode::Char('r') => Some(Action::ShortcutReset),
            KeyCode::Char('R') => Some(Action::ShortcutResetAll),
            _ => None,
        },
        Screen::Help => Some(Action::HelpBack),
        Screen::Practice => None,
    }
}
