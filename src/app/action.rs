//! 应用级 Action：键盘、鼠标、测试共用的唯一状态变更入口。

use chrono::Datelike;

use super::{
    App, ConfirmAction, ConfirmFocus, ConfirmState, FocusDir, HelpTab, Screen, SummaryButton,
};

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
    /// 键盘焦点：按方向键移动选中的元素（所有套外壳页面）。
    FocusMove(FocusDir),
    /// 键盘焦点：在导航栏与内容区之间换栏（Tab / Shift+Tab）。
    SwitchFocusPane,
    /// 通用选择：列表 / 卡片光标上移（到顶回绕）。
    SelectUp,
    /// 通用选择：列表 / 卡片光标下移（到底回绕）。
    SelectDown,
    /// 通用选择：切换到左栏（文章页 = 书本列表）。
    SelectLeftPane,
    /// 通用选择：切换到右栏（文章页 = 文章列表）。
    SelectRightPane,
    /// 通用选择：触发当前选中项。
    SelectActivate,
    /// 主页键盘焦点：触发选中的元素；没有选中时沿用「开始练习」。
    ActivateFocus,
    /// 主页键盘焦点：取消选中。
    ClearFocus,
    // 词库
    DictMove(i32),
    SelectDict(usize),
    ActivateDict(usize),
    // 设置
    SettingsMove(i32),
    SelectSetting(usize),
    ActivateSetting(usize),
    // 高级 FSRS
    AdvFsrsMove(i32),
    AdvFsrsSelect(usize),
    AdvFsrsActivate(usize),
    // 统计
    StatsScroll(i32),
    StatsToggleView,
    /// 选中指定日期（日历里点某一天）。
    ///
    /// 键盘在日历里按格子移动，最终也落到「选中某一天」这个绝对动作上。
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
    /// 编辑流程时在当前节点内左右移动步骤光标（h / l）。
    FlowStepMove(i32),
    FlowAddStep,
    FlowRemoveStep,
    FlowToggleShuffle,
    FlowToggleWordLoop,
    // 文章
    OpenArticles,
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
    /// 播放当前题的单词发音（键位表里的 `PlayWordPronunciation`）。
    WordTestSpeak,
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
    /// 键入练习页底部的动作按钮（下标按当前页的按钮表，行优先）。
    ///
    /// 鼠标点击与键盘 `Enter` 共用同一个入口：点哪个按钮就执行哪个，
    /// 并且把选中落到它上面（之后方向键从这里继续移动）。
    TypingButton(usize),
    // 结算
    /// 结算弹层：方向键移动按钮焦点（`-1` 向左/上、`+1` 向右/下，到边停）。
    SummaryMove(i32),
    /// 结算弹层：执行当前选中的按钮。
    SummaryActivate,
    SummaryContinue,
    SummaryRelearn,
    SummaryHome,
    // 确认
    /// 确认弹层：方向键移动按钮焦点（`-1` 向「确认」、`+1` 向「取消」，到边停）。
    ConfirmMove(i32),
    /// 确认弹层：执行当前选中的按钮。
    ConfirmActivate,
    ConfirmAccept,
    ConfirmCancel,
    // 输入
    EditSave,
    EditCancel,
    EditChar(char),
    EditBackspace,
    // 帮助
    /// 帮助页切换分页（快捷键 / 常见问题 / 关于与反馈）。
    HelpSelectTab(HelpTab),
    HelpScroll(i32),
    HelpBack,
}

