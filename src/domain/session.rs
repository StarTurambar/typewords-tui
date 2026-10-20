//! 单词练习会话控制器（对应 Web 的 usePracticeWordSession）。

use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

use crate::domain::flow::navigator::{NavState, Navigator, Notice};
use crate::domain::flow::{
    FlowStartResult, PracticeFlowConfig, PracticeFlowCursor, PracticePhase, flow_for_mode,
    resolve_flow_start, stage_for_cursor,
};
use crate::domain::question::build_question;
use crate::domain::rng::{Rng, shuffle};
use crate::domain::scheduler::Scheduler;
use crate::domain::statistics::flush_stat_to_store;
use crate::domain::timer::{self, close_open_segment};
use crate::domain::types::{
    BaseState, Dict, PracticeCache, PracticeCacheCompact, PracticeData, PracticeDataCompact,
    PracticeState, Question, RATING_GOOD, SettingState, TaskWords, TaskWordsStr, Word,
    WordPracticeMode, WordPracticeStage, WordPracticeType,
};
use crate::domain::typing::TypingSettings;

// ── 显隐策略 ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PracticeViewState {
    pub practice_type: WordPracticeType,
    pub is_word_masked: bool,
    pub is_show_translate: bool,
    pub reveal_all: bool,
}

pub fn resolve_view_defaults(practice_type: WordPracticeType) -> (bool, bool) {
    match practice_type {
        WordPracticeType::Spell => (true, true),
        WordPracticeType::Listen => (true, false),
        WordPracticeType::Dictation => (true, true),
        WordPracticeType::Identify => (false, false),
        _ => (false, true),
    }
}

#[derive(Clone, Debug)]
pub struct DisplayPolicy {
    pub practice_type: WordPracticeType,
    pub phase_key: String,
    pub word_mask_override: Option<bool>,
    pub translate_override: Option<bool>,
    pub show_full_word: bool,
    pub show_word_result: bool,
    pub reveal_words: HashSet<String>,
}

impl Default for DisplayPolicy {
    fn default() -> Self {
        Self {
            practice_type: WordPracticeType::FollowWrite,
            phase_key: String::new(),
            word_mask_override: None,
            translate_override: None,
            show_full_word: false,
            show_word_result: false,
            reveal_words: HashSet::new(),
        }
    }
}

// ── 会话 ───────────────────────────────────────────────────────────────────

pub struct PracticeSession {
    pub nav: Navigator,
    pub data: PracticeData,
    pub task_words: TaskWords,
    pub all_words: Vec<Word>,
    pub display: DisplayPolicy,
    pub mode: WordPracticeMode,
    pub complete: bool,
    pub last_word_key: String,
    /// 上一次推进是否要求重置当前单词的键入状态（对应 Web 的 `EventKey.resetWord`）。
    ///
    /// 进入组内巩固 / 错词巩固时若组头仍是同一个单词，单词键不变，界面不会自行重置，
    /// 必须由推进器显式置位。由界面层在 `after_advance` 读取并清除。
    pub reset_word: bool,
}

fn shuffle_words(rng: &mut Rng, words: &mut [Word]) {
    shuffle(rng, words);
}

impl PracticeSession {
    pub fn new(mode: WordPracticeMode) -> Self {
        Self::with_flow(mode, flow_for_mode(mode))
    }

    /// 用指定的流程定义创建会话（自定义流程走这里）。
    pub fn with_flow(
        mode: WordPracticeMode,
        flow: crate::domain::flow::PracticeFlowConfig,
    ) -> Self {
        Self {
            nav: Navigator::new(flow),
            data: PracticeData::default(),
            task_words: TaskWords::default(),
            all_words: Vec::new(),
            display: DisplayPolicy::default(),
            mode,
            complete: false,
            last_word_key: String::new(),
            reset_word: false,
        }
    }

    pub fn flow(&self) -> &PracticeFlowConfig {
        &self.nav.flow
    }

    pub fn cursor(&self) -> &PracticeFlowCursor {
        &self.nav.cursor
    }

    pub fn current_word(&self) -> Word {
        self.data
            .current_word()
            .cloned()
            .unwrap_or_else(Word::default)
    }

    pub fn has_words(&self) -> bool {
        !self.data.words.is_empty()
    }

    pub fn phase(&self) -> PracticePhase {
        self.nav.current_phase()
    }

    /// 当前阶段（上游 `WordPracticeStage`，写入存档的 `statStoreData.stage`）。
    pub fn stage(&self) -> WordPracticeStage {
        stage_for_cursor(&self.nav.flow, &self.nav.cursor)
    }

