//! 应用级 Action：键盘、鼠标、测试共用的唯一状态变更入口。

use chrono::Datelike;

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
    StatsToggleView,
    StatsDayMove(i32),
    /// 选中指定日期（日历里点某一天）。
    ///
    /// `StatsDayMove` 是相对移动，鼠标点的是具体某一天，需要绝对值。
    StatsSelectDay(chrono::NaiveDate),
    StatsMonthMove(i32),
    // 收藏到指定词典
    CollectMove(i32),
    CollectSelect,
    CollectCancel,
    // 练习流程
    OpenFlows,
    FlowMove(i32),
    FlowActivate,
    FlowDuplicate,
    FlowEdit,
    FlowDelete,
    FlowNextNode,
    FlowAddStep,
    FlowRemoveStep,
    FlowToggleShuffle,
    FlowToggleWordLoop,
    // 文章
    OpenArticles,
    ArticleSpeak,
    ArticleBookMove(i32),
    ArticleMove(i32),
    ArticleSelect,
    // 词条管理
    OpenSelectedWords,
    WordsMove(i32),
    WordsSearchChar(char),
    WordsSearchToggle,
    WordsSearchBackspace,
    /// 勾选 / 取消勾选词条。
    ///
    /// 携带被操作的下标：鼠标点的是哪一行就要改哪一行，早期实现用
    /// 「光标所在行」执行，点第 3 行却改动了光标行。
    WordsToggleMark(usize),
    WordsEdit,
    WordsAdd,
    WordsDelete,
    WordsMarkKnow,
    WordsMarkMastered,
    WordsMarkUnknown,
    WordsCopyDict,
    WordsSort,
    WordsNewDict,
    WordsEditField(i32),
    WordsEditChar(char),
    WordsEditBackspace,
    WordsEditSave,
    WordsEditCancel,
    // 单词测试
    WordTestChoose(usize),
    WordTestNext,
    WordTestExit,
    // 练习模式
    OpenModePicker,
    ModeMove(i32),
    ModeSelect,
    ModeCancel,
    // 跳组
    OpenGroupPicker,
    GroupMove(i32),
    GroupSelect,
    GroupCancel,
    // 快捷键
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
            Action::StatsToggleView => self.toggle_stats_view(),
            Action::StatsDayMove(delta) => self.move_stats_day(delta),
            Action::StatsSelectDay(date) => self.select_stats_day(date),
            Action::StatsMonthMove(delta) => self.move_stats_month(delta),
            Action::CollectMove(delta) => self.move_collect_cursor(delta),
            Action::CollectSelect => self.select_collect_dict(),
            Action::CollectCancel => self.collect_picker = None,
            Action::OpenFlows => self.open_flows(),
            Action::FlowMove(delta) => self.move_flow_edit_cursor(delta),
            Action::FlowActivate => self.activate_flow(),
            Action::FlowDuplicate => self.duplicate_flow(),
            Action::FlowEdit => self.toggle_flow_edit(),
            Action::FlowDelete => self.delete_flow(),
            Action::FlowNextNode => self.flow_next_node(),
            Action::FlowAddStep => self.flow_add_step(),
            Action::FlowRemoveStep => self.flow_remove_step(),
            Action::FlowToggleShuffle => self.flow_toggle_shuffle(),
            Action::FlowToggleWordLoop => self.flow_toggle_word_loop(),
            Action::OpenArticles => self.open_articles(),
            Action::ArticleBookMove(delta) => self.move_article_book(delta),
            Action::ArticleMove(delta) => self.move_article(delta),
            Action::ArticleSelect => self.select_article(),
            Action::ArticleSpeak => {
                self.speak_current_sentence();
            }
            Action::OpenSelectedWords => self.open_selected_words(),
            Action::WordsMove(delta) => self.move_words_cursor(delta),
            Action::WordsSearchChar(character) => self.words_search_push(character),
            Action::WordsSearchToggle => self.toggle_words_search(),
            Action::WordsSearchBackspace => self.words_search_backspace(),
            Action::WordsToggleMark(index) => self.words_toggle_mark(index),
            Action::WordsEdit => self.edit_current_word(),
            Action::WordsAdd => self.open_word_edit(None),
            Action::WordsDelete => self.delete_current_word(),
            Action::WordsMarkKnow => self.apply_word_mark(crate::domain::wordbook::MarkKind::Know),
            Action::WordsMarkMastered => {
                self.apply_word_mark(crate::domain::wordbook::MarkKind::Mastered)
            }
            Action::WordsMarkUnknown => {
                self.apply_word_mark(crate::domain::wordbook::MarkKind::Unknown)
            }
            Action::WordsCopyDict => self.copy_study_dict(),
            Action::WordsSort => self.cycle_words_sort(),
            Action::WordsNewDict => self.open_new_dict(),
            Action::WordsEditField(delta) => self.move_word_edit_field(delta),
            Action::WordsEditChar(character) => self.word_edit_push(character),
            Action::WordsEditBackspace => self.word_edit_backspace(),
            Action::WordsEditSave => self.save_word_edit(),
            Action::WordsEditCancel => self.word_edit = None,
            Action::WordTestChoose(index) => self.word_test_choose(index),
            Action::WordTestNext => self.word_test_next(),
            Action::WordTestExit => self.leave_word_test(),
            Action::OpenModePicker => self.open_mode_picker(),
            Action::ModeMove(delta) => self.move_mode_cursor(delta),
            Action::ModeSelect => self.select_practice_mode(),
            Action::ModeCancel => self.mode_picker = None,
            Action::OpenGroupPicker => self.open_group_picker(),
            Action::GroupMove(delta) => self.move_group_cursor(delta),
            Action::GroupSelect => self.select_group(),
            Action::GroupCancel => self.group_picker = None,
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
                if let Some(edit) = self.edit.as_mut() {
                    // 笔记与词典名是自由文本，其余设置项只接受数字
                    let accepted = match edit.target {
                        crate::app::EditTarget::Note | crate::app::EditTarget::DictName => {
                            !character.is_control() && edit.buffer.chars().count() < 200
                        }
                        _ => character.is_ascii_digit() && edit.buffer.len() < 6,
                    };
                    if accepted {
                        edit.buffer.push(character);
                    }
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
        // 离开单词测试页时清掉测试状态：否则状态残留，再切回来会显示
        // 上一轮的题目、分数与用时。
        if screen != Screen::WordTest {
            self.word_test = None;
            self.word_test_spend = 0;
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
            Screen::WordTest => {
                self.screen = Screen::WordTest;
            }
            Screen::Words => {
                self.screen = Screen::Words;
                self.words_cursor = 0;
                self.words_filter.clear();
                self.words_search_focus = false;
                self.words_marked.clear();
            }
            Screen::Articles => {
                // 走 `open_articles`：文章资源是懒加载的，光切 `screen`
                // 会得到一个右边空空的文章页（早期只有主页按 `a` 的那条
                // 路径会载入，侧栏与数字键进去都是空的）。
                self.open_articles();
            }
            Screen::Flows => {
                self.screen = Screen::Flows;
                self.flow_cursor = 0;
                self.flow_editing = false;
            }
            Screen::ArticlePractice => {
                self.screen = Screen::ArticlePractice;
            }
            Screen::Help => {
                self.help_scroll = 0;
                self.screen = Screen::Help;
            }
            Screen::Practice => {}
        }
    }

    pub(crate) fn back(&mut self) {
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
            Screen::WordTest => self.leave_word_test(),
            Screen::Words => self.navigate_to(Screen::DictPicker),
            Screen::Articles => self.navigate_to(Screen::Home),
            Screen::Flows => self.navigate_to(Screen::Home),
            Screen::ArticlePractice => {
                // 中途退出要保住进度：写一次断点缓存再回文章列表。
                self.save_article_practice_now();
                self.article_practice = None;
                self.navigate_to(Screen::Articles);
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
                // 清缓存前先把本轮统计落盘——否则「清空练习进度」会连
                // 已经学过的时长一起抹掉。
                self.flush_practice_stats();
                self.save_dict_now();
                self.clear_practice_cache();
                self.session = None;
                self.stat = crate::domain::types::PracticeState::default();
                self.typing = crate::domain::typing::TypingEngine::new();
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
        if edit.target == crate::app::EditTarget::DictName {
            let name = edit.buffer.trim().to_string();
            if !name.is_empty() {
                self.create_custom_dict(&name, Vec::new());
            }
            return;
        }
        if edit.target == crate::app::EditTarget::Note {
            self.save_word_note(edit.buffer);
            return;
        }
        let value = edit.buffer.parse::<i64>().unwrap_or_default();
        self.apply_number_value(edit.target, value);
    }

    /// 保存（或清空）当前单词的笔记。
    fn save_word_note(&mut self, note: String) {
        let Some(word) = self
            .session
            .as_ref()
            .map(|session| session.current_word().word)
            .filter(|word| !word.is_empty())
        else {
            return;
        };
        let note = note.trim().to_string();
        if note.is_empty() {
            self.boot.base.note_data.remove(&word);
        } else {
            self.boot.base.note_data.insert(word, note);
        }
        self.save_dict_now();
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
        // 上限按帮助页的实际条目数算，而不是写死 80——写死的值在条目
        // 变多时会截断，条目变少时又会滚过头。
        let max = crate::ui::help::max_scroll() as i32;
        self.help_scroll = next.min(max) as u16;
    }

    fn scroll_stats(&mut self, delta: i32) {
        let count = crate::ui::stats::body_line_count(self);
        let max = count.saturating_sub(1) as i32;
        let next = (self.stats_scroll as i32 + delta).clamp(0, max.max(0));
        self.stats_scroll = next as u16;
    }

    /// 切换统计页的视图：日历 / 按词典。
    fn toggle_stats_view(&mut self) {
        self.stats_by_dict = !self.stats_by_dict;
        self.stats_scroll = 0;
    }

    /// 移动日历选中的日期。
    fn move_stats_day(&mut self, delta: i32) {
        let Some(next) = self
            .stats_day
            .checked_add_signed(chrono::Duration::days(delta as i64))
        else {
            return;
        };
        self.stats_day = next;
        self.stats_month = (next.year(), next.month());
    }

    /// 选中日历里的某一天。
    pub fn select_stats_day(&mut self, date: chrono::NaiveDate) {
        self.stats_day = date;
        self.stats_month = (date.year(), date.month());
    }

    /// 翻到上 / 下一个月。
    fn move_stats_month(&mut self, delta: i32) {
        let (year, month) = self.stats_month;
        let total = year as i64 * 12 + (month as i64 - 1) + delta as i64;
        let year = total.div_euclid(12) as i32;
        let month = total.rem_euclid(12) as u32 + 1;
        self.stats_month = (year, month);
        // 选中的日期跟着落到新月份的同一天，越界则取该月最后一天
        let day_number = self.stats_day.day();
        let Some(first) = chrono::NaiveDate::from_ymd_opt(year, month, 1) else {
            return;
        };
        let next_month = if month == 12 {
            chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
        };
        let last_day = next_month
            .and_then(|d| d.pred_opt())
            .map(|d| d.day())
            .unwrap_or(day_number);
        self.stats_day = first.with_day(day_number.min(last_day)).unwrap_or(first);
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
            super::SettingKind::Mode => {
                self.open_mode_picker();
                return;
            }
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
            super::SettingKind::AudioSource => {
                self.boot.settings.tts_source = if self.boot.settings.tts_source == "youdao" {
                    "system".to_string()
                } else {
                    "youdao".to_string()
                };
            }
        }
        self.save_settings_now();
        self.refresh_task_preview();
    }

    pub(crate) fn cycle_theme(&mut self) {
        let ids = self.boot.themes.cycle_ids();
        if ids.is_empty() {
            return;
        }
        let current = self.boot.settings.theme.clone();
        let index = ids.iter().position(|id| *id == current).unwrap_or(0);
        self.boot.settings.theme = ids[(index + 1) % ids.len()].clone();
    }
}
