//! 应用级 Action：键盘、鼠标、测试共用的唯一状态变更入口。

use super::{App, ConfirmAction, ConfirmState, Screen};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    // 导航与全局
    Navigate(Screen),
    Back,
    QuitRequest,
    // 主页
    StartPractice,
    StartFreePractice,
    AdjustDaily(i64),
    CycleMode,
    // 词库
    DictMove(i32),
    SelectDict(usize),
    ActivateDict(usize),
    // 设置
    SettingsMove(i32),
    SelectSetting(usize),
    ActivateSetting(usize),
    // 统计
    StatsScroll(i32),
    // 快捷键
    OpenShortcuts,
    ShortcutMove(i32),
    ShortcutCapture,
    ShortcutClear,
    ShortcutReset,
    ShortcutResetAll,
    // 练习
    PracticeGotoWord(usize),
    // 结算
    SummaryContinue,
    SummaryRelearn,
    SummaryHome,
    // 确认
    ConfirmAccept,
    ConfirmCancel,
    // 输入
    EditSave,
    EditCancel,
    EditChar(char),
    EditBackspace,
    // 帮助
    HelpScroll(i32),
    HelpBack,
}

impl App {
    pub fn apply_action(&mut self, action: Action) {
        match action {
            Action::Navigate(screen) => self.navigate_to(screen),
            Action::Back => self.back(),
            Action::QuitRequest => {
                self.confirm = Some(ConfirmState {
                    message: super::confirm_message(ConfirmAction::Quit),
                    action: ConfirmAction::Quit,
                });
            }
            Action::StartPractice => self.start_practice(),
            Action::StartFreePractice => self.start_free_practice(),
            Action::AdjustDaily(delta) => self.adjust_daily_target(delta),
            Action::CycleMode => self.cycle_practice_mode(),
            Action::DictMove(delta) => self.move_dict_cursor(delta),
            Action::SelectDict(index) => {
                if index < self.boot.catalog.resources().len() {
                    self.dict_cursor = index;
                }
            }
            Action::ActivateDict(index) => self.activate_dict(index),
            Action::SettingsMove(delta) => self.move_settings_cursor(delta),
            Action::SelectSetting(index) => {
                if index < super::SETTING_ROWS.len() {
                    self.settings_cursor = index;
                }
            }
            Action::ActivateSetting(index) => {
                if index < super::SETTING_ROWS.len() {
                    self.settings_cursor = index;
                    self.activate_setting(index);
                }
            }
            Action::StatsScroll(delta) => self.scroll_stats(delta),
            Action::OpenShortcuts => self.open_shortcuts(),
            Action::ShortcutMove(delta) => self.move_shortcut_cursor(delta),
            Action::ShortcutCapture => self.start_shortcut_capture(),
            Action::ShortcutClear => self.clear_shortcut_binding(),
            Action::ShortcutReset => self.reset_shortcut_binding(),
            Action::ShortcutResetAll => self.reset_all_shortcut_bindings(),
            Action::PracticeGotoWord(index) => self.goto_practice_word(index),
            Action::SummaryContinue => self.continue_after_summary(),
            Action::SummaryRelearn => self.relearn_after_summary(),
            Action::SummaryHome => self.summary_home(),
            Action::ConfirmAccept => self.confirm_accept(),
            Action::ConfirmCancel => self.confirm = None,
            Action::EditSave => self.edit_save(),
            Action::EditCancel => self.edit = None,
            Action::EditChar(character) => {
                if let Some(edit) = self.edit.as_mut()
                    && character.is_ascii_digit()
                    && edit.buffer.len() < 6
                {
                    edit.buffer.push(character);
                }
            }
            Action::EditBackspace => {
                if let Some(edit) = self.edit.as_mut() {
                    edit.buffer.pop();
                }
            }
            Action::HelpScroll(delta) => self.scroll_help(delta),
            Action::HelpBack => self.help_back(),
        }
    }

    fn navigate_to(&mut self, screen: Screen) {
        if self.session.is_some() {
            return;
        }
        match screen {
            Screen::Home => {
                self.screen = Screen::Home;
                self.refresh_stats();
                self.refresh_task_preview();
            }
            Screen::DictPicker => {
                self.screen = Screen::DictPicker;
                self.dict_cursor = 0;
            }
            Screen::Stats => {
                self.screen = Screen::Stats;
                self.stats_scroll = 0;
            }
            Screen::Settings => {
                self.screen = Screen::Settings;
                self.settings_cursor = 0;
            }
            Screen::Shortcuts => {
                self.screen = Screen::Shortcuts;
                self.shortcut_cursor = 0;
                self.shortcut_capturing = false;
            }
            Screen::Help => {
                self.help_scroll = 0;
                self.screen = Screen::Help;
            }
            Screen::Practice => {}
        }
    }

    fn back(&mut self) {
        if self.confirm.is_some() {
            self.confirm = None;
            return;
        }
        if self.edit.is_some() {
            self.edit = None;
            return;
        }
        if self.summary.is_some() {
            self.summary_home();
            return;
        }
        match self.screen {
            Screen::Home | Screen::Practice => {}
            Screen::Help => self.help_back(),
            Screen::Settings => {
                self.save_settings_now();
                self.navigate_to(Screen::Home);
            }
            Screen::Shortcuts => {
                self.save_settings_now();
                self.navigate_to(Screen::Settings);
            }
            Screen::DictPicker | Screen::Stats => self.navigate_to(Screen::Home),
        }
    }