    pub fn practice_type(&self) -> WordPracticeType {
        self.phase().practice_type
    }

    pub fn phase_key(&self) -> String {
        let cursor = &self.nav.cursor;
        let loop_key = cursor
            .loop_range
            .as_ref()
            .map(|range| range.sub_step_index.to_string())
            .unwrap_or_else(|| "main".to_string());
        let action_key = if cursor.in_wrong_word_clear {
            cursor.end_action_index.unwrap_or(0).to_string()
        } else {
            "main".to_string()
        };
        format!(
            "{}@{}:{}:{}:{}:{}",
            self.nav.flow.id,
            self.nav.flow.version,
            cursor.node_index,
            cursor.step_index,
            action_key,
            loop_key
        )
    }

    pub fn view_state(&self) -> PracticeViewState {
        let (default_mask, default_translate) = resolve_view_defaults(self.practice_type());
        let is_word_masked = self.display.word_mask_override.unwrap_or(default_mask);
        let is_show_translate = self.display.translate_override.unwrap_or(default_translate);
        let reveal_all = self.display.show_full_word || self.display.show_word_result;
        PracticeViewState {
            practice_type: self.practice_type(),
            is_word_masked: if reveal_all { false } else { is_word_masked },
            is_show_translate: if reveal_all { true } else { is_show_translate },
            reveal_all,
        }
    }

    pub fn set_word_masked(&mut self, value: bool) {
        let (default_mask, _) = resolve_view_defaults(self.practice_type());
        self.display.word_mask_override = if value == default_mask {
            None
        } else {
            Some(value)
        };
    }

    pub fn toggle_dictation(&mut self) {
        let current = self.view_state().is_word_masked;
        self.set_word_masked(!current);
    }

    pub fn toggle_translate(&mut self) {
        let (_, default_translate) = resolve_view_defaults(self.practice_type());
        let next = !self.view_state().is_show_translate;
        self.display.translate_override = if next == default_translate {
            None
        } else {
            Some(next)
        };
    }

    /// 阶段或单词切换后同步显隐与键入状态；返回 (word_changed, phase_changed)。
    pub fn refresh(&mut self) -> (bool, bool) {
        let word_key = self.current_word().word;
        let word_changed = word_key != self.last_word_key;
        let phase_key = self.phase_key();
        let phase_changed = phase_key != self.display.phase_key;

        self.display.practice_type = self.practice_type();
        if phase_changed {
            self.display.phase_key = phase_key;
            self.display.word_mask_override = None;
            self.display.translate_override = None;
            self.display.show_full_word = false;
            self.display.show_word_result = false;
            self.display.reveal_words.clear();
        }
        if word_changed {
            self.display.show_full_word = false;
            self.display.show_word_result = false;
            self.last_word_key = word_key;
        }
        (word_changed, phase_changed)
    }

    /// 显示完整单词（遮罩状态下会记一次错误）。
    pub fn show_word(&mut self, base: &mut BaseState, stat: &mut PracticeState) {
        let word = self.current_word();
        if word.word.is_empty() {
            return;
        }
        let practice_type = self.practice_type();
        let masked = self.view_state().is_word_masked;
        if (practice_type != WordPracticeType::FollowWrite || masked)
            && !self.display.show_word_result
            && !self.display.reveal_words.contains(&word.word)
        {
            self.display.reveal_words.insert(word.word.clone());
            self.on_type_wrong(base, stat);
        }
        self.display.show_full_word = true;
    }

    pub fn hide_word(&mut self) {
        self.display.show_full_word = false;
    }

    pub fn update_question(&mut self, rng: &mut Rng) {
        let word = self.current_word();
        if self.practice_type() == WordPracticeType::Identify && !word.word.is_empty() {
            self.data.question = build_question(&word, &self.all_words, rng, 4);
        } else {
            self.data.question = None;
        }
    }

    pub fn typing_settings(settings: &SettingState) -> TypingSettings {
        TypingSettings {
            ignore_case: settings.ignore_case,
            repeat_count: settings.effective_repeat_count(),
            wait_ms: settings.wait_ms(),
            space_cooldown_ms: settings.space_cooldown_ms(),
            auto_next_word: settings.auto_next_word,
            input_wrong_clear: settings.input_wrong_clear,
        }
    }

    // ── 会话初始化 ───────────────────────────────────────────────────────

