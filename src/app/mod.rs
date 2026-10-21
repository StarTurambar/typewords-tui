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

use crate::domain::flow::PracticeStepTemplateId;
use crate::domain::flow::get_flow_config;
use crate::domain::flow::navigator::{Notice, NoticeLevel};
use crate::domain::rng::{self, Rng, shuffle};
use crate::domain::session::{PracticeSession, cache_from_compact, cache_to_compact};
use crate::domain::shortcut::{ShortcutAction, ShortcutMap};
use crate::domain::statistics::DayTotals;
use crate::domain::study_task::{create_shuffle_task, create_study_task};
use crate::domain::timer;
use crate::domain::types::{
    BaseState, Dict, PracticeState, SettingState, Statistics, TaskWords, Word, WordPracticeMode,
    WordPracticeType,
};
use crate::domain::typing::{KeyInput, TypingContext, TypingEngine, TypingEvent, TypingNotice};
use crate::paths::Paths;
use crate::store::{FileStore, LoadStatus};

pub mod action;
pub mod keymap;
pub mod mouse;

pub use action::Action;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Practice,
    WordTest,
    Words,
    Articles,
    ArticlePractice,
    Flows,
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

/// 进行中的文章练习。
#[derive(Clone, Debug)]
pub struct ArticlePracticeState {
    pub article_id: i64,
    pub title: String,
    /// 全部句子（段落展开后按顺序）
    pub sentences: Vec<crate::domain::article::Sentence>,
    /// 当前句下标
    pub index: usize,
    /// 跟打状态
    pub typing: crate::domain::article::SentenceTyping,
    /// 已完成的句子数
    pub finished: usize,
    /// 出现错误的句子数
    pub wrong_sentences: usize,
    /// 本轮用时（毫秒）
    pub spend: i64,
}

/// 流程列表项：内置流程 + 自建流程。
#[derive(Clone, Debug)]
pub struct FlowEntry {
    pub id: String,
    pub name: String,
    pub config: crate::domain::flow::PracticeFlowConfig,
    /// 是否是自建流程
    pub custom: bool,
    /// 是否正在使用
    pub active: bool,
}

/// 词条编辑弹层：单词 / 音标 / 释义 / 笔记四个字段。
#[derive(Clone, Debug)]
pub struct WordEditState {
    /// 原词在词表里的下标；`None` 表示新增
    pub index: Option<usize>,
    pub word: String,
    pub phonetic: String,
    pub translation: String,
    pub note: String,
    /// 当前编辑的字段
    pub field: usize,
}

impl WordEditState {
    pub const FIELDS: usize = 4;

    /// 当前字段的可变引用。
    pub fn current(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.word,
            1 => &mut self.phonetic,
            2 => &mut self.translation,
            _ => &mut self.note,
        }
    }
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
    /// 新建词典的名字（自由文本）
    DictName,
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

