//! 应用状态机与事件循环。

use std::time::Duration;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use ratatui::crossterm::execute;

use crate::bootstrap::{self, Bootstrapped};
use crate::cli::{self, Cli};
use chrono::Datelike;

use crate::domain::flow::get_flow_config;
use crate::domain::flow::navigator::{Notice, NoticeLevel};
use crate::domain::rng::{self, Rng, shuffle};
use crate::domain::session::{PracticeSession, cache_from_compact, cache_to_compact};
use crate::domain::shortcut::{ShortcutAction, ShortcutMap};
use crate::domain::statistics::DayTotals;
use crate::domain::study_task::{create_shuffle_task, create_study_task};
use crate::domain::timer;
use crate::domain::types::{
    BaseState, Dict, PracticeState, SettingState, TaskWords, WordPracticeMode, WordPracticeType,
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
    WordTest,
    DictPicker,
    Stats,
    Settings,
    Shortcuts,
    Help,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub level: NoticeLevel,
    pub until: i64,
}

/// 跳组弹层的状态。
#[derive(Clone, Debug)]
pub struct GroupPickerState {
    pub cursor: usize,
}

/// 练习模式选择弹层的状态。
#[derive(Clone, Debug)]
pub struct ModePickerState {
    pub cursor: usize,
}

/// 单词测试的进行中状态。
#[derive(Clone, Debug)]
pub struct WordTestState {
    /// 题目词表
    pub words: Vec<crate::domain::types::Word>,
    /// 候选词池（整本词库）
    pub pool: Vec<crate::domain::types::Word>,
    /// 当前题号
    pub index: usize,
    /// 当前题
    pub question: Option<crate::domain::types::Question>,
    /// 已选答案的下标
    pub selected: Option<usize>,
    /// 答对数
    pub correct: usize,
    /// 答错数
    pub wrong: usize,
}