    pub fn initialize_task(
        &mut self,
        task: TaskWords,
        rng: &mut Rng,
        stat: &mut PracticeState,
        now: i64,
    ) -> FlowStartResult {
        // 保留会话自带的流程（可能是用户自建流程），只重置游标
        self.nav.reset();
        self.complete = false;
        self.reset_word = false;
        let mut task_words = task;
        let start = resolve_flow_start(&self.nav.flow, &task_words, &[], &mut |words| {
            shuffle_words(rng, words)
        })
        .unwrap_or_else(|| FlowStartResult {
            words: Vec::new(),
            total: 0,
            new_word_number: 0,
            review_word_number: 0,
            cursor: crate::domain::flow::get_initial_cursor(),
        });
        task_words.new.retain(Word::is_usable);
        task_words.review.retain(Word::is_usable);
        self.task_words = task_words;
        self.nav.set_cursor(start.cursor.clone());
        self.nav.initialize_node_words(start.words.clone());

        self.data = PracticeData {
            words: start.words.clone(),
            ..PracticeData::default()
        };

        stat.reset_session();
        stat.total = start.total;
        stat.new_word_number = start.new_word_number;
        stat.review_word_number = start.review_word_number;
        stat.start_date = now;
        timer::resume(stat, now);

        self.update_question(rng);
        self.last_word_key = String::new();
        self.refresh();
        start
    }

    pub fn apply_cache(
        &mut self,
        cache: &PracticeCache,
        rng: &mut Rng,
        stat: &mut PracticeState,
        now: i64,
    ) -> bool {
        let previous = (
            self.task_words.clone(),
            self.data.clone(),
            stat.clone(),
            self.nav.build_snapshot(),
        );

        self.task_words = cache.task_words.clone();
        self.data = cache.practice_data.clone();
        *stat = cache.stat_store_data.clone();
        self.mode = self.nav.flow.mode;

        let restored =
            self.nav
                .restore_snapshot(&cache.session_snapshot, &self.task_words, &self.data);
        if !restored {
            self.task_words = previous.0;
            self.data = previous.1;
            *stat = previous.2;
            self.nav
                .restore_snapshot(&previous.3, &self.task_words, &self.data);
            return false;
        }

        self.mode = self.nav.flow.mode;
        if !stat.timer_paused {
            timer::resume(stat, now);
        }
        self.complete = false;
        self.reset_word = false;
        self.last_word_key = String::new();
        self.refresh();
        self.update_question(rng);
        true
    }

    pub fn build_cache(&self) -> PracticeCache {
        PracticeCache {
            task_words: self.task_words.clone(),
            practice_data: self.data.clone(),
            stat_store_data: PracticeState::default(),
            session_snapshot: self.nav.build_snapshot(),
        }
    }

    // ── 推进 ─────────────────────────────────────────────────────────────

    pub fn next(
        &mut self,
        base: &mut BaseState,
        stat: &mut PracticeState,
        rng: &mut Rng,
        is_typing: bool,
    ) -> Vec<Notice> {
        let known = base.known_words_set();
        let is_simple = move |word: &str| known.contains(&word.to_lowercase());
        let mut notices = Vec::new();
        let mut reset_word = false;
        let mut completed = false;
        {
            let mut state = NavState {
                data: &mut self.data,
                task_words: &self.task_words,
                stat,
                is_simple: &is_simple,
                rng,
                notices: &mut notices,
                reset_word: &mut reset_word,
                completed: &mut completed,
            };
            self.nav.next(&mut state, is_typing, false);
        }
        self.reset_word = reset_word;
        if completed {
            self.complete = true;
        }
        notices
    }

    pub fn prev(
        &mut self,
        base: &BaseState,
        stat: &mut PracticeState,
        rng: &mut Rng,
    ) -> Vec<Notice> {
        let known = base.known_words_set();
        let is_simple = move |word: &str| known.contains(&word.to_lowercase());
        let mut notices = Vec::new();
        let mut reset_word = false;
        let mut completed = false;
        let mut state = NavState {
            data: &mut self.data,
            task_words: &self.task_words,
            stat,
            is_simple: &is_simple,
            rng,
            notices: &mut notices,
            reset_word: &mut reset_word,
            completed: &mut completed,
        };
        self.nav.prev(&mut state);
        self.reset_word = reset_word;
        notices
    }

