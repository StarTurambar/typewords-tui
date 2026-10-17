//! 应用状态机与事件循环。

use std::time::Duration;

use anyhow::Result;
use chrono::TimeZone;
use ratatui::Frame;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use ratatui::crossterm::execute;

use crate::bootstrap::{self, Bootstrapped};
use crate::cli::{self, Cli};
use crate::domain::flow::get_flow_config;
use crate::domain::flow::navigator::{Notice, NoticeLevel};
use crate::domain::rng::{self, Rng, shuffle};
use crate::domain::session::{PracticeSession, cache_from_compact, cache_to_compact};
use crate::domain::study_task::{create_shuffle_task, create_study_task};
use crate::domain::timer;
use crate::domain::types::{
    BaseState, PracticeState, SettingState, TaskWords, WordPracticeMode, WordPracticeType,
};
use crate::domain::typing::{KeyInput, TypingContext, TypingEngine, TypingEvent, TypingNotice};
use crate::paths::Paths;
use crate::store::FileStore;

pub mod action;
pub mod keymap;
pub mod mouse;

pub use action::Action;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Practice,
    DictPicker,
    Stats,
    Settings,
    Help,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub level: NoticeLevel,
    pub until: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditTarget {
    PerDay,
    Ratio,
    Repeat,
    WaitMs,
    SpaceCooldown,
    EasyLimit,
    GoodLimit,
    HardLimit,
    Retention,
}

#[derive(Clone, Debug)]
pub struct EditState {
    pub target: EditTarget,
    pub buffer: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingKind {
    Mode,
    Theme,
    SoundType,
    Bool,
    Number(EditTarget),
    Action,
}

pub const SETTING_ROWS: &[(&str, SettingKind)] = &[
    ("word_practice_mode", SettingKind::Mode),
    ("theme", SettingKind::Theme),
    ("sound_type", SettingKind::SoundType),
    ("big_word", SettingKind::Bool),
    ("word_review_ratio", SettingKind::Number(EditTarget::Ratio)),
    ("auto_random_review", SettingKind::Bool),
    ("ignore_simple_word", SettingKind::Bool),
    ("auto_next_word", SettingKind::Bool),
    ("repeat_count", SettingKind::Number(EditTarget::Repeat)),
    ("wait_time", SettingKind::Number(EditTarget::WaitMs)),
    (
        "space_cooldown",
        SettingKind::Number(EditTarget::SpaceCooldown),
    ),
    ("input_wrong_clear", SettingKind::Bool),
    ("ignore_case", SettingKind::Bool),
    ("show_word_question", SettingKind::Bool),
    ("show_etymology", SettingKind::Bool),
    ("show_near_word", SettingKind::Bool),
    ("show_usage_tips", SettingKind::Bool),
    (
        "fsrs_easy_limit",
        SettingKind::Number(EditTarget::EasyLimit),
    ),
    (
        "fsrs_good_limit",
        SettingKind::Number(EditTarget::GoodLimit),
    ),
    (
        "fsrs_hard_limit",
        SettingKind::Number(EditTarget::HardLimit),
    ),
    (
        "request_retention",
        SettingKind::Number(EditTarget::Retention),
    ),
    ("reset_practice_progress", SettingKind::Action),
];

#[derive(Clone, Debug)]
pub struct SummaryData {
    pub spend_ms: i64,
    pub accuracy: u32,
    pub new_words: usize,
    pub review: usize,
    pub wrong: usize,
    pub last_learn_index: usize,
    pub length: usize,
    pub complete: bool,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Quit,
    ResetPractice,
}

#[derive(Clone, Debug)]
pub struct ConfirmState {
    pub message: String,
    pub action: ConfirmAction,
}

pub struct App {
    pub boot: Bootstrapped,
    pub store: FileStore,
    pub screen: Screen,
    pub session: Option<PracticeSession>,
    pub typing: TypingEngine,
    pub stat: PracticeState,
    pub rng: Rng,
    pub seed: u64,
    pub toasts: Vec<Toast>,
    pub quit: bool,
    pub dict_cursor: usize,
    pub settings_cursor: usize,
    pub edit: Option<EditState>,
    pub confirm: Option<ConfirmState>,
    pub summary: Option<SummaryData>,
    pub identify_select: Option<usize>,
    pub stats_scroll: u16,
    pub help_scroll: u16,
    pub show_toolbar: bool,
    pub show_panel: bool,
    pub show_meta: bool,
    pub task_preview: Option<(usize, usize, usize)>,
    pub today_spend: i64,
    pub today_new: usize,
    pub today_review: usize,
    pub today_wrong: usize,
    pub fatal: Option<String>,
    pub last_activity: i64,
    last_timer_tick: i64,
    pending_practice_save: Option<i64>,
}

impl App {
    pub fn base(&self) -> &BaseState {
        &self.boot.base
    }