/// 「收藏到指定词典」弹层的状态。
#[derive(Clone, Debug)]
pub struct CollectPickerState {
    pub cursor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditTarget {
    PerDay,
    Ratio,
    Repeat,
    RepeatCustom,
    WaitMs,
    SpaceCooldown,
    EasyLimit,
    GoodLimit,
    HardLimit,
    Retention,
    /// 单词笔记（自由文本，不是数字）
    Note,
    /// 发音倍速
    WordSoundSpeed,
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
    /// 动作行：按下后弹确认框，确认即执行携带的 `ConfirmAction`。
    Action(ConfirmAction),
    /// 入口行：按下后进入快捷键编辑页。
    Shortcuts,
    /// 音频来源：系统 TTS / 有道发音
    AudioSource,
}

pub const SETTING_ROWS: &[(&str, SettingKind)] = &[
    ("word_practice_mode", SettingKind::Mode),
    ("shortcut_settings", SettingKind::Shortcuts),
    ("keyboard_sound", SettingKind::Bool),
    ("effect_sound", SettingKind::Bool),
    ("word_sound", SettingKind::Bool),
    ("tts_source", SettingKind::AudioSource),
    (
        "word_sound_speed",
        SettingKind::Number(EditTarget::WordSoundSpeed),
    ),
    ("theme", SettingKind::Theme),
    ("sound_type", SettingKind::SoundType),
    ("big_word", SettingKind::Bool),
    ("word_review_ratio", SettingKind::Number(EditTarget::Ratio)),
    ("auto_random_review", SettingKind::Bool),
    ("ignore_simple_word", SettingKind::Bool),
    ("auto_next_word", SettingKind::Bool),
    ("repeat_count", SettingKind::Number(EditTarget::Repeat)),
    (
        "repeat_custom_count",
        SettingKind::Number(EditTarget::RepeatCustom),
    ),
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
    (
        "reset_practice_progress",
        SettingKind::Action(ConfirmAction::ResetPractice),
    ),
    (
        "clear_all_data",
        SettingKind::Action(ConfirmAction::ClearAllData),
    ),
];

#[derive(Clone, Debug, Default)]
pub struct SummaryData {
    pub spend_ms: i64,
    pub accuracy: u32,
    pub new_words: usize,
    pub review: usize,
    pub wrong: usize,
    pub last_learn_index: usize,
    pub length: usize,
    pub complete: bool,
    /// 错词 Top10：(单词, 错误次数)，按次数倒序
    pub wrong_top: Vec<(String, i64)>,
    /// 本周周一~周日的学习记录（只统计当前词典，与上游口径一致）
    pub week_days: [Option<DayTotals>; 7],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    Quit,
    /// 只删断点缓存，学习记录与设置不变。
    ResetPractice,
    /// 存档恢复默认：`dict.json` + `setting.json` + 断点缓存，对齐上游「清除所有数据」。
    ClearAllData,
}

/// 确认框文案：与动作一一对应，供键盘与鼠标路径共用。
pub fn confirm_message(action: ConfirmAction) -> String {
    match action {
        ConfirmAction::Quit => "确定退出 Type Words 吗？".to_string(),
        ConfirmAction::ResetPractice => "确定清空当前练习进度吗？".to_string(),
        ConfirmAction::ClearAllData => crate::domain::i18n::tr("clear_all_data_confirm"),
    }
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
    /// 统计页视图：false = 日历，true = 按词典
    pub stats_by_dict: bool,
    /// 统计页日历显示的月份（年, 月）
    pub stats_month: (i32, u32),
    /// 统计页日历选中的日期
    pub stats_day: chrono::NaiveDate,
    pub help_scroll: u16,
    pub show_toolbar: bool,
    pub show_panel: bool,
    pub show_meta: bool,
    /// 快捷键表（由设置派生，改绑后需调用 `refresh_shortcuts`）
    pub shortcuts: ShortcutMap,
    /// 快捷键编辑页的当前行
    pub shortcut_cursor: usize,
    /// 正在捕获新的键位
    pub shortcut_capturing: bool,
    /// 练习模式选择弹层
    pub mode_picker: Option<ModePickerState>,
    /// 单词测试（一题 4 选 1）
    pub word_test: Option<WordTestState>,
    /// 跳组弹层
    pub group_picker: Option<GroupPickerState>,
    /// 「收藏到指定词典」弹层
    pub collect_picker: Option<CollectPickerState>,
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

    /// 从设置重建快捷键表（改绑或载入设置后调用）。
    pub fn refresh_shortcuts(&mut self) {
        self.shortcuts = self.boot.settings.shortcut_map();
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
            stats_by_dict: false,
            stats_month: {
                let today = chrono::Local::now().date_naive();
                (today.year(), today.month())
            },
            stats_day: chrono::Local::now().date_naive(),
            help_scroll: 0,
            show_toolbar: true,
            show_panel: false,
            show_meta: true,
            shortcuts: ShortcutMap::default(),
            shortcut_cursor: 0,
            shortcut_capturing: false,
            mode_picker: None,
            word_test: None,
            group_picker: None,
            collect_picker: None,
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

        app.refresh_shortcuts();
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
        if self.boot.readonly_settings {
            return;
        }
        self.refresh_shortcuts();
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

    /// 清除所有数据：`dict.json` 与 `setting.json` 都回到默认值，并删掉断点缓存。
    ///
    /// 对齐上游 `dataSyncPersistence.clear()`——它把 base store 与 setting store 一起
    /// `setState(getDefault*())` 并清空练习缓存，即「恢复出厂」。因此学习记录、统计、
    /// FSRS 卡片、错词/收藏/已掌握、词典选择、主题与全部设置都会归零。
    ///
    /// 存档版本高于程序支持版本时是只读的（`readonly_*`），此时不清也不落盘，
    /// 否则会在界面上报「清除成功」而重启后数据原样回来。
    pub fn clear_all_data(&mut self) {
        if self.boot.readonly_dict || self.boot.readonly_settings {
            self.toast(NoticeLevel::Error, "存档版本高于当前程序，只读模式无法清除");
            return;
        }
        self.clear_practice_cache();
        self.session = None;
        self.typing = TypingEngine::new();
        self.stat = PracticeState::default();
        self.summary = None;
        self.identify_select = None;
        self.stats_scroll = 0;
        self.dict_cursor = 0;

        self.boot.base = BaseState::default();
        // 上游 clear() 会把 load 置 true 再落盘，这里保持一致。
        self.boot.base.load = true;
        self.boot.settings = SettingState::default();

        // 默认状态没有学习词典（study_index = -1），按全新安装的路径重新选一本。
        if let Err(err) = self.boot.ensure_study_dict() {
            self.fatal = Some(format!("加载词库失败: {err}"));
        }
        self.save_dict_now();
        self.save_settings_now();
        self.refresh_stats();
        self.refresh_task_preview();
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
        // 复用按日聚合：跨天会话的计数只归开始日，避免重复累计。
        let totals = crate::domain::statistics::daily_totals(&self.boot.base)
            .get(&today)
            .copied()
            .unwrap_or_default();
        self.today_spend = totals.spend;
        self.today_new = totals.new_words;
        self.today_review = totals.review;
        self.today_wrong = totals.wrong;
        if self.session.is_some() && self.summary.is_none() {
            self.today_spend += self.stat.spend;
        }
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
        // 单词测试模式走 4 选 1 的答题流程，不进入键入练习
        if crate::domain::constants::is_words_test_mode(mode) {
            self.start_word_test(mode);
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
        let (word_changed, phase_changed) = session.refresh();
        // 阶段与存档里的 `statStoreData.stage` 保持一致（上游同名字段）
        self.stat.stage = session.stage();
        let reset_word = std::mem::take(&mut session.reset_word);
        // 键入引擎里的输入属于「上一个 (单词, 阶段)」，任一变化都必须丢弃：
        // - 进入组内巩固 / 错词巩固时组头可能仍是同一个单词（单词键不变）；
        // - 切换步骤时新词表可能恰好以刚练完的单词开头（同样是单词键不变）；
        // 残留的输入会让当前单词再也无法完成，练习卡在巩固阶段。
        if word_changed || phase_changed || reset_word {
            self.typing.reset();
        }
        session.update_question(&mut self.rng);
        self.identify_select = None;
        if session.complete {
            self.finish_session();
        }
        self.schedule_practice_save();
    }

    /// 空闲自动暂停后，用户一有输入就恢复计时（对齐上游 `usePracticeIdleTimer`）。
    ///
    /// 只有「因空闲自动暂停」才会自动恢复；手动暂停（`Ctrl+P`）保持暂停。
    fn resume_after_idle(&mut self) {
        if self.screen != Screen::Practice || self.session.is_none() {
            return;
        }
        if !self.stat.timer_paused {
            return;
        }
        if self.stat.timer_pause_reason.as_deref() != Some("auto_idle") {
            return;
        }
        timer::resume(&mut self.stat, timer::now_ms());
        self.toast(
            NoticeLevel::Success,
            crate::domain::i18n::tr("timer_resume"),
        );
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
        // 错词 Top10：按本轮错误次数倒序
        let mut wrong_top: Vec<(String, i64)> = session
            .data
            .wrong_times_map
            .iter()
            .map(|(word, times)| (word.clone(), *times))
            .filter(|(_, times)| *times > 0)
            .collect();
        wrong_top.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
        wrong_top.truncate(10);
        // 本周记录：只算当前词典
        let week_days = self
            .boot
            .base
            .study_dict()
            .map(|dict| crate::ui::insights::week_days_for_dict(dict, crate::ui::insights::today()))
            .unwrap_or([None; 7]);
        self.summary = Some(SummaryData {
            spend_ms: self.stat.spend,
            accuracy,
            new_words: self.stat.new_word_number,
            review: self.stat.review_word_number,
            wrong,
            last_learn_index,
            length,
            complete,
            wrong_top,
            week_days,
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
                TypingEvent::Completed => {
                    crate::audio::play_correct(&self.boot.settings);
                    self.advance_word(true);
                }
                TypingEvent::Wrong => {
                    crate::audio::play_wrong(&self.boot.settings);
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

    /// 跳到词表中的第 `index` 个词（点击词表侧栏）。
    fn goto_practice_word(&mut self, index: usize) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if index >= session.data.words.len() {
            return;
        }
        session.data.index = index;
        self.typing.reset();
        session.update_question(&mut self.rng);
        self.identify_select = None;
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

    fn identify_know(&mut self) -> bool {
        if !self.is_identify() {
            return false;
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
        true
    }

    fn identify_unknown(&mut self) -> bool {
        if !self.is_identify() {
            return false;
        }
        if let Some(session) = self.session.as_mut() {
            session.display.show_word_result = true;
            session.on_type_wrong(&mut self.boot.base, &mut self.stat);
        }
        self.schedule_practice_save();
        true
    }

    fn identify_mastered(&mut self) -> bool {
        if !self.is_identify() {
            return false;
        }
        if let Some(session) = self.session.as_mut() {
            session.on_word_mastered(&mut self.boot.base);
        }
        self.typing.reset();
        self.advance_word(false);
        true
    }

    fn identify_choose(&mut self, index: usize) -> bool {
        if !self.is_identify() {
            return false;
        }
        let Some(word) = self.session.as_ref().map(|session| session.current_word()) else {
            return false;
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
            return false;
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
        true
    }

    // ── 练习内动作 ───────────────────────────────────────────────────────

    /// 简洁模式：同时隐藏工具栏与词表面板（上游 `toggleConciseMode`）。
    fn toggle_concise_mode(&mut self) {
        // 只要还有一块可见就进入简洁模式；两块都藏起来时恢复
        let next = !(self.show_toolbar || self.show_panel);
        self.show_toolbar = next;
        self.show_panel = next;
    }

    /// 当前词库每组的词数。
    fn words_per_group(&self) -> usize {
        self.boot
            .base
            .study_dict()
            .map(Dict::per_day)
            .unwrap_or(20)
            .max(1)
    }

    /// 当前学习到第几组（1 起）。
    ///
    /// 练习中的推进量还没写回词典进度，因此要把本轮已学的新词数算进去。
    pub fn current_group(&self) -> usize {
        let per_day = self.words_per_group();
        let learned = self
            .boot
            .base
            .study_dict()
            .map(|dict| dict.last_learn_index)
            .unwrap_or(0)
            + self.stat.new_word_number;
        learned / per_day + 1
    }

    /// 词库一共多少组。
    pub fn total_groups(&self) -> usize {
        let per_day = self.words_per_group();
        let length = self
            .boot
            .base
            .study_dict()
            .map(|dict| dict.length)
            .unwrap_or(0);
        length.div_ceil(per_day).max(1)
    }

    /// 跳到指定组（1 起）重新开始本轮。
    pub fn jump_to_group(&mut self, group: usize) {
        let group = group.clamp(1, self.total_groups());
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let task = session.create_task_from_group(
            &mut self.boot.base,
            &self.boot.settings,
            &mut self.rng,
            group,
        );
        if task.is_empty() {
            self.toast(
                NoticeLevel::Warning,
                crate::domain::i18n::tr("no_words_to_study"),
            );
            return;
        }
        let mode = self.boot.settings.word_practice_mode;
        self.begin_session_with_task(task, mode);
        self.toast(NoticeLevel::Success, format!("已跳到第 {group} 组"));
    }

    /// 重学当前组：按组重建会话，词表回到组首。
    fn repeat_current_group(&mut self) {
        let group = self.current_group();
        self.jump_to_group(group);
    }

    /// 上一组 / 下一组。
    fn shift_group(&mut self, delta: i32) {
        let group = (self.current_group() as i32 + delta).clamp(1, self.total_groups() as i32);
        self.jump_to_group(group as usize);
    }

    /// 默写当前组：按组重建会话，但用默写模式。
    fn dictate_current_group(&mut self) {
        let group = self.current_group();
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let task = session.create_task_from_group(
            &mut self.boot.base,
            &self.boot.settings,
            &mut self.rng,
            group,
        );
        if task.is_empty() {
            self.toast(
                NoticeLevel::Warning,
                crate::domain::i18n::tr("no_words_to_study"),
            );
            return;
        }
        self.begin_session_with_task(task, WordPracticeMode::DictationOnly);
        self.toast(NoticeLevel::Success, format!("第 {group} 组进入默写"));
    }

    // ── 收藏到指定词典 ───────────────────────────────────────────────────

    /// 可收藏的目标词典（收藏 + 自建词典）。
    pub fn collectible_dicts(&self) -> Vec<(usize, String)> {
        self.boot
            .base
            .collectible_dict_indices()
            .into_iter()
            .filter_map(|index| {
                self.boot
                    .base
                    .word
                    .book_list
                    .get(index)
                    .map(|dict| (index, dict.name.clone()))
            })
            .collect()
    }

    fn open_collect_picker(&mut self) {
        let total = self.collectible_dicts().len();
        if total == 0 {
            self.toast(NoticeLevel::Warning, "没有可收藏的词典");
            return;
        }
        self.collect_picker = Some(CollectPickerState { cursor: 0 });
    }

    fn move_collect_cursor(&mut self, delta: i32) {
        let total = self.collectible_dicts().len() as i32;
        if total == 0 {
            return;
        }
        let Some(picker) = self.collect_picker.as_mut() else {
            return;
        };
        let next = (picker.cursor as i32 + delta).clamp(0, total - 1);
        picker.cursor = next as usize;
    }

    /// 把当前词收藏到选中的词典。
    fn select_collect_dict(&mut self) {
        let Some(picker) = self.collect_picker.take() else {
            return;
        };
        let Some((index, name)) = self.collectible_dicts().get(picker.cursor).cloned() else {
            return;
        };
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.toggle_collect_to(&mut self.boot.base, index);
        self.toast(NoticeLevel::Success, format!("已收藏到「{name}」"));
        self.schedule_practice_save();
    }

    // ── 单词测试 ─────────────────────────────────────────────────────────

    /// 开始单词测试：题面是一题 4 选 1。
    ///
    /// 对齐上游 `words-test/[id].vue`：「单词测试」用今日任务的复习词，
    /// 「随机单词测试」用整本词库打乱；题目的候选池始终是整本词库。
    pub fn start_word_test(&mut self, mode: WordPracticeMode) {
        let Some(dict) = self.boot.base.study_dict() else {
            return;
        };
        let pool = dict.words.clone();
        if pool.is_empty() {
            self.toast(NoticeLevel::Warning, "没有可测试的单词");
            return;
        }
        let mut words = match mode {
            WordPracticeMode::ReviewWordsTest => {
                let review = self
                    .session
                    .as_ref()
                    .map(|session| session.task_words.review.clone())
                    .unwrap_or_default();
                if review.is_empty() {
                    let mut all = pool.clone();
                    shuffle(&mut self.rng, &mut all);
                    all
                } else {
                    review
                }
            }
            _ => {
                let mut all = pool.clone();
                shuffle(&mut self.rng, &mut all);
                all
            }
        };
        words.truncate(100);
        let question = words.first().and_then(|word| {
            crate::domain::question::build_question(word, &pool, &mut self.rng, 4)
        });
        self.word_test = Some(WordTestState {
            words,
            pool,
            index: 0,
            question,
            selected: None,
            correct: 0,
            wrong: 0,
        });
        self.screen = Screen::WordTest;
        self.summary = None;
    }

    /// 回答当前题。
    fn word_test_choose(&mut self, index: usize) {
        let Some(state) = self.word_test.as_mut() else {
            return;
        };
        if state.selected.is_some() {
            return;
        }
        let Some(question) = state.question.as_ref() else {
            return;
        };
        if index >= question.candidates.len() {
            return;
        }
        state.selected = Some(index);
        if index == question.correct_index {
            state.correct += 1;
            return;
        }
        state.wrong += 1;
        // 答错：把正确答案放进错词本
        let answer = question.candidates[question.correct_index].word.clone();
        if let Some(wrong) = self.boot.base.wrong_mut() {
            if !wrong
                .words
                .iter()
                .any(|item| item.word.eq_ignore_ascii_case(&answer.word))
            {
                wrong.words.push(answer);
            }
            wrong.length = wrong.words.len();
        }
        self.save_dict_now();
    }

    /// 下一题；答完最后一题时给结算。
    fn word_test_next(&mut self) {
        let Some(state) = self.word_test.as_mut() else {
            return;
        };
        if state.selected.is_none() {
            return;
        }
        if state.index + 1 >= state.words.len() {
            self.finish_word_test();
            return;
        }
        state.index += 1;
        state.selected = None;
        let question = state.words.get(state.index).and_then(|word| {
            crate::domain::question::build_question(word, &state.pool, &mut self.rng, 4)
        });
        state.question = question;
    }

    /// 单词测试结算。
    fn finish_word_test(&mut self) {
        let Some(state) = self.word_test.take() else {
            return;
        };
        let total = state.correct + state.wrong;
        let accuracy = if total == 0 {
            100
        } else {
            ((state.correct as f64 / total as f64) * 100.0).round() as u32
        };
        self.summary = Some(SummaryData {
            spend_ms: self.stat.spend,
            accuracy,
            new_words: 0,
            review: total,
            wrong: state.wrong,
            last_learn_index: 0,
            length: total,
            complete: true,
            wrong_top: Vec::new(),
            week_days: [None; 7],
        });
        self.screen = Screen::Practice;
    }

    /// 退出单词测试，回主页。
    pub fn leave_word_test(&mut self) {
        self.word_test = None;
        self.screen = Screen::Home;
        self.refresh_stats();
        self.refresh_task_preview();
    }

    // ── 练习模式选择 ─────────────────────────────────────────────────────

    /// 打开练习模式选择弹层，光标停在当前模式。
    pub fn open_mode_picker(&mut self) {
        let modes = practice_modes();
        let current = self.boot.settings.word_practice_mode;
        let cursor = modes.iter().position(|mode| *mode == current).unwrap_or(0);
        self.mode_picker = Some(ModePickerState { cursor });
    }

    fn move_mode_cursor(&mut self, delta: i32) {
        let total = practice_modes().len() as i32;
        if total == 0 {
            return;
        }
        let Some(picker) = self.mode_picker.as_mut() else {
            return;
        };
        let next = (picker.cursor as i32 + delta).rem_euclid(total);
        picker.cursor = next as usize;
    }

    /// 选中练习模式并保存。
    fn select_practice_mode(&mut self) {
        let Some(picker) = self.mode_picker.take() else {
            return;
        };
        let Some(mode) = practice_modes().get(picker.cursor).copied() else {
            return;
        };
        self.boot.settings.word_practice_mode = mode;
        self.save_settings_now();
        self.refresh_task_preview();
        self.toast(
            NoticeLevel::Success,
            format!(
                "练习模式：{}",
                crate::domain::constants::practice_mode_name(mode)
            ),
        );
    }

    // ── 跳组弹层 ─────────────────────────────────────────────────────────

    /// 打开跳组弹层，光标停在当前组。
    pub fn open_group_picker(&mut self) {
        let cursor = self.current_group().saturating_sub(1);
        self.group_picker = Some(GroupPickerState { cursor });
    }

    fn move_group_cursor(&mut self, delta: i32) {
        let total = self.total_groups() as i32;
        if total == 0 {
            return;
        }
        let Some(picker) = self.group_picker.as_mut() else {
            return;
        };
        let next = (picker.cursor as i32 + delta).clamp(0, total - 1);
        picker.cursor = next as usize;
    }

    /// 跳到弹层选中的组并关闭弹层。
    fn select_group(&mut self) {
        let Some(picker) = self.group_picker.take() else {
            return;
        };
        self.jump_to_group(picker.cursor + 1);
    }

    // ── 快捷键编辑 ───────────────────────────────────────────────────────

    /// 打开快捷键编辑页（从设置页进入）。
    pub fn open_shortcuts(&mut self) {
        self.screen = Screen::Shortcuts;
        self.shortcut_cursor = 0;
        self.shortcut_capturing = false;
    }

    /// 当前光标对应的动作。
    fn current_shortcut_action(&self) -> Option<ShortcutAction> {
        ShortcutAction::ALL.get(self.shortcut_cursor).copied()
    }

    fn move_shortcut_cursor(&mut self, delta: i32) {
        let total = ShortcutAction::ALL.len() as i32;
        if total == 0 {
            return;
        }
        let next = (self.shortcut_cursor as i32 + delta).clamp(0, total - 1);
        self.shortcut_cursor = next as usize;
    }

    fn start_shortcut_capture(&mut self) {
        if self.current_shortcut_action().is_some() {
            self.shortcut_capturing = true;
        }
    }

    /// 捕获到的新键位。
    fn apply_shortcut_capture(&mut self, key: KeyEvent) {
        self.shortcut_capturing = false;
        let Some(action) = self.current_shortcut_action() else {
            return;
        };
        let Some(shortcut) = keymap::to_shortcut_key(key) else {
            self.toast(NoticeLevel::Warning, "该按键无法用作快捷键");
            return;
        };
        // 空白与退格没有意义，直接忽略
        if matches!(
            shortcut.key,
            crate::domain::shortcut::BaseKey::Space | crate::domain::shortcut::BaseKey::Backspace
        ) {
            self.toast(NoticeLevel::Warning, "该按键无法用作快捷键");
            return;
        }
        let mut map = self.shortcuts.clone();
        if let Err(message) = map.set(action, shortcut) {
            self.toast(NoticeLevel::Warning, message);
            return;
        }
        self.boot.settings.shortcut_key_map = map.to_stored();
        self.refresh_shortcuts();
        self.save_settings_now();
        self.toast(
            NoticeLevel::Success,
            format!("「{}」已绑定到 {}", action.label(), shortcut.to_upstream()),
        );
    }

    fn clear_shortcut_binding(&mut self) {
        let Some(action) = self.current_shortcut_action() else {
            return;
        };
        self.boot
            .settings
            .shortcut_key_map
            .remove(action.upstream_name());
        self.refresh_shortcuts();
        self.save_settings_now();
        self.toast(
            NoticeLevel::Info,
            format!("「{}」已清除绑定", action.label()),
        );
    }

    fn reset_shortcut_binding(&mut self) {
        let Some(action) = self.current_shortcut_action() else {
            return;
        };
        let mut map = self.shortcuts.clone();
        map.reset(action);
        self.boot.settings.shortcut_key_map = map.to_stored();
        self.refresh_shortcuts();
        self.save_settings_now();
    }

    fn reset_all_shortcut_bindings(&mut self) {
        let mut map = self.shortcuts.clone();
        map.reset_all();
        self.boot.settings.shortcut_key_map = map.to_stored();
        self.refresh_shortcuts();
        self.save_settings_now();
        self.toast(NoticeLevel::Success, "已恢复全部默认键位");
    }

    // ── 键位分发 ─────────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: KeyEvent) {
        self.last_activity = timer::now_ms();
        self.resume_after_idle();

        // 捕获键位时吞掉所有按键
        if self.shortcut_capturing {
            if key.code == KeyCode::Esc {
                self.shortcut_capturing = false;
            } else {
                self.apply_shortcut_capture(key);
            }
            return;
        }

        if self.edit.is_some()
            || self.confirm.is_some()
            || self.summary.is_some()
            || self.mode_picker.is_some()
            || self.group_picker.is_some()
            || self.collect_picker.is_some()
        {
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

    /// 练习页按键：先查快捷键表，剩下的按键交给键入引擎。
    ///
    /// 快捷键表与上游 `shortcutKeyMap` 同构，动作 → 键位；
    /// TUI 专有键位（暂停计时、退出等）放在 `handle_practice_tui_key`。
    fn on_practice_key(&mut self, key: KeyEvent) {
        if let Some(shortcut) = keymap::to_shortcut_key(key) {
            let actions = self.shortcuts.actions_for(&shortcut);
            for action in actions {
                if self.handle_practice_shortcut(action) {
                    return;
                }
            }
        }
        if self.handle_practice_tui_key(key) {
            return;
        }
        if let Some(input) = key_to_input(&key) {
            crate::audio::play_key(&self.boot.settings);
            self.handle_typing_key(input);
        }
    }

    /// 处理练习页的快捷键动作；返回 `true` 表示该动作已消费按键。
    fn handle_practice_shortcut(&mut self, action: ShortcutAction) -> bool {
        use ShortcutAction::*;
        match action {
            ShowWord => {
                if let Some(session) = self.session.as_mut() {
                    session.show_word(&mut self.boot.base, &mut self.stat);
                }
                true
            }
            Ignore => {
                self.skip_word();
                true
            }
            ToggleCollect => {
                if let Some(session) = self.session.as_mut() {
                    session.toggle_collect(&mut self.boot.base);
                }
                true
            }
            CollectToDict => {
                self.open_collect_picker();
                true
            }
            ToggleSimple => {
                if let Some(session) = self.session.as_mut() {
                    session.toggle_word_simple_for_current(&mut self.boot.base);
                }
                self.typing.reset();
                self.advance_word(false);
                true
            }
            ToggleShowTranslate => {
                if let Some(session) = self.session.as_mut() {
                    session.toggle_translate();
                }
                true
            }
            ToggleDictation => {
                if let Some(session) = self.session.as_mut() {
                    session.toggle_dictation();
                }
                true
            }
            ToggleToolbar => {
                self.show_toolbar = !self.show_toolbar;
                true
            }
            TogglePanel => {
                self.show_panel = !self.show_panel;
                true
            }
            RandomWrite => {
                if let Some(session) = self.session.as_mut() {
                    session.random_write(&mut self.rng);
                }
                self.typing.reset();
                self.after_advance();
                true
            }
            Next => {
                self.advance_word(false);
                true
            }
            Previous => {
                self.prev_word();
                true
            }
            NextStep => {
                self.skip_step();
                true
            }
            RepeatChapter => {
                self.repeat_current_group();
                true
            }
            PreviousChapter => {
                self.shift_group(-1);
                true
            }
            NextChapter => {
                self.shift_group(1);
                true
            }
            DictationChapter => {
                self.dictate_current_group();
                true
            }
            ToggleConciseMode => {
                self.toggle_concise_mode();
                true
            }
            ToggleTheme => {
                self.cycle_theme();
                true
            }
            KnowWord => self.is_identify() && self.identify_know(),
            UnknownWord => self.is_identify() && self.identify_unknown(),
            MasteredWord => self.is_identify() && self.identify_mastered(),
            SelfTestingChooseA => self.identify_choose(0),
            SelfTestingChooseB => self.identify_choose(1),
            SelfTestingChooseC => self.identify_choose(2),
            SelfTestingChooseD => self.identify_choose(3),
            _ => false,
        }
    }

    /// TUI 专有键位：上游没有对应动作，或功能尚未实现。
    fn handle_practice_tui_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('?') => {
                self.help_scroll = 0;
                self.screen = Screen::Help;
                true
            }
            // 播放单词发音（上游语义）
            KeyCode::Char('p') if ctrl => {
                if let Some(word) = self
                    .session
                    .as_ref()
                    .map(|session| session.current_word().word)
                    .filter(|word| !word.is_empty())
                {
                    crate::audio::pronounce(&self.boot.settings, &word);
                }
                true
            }
            // 暂停计时：上游没有手动暂停，这是 TUI 专有键位
            KeyCode::Char(',') if ctrl => {
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
                true
            }
            // 例句 / 词源开关：上游该键位是「编辑文章」
            KeyCode::Char('e') if ctrl => {
                self.show_meta = !self.show_meta;
                true
            }
            // 编辑当前词的笔记：上游在练习页内联编辑，TUI 用输入弹层
            KeyCode::Char('o') if ctrl => {
                let note = self
                    .session
                    .as_ref()
                    .map(|session| session.current_word().word)
                    .and_then(|word| self.boot.base.note_data.get(&word).cloned())
                    .unwrap_or_default();
                self.edit = Some(EditState {
                    target: EditTarget::Note,
                    buffer: note,
                });
                true
            }
            KeyCode::Char('c') if ctrl => {
                self.save_practice_now();
                self.save_dict_now();
                self.leave_practice();
                true
            }
            _ => false,
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
            "keyboard_sound" => settings.keyboard_sound = !settings.keyboard_sound,
            "effect_sound" => settings.effect_sound = !settings.effect_sound,
            "word_sound" => settings.word_sound = !settings.word_sound,
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
            EditTarget::RepeatCustom => settings.repeat_custom_count.unwrap_or(1),
            EditTarget::WaitMs => settings.wait_time_for_change_word,
            EditTarget::SpaceCooldown => settings.space_cooldown_time,
            EditTarget::EasyLimit => settings.fsrs_easy_limit,
            EditTarget::GoodLimit => settings.fsrs_good_limit,
            EditTarget::HardLimit => settings.fsrs_hard_limit,
            EditTarget::Retention => {
                (settings.fsrs_parameters.request_retention * 100.0).round() as i64
            }
            EditTarget::Note => 0,
            EditTarget::WordSoundSpeed => (settings.word_sound_speed * 100.0).round() as i64,
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
                    EditTarget::RepeatCustom => {
                        settings.repeat_custom_count = Some(value.clamp(1, 100))
                    }
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
                    EditTarget::Note => {}
                    EditTarget::WordSoundSpeed => {
                        settings.word_sound_speed = (value.clamp(50, 300) as f64) / 100.0
                    }
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
                let spend_before = self.stat.spend;
                timer::tick(&mut self.stat, self.last_activity, now);
                // 只在本次 tick 真的累计了时长且恰好跨过 30 秒整数倍时调度，
                // 避免暂停在整数倍处时每个 tick 重复请求防抖保存。
                if should_schedule_periodic_save(spend_before, self.stat.spend) {
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

fn practice_modes() -> [WordPracticeMode; 9] {
    crate::domain::constants::PRACTICE_MODES
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

/// 练习计时是否在本次 tick 跨过了 30 秒整数倍（用于周期保存）。
fn should_schedule_periodic_save(previous_spend: i64, spend: i64) -> bool {
    spend != previous_spend && spend > 0 && spend % 30_000 == 0
}

#[cfg(test)]
mod tests {
    use super::should_schedule_periodic_save;

    #[test]
    fn periodic_save_triggers_only_when_spend_advances() {
        assert!(should_schedule_periodic_save(29_000, 30_000));
        assert!(should_schedule_periodic_save(59_000, 60_000));
        // 暂停/空闲时 spend 不变，不应反复调度。
        assert!(!should_schedule_periodic_save(30_000, 30_000));
        // 非整数倍不触发。
        assert!(!should_schedule_periodic_save(30_000, 31_000));
        assert!(!should_schedule_periodic_save(0, 0));
    }
}