    pub fn skip_step(
        &mut self,
        base: &BaseState,
        stat: &mut PracticeState,
        rng: &mut Rng,
    ) -> Vec<Notice> {
        let known = base.known_words_set();
        let is_simple = move |word: &str| known.contains(&word.to_lowercase());
        let mut notices = Vec::new();
        let mut reset_word = false;
        let mut completed = false;
        let mut state = NavState {
            data: &mut self.data,
            task_words: &self.task_words,
            stat,
            is_simple: &is_simple,
            rng,
            notices: &mut notices,
            reset_word: &mut reset_word,
            completed: &mut completed,
        };
        self.nav.skip_step(&mut state);
        self.reset_word = reset_word;
        if completed {
            self.complete = true;
        }
        notices
    }

    // ── 单词操作 ─────────────────────────────────────────────────────────

    fn add_exclude_word(&mut self) {
        let key = self.current_word().word;
        if !key.is_empty() && !self.data.exclude_words.contains(&key) {
            self.data.exclude_words.push(key);
        }
    }

    pub fn on_type_wrong(&mut self, base: &mut BaseState, _stat: &mut PracticeState) {
        let word = self.current_word();
        if word.word.is_empty() {
            return;
        }
        self.data.wrong_times += 1;
        if !self.data.all_wrong_words.contains(&word.word) {
            self.data.all_wrong_words.push(word.word.clone());
        }

        if let Some(wrong_dict) = base.wrong_mut()
            && !wrong_dict.words.iter().any(|item| item.word == word.word)
        {
            wrong_dict.words.push(word.clone());
            wrong_dict.length = wrong_dict.words.len();
        }

        if !self
            .data
            .wrong_words
            .iter()
            .any(|item| item.word == word.word)
        {
            self.data.wrong_words.push(word.clone());
        }
        if let Some(index) = self
            .data
            .exclude_words
            .iter()
            .position(|key| key == &word.word)
        {
            self.data.exclude_words.remove(index);
        }
    }

    fn resolve_identify_correct(&mut self, base: &mut BaseState) {
        let word = self.current_word();
        if let Some(index) = self
            .data
            .wrong_words
            .iter()
            .position(|item| item.word == word.word)
        {
            self.data.wrong_words.remove(index);
        }
        self.data.all_wrong_words.retain(|key| key != &word.word);
        self.data.wrong_times = 0;
        self.add_exclude_word();

        if let Some(wrong_dict) = base.wrong_mut()
            && let Some(index) = wrong_dict
                .words
                .iter()
                .position(|item| item.word == word.word)
        {
            wrong_dict.words.remove(index);
            wrong_dict.length = wrong_dict.words.len();
        }
    }

    pub fn on_word_know(&mut self, base: &mut BaseState) {
        let word = self.current_word().word;
        self.data.rating_map.insert(word, RATING_GOOD);
        self.resolve_identify_correct(base);
    }

    pub fn toggle_word_simple_for_current(&mut self, base: &mut BaseState) {
        let word = self.current_word();
        if word.word.is_empty() {
            return;
        }
        let is_simple = base
            .known()
            .map(|dict| {
                dict.words
                    .iter()
                    .any(|item| item.word.to_lowercase() == word.word.to_lowercase())
            })
            .unwrap_or(false);

        if let Some(known_dict) = base.known_mut() {
            if is_simple {
                known_dict
                    .words
                    .retain(|item| item.word.to_lowercase() != word.word.to_lowercase());
            } else {
                known_dict.words.push(word.clone());
            }
            known_dict.length = known_dict.words.len();
        }

        if let Some(index) = self
            .data
            .exclude_words
            .iter()
            .position(|key| key == &word.word)
        {
            self.data.exclude_words.remove(index);
        } else if !is_simple {
            self.data.exclude_words.push(word.word);
        }
    }

    pub fn on_word_mastered(&mut self, base: &mut BaseState) {
        self.toggle_word_simple_for_current(base);
        self.resolve_identify_correct(base);
    }

    pub fn skip(
        &mut self,
        base: &mut BaseState,
        stat: &mut PracticeState,
        rng: &mut Rng,
    ) -> Vec<Notice> {
        self.add_exclude_word();
        self.next(base, stat, rng, false)
    }

    pub fn random_write(&mut self, rng: &mut Rng) {
        shuffle(rng, &mut self.data.words);
        self.data.index = 0;
        self.set_word_masked(true);
    }

    pub fn toggle_collect(&mut self, base: &mut BaseState) {
        let word = self.current_word();
        if word.word.is_empty() {
            return;
        }
        if let Some(collect) = base.collect_word_mut() {
            if let Some(index) = collect
                .words
                .iter()
                .position(|item| item.word.to_lowercase() == word.word.to_lowercase())
            {
                collect.words.remove(index);
            } else {
                collect.words.push(word);
            }
            collect.length = collect.words.len();
        }
    }