    fn help_back(&mut self) {
        self.screen = if self.session.is_some() {
            Screen::Practice
        } else {
            Screen::Home
        };
    }

    fn summary_home(&mut self) {
        self.clear_practice_cache();
        self.session = None;
        self.summary = None;
        self.screen = Screen::Home;
        self.refresh_task_preview();
        self.refresh_stats();
    }

    fn confirm_accept(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        match confirm.action {
            ConfirmAction::Quit => {
                self.save_all();
                self.quit = true;
            }
            ConfirmAction::ResetPractice => {
                self.clear_practice_cache();
                self.toast(
                    crate::domain::flow::navigator::NoticeLevel::Success,
                    "已清空练习进度",
                );
            }
            ConfirmAction::ClearAllData => {
                self.clear_all_data();
                self.toast(
                    crate::domain::flow::navigator::NoticeLevel::Success,
                    crate::domain::i18n::tr("clear_success"),
                );
            }
        }
    }

    fn edit_save(&mut self) {
        let Some(edit) = self.edit.take() else {
            return;
        };
        let value = edit.buffer.parse::<i64>().unwrap_or_default();
        self.apply_number_value(edit.target, value);
    }

    /// 供主页/词库页共用。
    pub fn adjust_daily_target(&mut self, delta: i64) {
        let changed = {
            let Some(dict) = self.boot.base.study_dict_mut() else {
                return;
            };
            let max = dict.words.len().max(1) as i64;
            let current = dict.per_day_study_number as i64;
            let next = (current + delta).clamp(1, max);
            if next == current {
                None
            } else {
                dict.per_day_study_number = next as usize;
                Some(next)
            }
        };
        if let Some(next) = changed {
            self.save_dict_now();
            self.refresh_task_preview();
            self.toast(
                crate::domain::flow::navigator::NoticeLevel::Info,
                format!("每日词数：{next}"),
            );
        }
    }

    pub fn cycle_practice_mode(&mut self) {
        let modes = super::practice_modes();
        let current = self.boot.settings.word_practice_mode;
        let index = modes.iter().position(|mode| *mode == current).unwrap_or(0);
        self.boot.settings.word_practice_mode = modes[(index + 1) % modes.len()];
        self.save_settings_now();
        self.refresh_task_preview();
    }

    fn move_dict_cursor(&mut self, delta: i32) {
        let count = self.boot.catalog.resources().len();
        if count == 0 {
            self.dict_cursor = 0;
            return;
        }
        let next = (self.dict_cursor as i32 + delta).clamp(0, count as i32 - 1);
        self.dict_cursor = next as usize;
    }

    fn move_settings_cursor(&mut self, delta: i32) {
        let count = super::SETTING_ROWS.len();
        if count == 0 {
            return;
        }
        let next = (self.settings_cursor as i32 + delta).clamp(0, count as i32 - 1);
        self.settings_cursor = next as usize;
    }

    fn scroll_help(&mut self, delta: i32) {
        let next = (self.help_scroll as i32 + delta).max(0);
        self.help_scroll = next.min(80) as u16;
    }

    fn scroll_stats(&mut self, delta: i32) {
        let count = crate::ui::insights::recent_days(&self.boot.base, 14).len();
        let max = count.saturating_sub(1) as i32;
        let next = (self.stats_scroll as i32 + delta).clamp(0, max.max(0));
        self.stats_scroll = next as u16;
    }

    pub fn activate_dict(&mut self, index: usize) {
        let Some(resource) = self.boot.catalog.resources().get(index).cloned() else {
            return;
        };
        let id = resource.id_string();
        let name = resource.name.clone();
        match self.boot.select_dict(&id) {
            Ok(()) => {
                self.dict_cursor = index;
                self.save_dict_now();
                self.refresh_task_preview();
                self.screen = Screen::Home;
                self.toast(
                    crate::domain::flow::navigator::NoticeLevel::Success,
                    format!("已切换到词库 {name}"),
                );
            }
            Err(err) => self.toast(
                crate::domain::flow::navigator::NoticeLevel::Error,
                format!("切换词库失败: {err}"),
            ),
        }
    }

    fn activate_setting(&mut self, index: usize) {
        let Some((key, kind)) = super::SETTING_ROWS.get(index).copied() else {
            return;
        };
        match kind {
            super::SettingKind::Mode => self.cycle_practice_mode(),
            super::SettingKind::Theme => self.cycle_theme(),
            super::SettingKind::SoundType => {
                self.boot.settings.sound_type = if self.boot.settings.sound_type == "us" {
                    "uk".to_string()
                } else {
                    "us".to_string()
                };
            }
            super::SettingKind::Bool => self.toggle_setting_bool(key),
            super::SettingKind::Number(target) => {
                self.edit = Some(super::EditState {
                    target,
                    buffer: self.setting_number_value(target).to_string(),
                });
            }
            super::SettingKind::Action(action) => {
                self.confirm = Some(ConfirmState {
                    message: super::confirm_message(action),
                    action,
                });
            }
            super::SettingKind::Shortcuts => {
                self.open_shortcuts();
                return;
            }
        }
        self.save_settings_now();
        self.refresh_task_preview();
    }

    fn cycle_theme(&mut self) {
        let ids = self.boot.themes.cycle_ids();
        if ids.is_empty() {
            return;
        }
        let current = self.boot.settings.theme.clone();
        let index = ids.iter().position(|id| *id == current).unwrap_or(0);
        self.boot.settings.theme = ids[(index + 1) % ids.len()].clone();
    }
}