    pub fn settings(&self) -> &SettingState {
        &self.boot.settings
    }

    pub fn theme(&self) -> crate::ui::Theme {
        let (spec, _) = self.boot.themes.resolve(&self.boot.settings.theme);
        crate::ui::Theme::from_spec(spec)
    }

    pub fn theme_label(&self) -> String {
        self.boot.themes.label(&self.boot.settings.theme)
    }

    pub fn toast(&mut self, level: NoticeLevel, message: impl Into<String>) {
        let now = timer::now_ms();
        self.toasts.push(Toast {
            message: message.into(),
            level,
            until: now + 3200,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn current_toast(&self, now: i64) -> Option<&Toast> {
        self.toasts.iter().rev().find(|toast| toast.until > now)
    }

    // ── 启动与存档 ───────────────────────────────────────────────────────

    pub fn bootstrap(paths: Paths, cli: Cli) -> Result<Self> {
        let mut boot = bootstrap::load(&paths, cli.dict.as_deref())?;
        if let Some(mode) = cli.mode.as_deref() {
            boot.settings.word_practice_mode = cli::parse_mode(mode)?;
        }
        if let Some(theme) = cli.theme.as_deref() {
            boot.settings.theme = theme.to_string();
        }
        let seed = cli.seed.unwrap_or_else(rng::random_seed);
        let store = boot.store.clone();
        let notices = std::mem::take(&mut boot.notices);
        let now = timer::now_ms();

        let mut app = Self {
            boot,
            store,
            screen: Screen::Home,
            session: None,
            typing: TypingEngine::new(),
            stat: PracticeState::default(),
            rng: rng::seeded(seed),
            seed,
            toasts: Vec::new(),
            quit: false,
            dict_cursor: 0,
            settings_cursor: 0,
            edit: None,
            confirm: None,
            summary: None,
            identify_select: None,
            stats_scroll: 0,
            help_scroll: 0,
            show_toolbar: true,
            show_panel: false,
            show_meta: true,
            task_preview: None,
            today_spend: 0,
            today_new: 0,
            today_review: 0,
            today_wrong: 0,
            fatal: None,
            last_activity: now,
            last_timer_tick: now,
            pending_practice_save: None,
        };

        for notice in notices {
            app.toast(NoticeLevel::Warning, notice);
        }
        let (_, theme_fallback) = app.boot.themes.resolve(&app.boot.settings.theme);
        if theme_fallback {
            app.toast(
                NoticeLevel::Warning,
                format!("主题 {} 不存在，已回退到深色", app.boot.settings.theme),
            );
        }

        if cli.reset_practice {
            let _ = app.store.remove_practice();
            app.toast(NoticeLevel::Success, "已清空练习进度");
        } else {
            app.try_restore_practice();
        }

        if let Err(err) = app.boot.ensure_study_dict() {
            app.fatal = Some(format!("加载词库失败: {err}"));
        }
        app.refresh_stats();
        app.refresh_task_preview();
        Ok(app)
    }

    fn try_restore_practice(&mut self) {
        let loaded = self.store.load_practice();
        if let Some(notice) = loaded.notice() {
            self.toast(NoticeLevel::Warning, notice);
        }
        let Some(compact) = loaded.value else {
            return;
        };
        let Some(dict) = self.boot.base.study_dict() else {
            return;
        };
        let Some(cache) = cache_from_compact(&compact, &dict.words) else {
            return;
        };
        let mode = get_flow_config(&cache.session_snapshot.flow_id).mode;
        let mut all_words = dict.words.clone();
        let mut session = PracticeSession::new(mode);
        shuffle(&mut self.rng, &mut all_words);
        session.all_words = all_words;
        if session.apply_cache(&cache, &mut self.rng, &mut self.stat, timer::now_ms()) {
            self.session = Some(session);
            self.screen = Screen::Practice;
            self.typing.reset();
            self.toast(NoticeLevel::Info, "已恢复上次练习进度");
        }
    }

    pub fn save_dict_now(&mut self) {
        if self.boot.readonly_dict {
            return;
        }
        if let Err(err) = self.store.save_dict(&self.boot.base) {
            self.toast(NoticeLevel::Error, format!("保存词典失败: {err}"));
        }
    }

    pub fn save_settings_now(&mut self) {
        if let Err(err) = self.store.save_settings(&self.boot.settings) {
            self.toast(NoticeLevel::Error, format!("保存设置失败: {err}"));
        }
    }

    pub fn save_practice_now(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let mut cache = session.build_cache();
        cache.stat_store_data = self.stat.clone();
        let compact = cache_to_compact(&cache);
        if let Err(err) = self.store.save_practice(Some(&compact)) {
            self.toast(NoticeLevel::Error, format!("保存练习进度失败: {err}"));
        }
        self.pending_practice_save = None;
    }

    pub fn clear_practice_cache(&mut self) {
        let _ = self.store.remove_practice();
        self.pending_practice_save = None;
    }

    fn schedule_practice_save(&mut self) {
        self.pending_practice_save = Some(timer::now_ms() + 500);
    }

    fn save_all(&mut self) {
        self.save_dict_now();
        self.save_settings_now();
        if self.session.is_some() && self.summary.is_none() {
            self.save_practice_now();
        }
    }

    // ── 统计 / 任务预览 ──────────────────────────────────────────────────

    pub fn refresh_stats(&mut self) {
        let today = chrono::Local::now().date_naive();
        let mut spend = 0i64;
        let mut new_words = 0usize;
        let mut review = 0usize;
        let mut wrong = 0usize;
        for dict in &self.boot.base.word.book_list {
            for item in &dict.statistics {
                let day = chrono::Local
                    .timestamp_millis_opt(item.start_date)
                    .single()
                    .map(|dt| dt.date_naive())
                    .unwrap_or(today);
                if day == today {
                    spend += item.spend;
                    new_words += item.new_words;
                    review += item.review;
                    wrong += item.wrong;
                }
            }
        }
        if self.session.is_some() && self.summary.is_none() {
            spend += self.stat.spend;
        }
        self.today_spend = spend;
        self.today_new = new_words;
        self.today_review = review;
        self.today_wrong = wrong;
    }

    pub fn refresh_task_preview(&mut self) {
        if self.boot.base.study_dict().is_none() {
            self.task_preview = None;
            return;
        }
        let result = create_study_task(&self.boot.base, &self.boot.settings, &mut self.rng);
        self.task_preview = Some((
            result.task_words.new.len(),
            result.task_words.review.len(),
            result.due_review_count,
        ));
    }

    pub fn study_progress(&self) -> u32 {
        self.boot.base.study_progress()
    }

    // ── 练习启动 ─────────────────────────────────────────────────────────

    pub fn start_practice(&mut self) {
        self.start_practice_with_mode(self.boot.settings.word_practice_mode);
    }

    /// 自由练习：不修改 setting，只以 Free 流程启动一次。
    pub fn start_free_practice(&mut self) {
        self.start_practice_with_mode(WordPracticeMode::Free);
    }

    pub fn start_practice_with_mode(&mut self, mode: WordPracticeMode) {
        if self.boot.base.study_dict().is_none() {
            self.toast(NoticeLevel::Warning, crate::domain::i18n::tr("no_assets"));
            return;
        }
        let task = if mode == WordPracticeMode::Shuffle {
            create_shuffle_task(&self.boot.base, &self.boot.settings, None, &mut self.rng)
        } else {
            create_study_task(&self.boot.base, &self.boot.settings, &mut self.rng).task_words
        };
        if task.is_empty() {
            self.toast(
                NoticeLevel::Warning,
                crate::domain::i18n::tr("no_words_to_study"),
            );
            return;
        }
        self.begin_session_with_task(task, mode);
    }

    pub fn begin_session_with_task(&mut self, task: TaskWords, mode: WordPracticeMode) {
        let Some(dict) = self.boot.base.study_dict() else {
            return;
        };
        let mut all_words = dict.words.clone();
        shuffle(&mut self.rng, &mut all_words);
        let mut session = PracticeSession::new(mode);
        session.all_words = all_words;

        self.stat = PracticeState::default();
        session.initialize_task(task, &mut self.rng, &mut self.stat, timer::now_ms());
        self.session = Some(session);
        self.screen = Screen::Practice;
        self.summary = None;
        self.identify_select = None;
        self.typing = TypingEngine::new();
        self.last_activity = timer::now_ms();
        self.last_timer_tick = timer::now_ms();
        self.after_advance();
        self.save_practice_now();
        self.refresh_stats();
    }

    pub fn leave_practice(&mut self) {
        if self.summary.is_none() {
            self.save_practice_now();
            self.save_dict_now();
        }
        self.session = None;
        self.summary = None;
        self.screen = Screen::Home;
        self.typing = TypingEngine::new();
        self.refresh_task_preview();
        self.refresh_stats();
    }

    fn after_advance(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let (word_changed, _phase_changed) = session.refresh();
        if word_changed {
            self.typing.reset();
        }
        session.update_question(&mut self.rng);
        self.identify_select = None;
        if session.complete {
            self.finish_session();
        }
        self.schedule_practice_save();
    }

    fn finish_session(&mut self) {
        if self.summary.is_some() {
            return;
        }
        let now = timer::now_ms();
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.settle(
            &mut self.boot.base,
            &self.boot.settings,
            &mut self.stat,
            now,
        );
        let total = self.stat.total;
        let wrong = self.stat.wrong;
        let accuracy = if total == 0 {
            100
        } else {
            (((total.saturating_sub(wrong)) as f64 / total as f64) * 100.0).round() as u32
        };
        let (last_learn_index, length, complete) = self
            .boot
            .base
            .study_dict()
            .map(|dict| (dict.last_learn_index, dict.length, dict.complete))
            .unwrap_or((0, 0, false));
        self.summary = Some(SummaryData {
            spend_ms: self.stat.spend,
            accuracy,
            new_words: self.stat.new_word_number,
            review: self.stat.review_word_number,
            wrong,
            last_learn_index,
            length,
            complete,
        });
        self.save_dict_now();
        self.clear_practice_cache();
        self.refresh_stats();
        self.refresh_task_preview();
    }

    pub fn continue_after_summary(&mut self) {
        let Some(session) = self.session.as_mut() else {
            self.leave_practice();
            return;
        };
        let was_complete = self
            .summary
            .as_ref()
            .map(|item| item.complete)
            .unwrap_or(false);
        let task = session.create_next_task(
            &mut self.boot.base,
            &self.boot.settings,
            &mut self.rng,
            &self.stat,
            was_complete,
        );
        if task.is_empty() {
            self.toast(
                NoticeLevel::Warning,
                crate::domain::i18n::tr("no_words_to_study"),
            );
            self.leave_practice();
            return;
        }
        let mode = self.boot.settings.word_practice_mode;
        self.begin_session_with_task(task, mode);
    }

    pub fn relearn_after_summary(&mut self) {
        let Some(session) = self.session.as_mut() else {
            self.leave_practice();
            return;
        };
        let task = session.create_repeat_task(&mut self.boot.base, &self.boot.settings, &self.stat);
        let mode = self.boot.settings.word_practice_mode;
        self.begin_session_with_task(task, mode);
    }

    // ── 键入 ─────────────────────────────────────────────────────────────

    fn typing_context_parts(&self) -> Option<(String, WordPracticeType, bool, bool)> {
        let session = self.session.as_ref()?;
        let word = session.current_word();
        if word.word.is_empty() {
            return None;
        }
        let view = session.view_state();
        Some((
            word.word,
            view.practice_type,
            view.is_word_masked,
            session.display.show_word_result,
        ))
    }

    fn typing_settings(&self) -> crate::domain::typing::TypingSettings {
        PracticeSession::typing_settings(&self.boot.settings)
    }

    fn handle_typing_key(&mut self, key: KeyInput) {
        let Some((target, practice_type, is_word_masked, show_word_result)) =
            self.typing_context_parts()
        else {
            return;
        };
        let settings = self.typing_settings();
        let ctx = TypingContext {
            target: &target,
            practice_type,
            is_word_masked,
            show_word_result,
            settings: &settings,
        };
        let now = timer::now_ms();
        let events = self.typing.handle_key(key, &ctx, now);
        self.handle_typing_events(events);
    }

    fn handle_typing_events(&mut self, events: Vec<TypingEvent>) {
        for event in events {
            match event {
                TypingEvent::Completed => self.advance_word(true),
                TypingEvent::Wrong => {
                    if let Some(session) = self.session.as_mut() {
                        session.on_type_wrong(&mut self.boot.base, &mut self.stat);
                    }
                    self.schedule_practice_save();
                }
                TypingEvent::ShowWordResult(value) => {
                    if let Some(session) = self.session.as_mut() {
                        session.display.show_word_result = value;
                    }
                }
                TypingEvent::Notice(notice) => {
                    let message = match notice {
                        TypingNotice::DeleteReinput => {
                            crate::domain::i18n::tr("press_delete_reinput")
                        }
                        TypingNotice::SpaceContinue => {
                            crate::domain::i18n::tr("press_space_continue")
                        }
                    };
                    self.toast(NoticeLevel::Info, message);
                }
                TypingEvent::ResetForRepeat => {
                    self.typing.reset();
                }
            }
        }
    }

    fn advance_word(&mut self, is_typing: bool) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let notices = session.next(
            &mut self.boot.base,
            &mut self.stat,
            &mut self.rng,
            is_typing,
        );
        self.handle_notices(notices);
        self.after_advance();
    }

    fn prev_word(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let notices = session.prev(&self.boot.base, &mut self.stat, &mut self.rng);
        self.handle_notices(notices);
        self.typing.reset();
        self.after_advance();
    }

    fn skip_step(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let notices = session.skip_step(&self.boot.base, &mut self.stat, &mut self.rng);
        self.handle_notices(notices);
        self.typing.reset();
        self.after_advance();
    }

    fn skip_word(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let notices = session.skip(&mut self.boot.base, &mut self.stat, &mut self.rng);
        self.handle_notices(notices);
        self.typing.reset();
        self.after_advance();
    }

    fn handle_notices(&mut self, notices: Vec<Notice>) {
        for notice in notices {
            self.toast(notice.level, notice.message);
        }
    }

    // ── 自测操作 ─────────────────────────────────────────────────────────

    fn is_identify(&self) -> bool {
        self.session
            .as_ref()
            .map(|session| session.practice_type() == WordPracticeType::Identify)
            .unwrap_or(false)
    }

    fn identify_know(&mut self) {
        if !self.is_identify() {
            return;
        }
        let word = self
            .session
            .as_ref()
            .map(|session| session.current_word())
            .unwrap_or_default();
        if let Some(session) = self.session.as_mut() {
            session.display.show_word_result = true;
            session.on_word_know(&mut self.boot.base);
        }
        self.typing.mark_correct(&word.word);
        self.schedule_practice_save();
    }

    fn identify_unknown(&mut self) {
        if !self.is_identify() {
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.display.show_word_result = true;
            session.on_type_wrong(&mut self.boot.base, &mut self.stat);
        }
        self.schedule_practice_save();
    }

    fn identify_mastered(&mut self) {
        if !self.is_identify() {
            return;
        }
        if let Some(session) = self.session.as_mut() {
            session.on_word_mastered(&mut self.boot.base);
        }
        self.typing.reset();
        self.advance_word(false);
    }

    fn identify_choose(&mut self, index: usize) {
        if !self.is_identify() {
            return;
        }
        let Some(word) = self.session.as_ref().map(|session| session.current_word()) else {
            return;
        };
        let correct = self
            .session
            .as_ref()
            .and_then(|session| session.question())
            .map(|question| question.correct_index == index)
            .unwrap_or(false);
        let valid = self
            .session
            .as_ref()
            .and_then(|session| session.question())
            .map(|question| index < question.candidates.len())
            .unwrap_or(false);
        if !valid {
            return;
        }
        self.identify_select = Some(index);
        if let Some(session) = self.session.as_mut() {
            session.display.show_word_result = true;
        }
        if correct {
            if let Some(session) = self.session.as_mut() {
                session.on_word_know(&mut self.boot.base);
            }
            self.typing.mark_correct(&word.word);
        } else if let Some(session) = self.session.as_mut() {
            session.on_type_wrong(&mut self.boot.base, &mut self.stat);
        }
        self.schedule_practice_save();
    }

    // ── 键位分发 ─────────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: KeyEvent) {
        self.last_activity = timer::now_ms();

        if self.edit.is_some() || self.confirm.is_some() || self.summary.is_some() {
            if let Some(action) = keymap::overlay_action(self, key) {
                self.apply_action(action);
            }
            return;
        }

        if self.screen == Screen::Practice {
            self.on_practice_key(key);
            return;
        }

        if let Some(action) = keymap::screen_action(self, key) {
            self.apply_action(action);
        }
    }

    fn on_practice_key(&mut self, key: KeyEvent) {
        let modifiers = key.modifiers;
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        let alt = modifiers.contains(KeyModifiers::ALT);
        let shift = modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Esc => {
                if let Some(session) = self.session.as_mut() {
                    session.show_word(&mut self.boot.base, &mut self.stat);
                }
            }
            KeyCode::Tab => self.skip_word(),
            KeyCode::Enter => {
                if ctrl {
                    self.save_practice_now();
                    self.save_dict_now();
                } else if let Some(session) = self.session.as_mut() {
                    session.toggle_collect(&mut self.boot.base);
                }
            }
            KeyCode::Char('?') => {
                self.help_scroll = 0;
                self.screen = Screen::Help;
            }
            KeyCode::Char('\u{60}') => {
                if let Some(session) = self.session.as_mut() {
                    session.toggle_word_simple_for_current(&mut self.boot.base);
                }
                self.typing.reset();
                self.advance_word(false);
            }
            KeyCode::Char('1') if alt => self.identify_choose(0),
            KeyCode::Char('2') if alt => self.identify_choose(1),
            KeyCode::Char('3') if alt => self.identify_choose(2),
            KeyCode::Char('4') if alt => self.identify_choose(3),
            KeyCode::Char('1') if !ctrl && self.is_identify() => self.identify_know(),
            KeyCode::Char('2') if !ctrl && self.is_identify() => self.identify_unknown(),
            KeyCode::Char('3') if !ctrl && self.is_identify() => self.identify_mastered(),
            KeyCode::Char('z') if ctrl => {
                if let Some(session) = self.session.as_mut() {
                    session.toggle_translate();
                }
            }
            KeyCode::Char('i') if ctrl => {
                if let Some(session) = self.session.as_mut() {
                    session.toggle_dictation();
                }
            }
            KeyCode::Char('p') if ctrl => {
                if self.stat.timer_paused {
                    timer::resume(&mut self.stat, timer::now_ms());
                    self.toast(
                        NoticeLevel::Success,
                        crate::domain::i18n::tr("timer_resume"),
                    );
                } else {
                    timer::pause(&mut self.stat, "manual", timer::now_ms());
                    self.toast(NoticeLevel::Info, crate::domain::i18n::tr("timer_paused"));
                }
            }
            KeyCode::Char('r') if ctrl => {
                if let Some(session) = self.session.as_mut() {
                    session.random_write(&mut self.rng);
                }
                self.typing.reset();
                self.after_advance();
            }
            KeyCode::Char('b') if ctrl => self.show_toolbar = !self.show_toolbar,
            KeyCode::Char('l') if ctrl => self.show_panel = !self.show_panel,
            KeyCode::Char('e') if ctrl => self.show_meta = !self.show_meta,
            KeyCode::Char('c') if ctrl => {
                self.save_practice_now();
                self.save_dict_now();
                self.leave_practice();
            }
            KeyCode::Char('n') if ctrl => self.advance_word(false),
            KeyCode::Char('j') if ctrl => self.skip_step(),
            KeyCode::Right if ctrl => self.advance_word(false),
            KeyCode::Left if ctrl => self.prev_word(),
            KeyCode::Right if shift => self.skip_step(),
            _ => {
                if let Some(input) = key_to_input(&key) {
                    self.handle_typing_key(input);
                }
            }
        }
    }

