//! 键位映射：非练习页与弹层只产出 Action，不直接改状态。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, App, Screen};
use crate::domain::shortcut::{BaseKey, ShortcutAction, ShortcutKey};

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
    if app.mode_picker.is_some() {
        return mode_action(key);
    }
    if app.group_picker.is_some() {
        return group_action(key);
    }
    if app.collect_picker.is_some() {
        return collect_action(key);
    }
    if app.word_edit.is_some() {
        return word_edit_action(key);
    }
    None
}

fn word_edit_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::WordsEditCancel),
        KeyCode::Enter => Some(Action::WordsEditSave),
        KeyCode::Tab | KeyCode::Down => Some(Action::WordsEditField(1)),
        KeyCode::BackTab | KeyCode::Up => Some(Action::WordsEditField(-1)),
        KeyCode::Backspace => Some(Action::WordsEditBackspace),
        // 带 Ctrl / Alt 的组合键不是文本输入：`Ctrl+C` 会被终端报成
        // `Char('c')`，不加判断就会把字母 c 写进词条。
        KeyCode::Char(character) if !has_command_modifier(key) => {
            Some(Action::WordsEditChar(character))
        }
        _ => None,
    }
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

/// 词条管理页的键位。
///
/// 搜索框有焦点时普通按键输入搜索词，`Esc` 结束搜索；没有焦点时按键是动作。
fn words_action(app: &App, key: KeyEvent) -> Option<Action> {
    if app.words_search_focus {
        return match key.code {
            KeyCode::Esc => Some(Action::WordsSearchToggle),
            KeyCode::Backspace => Some(Action::WordsSearchBackspace),
            // 组合键不是搜索词：`Ctrl+C` 会被终端报成 `Char('c')`。
            KeyCode::Char(character) if !has_command_modifier(key) => {
                Some(Action::WordsSearchChar(character))
            }
            _ => None,
        };
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::Back),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::WordsMove(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::WordsMove(1)),
        KeyCode::PageUp => Some(Action::WordsMove(-10)),
        KeyCode::PageDown => Some(Action::WordsMove(10)),
        KeyCode::Home => Some(Action::WordsMove(-100_000)),
        KeyCode::End => Some(Action::WordsMove(100_000)),
        KeyCode::Enter => Some(Action::WordsEdit),
        KeyCode::Char(' ') => Some(Action::WordsToggleMark(
            app.filtered_word_indices()
                .get(app.words_cursor)
                .copied()
                .unwrap_or(usize::MAX),
        )),
        KeyCode::Char('/') => Some(Action::WordsSearchToggle),
        KeyCode::Char('a') => Some(Action::WordsAdd),
        KeyCode::Char('d') => Some(Action::WordsDelete),
        KeyCode::Char('1') => Some(Action::WordsMarkKnow),
        KeyCode::Char('2') => Some(Action::WordsMarkMastered),
        KeyCode::Char('3') => Some(Action::WordsMarkUnknown),
        KeyCode::Char('c') => Some(Action::WordsCopyDict),
        KeyCode::Char('o') => Some(Action::WordsSort),
        KeyCode::Char('n') => Some(Action::WordsNewDict),
        _ => None,
    }
}

fn mode_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::ModeCancel),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::ModeMove(-1)),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::ModeMove(1)),
        KeyCode::Enter => Some(Action::ModeSelect),
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
        // 是否只接受数字由 apply_action 按编辑目标判断。
        // `Ctrl+C` 会被终端报成 `Char('c')`，因此组合键不能当文本。
        KeyCode::Char(character) if !character.is_control() && !has_command_modifier(key) => {
            Some(Action::EditChar(character))
        }
        _ => None,
    }
}

/// 是否是带 `Ctrl` / `Alt` 的组合键。
///
/// 终端把 `Ctrl+C` 报成 `Char('c')` + `CONTROL` 修饰位——只看
/// `KeyCode::Char` 分支会把组合键误当成普通字母。输入类弹层必须用
/// 它把组合键挡在外面。
fn has_command_modifier(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT)
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