    /// 把当前词收藏到指定词典（再次触发则取消收藏）。
    ///
    /// `dict_index` 是 `BaseState::word.book_list` 的下标。
    pub fn toggle_collect_to(&mut self, base: &mut BaseState, dict_index: usize) {
        let word = self.current_word();
        if word.word.is_empty() {
            return;
        }
        let Some(dict) = base.word.book_list.get_mut(dict_index) else {
            return;
        };
        if let Some(position) = dict
            .words
            .iter()
            .position(|item| item.word.to_lowercase() == word.word.to_lowercase())
        {
            dict.words.remove(position);
        } else {
            dict.words.push(word);
        }
        dict.length = dict.words.len();
    }

    pub fn is_collect(&self, base: &BaseState) -> bool {
        let word = self.current_word();
        base.collect_word()
            .map(|dict| {
                dict.words
                    .iter()
                    .any(|item| item.word.to_lowercase() == word.word.to_lowercase())
            })
            .unwrap_or(false)
    }

    pub fn is_simple(&self, base: &BaseState) -> bool {
        let word = self.current_word();
        base.known()
            .map(|dict| {
                dict.words
                    .iter()
                    .any(|item| item.word.to_lowercase() == word.word.to_lowercase())
            })
            .unwrap_or(false)
    }

    // ── 结算 ─────────────────────────────────────────────────────────────

    /// 推进当前词典的学习进度（对应 Web 的 updateCompletedDictProgress）。
    pub fn advance_dict_progress(base: &mut BaseState, new_word_number: usize, check_all: bool) {
        let ignore_list = base.known_words();
        if let Some(dict) = base.study_dict_mut() {
            dict.last_learn_index = dict.last_learn_index.saturating_add(new_word_number);
            let start = if check_all { 0 } else { dict.last_learn_index };
            let words_to_check: Vec<String> = dict
                .words
                .iter()
                .skip(start)
                .map(|word| word.word.clone())
                .collect();
            let ignore_count = ignore_list
                .iter()
                .filter(|key| words_to_check.iter().any(|word| word == *key))
                .count();
            if dict.last_learn_index + ignore_count >= dict.length {
                dict.complete = true;
                dict.last_learn_index = dict.length;
            }
        }
    }

    pub fn settle(
        &mut self,
        base: &mut BaseState,
        settings: &SettingState,
        stat: &mut PracticeState,
        now: i64,
    ) {
        stat.wrong = self.data.all_wrong_words.len();
        if self.mode != WordPracticeMode::Shuffle {
            Self::advance_dict_progress(base, stat.new_word_number, false);
        }
        close_open_segment(stat, now);
        let (segments, spend) = timer::normalize_practice_timer(&stat.segments, stat.spend, now);
        stat.segments = segments;
        stat.spend = spend;

        if let Some(dict) = base.study_dict_mut() {
            flush_stat_to_store(dict, stat);
        }

        let scheduler = Scheduler::new(settings);
        let entries: Vec<(String, i64)> = self
            .data
            .wrong_times_map
            .iter()
            .map(|(word, times)| (word.clone(), *times))
            .collect();
        for (word, wrong_times) in entries {
            let rating = self
                .data
                .rating_map
                .get(&word)
                .copied()
                .unwrap_or_else(|| scheduler.grade_by_wrong_times(Some(wrong_times)));
            let previous = base.fsrs_data.get(&word).cloned();
            let reviewed_at = DateTime::from_timestamp_millis(now).unwrap_or_else(Utc::now);
            let card = scheduler.next_card(previous.as_ref(), rating, reviewed_at);
            base.fsrs_data.insert(word, card);
        }
    }

    pub fn create_repeat_task(
        &mut self,
        base: &mut BaseState,
        settings: &SettingState,
        stat: &PracticeState,
    ) -> TaskWords {
        let mut task_words = self.task_words.clone();
        let ignore_set = base.ignore_words_set(settings.ignore_simple_word);
        if self.mode == WordPracticeMode::Shuffle {
            task_words
                .review
                .retain(|word| !ignore_set.contains(&word.lower()));
        } else {
            if let Some(dict) = base.study_dict_mut() {
                dict.last_learn_index = dict.last_learn_index.saturating_sub(stat.new_word_number);
            }
            task_words
                .new
                .retain(|word| !ignore_set.contains(&word.lower()));
            task_words
                .review
                .retain(|word| !ignore_set.contains(&word.lower()));
        }
        task_words
    }