    fn toggle_setting_bool(&mut self, key: &str) {
        let settings = &mut self.boot.settings;
        match key {
            "auto_random_review" => {
                settings.auto_add_random_review_when_no_due =
                    !settings.auto_add_random_review_when_no_due
            }
            "ignore_simple_word" => settings.ignore_simple_word = !settings.ignore_simple_word,
            "auto_next_word" => settings.auto_next_word = !settings.auto_next_word,
            "input_wrong_clear" => settings.input_wrong_clear = !settings.input_wrong_clear,
            "ignore_case" => settings.ignore_case = !settings.ignore_case,
            "show_word_question" => settings.show_word_question = !settings.show_word_question,
            "show_etymology" => {
                settings.show_etymology_and_rel_words = !settings.show_etymology_and_rel_words
            }
            "show_near_word" => settings.show_near_word = !settings.show_near_word,
            "show_usage_tips" => settings.show_usage_tips = !settings.show_usage_tips,
            "big_word" => settings.tui_big_word = !settings.tui_big_word,
            _ => {}
        }
    }

    pub fn setting_number_value(&self, target: EditTarget) -> i64 {
        let settings = &self.boot.settings;
        match target {
            EditTarget::PerDay => self
                .boot
                .base
                .study_dict()
                .map(|dict| dict.per_day_study_number as i64)
                .unwrap_or(20),
            EditTarget::Ratio => settings.word_review_ratio,
            EditTarget::Repeat => settings.repeat_count,
            EditTarget::WaitMs => settings.wait_time_for_change_word,
            EditTarget::SpaceCooldown => settings.space_cooldown_time,
            EditTarget::EasyLimit => settings.fsrs_easy_limit,
            EditTarget::GoodLimit => settings.fsrs_good_limit,
            EditTarget::HardLimit => settings.fsrs_hard_limit,
            EditTarget::Retention => {
                (settings.fsrs_parameters.request_retention * 100.0).round() as i64
            }
        }
    }