/// 单词测试页的选项键位。
///
/// 走键位表（`ChooseA`~`ChooseD` / `WordTestingNext`），用户改绑后新键位
/// 同样生效。默认绑定是 `1`~`4` 与空格，与上游 `words-test/[id].vue` 一致。
fn word_test_shortcut_action(app: &App, key: KeyEvent) -> Option<Action> {
    let shortcut = to_shortcut_key(key)?;
    for action in app.shortcuts.actions_for(&shortcut) {
        let action = match action {
            ShortcutAction::ChooseA => Action::WordTestChoose(0),
            ShortcutAction::ChooseB => Action::WordTestChoose(1),
            ShortcutAction::ChooseC => Action::WordTestChoose(2),
            ShortcutAction::ChooseD => Action::WordTestChoose(3),
            ShortcutAction::WordTestingNext => Action::WordTestNext,
            _ => continue,
        };
        return Some(action);
    }
    None
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
    // 数字切页只在没有会话、且当前页不占用数字键时生效
    let digits_are_page_switches = matches!(
        app.screen,
        Screen::Home | Screen::DictPicker | Screen::Stats | Screen::Settings | Screen::Help
    );
    if app.session.is_none()
        && digits_are_page_switches
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
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::OpenFlows)
            }
            KeyCode::Char('f') => Some(Action::StartFreePractice),
            KeyCode::Char('d') => Some(Action::Navigate(Screen::DictPicker)),
            KeyCode::Char('s') => Some(Action::Navigate(Screen::Settings)),
            KeyCode::Char('t') => Some(Action::Navigate(Screen::Stats)),
            KeyCode::Char('m') => Some(Action::OpenModePicker),
            KeyCode::Char('a') => Some(Action::OpenArticles),
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
            KeyCode::Char('w') => Some(Action::OpenSelectedWords),
            KeyCode::Char('a') => Some(Action::OpenArticles),
            KeyCode::Char('n') => Some(Action::WordsNewDict),
            _ => None,
        },
        Screen::Stats => match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => Some(Action::Back),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::StatsScroll(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::StatsScroll(1)),
            KeyCode::PageUp => Some(Action::StatsScroll(-5)),
            KeyCode::PageDown => Some(Action::StatsScroll(5)),
            KeyCode::Left => Some(Action::StatsDayMove(-1)),
            KeyCode::Right => Some(Action::StatsDayMove(1)),
            KeyCode::Char(',') => Some(Action::StatsMonthMove(-1)),
            KeyCode::Char('.') => Some(Action::StatsMonthMove(1)),
            KeyCode::Char('v') => Some(Action::StatsToggleView),
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
        Screen::Flows => match key.code {
            KeyCode::Esc => {
                if app.flow_editing {
                    Some(Action::FlowEdit)
                } else {
                    Some(Action::Back)
                }
            }
            KeyCode::Up | KeyCode::Char('k') => Some(Action::FlowMove(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::FlowMove(1)),
            KeyCode::Tab => Some(Action::FlowNextNode),
            KeyCode::Enter => Some(Action::FlowActivate),
            KeyCode::Char('n') => Some(Action::FlowDuplicate),
            KeyCode::Char('e') => Some(Action::FlowEdit),
            KeyCode::Delete => Some(Action::FlowDelete),
            KeyCode::Char('a') if app.flow_editing => Some(Action::FlowAddStep),
            KeyCode::Char('d') if app.flow_editing => Some(Action::FlowRemoveStep),
            KeyCode::Char('s') if app.flow_editing => Some(Action::FlowToggleShuffle),
            KeyCode::Char('g') if app.flow_editing => Some(Action::FlowToggleWordLoop),
            _ => None,
        },
        Screen::Articles => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::Back),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => Some(Action::ArticleMove(1)),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => Some(Action::ArticleMove(-1)),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::ArticleBookMove(-1)),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::ArticleBookMove(1)),
            KeyCode::Enter => Some(Action::ArticleSelect),
            _ => None,
        },
        Screen::ArticlePractice => match key.code {
            KeyCode::Esc => Some(Action::Back),
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::ArticleSpeak)
            }
            _ => None,
        },
        Screen::Words => words_action(app, key),
        Screen::WordTest => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::WordTestExit),
            _ => word_test_shortcut_action(app, key),
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