impl App {
    pub fn apply_action(&mut self, action: Action) {
        let screen_before = self.screen;
        match action {
            Action::Navigate(screen) => self.navigate_to(screen),
            Action::Back => self.back(),
            Action::QuitRequest => {
                self.confirm = Some(ConfirmState::new(
                    super::confirm_message(ConfirmAction::Quit),
                    ConfirmAction::Quit,
                ));
            }
            Action::StartPractice => self.start_practice(),
            Action::StartFreePractice => self.start_free_practice(),
            Action::AdjustDaily(delta) => self.adjust_daily_target(delta),
            Action::FocusMove(direction) => self.move_focus(direction),
            Action::SwitchFocusPane => self.switch_focus_pane(),
            Action::ActivateFocus => self.activate_focus(),
            Action::ClearFocus => self.clear_focus(),
            // 通用选择：把「列表 / 卡片光标」的语义集中在一处，各页面只负责
            // 把自己的光标接进来，方向键行为因此完全一致。
            Action::SelectUp => self.move_selection(-1),
            Action::SelectDown => self.move_selection(1),
            Action::SelectLeftPane => self.switch_selection_pane(true),
            Action::SelectRightPane => self.switch_selection_pane(false),
            Action::SelectActivate => self.activate_selection(),
            Action::DictMove(delta) => self.move_dict_cursor(delta),
            Action::SelectDict(index) => {
                if index < self.dict_entry_count() {
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
            Action::AdvFsrsMove(delta) => self.move_adv_fsrs_cursor(delta),
            Action::AdvFsrsSelect(index) => {
                if index < super::AdvFsrsRow::ALL.len() {
                    self.adv_fsrs_cursor = index;
                }
            }
            Action::AdvFsrsActivate(index) => {
                if index < super::AdvFsrsRow::ALL.len() {
                    self.adv_fsrs_cursor = index;
                    self.activate_adv_fsrs(index);
                }
            }
            Action::StatsScroll(delta) => self.scroll_stats(delta),
            Action::StatsToggleView => self.toggle_stats_view(),
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
            Action::FlowStepMove(delta) => self.move_flow_step_cursor(delta),
            Action::FlowAddStep => self.flow_add_step(),
            Action::FlowRemoveStep => self.flow_remove_step(),
            Action::FlowToggleShuffle => self.flow_toggle_shuffle(),
            Action::FlowToggleWordLoop => self.flow_toggle_word_loop(),
            Action::OpenArticles => self.open_articles(),
            Action::ArticleBookMove(delta) => self.move_article_book(delta),
            Action::ArticleMove(delta) => self.move_article(delta),
            Action::ArticleSelect => self.select_article(),
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
            Action::WordTestSpeak => self.word_test_speak(),
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
            Action::TypingButton(index) => self.activate_typing_button(index),
            Action::SummaryMove(delta) => {
                if let Some(summary) = self.summary.as_mut() {
                    summary.focus = summary.focus.shifted(delta);
                }
            }
            // Enter 执行的是「当前选中的按钮」，与鼠标点同一个按钮等价；
            // 结算刚弹出时焦点是「继续」，因此旧的 Enter 语义不变。
            Action::SummaryActivate => match self.summary.as_ref().map(|summary| summary.focus) {
                Some(SummaryButton::Continue) => self.continue_after_summary(),
                Some(SummaryButton::Relearn) => self.relearn_after_summary(),
                Some(SummaryButton::Home) => self.summary_home(),
                None => {}
            },
            Action::SummaryContinue => self.continue_after_summary(),
            Action::SummaryRelearn => self.relearn_after_summary(),
            Action::SummaryHome => self.summary_home(),
            Action::ConfirmMove(delta) => {
                if let Some(confirm) = self.confirm.as_mut() {
                    confirm.move_focus(delta);
                }
            }
            // 默认焦点在「确认」，所以 Enter 仍是「确认」；焦点移到「取消」后
            // 再按 Enter 就是取消，不会误执行危险动作。
            Action::ConfirmActivate => match self.confirm.as_ref().map(|confirm| confirm.focus) {
                Some(ConfirmFocus::Accept) => self.confirm_accept(),
                Some(ConfirmFocus::Cancel) => self.confirm = None,
                None => {}
            },
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
                        // 权重是浮点：允许数字与一个小数点（合法区间全部非负，
                        // 因此不需要负号）。
                        crate::app::EditTarget::Weight(_) => {
                            (character.is_ascii_digit()
                                || (character == '.' && !edit.buffer.contains('.')))
                                && edit.buffer.len() < 12
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
            Action::HelpSelectTab(tab) => self.select_help_tab(tab),
            Action::HelpScroll(delta) => self.scroll_help(delta),
            Action::HelpBack => self.help_back(),
        }
        // 切屏路径（navigate_to / open_articles / back / finish_session …）都通过
        // set_screen 迁移焦点；这里兜底处理直接改 screen 的遗漏分支。
        if self.screen != screen_before {
            self.focus_default();
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
                self.set_screen(Screen::Home);
                self.refresh_stats();
                self.refresh_task_preview();
            }
            Screen::DictPicker => {
                self.set_screen(Screen::DictPicker);
                // 光标停在当前学习词典上：列表开头是三本内置词典，停在 0 时
                // 一次 Enter 就会把学习词典换成「收藏」。
                self.dict_cursor = self.dict_entry_of_study().unwrap_or(0);
            }
            Screen::Stats => {
                self.set_screen(Screen::Stats);
                self.stats_scroll = 0;
            }
            Screen::Settings => {
                self.set_screen(Screen::Settings);
                self.settings_cursor = 0;
            }
            Screen::Shortcuts => {
                self.set_screen(Screen::Shortcuts);
                self.shortcut_cursor = 0;
                self.shortcut_capturing = false;
            }
            Screen::AdvFsrs => {
                self.set_screen(Screen::AdvFsrs);
                self.adv_fsrs_cursor = 0;
            }
            Screen::WordTest => {
                self.set_screen(Screen::WordTest);
            }
            Screen::Words => {
                self.set_screen(Screen::Words);
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
                self.set_screen(Screen::Flows);
                self.flow_cursor = 0;
                self.flow_editing = false;
            }
            Screen::ArticlePractice => {
                self.set_screen(Screen::ArticlePractice);
            }
            Screen::Help => {
                // 进帮助页总是从「快捷键」分页开始：上一次停在哪一页不该
                // 影响下一次打开；分页与滚动也一起回到起点。已经在帮助页上
                // （例如点击当前高亮的侧栏项）时保持现状，与 Enter 的行为一致。
                if self.screen != Screen::Help {
                    self.reset_help_state();
                }
                self.set_screen(Screen::Help);
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
            Screen::AdvFsrs => {
                self.save_settings_now();
                self.navigate_to(Screen::Settings);
            }
            Screen::WordTest => self.leave_word_test(),
            Screen::Words => self.navigate_to(Screen::DictPicker),
            Screen::Articles => self.navigate_to(Screen::Home),
            Screen::Flows => self.navigate_to(Screen::Home),
            Screen::ArticlePractice => {
                // 中途退出要保住进度：写一次断点缓存再回文章列表。
                //
                // 这里直接切页，不走 `navigate_to`——后者在存在单词练习
                // 会话时会静默返回，留下「屏幕是跟打页、状态已清空」的
                // 空白死页（既画不出内容，也没有任何按键能退出去）。
                if self.article_practice.is_some() {
                    self.save_article_practice_now();
                    self.article_practice = None;
                }
                self.refresh_article_resume();
                if self.article_list.is_empty() {
                    // 防御：极少数情况下列表没载入过，补一次加载再落地。
                    self.open_articles();
                } else {
                    self.set_screen(Screen::Articles);
                }
            }
            Screen::DictPicker | Screen::Stats => self.navigate_to(Screen::Home),
        }
    }

    /// 退出帮助页：从哪来就回哪去。
    ///
    /// 练习 / 跟打中按 `?` 打开的是整屏帮助（不画外壳），返回时回到对应
    /// 的练习页而不是主页——否则跟打进度还在、人却已经站在主页上了。
    fn help_back(&mut self) {
        let screen = if self.session.is_some() {
            Screen::Practice
        } else if self.article_practice.is_some() {
            Screen::ArticlePractice
        } else {
            Screen::Home
        };
        self.set_screen(screen);
    }

    fn summary_home(&mut self) {
        // 结算路径（`finish_session` / `finish_article_practice`）已经删掉
        // 本轮自己的缓存，这里只清定时器状态。早先这里调 `clear_practice_cache`
        // 会把**另一轮**的断点（尤其是文章跟打）一起删掉。
        self.pending_practice_save = None;
        self.pending_article_save = None;
        self.session = None;
        self.summary = None;
        self.set_screen(Screen::Home);
        self.refresh_task_preview();
        self.refresh_stats();
        // 文章结算结束时 screen 已经是 Home（回主页发生在弹层出现时），
        // set_screen 不会触发切屏分支；这里补上默认选中。
        self.focus_home_default();
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
                // 读不出来的缓存不会被删：提示要说清「没清」而不是「已清空」。
                if self.clear_practice_cache() {
                    self.toast(
                        crate::domain::flow::navigator::NoticeLevel::Success,
                        "已清空练习进度",
                    );
                } else {
                    self.toast(
                        crate::domain::flow::navigator::NoticeLevel::Warning,
                        "练习进度未清空：缓存读不出来，已保留原文件",
                    );
                }
                self.session = None;
                self.stat = crate::domain::types::PracticeState::default();
                self.typing = crate::domain::typing::TypingEngine::new();
            }
            // 成功 / 未清干净的提示由 `clear_all_data` 自己按实际结果弹。
            ConfirmAction::ClearAllData => self.clear_all_data(),
            ConfirmAction::ResetFsrs => self.reset_fsrs_parameters(),
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
        // 权重是浮点、且要按与调度器同源的区间夹取，不走整数路径。
        if let crate::app::EditTarget::Weight(index) = edit.target {
            self.apply_weight_text(index, &edit.buffer);
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
            // 用 `Dict::per_day()` 的夹取结果作基准，而不是原始字段：
            // 主页「每日目标」显示的是夹取后的值，原始目标大于词库词数时
            // （例如 12 个词的词库存着 20），按 - 只是把原始值夹回显示值
            // （20 - 5 = 15 → 夹到 12），界面数字不动，看起来像按键失灵，
            // 而原始值已经被悄悄改掉了。
            let current = dict.per_day() as i64;
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

    /// 通用选择：把「列表 / 卡片光标」的语义集中在一处，各页面只负责把自己的
    /// 光标接进来，方向键行为因此完全一致（↑/↓ 移动、Enter 触发、Esc 返回）。
    ///
    /// 文章页多一维（左栏书本 / 右栏文章），由 `article_pane_left` 区分；
    /// `←`/`→` 换栏。
    pub(crate) fn move_selection(&mut self, delta: i32) {
        match self.screen {
            Screen::DictPicker => self.move_dict_cursor(delta),
            Screen::Settings => self.move_settings_cursor(delta),
            Screen::AdvFsrs => self.move_adv_fsrs_cursor(delta),
            Screen::Flows => self.move_flow_edit_cursor(delta),
            Screen::Words => self.move_words_cursor(delta),
            Screen::Shortcuts => self.move_shortcut_cursor(delta),
            // 文章页：↑/↓ 在书本栏换书、在文章栏选文章；←/→ 切栏。
            Screen::Articles => {
                if self.article_pane_left {
                    self.move_article_book(delta)
                } else {
                    self.move_article(delta)
                }
            }
            _ => {}
        }
    }

    /// `left = true` 进左栏（书本），否则进右栏（文章）。
    ///
    /// 切到书本栏时按 ↑/↓ 换书（并载入该书的文章）；切回文章栏后 ↑/↓ 选文章。
    pub(crate) fn switch_selection_pane(&mut self, left: bool) {
        if self.screen == Screen::Articles {
            self.article_pane_left = left;
        }
    }

    pub(crate) fn activate_selection(&mut self) {
        match self.screen {
            Screen::DictPicker => self.activate_dict(self.dict_cursor),
            Screen::Settings => self.activate_setting(self.settings_cursor),
            Screen::AdvFsrs => self.activate_adv_fsrs(self.adv_fsrs_cursor),
            Screen::Flows => self.activate_flow(),
            Screen::Articles => self.select_article(),
            Screen::Words => self.edit_current_word(),
            Screen::Shortcuts => self.start_shortcut_capture(),
            _ => {}
        }
    }

    /// 非主页内容区一次方向移动；返回页面自己的光标是否真的动了。
    ///
    /// 动不了的那次由调用方按几何决定是否跨到导航栏：宽屏内容区 ←、窄屏内容区
    /// 最顶部 ↑。统计页日历的左边缘（行首没有日期）也停住，正好把跨栏机会
    /// 留给 ←。
    pub(crate) fn move_selection_page(&mut self, direction: FocusDir) -> bool {
        let forward = matches!(direction, FocusDir::Down | FocusDir::Right);
        match (self.screen, direction) {
            (Screen::Articles, FocusDir::Left) => {
                if self.article_pane_left {
                    false
                } else {
                    self.switch_selection_pane(true);
                    true
                }
            }
            (Screen::Articles, FocusDir::Right) => {
                if self.article_pane_left {
                    self.switch_selection_pane(false);
                    true
                } else {
                    false
                }
            }
            (Screen::Articles, FocusDir::Up | FocusDir::Down) => {
                let before = (
                    self.article_book_cursor,
                    self.article_cursor,
                    self.article_pane_left,
                );
                self.move_selection(if forward { 1 } else { -1 });
                before
                    != (
                        self.article_book_cursor,
                        self.article_cursor,
                        self.article_pane_left,
                    )
            }
            (Screen::DictPicker, FocusDir::Up | FocusDir::Down) => {
                let before = self.dict_cursor;
                self.move_selection(if forward { 1 } else { -1 });
                before != self.dict_cursor
            }
            (Screen::Settings, FocusDir::Up | FocusDir::Down) => {
                let before = self.settings_cursor;
                self.move_selection(if forward { 1 } else { -1 });
                before != self.settings_cursor
            }
            (Screen::AdvFsrs, FocusDir::Up | FocusDir::Down) => {
                let before = self.adv_fsrs_cursor;
                self.move_selection(if forward { 1 } else { -1 });
                before != self.adv_fsrs_cursor
            }
            (Screen::Words, FocusDir::Up | FocusDir::Down) => {
                let before = self.words_cursor;
                self.move_selection(if forward { 1 } else { -1 });
                before != self.words_cursor
            }
            (Screen::Flows, FocusDir::Up | FocusDir::Down) => {
                // 流程页有三个光标：列表光标、节点光标（编辑态 ↑/↓）、步骤光标
                // （编辑态 h/l）。只比 flow_cursor 会把「编辑态移动节点」误判成
                // 页面光标没动，窄屏「最顶部 ↑ 进顶栏」就会在非首节点上抢走
                // 焦点；三个一起比才是「页面光标到底动没动」。
                let before = (
                    self.flow_cursor,
                    self.flow_node_cursor,
                    self.flow_step_cursor,
                );
                self.move_selection(if forward { 1 } else { -1 });
                before
                    != (
                        self.flow_cursor,
                        self.flow_node_cursor,
                        self.flow_step_cursor,
                    )
            }
            (Screen::Shortcuts, FocusDir::Up | FocusDir::Down) => {
                let before = self.shortcut_cursor;
                self.move_selection(if forward { 1 } else { -1 });
                before != self.shortcut_cursor
            }
            (Screen::Stats, direction) => self.move_stats_page(direction),
            (Screen::Help, FocusDir::Up | FocusDir::Down) => {
                let before = self.help_scroll;
                self.scroll_help(if forward { 1 } else { -1 });
                before != self.help_scroll
            }
            _ => false,
        }
    }

    /// 统计页内容区的一次方向移动：在日历里按视觉位置选日期。
    ///
    /// 日历每一格就是一个日期，方向键跟着格子走而不是「日期 ±1」：
    ///
    /// - ← / →：同一行相邻的一天。行首（周一）与 1 号左边没有格子，
    ///   停住的那一次由调用方跨到导航栏的「统计」页签；
    /// - ↑ / ↓：同一列的上 / 下一周（±7 天）。第一周上面、最后一周下面
    ///   没有格子，同样停住；
    /// - 不跨月也不回绕：翻月是 `,` / `.` 的职责，方向键只在当前月历里移动。
    ///
    /// 按词典视图没有日历，↑ / ↓ 保持「滚动正文」的旧语义（那里只有一列
    /// 累计数据，没有上下的日期可跳）；左右键无事可做。
    fn move_stats_page(&mut self, direction: FocusDir) -> bool {
        if self.stats_by_dict {
            return match direction {
                FocusDir::Up | FocusDir::Down => {
                    let before = self.stats_scroll;
                    self.scroll_stats(if direction == FocusDir::Down { 1 } else { -1 });
                    before != self.stats_scroll
                }
                FocusDir::Left | FocusDir::Right => false,
            };
        }
        let day = self.stats_day;
        let last = month_last_day((day.year(), day.month()));
        let column = day.weekday().num_days_from_monday();
        let target = match direction {
            FocusDir::Left if column > 0 && day.day() > 1 => day.pred_opt(),
            FocusDir::Right if column < 6 && day.day() < last => day.succ_opt(),
            FocusDir::Up if day.day() > 7 => day.checked_sub_signed(chrono::Duration::days(7)),
            FocusDir::Down if day.day() + 7 <= last => {
                day.checked_add_signed(chrono::Duration::days(7))
            }
            _ => None,
        };
        let Some(target) = target else {
            return false;
        };
        self.select_stats_day(target);
        true
    }

    /// 非主页内容区 Enter：执行页面当前选中项。
    pub(crate) fn activate_page_selection(&mut self) {
        match self.screen {
            // 统计页的内容区是整块仪表盘：Enter 与 `v` 一样切换视图。
            Screen::Stats => self.toggle_stats_view(),
            Screen::Help => self.help_back(),
            _ => self.activate_selection(),
        }
    }

    fn move_dict_cursor(&mut self, delta: i32) {
        let count = self.dict_entry_count();
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

    /// 帮助页回到默认分页并滚回顶部（重新打开帮助时用）。
    pub(crate) fn reset_help_state(&mut self) {
        self.help_tab = HelpTab::Shortcuts;
        self.help_scroll = 0;
    }

    /// 切换帮助页分页；换页回到顶部，不把上一页的滚动位置带过去。
    fn select_help_tab(&mut self, tab: HelpTab) {
        if self.help_tab == tab {
            return;
        }
        self.help_tab = tab;
        self.help_scroll = 0;
    }

    fn scroll_help(&mut self, delta: i32) {
        // 上限按当前分页、当前终端尺寸下的实际屏幕行数算，而不是写死 80——
        // 写死的值在条目变多时会截断，条目变少时又会滚过头。三个分页长度
        // 不同、正文还会按折行，必须现算。
        let max = crate::ui::help::max_scroll(self);
        self.help_scroll = scroll_by(self.help_scroll, delta, max);
    }

    fn scroll_stats(&mut self, delta: i32) {
        // 上限 = 总行数 - 视口行数：内容不足一屏时是 0，按不出任何滚动。
        // 旧实现用「总行数 - 1」，没有溢出也能一直按 ↓ 把正文滚成只剩最后
        // 一行；而且行数按写死的 80 列算，窗口一变上限就错。
        let max = crate::ui::stats::max_scroll(self);
        self.stats_scroll = scroll_by(self.stats_scroll, delta, max);
    }

    /// 切换统计页的视图：日历 / 按词典。
    fn toggle_stats_view(&mut self) {
        self.stats_by_dict = !self.stats_by_dict;
        self.stats_scroll = 0;
    }

    /// 选中日历里的某一天（键盘移动与鼠标点击共用）。
    ///
    /// 月份跟着日期走；滚轮可能已经把日历滚出视野，选中后顺手把它滚回来。
    pub fn select_stats_day(&mut self, date: chrono::NaiveDate) {
        self.stats_day = date;
        self.stats_month = (date.year(), date.month());
        let scroll = crate::ui::stats::scroll_to_selected_day(self);
        self.stats_scroll = scroll;
    }

    /// 翻到上 / 下一个月（选中的日期落到新月份的同一天，越界取月末）。
    fn move_stats_month(&mut self, delta: i32) {
        let (year, month) = self.stats_month;
        let total = year as i64 * 12 + (month as i64 - 1) + delta as i64;
        let year = total.div_euclid(12) as i32;
        let month = total.rem_euclid(12) as u32 + 1;
        let day_number = self.stats_day.day();
        let Some(first) = chrono::NaiveDate::from_ymd_opt(year, month, 1) else {
            return;
        };
        let last_day = month_last_day((year, month));
        let next = first.with_day(day_number.min(last_day)).unwrap_or(first);
        self.select_stats_day(next);
    }

    /// 把词库页第 `position` 行设为当前学习词典。
    ///
    /// 内置词典（收藏 / 错词 / 已掌握）与资源词库走两条不同的装载路径：
    /// 前者词条在存档里，改 `studyIndex` 即可；后者要按 id 从词条文件读词表。
    pub fn activate_dict(&mut self, position: usize) {
        let Some(entry) = self.dict_entry_at(position) else {
            return;
        };
        let name = entry.name.clone();
        let switched = match entry.kind {
            crate::app::DictEntryKind::System => {
                self.boot.base.set_study_index(entry.index);
                Ok(())
            }
            crate::app::DictEntryKind::Resource => self.boot.select_dict(&entry.id),
        };
        match switched {
            Ok(()) => {
                self.dict_cursor = position;
                self.save_dict_now();
                // 切换资源词库会经 `normalize_study_progress` 校正进度，
                // 必要时删掉练习断点；不让「上次任务」的缓存结论过期。
                if entry.kind == crate::app::DictEntryKind::Resource {
                    self.refresh_practice_resume();
                }
                self.refresh_task_preview();
                self.set_screen(Screen::Home);
                let message = match entry.kind {
                    crate::app::DictEntryKind::System => format!("已切换到内置词典 {name}"),
                    crate::app::DictEntryKind::Resource => format!("已切换到词库 {name}"),
                };
                self.toast(
                    crate::domain::flow::navigator::NoticeLevel::Success,
                    message,
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
                self.confirm = Some(ConfirmState::new(super::confirm_message(action), action));
            }
            super::SettingKind::Shortcuts => {
                self.open_shortcuts();
                return;
            }
            super::SettingKind::AdvancedFsrs => {
                self.navigate_to(Screen::AdvFsrs);
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

/// 以**实际生效的偏移**为基准做一次滚动：窗口变高后 max 会变小，旧偏移
/// 可能已经越界；若直接从旧值加减，第一次按键会被 clamp 回原位，看起来
/// 像按键失灵。帮助页与统计页共用这一条。
fn scroll_by(current: u16, delta: i32, max: u16) -> u16 {
    let max = max as i32;
    let current = (current as i32).min(max);
    (current + delta).clamp(0, max) as u16
}

/// 某年某月的最后一天（统计页日历边界用）。
fn month_last_day((year, month): (i32, u32)) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|date| date.pred_opt())
        .map(|date| date.day())
        .unwrap_or(28)
}