    fn apply_number_value(&mut self, target: EditTarget, value: i64) {
        match target {
            EditTarget::PerDay => {
                if let Some(dict) = self.boot.base.study_dict_mut() {
                    let max = dict.words.len().max(1) as i64;
                    dict.per_day_study_number = value.clamp(1, max) as usize;
                }
                self.save_dict_now();
            }
            _ => {
                let settings = &mut self.boot.settings;
                match target {
                    EditTarget::Ratio => settings.word_review_ratio = value.clamp(0, 10),
                    EditTarget::Repeat => settings.repeat_count = value.clamp(1, 100),
                    EditTarget::WaitMs => {
                        settings.wait_time_for_change_word = value.clamp(0, 10_000)
                    }
                    EditTarget::SpaceCooldown => {
                        settings.space_cooldown_time = value.clamp(0, 10_000)
                    }
                    EditTarget::EasyLimit => settings.fsrs_easy_limit = value.clamp(0, 20),
                    EditTarget::GoodLimit => settings.fsrs_good_limit = value.clamp(0, 20),
                    EditTarget::HardLimit => settings.fsrs_hard_limit = value.clamp(0, 20),
                    EditTarget::Retention => {
                        settings.fsrs_parameters.request_retention =
                            (value.clamp(50, 99) as f64) / 100.0
                    }
                    EditTarget::PerDay => {}
                }
                self.save_settings_now();
            }
        }
        self.refresh_task_preview();
    }