/// 设置页的行（顺序即展示顺序）。
///
/// 分区由 `src/ui/settings.rs` 的 `SECTIONS` 划分：通用 → 音频 → 复习 →
/// 键入 → 显示 → FSRS → 其他。分区起点用行 key 定位，因此这里的顺序必须
/// 与 `SECTIONS` 里的起点一致（见 `docs/settings.md` 的「分区与顺序」）。
pub const SETTING_ROWS: &[(&str, SettingKind)] = &[
    // 通用
    ("word_practice_mode", SettingKind::Mode),
    ("theme", SettingKind::Theme),
    ("sound_type", SettingKind::SoundType),
    ("big_word", SettingKind::Bool),
    ("shortcut_settings", SettingKind::Shortcuts),
    // 音频
    ("keyboard_sound", SettingKind::Bool),
    ("effect_sound", SettingKind::Bool),
    ("word_sound", SettingKind::Bool),
    ("tts_source", SettingKind::AudioSource),
    (
        "word_sound_speed",
        SettingKind::Number(EditTarget::WordSoundSpeed),
    ),
    // 复习
    ("word_review_ratio", SettingKind::Number(EditTarget::Ratio)),
    ("auto_random_review", SettingKind::Bool),
    ("ignore_simple_word", SettingKind::Bool),
    // 键入
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
    // 显示
    ("show_word_question", SettingKind::Bool),
    ("show_etymology", SettingKind::Bool),
    ("show_near_word", SettingKind::Bool),
    ("show_usage_tips", SettingKind::Bool),
    // FSRS
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
    // 其他
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
    /// 解析后的路径（新建词典、写词条文件时需要）
    pub paths: Option<crate::paths::Paths>,
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
    /// 单词测试本轮的用时（毫秒）
    pub word_test_spend: i64,
    /// 词条管理页的光标（过滤后的下标）
    pub words_cursor: usize,
    /// 词条管理页的搜索词
    pub words_filter: String,
    /// 词条管理页：搜索框是否有焦点（有焦点时普通按键输入搜索词）
    pub words_search_focus: bool,
    /// 词条管理页：批量标记模式下已勾选的词（原始下标）
    pub words_marked: Vec<usize>,
    /// 词条管理页：排序方式
    pub words_sort: crate::domain::wordbook::SortOrder,
    /// 词条编辑弹层
    pub word_edit: Option<WordEditState>,
    /// 文章书本列表（`Assets` → 目录项）
    pub article_books: Vec<crate::domain::article::ArticleBook>,
    /// 当前书本里的文章
    pub article_list: Vec<crate::domain::article::Article>,
    /// 文章书本列表的光标
    pub article_book_cursor: usize,
    /// 文章列表的光标
    pub article_cursor: usize,
    /// 进行中的文章练习
    pub article_practice: Option<ArticlePracticeState>,
    /// 自定义练习流程存档
    pub flow_store: crate::domain::flow::store::FlowStore,
    /// 流程列表的光标
    pub flow_cursor: usize,
    /// 是否在编辑流程结构
    pub flow_editing: bool,
    /// 编辑时所在节点
    pub flow_node_cursor: usize,
    /// 编辑时所在步骤
    pub flow_step_cursor: usize,
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
            paths: Some(paths.clone()),
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
            word_test_spend: 0,
            words_cursor: 0,
            words_filter: String::new(),
            words_search_focus: false,
            words_marked: Vec::new(),
            words_sort: crate::domain::wordbook::SortOrder::Normal,
            word_edit: None,
            article_books: Vec::new(),
            article_list: Vec::new(),
            article_book_cursor: 0,
            article_cursor: 0,
            article_practice: None,
            flow_store: Default::default(),
            flow_cursor: 0,
            flow_editing: false,
            flow_node_cursor: 0,
            flow_step_cursor: 0,
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
        {
            let loaded = app.store.load_flows();
            app.flow_store = loaded.value;
            if let LoadStatus::Corrupt(reason) = loaded.status {
                app.toast(
                    NoticeLevel::Warning,
                    format!("流程存档已损坏，已隔离并恢复默认值: {reason}"),
                );
            }
        }
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
        let flow = match self.flow_store.flows.get(&cache.session_snapshot.flow_id) {
            Some(stored) => stored.config.clone(),
            None => get_flow_config(&cache.session_snapshot.flow_id),
        };
        let mode = flow.mode;
        let mut all_words = dict.words.clone();
        let mut session = PracticeSession::with_flow(mode, flow);
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

    /// 今日任务的词表（不建会话）。
    ///
    /// 单词测试模式要用它取复习词——它不经过练习会话，所以不能从
    /// `self.session` 里拿。
    fn today_task_words(&mut self) -> Option<TaskWords> {
        self.boot.base.study_dict()?;
        Some(create_study_task(&self.boot.base, &self.boot.settings, &mut self.rng).task_words)
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
        // 自定义流程模式需要一个已启用的自建流程（对齐上游的同样提示）
        if mode == WordPracticeMode::Custom && self.flow_store.active().is_none() {
            self.toast(NoticeLevel::Warning, "请先在流程页创建并启用一个自定义流程");
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
        let mut session = PracticeSession::with_flow(mode, self.active_flow(mode));
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
    /// 取「下一个未学词所在的组」：`last_learn_index` 是词典进度，`last_learn_index
    /// / per_day + 1` 就是该词落在第几组——刚好学完一组时指向下一组。
    ///
    /// 早期实现在这里加上了 `stat.new_word_number`（本轮任务的新词总数），
    /// 于是「重学本组」会把当前组算成下一组：正在学第 1 组时按 `Ctrl+Enter`
    /// 会跳到第 2 组的词。练习中的推进量不该影响「当前组」——词典进度要等到
    /// 结算（`settle`）才写回。
    pub fn current_group(&self) -> usize {
        let per_day = self.words_per_group();
        let learned = self
            .boot
            .base
            .study_dict()
            .map(|dict| dict.last_learn_index)
            .unwrap_or(0);
        learned / per_day + 1
    }

    /// 词库一共多少组。
    ///
    /// 词数取 `effective_length()`，与 `Dict::per_day()` 用的是同一个口径——
    /// 早期实现混用了 `length`，两者不一致时组数会算错。
    pub fn total_groups(&self) -> usize {
        let per_day = self.words_per_group();
        let length = self
            .boot
            .base
            .study_dict()
            .map(|dict| dict.effective_length())
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
                // 用**今日任务**的复习词出题（对齐上游 `routeData.taskWords.review`）。
                // 早期实现从 `self.session` 取，而单词测试模式在任何会话创建
                // 之前就调用了它，`session` 必然是 `None`，于是永远退化成整本
                // 词库随机出题。
                let review = self
                    .today_task_words()
                    .map(|task| task.review)
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
        // 单词测试有自己的计时起点，不复用上一轮练习的 `stat.spend`。
        self.word_test_spend = 0;
        self.last_timer_tick = timer::now_ms();
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
        // 单词测试有自己的用时统计，不复用上一轮练习残留的 `stat.spend`。
        let spend = self.word_test_spend;
        self.summary = Some(SummaryData {
            spend_ms: spend,
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
        // 单词测试没有练习会话，结算后要回主页——停在 `Screen::Practice`
        // 会渲染成一个没有会话的空白页，且没有任何退路。
        self.screen = Screen::Home;
    }

    /// 退出单词测试，回主页。
    pub fn leave_word_test(&mut self) {
        self.word_test = None;
        self.screen = Screen::Home;
        self.refresh_stats();
        self.refresh_task_preview();
    }

    // ── 自定义练习流程 ─────────────────────────────────────────────────────

    /// 流程列表：先列内置流程，再列自建流程。
    pub fn flow_entries(&self) -> Vec<FlowEntry> {
        let mut entries: Vec<FlowEntry> = Vec::new();
        for mode in crate::domain::constants::PRACTICE_MODES {
            // `Custom` 不是内置流程，它对应下面列出的已启用自建流程。
            if mode == WordPracticeMode::Custom {
                continue;
            }
            let config = crate::domain::flow::flow_for_mode(mode);
            entries.push(FlowEntry {
                id: config.id.clone(),
                name: crate::domain::constants::practice_mode_name(mode).to_string(),
                active: self.flow_store.active_id.is_empty()
                    && self.flow_store.active().is_none()
                    && config.id == crate::domain::flow::get_flow_id_for_mode(mode),
                config,
                custom: false,
            });
        }
        for (id, flow) in &self.flow_store.flows {
            entries.push(FlowEntry {
                id: id.clone(),
                name: flow.name.clone(),
                config: flow.config.clone(),
                custom: true,
                active: self.flow_store.active_id == *id,
            });
        }
        entries
    }

    /// 打开流程页。
    pub fn open_flows(&mut self) {
        self.screen = Screen::Flows;
        self.flow_cursor = 0;
        self.flow_editing = false;
        self.flow_node_cursor = 0;
        self.flow_step_cursor = 0;
    }

    /// 移动流程列表的光标。
    fn move_flow_cursor(&mut self, delta: i32) {
        let total = self.flow_entries().len() as i32;
        if total == 0 {
            return;
        }
        let next = (self.flow_cursor as i32 + delta).clamp(0, total - 1);
        self.flow_cursor = next as usize;
    }

    /// 启用选中的流程。
    fn activate_flow(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if entry.custom {
            self.flow_store.activate(&entry.id);
            self.save_flow_store();
            self.toast(NoticeLevel::Success, format!("已启用「{}」", entry.name));
        } else {
            self.flow_store.activate("");
            self.save_flow_store();
            self.toast(
                NoticeLevel::Success,
                format!("已切回内置流程「{}」", entry.name),
            );
        }
    }

    /// 以选中的流程为模板新建一份自建副本。
    fn duplicate_flow(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        let id = self.flow_store.unique_custom_id();
        let now = timer::now_ms();
        let mut config = entry.config.clone();
        config.id = id.clone();
        config.mode = crate::domain::types::WordPracticeMode::Custom;
        self.flow_store.upsert(
            &id,
            crate::domain::flow::store::StoredFlow {
                name: format!("{} 副本", entry.name),
                config,
                created_at: now,
                updated_at: now,
            },
        );
        self.flow_store.activate(&id);
        self.save_flow_store();
        self.flow_cursor = self.flow_entries().len().saturating_sub(1);
        self.toast(NoticeLevel::Success, "已新建流程副本");
    }

    /// 删除选中的自建流程。
    fn delete_flow(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if !entry.custom {
            self.toast(NoticeLevel::Warning, "内置流程不能删除");
            return;
        }
        self.flow_store.remove(&entry.id);
        self.save_flow_store();
        self.flow_cursor = self.flow_cursor.saturating_sub(1);
        self.toast(NoticeLevel::Success, "已删除流程");
    }

    /// 进入 / 退出结构编辑。
    fn toggle_flow_edit(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if !entry.custom {
            self.toast(NoticeLevel::Warning, "内置流程不能编辑，请先新建副本");
            return;
        }
        self.flow_editing = !self.flow_editing;
        self.flow_node_cursor = 0;
        self.flow_step_cursor = 0;
    }

    /// 编辑模式下移动光标。
    fn move_flow_edit_cursor(&mut self, delta: i32) {
        if !self.flow_editing {
            self.move_flow_cursor(delta);
            return;
        }
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        let node_count = entry.config.nodes.len() as i32;
        if node_count == 0 {
            return;
        }
        let node = (self.flow_node_cursor as i32 + delta).clamp(0, node_count - 1) as usize;
        self.flow_node_cursor = node;
        let step_count = entry.config.nodes[node].steps.len() as i32;
        self.flow_step_cursor = self.flow_step_cursor.min(step_count.max(1) as usize - 1);
    }

    /// 编辑模式下切换节点。
    fn flow_next_node(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if entry.config.nodes.is_empty() {
            return;
        }
        self.flow_node_cursor = (self.flow_node_cursor + 1) % entry.config.nodes.len();
        self.flow_step_cursor = 0;
    }

    /// 编辑模式下追加一个步骤。
    fn flow_add_step(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if !entry.custom {
            return;
        }
        let node = self
            .flow_node_cursor
            .min(entry.config.nodes.len().saturating_sub(1));
        let Some(flow) = self.flow_store.flows.get_mut(&entry.id) else {
            return;
        };
        let Some(node) = flow.config.nodes.get_mut(node) else {
            return;
        };
        // 在当前步骤之后插入，模板按 跟写 → 拼写 → 自测 → 听写 → 默写 循环
        let template = node
            .steps
            .get(self.flow_step_cursor)
            .map(|step| step.template_id)
            .unwrap_or(PracticeStepTemplateId::FollowWrite);
        let next = match template {
            PracticeStepTemplateId::FollowWrite => PracticeStepTemplateId::Spell,
            PracticeStepTemplateId::Spell => PracticeStepTemplateId::Identify,
            PracticeStepTemplateId::Identify => PracticeStepTemplateId::Listen,
            PracticeStepTemplateId::Listen => PracticeStepTemplateId::Dictation,
            PracticeStepTemplateId::Dictation => PracticeStepTemplateId::FollowWrite,
        };
        let step = crate::domain::flow::PracticeFlowStep {
            template_id: next,
            word_advance: crate::domain::flow::WordAdvance::Increment,
            on_end: Vec::new(),
            shuffle_on_enter: false,
        };
        let index = (self.flow_step_cursor + 1).min(node.steps.len());
        node.steps.insert(index, step);
        self.flow_step_cursor = index;
        self.touch_flow(&entry.id);
    }

    /// 编辑模式下删除当前步骤。
    fn flow_remove_step(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if !entry.custom {
            return;
        }
        let node = self
            .flow_node_cursor
            .min(entry.config.nodes.len().saturating_sub(1));
        let Some(flow) = self.flow_store.flows.get_mut(&entry.id) else {
            return;
        };
        let Some(node) = flow.config.nodes.get_mut(node) else {
            return;
        };
        if node.steps.len() <= 1 {
            self.toast(NoticeLevel::Warning, "每个节点至少保留一个步骤");
            return;
        }
        if self.flow_step_cursor < node.steps.len() {
            node.steps.remove(self.flow_step_cursor);
        }
        self.flow_step_cursor = self.flow_step_cursor.saturating_sub(1);
        self.touch_flow(&entry.id);
    }

    /// 编辑模式下切换当前步骤的「进入时乱序」。
    fn flow_toggle_shuffle(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if !entry.custom {
            return;
        }
        let node = self
            .flow_node_cursor
            .min(entry.config.nodes.len().saturating_sub(1));
        let Some(flow) = self.flow_store.flows.get_mut(&entry.id) else {
            return;
        };
        let Some(node) = flow.config.nodes.get_mut(node) else {
            return;
        };
        if let Some(step) = node.steps.get_mut(self.flow_step_cursor) {
            step.shuffle_on_enter = !step.shuffle_on_enter;
        }
        self.touch_flow(&entry.id);
    }

    /// 编辑模式下切换当前步骤是否「组内巩固」。
    fn flow_toggle_word_loop(&mut self) {
        let Some(entry) = self.flow_entries().get(self.flow_cursor).cloned() else {
            return;
        };
        if !entry.custom {
            return;
        }
        let node = self
            .flow_node_cursor
            .min(entry.config.nodes.len().saturating_sub(1));
        let Some(flow) = self.flow_store.flows.get_mut(&entry.id) else {
            return;
        };
        let Some(node) = flow.config.nodes.get_mut(node) else {
            return;
        };
        if let Some(step) = node.steps.get_mut(self.flow_step_cursor) {
            step.word_advance = match &step.word_advance {
                crate::domain::flow::WordAdvance::WordLoop { .. } => {
                    crate::domain::flow::WordAdvance::Increment
                }
                _ => crate::domain::flow::WordAdvance::WordLoop {
                    group_size: crate::domain::constants::GROUP_SIZE,
                    sub_steps: vec![PracticeStepTemplateId::Spell.into()],
                },
            };
        }
        self.touch_flow(&entry.id);
    }

    /// 标记流程已修改并落盘。
    fn touch_flow(&mut self, id: &str) {
        let now = timer::now_ms();
        if let Some(flow) = self.flow_store.flows.get_mut(id) {
            flow.updated_at = now;
        }
        self.save_flow_store();
    }

    /// 写回流程存档。
    fn save_flow_store(&mut self) {
        if let Err(err) = self.store.save_flows(&self.flow_store) {
            self.toast(NoticeLevel::Error, format!("保存流程失败: {err}"));
        }
    }

    // ── 文章背诵 ─────────────────────────────────────────────────────────

    /// 打开文章书本列表。
    pub fn open_articles(&mut self) {
        let Some(paths) = self.paths.clone() else {
            return;
        };
        match crate::domain::article::load_books(&paths) {
            Ok(books) => {
                self.article_books = books;
                self.article_book_cursor = 0;
                self.article_cursor = 0;
                self.article_list.clear();
                self.screen = Screen::Articles;
                // 立刻载入第一本书的文章——早期实现只在移动书本光标时才载入，
                // 第一次进文章页右边是空的，看不到任何文章。
                self.load_selected_book();
            }
            Err(err) => self.toast(NoticeLevel::Error, format!("读取文章目录失败: {err}")),
        }
    }

    /// 移动书本列表的光标。
    fn move_article_book(&mut self, delta: i32) {
        let total = self.article_books.len() as i32;
        if total == 0 {
            return;
        }
        let next = (self.article_book_cursor as i32 + delta).clamp(0, total - 1);
        self.article_book_cursor = next as usize;
        self.article_cursor = 0;
        self.load_selected_book();
    }

    /// 移动文章列表的光标。
    fn move_article(&mut self, delta: i32) {
        let total = self.article_list.len() as i32;
        if total == 0 {
            return;
        }
        let next = (self.article_cursor as i32 + delta).clamp(0, total - 1);
        self.article_cursor = next as usize;
    }

    /// 选中一篇文章开始练习。
    fn select_article(&mut self) {
        let index = self.article_cursor;
        self.start_article_practice(index);
    }

    /// 加载选中书本里的文章。
    fn load_selected_book(&mut self) {
        let Some(paths) = self.paths.clone() else {
            return;
        };
        let Some(book) = self.article_books.get(self.article_book_cursor).cloned() else {
            self.article_list.clear();
            return;
        };
        match crate::domain::article::load_articles(&paths, &book) {
            Ok(articles) => self.article_list = articles,
            Err(err) => {
                self.article_list.clear();
                self.toast(NoticeLevel::Error, format!("读取文章失败: {err}"));
            }
        }
    }

    /// 开始一篇文章的跟打练习。
    pub fn start_article_practice(&mut self, index: usize) {
        let Some(article) = self.article_list.get(index).cloned() else {
            return;
        };
        let mut sentences: Vec<crate::domain::article::Sentence> = Vec::new();
        for section in &article.sections {
            sentences.extend(section.iter().cloned());
        }
        if sentences.is_empty() {
            self.toast(NoticeLevel::Warning, "这篇文章没有可练习的句子");
            return;
        }
        self.article_practice = Some(ArticlePracticeState {
            article_id: article.id,
            title: article.title.clone(),
            sentences,
            index: 0,
            typing: Default::default(),
            finished: 0,
            wrong_sentences: 0,
            spend: 0,
        });
        self.screen = Screen::ArticlePractice;
        self.stat = PracticeState::default();
        self.speak_current_sentence();
    }

    /// 朗读当前句子。
    fn speak_current_sentence(&mut self) {
        let Some(practice) = self.article_practice.as_ref() else {
            return;
        };
        let Some(sentence) = practice.sentences.get(practice.index) else {
            return;
        };
        let text = sentence.text.trim().to_string();
        if text.is_empty() {
            return;
        }
        crate::audio::speak_sentence(&self.boot.settings, &text);
    }

    /// 处理文章跟打的一个按键。
    fn article_typing_key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};
        let Some(practice) = self.article_practice.as_mut() else {
            return;
        };
        let Some(sentence) = practice.sentences.get(practice.index) else {
            return;
        };
        let expected = sentence.text.clone();
        match key.code {
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                let ignore_case = self.boot.settings.ignore_case;
                practice.typing.input(&expected, character, ignore_case);
            }
            KeyCode::Backspace => {
                practice.typing.typed = practice.typing.typed.saturating_sub(1);
            }
            _ => {}
        }
        // 整句输完 → 下一句
        let done = practice
            .sentences
            .get(practice.index)
            .map(|sentence| practice.typing.is_done(&sentence.text))
            .unwrap_or(false);
        if done {
            self.finish_article_sentence();
        }
    }

    /// 完成一句，进入下一句。
    fn finish_article_sentence(&mut self) {
        let Some(practice) = self.article_practice.as_mut() else {
            return;
        };
        if practice.typing.wrong {
            practice.wrong_sentences += 1;
        }
        practice.finished += 1;
        practice.index += 1;
        practice.typing = Default::default();
        if practice.index >= practice.sentences.len() {
            self.finish_article_practice();
            return;
        }
        self.speak_current_sentence();
    }

    /// 文章练习结束，写统计并给结算。
    fn finish_article_practice(&mut self) {
        let Some(practice) = self.article_practice.take() else {
            return;
        };
        let total = practice.finished;
        let wrong = practice.wrong_sentences;
        let accuracy = if total == 0 {
            100
        } else {
            (((total - wrong) as f64 / total as f64) * 100.0).round() as u32
        };
        // 文章练习的统计写进当前词典的 statistics（带文章标题）
        if let Some(dict) = self.boot.base.study_dict_mut() {
            let now = timer::now_ms();
            let stat = Statistics {
                start_date: now,
                spend: practice.spend,
                total,
                new_words: 0,
                review: total,
                wrong,
                title: Some(practice.title.clone()),
                segments: None,
                session_role: Some("single".to_string()),
            };
            dict.statistics.push(stat);
        }
        self.save_dict_now();
        self.summary = Some(SummaryData {
            spend_ms: practice.spend,
            accuracy,
            new_words: 0,
            review: total,
            wrong,
            last_learn_index: 0,
            length: total,
            complete: true,
            wrong_top: Vec::new(),
            week_days: [None; 7],
        });
        self.screen = Screen::Practice;
    }

    // ── 词条管理 ─────────────────────────────────────────────────────────

    /// 打开词库页当前选中词典的词条管理页。
    fn open_selected_words(&mut self) {
        let index = self.dict_cursor;
        let Some(resource) = self.boot.catalog.resources().get(index).cloned() else {
            return;
        };
        let id = resource.id_string();
        if let Err(err) = self.boot.select_dict(&id) {
            self.toast(NoticeLevel::Error, format!("打开词库失败: {err}"));
            return;
        }
        self.save_dict_now();
        self.open_words();
    }

    /// 打开词条管理页。
    pub fn open_words(&mut self) {
        if self.boot.base.study_dict().is_none() {
            self.toast(NoticeLevel::Warning, "没有可管理的词库");
            return;
        }
        self.screen = Screen::Words;
        self.words_cursor = 0;
        self.words_filter.clear();
        self.words_search_focus = false;
        self.words_marked.clear();
    }

    /// 当前词典里符合搜索词的词条下标。
    pub fn filtered_word_indices(&self) -> Vec<usize> {
        let Some(dict) = self.boot.base.study_dict() else {
            return Vec::new();
        };
        crate::domain::wordbook::filter_words(&dict.words, &self.words_filter)
    }

    /// 当前词典是不是系统词典（收藏 / 错词 / 已掌握，词条存在存档里）。
    pub fn words_is_system(&self) -> bool {
        self.boot
            .base
            .study_dict()
            .map(|dict| dict.system)
            .unwrap_or(false)
    }

    /// 保存词表：系统词典存在存档里，其余词典写回词条文件。
    ///
    /// 词条文件写入**数据目录**（而不是资源目录里的内置词库），并用
    /// 「临时文件 + fsync + rename」的原子写——资源目录里的内置词库随发布包
    /// 分发，一旦被改写就再也恢复不了。
    fn persist_words(&mut self) {
        if self.words_is_system() {
            self.save_dict_now();
            return;
        }
        let Some(dict) = self.boot.base.study_dict() else {
            return;
        };
        let Some(resource) = self.boot.catalog.find(&dict.id).cloned() else {
            self.toast(NoticeLevel::Warning, "找不到词库文件，改动没有保存");
            return;
        };
        let path = self.boot.catalog.dict_write_file(&resource);
        let text = match serde_json::to_string_pretty(&dict.words) {
            Ok(text) => text,
            Err(err) => {
                self.toast(NoticeLevel::Error, format!("保存词条失败: {err}"));
                return;
            }
        };
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            self.toast(
                NoticeLevel::Error,
                format!("创建目录 {} 失败: {err}", parent.display()),
            );
            return;
        }
        if let Err(err) = crate::store::write_atomic(&path, &text) {
            self.toast(
                NoticeLevel::Error,
                format!("写入 {} 失败: {err}", path.display()),
            );
            return;
        }
        self.toast(NoticeLevel::Success, "词条已保存");
    }

    /// 移动词条列表的光标。
    fn move_words_cursor(&mut self, delta: i32) {
        let total = self.filtered_word_indices().len() as i32;
        if total == 0 {
            self.words_cursor = 0;
            return;
        }
        let next = (self.words_cursor as i32 + delta).clamp(0, total - 1);
        self.words_cursor = next as usize;
    }

    /// 在搜索框里追加字符。
    fn words_search_push(&mut self, character: char) {
        if !self.words_search_focus {
            return;
        }
        self.words_filter.push(character);
        self.words_cursor = 0;
    }

    /// 删除搜索框的最后一个字符。
    fn words_search_backspace(&mut self) {
        if !self.words_search_focus {
            return;
        }
        self.words_filter.pop();
        self.words_cursor = 0;
    }

    /// 切换词条排序方式。
    fn cycle_words_sort(&mut self) {
        self.words_sort = self.words_sort.next();
        let order = self.words_sort;
        let Some(dict) = self.boot.base.study_dict_mut() else {
            return;
        };
        // 排序前记住勾选的是哪些词，排序后按词重新定位下标。
        let marked_words: Vec<String> = self
            .words_marked
            .iter()
            .filter_map(|index| dict.words.get(*index).map(|word| word.word.clone()))
            .collect();
        let mut words = std::mem::take(&mut dict.words);
        crate::domain::wordbook::sort_words(&mut words, order, &mut self.rng);
        if let Some(dict) = self.boot.base.study_dict_mut() {
            dict.words = words;
        }
        self.words_marked = marked_words
            .iter()
            .filter_map(|word| {
                self.boot
                    .base
                    .study_dict()
                    .and_then(|dict| dict.words.iter().position(|item| &item.word == word))
            })
            .collect();
        self.persist_words();
        self.toast(NoticeLevel::Success, format!("排序：{}", order.label()));
    }

    /// 切换搜索框的焦点。
    fn toggle_words_search(&mut self) {
        self.words_search_focus = !self.words_search_focus;
        if self.words_search_focus {
            self.words_filter.clear();
        }
        self.words_cursor = 0;
    }

    /// 勾选 / 取消勾选当前词条。
    fn words_toggle_mark(&mut self) {
        let indices = self.filtered_word_indices();
        let Some(index) = indices.get(self.words_cursor).copied() else {
            return;
        };
        match self.words_marked.iter().position(|item| *item == index) {
            Some(position) => {
                self.words_marked.remove(position);
            }
            None => self.words_marked.push(index),
        }
    }

    /// 编辑当前词条。
    fn edit_current_word(&mut self) {
        let indices = self.filtered_word_indices();
        let Some(index) = indices.get(self.words_cursor).copied() else {
            return;
        };
        self.open_word_edit(Some(index));
    }

    /// 切换词条编辑弹层的字段。
    fn move_word_edit_field(&mut self, delta: i32) {
        let Some(edit) = self.word_edit.as_mut() else {
            return;
        };
        let total = WordEditState::FIELDS as i32;
        edit.field = ((edit.field as i32 + delta).rem_euclid(total)) as usize;
    }

    /// 在词条编辑弹层里追加字符。
    fn word_edit_push(&mut self, character: char) {
        if let Some(edit) = self.word_edit.as_mut() {
            edit.current().push(character);
        }
    }

    /// 删除词条编辑弹层当前字段的最后一个字符。
    fn word_edit_backspace(&mut self) {
        if let Some(edit) = self.word_edit.as_mut() {
            edit.current().pop();
        }
    }

    /// 打开词条编辑弹层（`index` 为 `None` 表示新增）。
    pub fn open_word_edit(&mut self, index: Option<usize>) {
        let word = match index {
            Some(index) => self
                .boot
                .base
                .study_dict()
                .and_then(|dict| dict.words.get(index).cloned()),
            None => None,
        };
        self.word_edit = Some(match word {
            Some(word) => WordEditState {
                index,
                word: word.word.clone(),
                phonetic: word.phonetic0.clone(),
                translation: word
                    .trans
                    .iter()
                    .map(|item| format!("{}{}", item.pos, item.cn))
                    .collect::<Vec<_>>()
                    .join("；"),
                note: self
                    .boot
                    .base
                    .note_data
                    .get(&word.word)
                    .cloned()
                    .unwrap_or_default(),
                field: 0,
            },
            None => WordEditState {
                index: None,
                word: String::new(),
                phonetic: String::new(),
                translation: String::new(),
                note: String::new(),
                field: 0,
            },
        });
    }

    /// 保存词条编辑。
    ///
    /// 只覆盖用户在弹层里编辑的三个字段（单词 / 音标 / 释义），其余字段
    /// （例句、短语、近义词、词源、关联词、原词性、词频、id）沿用原词条——
    /// 早期实现用 `..Default::default()` 重建整个词条，改一个词就会把这些
    /// 内容永久清空。
    fn save_word_edit(&mut self) {
        let Some(edit) = self.word_edit.take() else {
            return;
        };
        let word_text = edit.word.trim().to_string();
        if word_text.is_empty() {
            self.toast(NoticeLevel::Warning, "单词不能为空");
            return;
        }
        let note = edit.note.trim().to_string();
        let Some(dict) = self.boot.base.study_dict_mut() else {
            return;
        };
        // 以原词条为基底，只替换被编辑的字段。
        let mut entry = match edit.index {
            Some(index) => dict.words.get(index).cloned(),
            None => None,
        }
        .unwrap_or_default();
        entry.word = word_text.clone();
        entry.phonetic0 = edit.phonetic.trim().to_string();
        entry.phonetic1 = edit.phonetic.trim().to_string();
        entry.trans = vec![crate::domain::types::Trans {
            // 保留原文的词性（pos），它不在弹层的编辑范围内。
            pos: edit
                .index
                .and_then(|index| dict.words.get(index))
                .and_then(|word| word.trans.first())
                .map(|trans| trans.pos.clone())
                .unwrap_or_default(),
            cn: edit.translation.trim().to_string(),
            frequency: edit
                .index
                .and_then(|index| dict.words.get(index))
                .and_then(|word| word.trans.first())
                .and_then(|trans| trans.frequency),
        }];
        match edit.index {
            Some(index) if index < dict.words.len() => dict.words[index] = entry,
            Some(_) => {}
            None => dict.words.push(entry),
        }
        dict.length = dict.words.len();
        // 笔记存在存档的 note_data 里
        if note.is_empty() {
            self.boot.base.note_data.remove(&word_text);
        } else {
            self.boot.base.note_data.insert(word_text, note);
        }
        self.persist_words();
    }

    /// 删除当前选中的词条。
    fn delete_current_word(&mut self) {
        let indices = self.filtered_word_indices();
        let Some(index) = indices.get(self.words_cursor).copied() else {
            return;
        };
        let Some(dict) = self.boot.base.study_dict_mut() else {
            return;
        };
        if index >= dict.words.len() {
            return;
        }
        let word = dict.words.remove(index);
        dict.length = dict.words.len();
        self.boot.base.note_data.remove(&word.word);
        // 删除后更大的下标会整体前移一位，勾选必须跟着重映射，
        // 否则被勾选的词会「漂移」到别的词上。
        self.words_marked = self
            .words_marked
            .iter()
            .filter_map(|item| match item.cmp(&index) {
                std::cmp::Ordering::Less => Some(*item),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some(item - 1),
            })
            .collect();
        self.persist_words();
    }

    /// 批量标记：把勾选的词条按 `kind` 处理。
    fn apply_word_mark(&mut self, kind: crate::domain::wordbook::MarkKind) {
        let marked = self.words_marked.clone();
        if marked.is_empty() {
            self.toast(NoticeLevel::Warning, "先用空格勾选词条");
            return;
        }
        let Some(dict) = self.boot.base.study_dict() else {
            return;
        };
        let words: Vec<crate::domain::types::Word> = marked
            .iter()
            .filter_map(|index| dict.words.get(*index).cloned())
            .collect();
        match kind {
            crate::domain::wordbook::MarkKind::Mastered => {
                crate::domain::wordbook::mark_mastered(&mut self.boot.base, &words);
            }
            crate::domain::wordbook::MarkKind::Unknown => {
                crate::domain::wordbook::mark_unknown(&mut self.boot.base, &words);
            }
            crate::domain::wordbook::MarkKind::Know => {
                // 「认识」= 记一次 Good 评级 + 从错词本移除（对齐上游
                // `onWordKnow` 的 `resolveIdentifyCorrect`）。
                crate::domain::wordbook::mark_know(&mut self.boot.base, &words);
                self.rate_marked_words(&words);
            }
        }
        self.words_marked.clear();
        self.save_dict_now();
        self.toast(
            NoticeLevel::Success,
            format!("已处理 {} 个词条", words.len()),
        );
    }

    /// 给一批词条记一次 Good 评级（写 FSRS 卡片）。
    ///
    /// 对齐上游批量标记的 `know` 分支：`ratingMap[word] = Rating.Good`。
    /// 卡片按当前设置推进到下次复习时间；没有卡片的词从零开始建卡。
    fn rate_marked_words(&mut self, words: &[Word]) {
        if words.is_empty() {
            return;
        }
        let scheduler = crate::domain::scheduler::Scheduler::new(&self.boot.settings);
        let now = timer::now_ms();
        let reviewed_at =
            chrono::DateTime::from_timestamp_millis(now).unwrap_or_else(chrono::Utc::now);
        for word in words {
            if word.word.is_empty() {
                continue;
            }
            let previous = self.boot.base.fsrs_data.get(&word.word).cloned();
            let card = scheduler.next_card(
                previous.as_ref(),
                crate::domain::types::RATING_GOOD,
                reviewed_at,
            );
            self.boot.base.fsrs_data.insert(word.word.clone(), card);
        }
    }

    /// 把当前词典复制成自定义词典（副本落在数据目录里，可以随意编辑）。
    fn copy_study_dict(&mut self) {
        let Some(dict) = self.boot.base.study_dict().cloned() else {
            return;
        };
        let name = format!("{} 副本", dict.name);
        self.create_custom_dict(&name, dict.words);
    }

    /// 打开「新建词典」的输入弹层。
    fn open_new_dict(&mut self) {
        self.edit = Some(EditState {
            target: EditTarget::DictName,
            buffer: String::new(),
        });
    }

    /// 新建一本自定义词典：词条文件落在数据目录里，并切换过去。
    fn create_custom_dict(&mut self, name: &str, words: Vec<crate::domain::types::Word>) {
        let Some(paths) = self.paths.clone() else {
            return;
        };
        match crate::domain::catalog::create_custom_dict(&paths, name, words) {
            Ok(resource) => {
                let id = resource.id_string();
                if let Ok(catalog) = crate::domain::catalog::Catalog::load(&paths) {
                    self.boot.catalog = catalog;
                }
                if let Err(err) = self.boot.select_dict(&id) {
                    self.toast(NoticeLevel::Error, format!("切换词库失败: {err}"));
                    return;
                }
                self.save_dict_now();
                self.refresh_task_preview();
                self.toast(NoticeLevel::Success, format!("已新建「{name}」"));
            }
            Err(err) => self.toast(NoticeLevel::Error, format!("新建词典失败: {err}")),
        }
    }

    // ── 练习模式选择 ─────────────────────────────────────────────────────

    /// 当前练习要使用的流程。
    ///
    /// 自建流程只在 `Custom` 模式下生效（对齐上游 `getFlowIdForMode`）：
    /// `Custom` 用已启用的自建流程，没有可用流程时回退到内置 system 流程；
    /// 其余模式一律用对应的内置流程。
    fn active_flow(&self, mode: WordPracticeMode) -> crate::domain::flow::PracticeFlowConfig {
        if mode == WordPracticeMode::Custom {
            return match self.flow_store.active() {
                Some(flow) => crate::domain::flow::resolve_flow_config_or_system(&flow.config),
                None => crate::domain::flow::get_flow_config("system"),
            };
        }
        crate::domain::flow::flow_for_mode(mode)
    }

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
        let mut map = self.shortcuts.clone();
        map.unbind(action);
        self.boot.settings.shortcut_key_map = map.to_stored();
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
            || self.word_edit.is_some()
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

        if self.screen == Screen::ArticlePractice {
            if key.code == KeyCode::Esc {
                self.back();
                return;
            }
            if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.speak_current_sentence();
                return;
            }
            self.article_typing_key(key);
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
            EditTarget::DictName => 0,
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
                    EditTarget::DictName => {}
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

        // 文章跟打页也要计时：早期实现的计时分支只在 `Screen::Practice` 下
        // 运行，文章页的 `spend` 从未被写入，用时与统计恒为 0。
        if self.screen == Screen::ArticlePractice
            && let Some(practice) = self.article_practice.as_mut()
            && now - self.last_timer_tick >= 1000
        {
            self.last_timer_tick = now;
            practice.spend += 1000;
        }

        // 单词测试页同理：它没有练习会话，用时单独记。
        if self.screen == Screen::WordTest
            && self.word_test.is_some()
            && now - self.last_timer_tick >= 1000
        {
            self.last_timer_tick = now;
            self.word_test_spend += 1000;
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

fn practice_modes() -> [WordPracticeMode; 10] {
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