    pub fn create_next_task(
        &mut self,
        base: &mut BaseState,
        settings: &SettingState,
        rng: &mut Rng,
        stat: &PracticeState,
        was_complete: bool,
    ) -> TaskWords {
        if self.mode == WordPracticeMode::Shuffle {
            return crate::domain::study_task::create_shuffle_task(base, settings, None, rng);
        }
        if !was_complete {
            Self::advance_dict_progress(base, stat.new_word_number, true);
        }
        crate::domain::study_task::create_study_task(base, settings, rng).task_words
    }

    pub fn create_task_from_group(
        &mut self,
        base: &mut BaseState,
        settings: &SettingState,
        rng: &mut Rng,
        group: usize,
    ) -> TaskWords {
        let per_day = base.study_dict().map(Dict::per_day).unwrap_or(20);
        if let Some(dict) = base.study_dict_mut() {
            dict.last_learn_index = group.saturating_sub(1) * per_day;
        }
        crate::domain::study_task::create_study_task(base, settings, rng).task_words
    }

    /// 当前会话使用的提示词（自测面板用）。
    pub fn question(&self) -> Option<&Question> {
        self.data.question.as_ref()
    }
}

// ── 缓存序列化（Words ↔ 字符串）────────────────────────────────────────────

pub fn cache_to_compact(cache: &PracticeCache) -> PracticeCacheCompact {
    let practice_data = PracticeDataCompact {
        index: cache.practice_data.index,
        exclude_words: cache.practice_data.exclude_words.clone(),
        all_wrong_words: cache.practice_data.all_wrong_words.clone(),
        is_typing_wrong_word: cache.practice_data.is_typing_wrong_word,
        wrong_times_map: cache.practice_data.wrong_times_map.clone(),
        wrong_times: cache.practice_data.wrong_times,
        rating_map: cache.practice_data.rating_map.clone(),
        words_str: cache
            .practice_data
            .words
            .iter()
            .map(|word| word.word.clone())
            .collect(),
        wrong_words_str: cache
            .practice_data
            .wrong_words
            .iter()
            .map(|word| word.word.clone())
            .collect(),
        question: None,
    };
    PracticeCacheCompact {
        task_words_str: TaskWordsStr {
            new: cache
                .task_words
                .new
                .iter()
                .map(|word| word.word.clone())
                .collect(),
            review: cache
                .task_words
                .review
                .iter()
                .map(|word| word.word.clone())
                .collect(),
        },
        practice_data: Some(practice_data),
        stat_store_data: Some(cache.stat_store_data.clone()),
        session_snapshot: Some(cache.session_snapshot.clone()),
    }
}

pub fn cache_from_compact(compact: &PracticeCacheCompact, words: &[Word]) -> Option<PracticeCache> {
    let word_map: HashMap<&str, &Word> = words
        .iter()
        .map(|word| (word.word.as_str(), word))
        .collect();
    let restore = |keys: &[String]| -> Vec<Word> {
        keys.iter()
            .filter_map(|key| word_map.get(key.as_str()).map(|word| (*word).clone()))
            .collect()
    };

    let task_words = TaskWords {
        new: restore(&compact.task_words_str.new),
        review: restore(&compact.task_words_str.review),
    };
    if task_words.is_empty() {
        return None;
    }

    let practice_data_compact = compact.practice_data.as_ref()?;
    let stat_store_data = compact.stat_store_data.clone()?;
    let session_snapshot = compact.session_snapshot.clone()?;

    let words = restore(&practice_data_compact.words_str);
    let index = if words.is_empty() {
        0
    } else {
        practice_data_compact.index.min(words.len() - 1)
    };
    let practice_data = PracticeData {
        index,
        words,
        wrong_words: restore(&practice_data_compact.wrong_words_str),
        exclude_words: practice_data_compact.exclude_words.clone(),
        all_wrong_words: practice_data_compact.all_wrong_words.clone(),
        is_typing_wrong_word: practice_data_compact.is_typing_wrong_word,
        wrong_times_map: practice_data_compact.wrong_times_map.clone(),
        wrong_times: practice_data_compact.wrong_times,
        rating_map: practice_data_compact.rating_map.clone(),
        question: None,
    };

    Some(PracticeCache {
        task_words,
        practice_data,
        stat_store_data,
        session_snapshot,
    })
}