    // ── 定时器 ───────────────────────────────────────────────────────────

    pub fn on_tick(&mut self, now: i64) {
        if self.toasts.iter().all(|toast| toast.until <= now) {
            self.toasts.clear();
        }

        if self.screen == Screen::Practice && self.session.is_some() {
            if now - self.last_timer_tick >= 1000 {
                self.last_timer_tick = now;
                timer::tick(&mut self.stat, self.last_activity, now);
                if self.stat.spend % 30_000 == 0 && self.stat.spend > 0 {
                    self.schedule_practice_save();
                }
            }
            if let Some((target, practice_type, is_word_masked, show_word_result)) =
                self.typing_context_parts()
            {
                let settings = self.typing_settings();
                let ctx = TypingContext {
                    target: &target,
                    practice_type,
                    is_word_masked,
                    show_word_result,
                    settings: &settings,
                };
                let events = self.typing.on_tick(&ctx, now);
                if !events.is_empty() {
                    self.handle_typing_events(events);
                }
            }
        }

        if let Some(deadline) = self.pending_practice_save
            && now >= deadline
        {
            self.save_practice_now();
        }
    }

    // ── 渲染与主循环 ─────────────────────────────────────────────────────

    pub fn render(&self, frame: &mut Frame) {
        crate::ui::render(self, frame);
    }

    pub fn run(mut self) -> Result<()> {
        install_panic_hook();
        let mut terminal = ratatui::init();
        let _ = execute!(std::io::stdout(), EnableMouseCapture);
        let result = self.event_loop(&mut terminal);
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        ratatui::restore();
        result
    }

    fn event_loop(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            self.on_tick(timer::now_ms());
            if self.quit {
                break;
            }
            if event::poll(Duration::from_millis(100))? {
                // 一次 poll 最多处理 64 个事件再重绘，避免鼠标移动事件触发高频重绘。
                let mut drained = 0usize;
                loop {
                    match event::read()? {
                        Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
                        Event::Mouse(mouse) => self.on_mouse(mouse, terminal_area()),
                        Event::Resize(_, _) => {}
                        _ => {}
                    }
                    drained += 1;
                    if drained >= 64 || !event::poll(Duration::from_millis(0))? {
                        break;
                    }
                }
            }
        }
        self.save_all();
        Ok(())
    }
}

fn practice_modes() -> [WordPracticeMode; 7] {
    [
        WordPracticeMode::System,
        WordPracticeMode::Free,
        WordPracticeMode::Review,
        WordPracticeMode::Shuffle,
        WordPracticeMode::IdentifyOnly,
        WordPracticeMode::DictationOnly,
        WordPracticeMode::ListenOnly,
    ]
}

fn key_to_input(key: &KeyEvent) -> Option<KeyInput> {
    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match key.code {
        KeyCode::Backspace => Some(KeyInput::Backspace),
        KeyCode::Char(' ') => Some(KeyInput::Space),
        KeyCode::Char(character) => Some(KeyInput::Char(character)),
        _ => None,
    }
}

fn terminal_area() -> ratatui::layout::Rect {
    ratatui::crossterm::terminal::size()
        .map(|(width, height)| ratatui::layout::Rect::new(0, 0, width, height))
        .unwrap_or(ratatui::layout::Rect::new(0, 0, 80, 24))
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        ratatui::restore();
        default_hook(info);
    }));
}

pub fn run(paths: Paths, cli: Cli) -> Result<()> {
    let app = App::bootstrap(paths, cli)?;
    if let Some(fatal) = &app.fatal {
        anyhow::bail!("{fatal}");
    }
    app.run()
}

/// 供测试使用：不进入终端，直接创建应用。
pub fn build_for_test(paths: Paths, cli: Cli) -> Result<App> {
    App::bootstrap(paths, cli)
}

/// 按自然日聚合已持久化的学习记录：day → (spend, new, review, wrong)。
pub fn aggregate_stats(base: &BaseState) -> Vec<(String, i64, usize, usize, usize)> {
    crate::domain::statistics::daily_totals(base)
        .into_iter()
        .map(|(day, totals)| {
            (
                day.format("%Y-%m-%d").to_string(),
                totals.spend,
                totals.new_words,
                totals.review,
                totals.wrong,
            )
        })
        .collect()
}
