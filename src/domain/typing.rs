//! 键入状态机（对应 Web 的 usePracticeWordTyping + visible-word-typing）。

use crate::domain::types::WordPracticeType;

/// 可打印字符 / 空格 / 退格（由界面层从终端按键映射而来）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Char(char),
    Space,
    Backspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypingNotice {
    DeleteReinput,
    SpaceContinue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypingEvent {
    /// 当前单词练习完成（界面应推进到下一词）。
    Completed,
    /// 记一次错误。
    Wrong,
    /// 需要设置 showWordResult（默写揭示、听写揭示等）。
    ShowWordResult(bool),
    /// 提示文案。
    Notice(TypingNotice),
    /// 重复练习：重置界面上的键入显示。
    ResetForRepeat,
}

#[derive(Clone, Debug)]
pub struct TypingSettings {
    pub ignore_case: bool,
    pub repeat_count: i64,
    pub wait_ms: u64,
    pub space_cooldown_ms: u64,
    pub auto_next_word: bool,
    pub input_wrong_clear: bool,
}

pub struct TypingContext<'a> {
    pub target: &'a str,
    pub practice_type: WordPracticeType,
    pub is_word_masked: bool,
    pub show_word_result: bool,
    pub settings: &'a TypingSettings,
}

const WRONG_CLEAR_DELAY_MS: i64 = 500;

fn char_map(character: char) -> char {
    match character {
        '’' | '‘' => '\'',
        '“' | '”' => '"',
        '。' => '.',
        '，' => ',',
        '？' => '?',
        '【' => '[',
        '】' => ']',
        '￥' => '$',
        '！' => '!',
        '（' => '(',
        '）' => ')',
        '《' => '<',
        '》' => '>',
        other => other,
    }
}

pub fn normalize_word(word: &str) -> String {
    word.chars().map(char_map).collect()
}

/// 全角标点 → ASCII 快捷键映射（复用 Web 的 normalizePracticeInputCharacter 语义）。
fn normalize_input_character(input: char, target: Option<char>) -> char {
    let Some(target) = target else {
        return input;
    };
    let accepted = match target {
        '！' => Some('!'),
        '￥' => Some('$'),
        '…' => Some('^'),
        '（' => Some('('),
        '—' => Some('-'),
        '？' => Some('/'),
        '》' => Some('>'),
        '《' => Some('<'),
        '“' | '”' => Some('"'),
        '：' => Some(':'),
        '）' => Some(')'),
        '、' => Some('\\'),
        '。' => Some('.'),
        '，' => Some(','),
        '‘' | '’' => Some('\''),
        '；' => Some(';'),
        '【' => Some('['),
        '】' => Some(']'),
        _ => None,
    };
    if accepted == Some(input) {
        target
    } else {
        input
    }
}

fn chars_equal(left: char, right: char, ignore_case: bool) -> bool {
    if left == right {
        return true;
    }
    if !ignore_case {
        return false;
    }
    left.to_lowercase().eq(right.to_lowercase())
}

fn is_input_char_correct(input: char, target: Option<char>, ignore_case: bool) -> bool {
    match target {
        Some(target) => chars_equal(input, target, ignore_case),
        None => false,
    }
}

fn is_whole_input_correct(input: &str, target: &str, ignore_case: bool) -> bool {
    let input_chars: Vec<char> = input.chars().collect();
    let target_chars: Vec<char> = target.chars().collect();
    if input_chars.len() != target_chars.len() {
        return false;
    }
    input_chars
        .iter()
        .zip(target_chars.iter())
        .all(|(left, right)| chars_equal(*left, *right, ignore_case))
}

#[derive(Clone, Debug, Default)]
pub struct TypingEngine {
    pub input: String,
    pub wrong: String,
    pub whole_input_attempt: Option<bool>,
    pub input_lock: bool,
    pub word_repeat_count: i64,
    pub word_completed_time: i64,
    press_number: i32,
    jump_deadline: Option<i64>,
    repeat_deadline: Option<i64>,
    wrong_clear_deadline: Option<i64>,
    jump_word: Option<String>,
    repeat_word: Option<String>,
    wrong_clear_word: Option<String>,
}

impl TypingEngine {
    pub fn new() -> Self {
        Self::default()
    }

    fn normalized(value: &str, practice_type: WordPracticeType) -> String {
        if practice_type == WordPracticeType::Dictation {
            normalize_word(value)
        } else {
            value.to_string()
        }
    }

    pub fn is_word_correct(&self, ctx: &TypingContext) -> bool {
        let input = Self::normalized(&self.input, ctx.practice_type);
        let target = Self::normalized(ctx.target, ctx.practice_type);
        is_whole_input_correct(&input, &target, ctx.settings.ignore_case)
    }

    pub fn is_word_right(&self, ctx: &TypingContext) -> bool {
        self.is_word_correct(ctx)
    }

    fn clear_timers(&mut self) {
        self.jump_deadline = None;
        self.repeat_deadline = None;
        self.wrong_clear_deadline = None;
        self.jump_word = None;
        self.repeat_word = None;
        self.wrong_clear_word = None;
    }

    fn should_repeat(&self, ctx: &TypingContext) -> bool {
        if ctx.practice_type != WordPracticeType::FollowWrite {
            return false;
        }
        ctx.settings.repeat_count > self.word_repeat_count + 1
    }

    fn complete_type_word(
        &mut self,
        delay: bool,
        ctx: &TypingContext,
        now: i64,
        events: &mut Vec<TypingEvent>,
    ) {
        if self.should_repeat(ctx) {
            if self.repeat_deadline.is_some() {
                self.repeat_deadline = None;
            }
            let word_key = ctx.target.to_string();
            self.repeat_word = Some(word_key);
            self.repeat_deadline = Some(now + ctx.settings.wait_ms as i64);
            return;
        }
        if !delay {
            events.push(TypingEvent::Completed);
            return;
        }
        self.jump_word = Some(ctx.target.to_string());
        self.jump_deadline = Some(now + ctx.settings.wait_ms as i64);
    }

    fn complete_current_input(
        &mut self,
        ctx: &TypingContext,
        now: i64,
        events: &mut Vec<TypingEvent>,
    ) {
        self.word_completed_time = now;
        if ctx.practice_type == WordPracticeType::Listen && !ctx.show_word_result {
            events.push(TypingEvent::ShowWordResult(true));
        }
        if matches!(
            ctx.practice_type,
            WordPracticeType::FollowWrite | WordPracticeType::Spell
        ) && ctx.settings.auto_next_word
        {
            self.complete_type_word(true, ctx, now, events);
        }
    }

    fn typo(&mut self, events: &mut Vec<TypingEvent>) {
        events.push(TypingEvent::Wrong);
    }

    fn target_char(&self, ctx: &TypingContext) -> Option<char> {
        let index = self.input.chars().count();
        ctx.target.chars().nth(index)
    }

    fn backspace(&mut self, ctx: &TypingContext, events: &mut Vec<TypingEvent>) {
        self.input_lock = false;
        if ctx.practice_type == WordPracticeType::Dictation && ctx.show_word_result {
            self.input.clear();
            self.wrong.clear();
        } else if !self.wrong.is_empty() {
            self.wrong.clear();
        } else {
            self.input.pop();
        }
        if self.input.is_empty() {
            events.push(TypingEvent::ShowWordResult(false));
            self.whole_input_attempt = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyInput, ctx: &TypingContext, now: i64) -> Vec<TypingEvent> {
        let mut events = Vec::new();
        self.type_key(key, ctx, now, &mut events);
        events
    }

    pub fn confirm(&mut self, ctx: &TypingContext, now: i64) -> Vec<TypingEvent> {
        self.handle_key(KeyInput::Space, ctx, now)
    }

    fn type_key(
        &mut self,
        key: KeyInput,
        ctx: &TypingContext,
        now: i64,
        events: &mut Vec<TypingEvent>,
    ) {
        if key == KeyInput::Backspace {
            self.backspace(ctx, events);
            return;
        }
        let is_space = key == KeyInput::Space;
        let key_char = match key {
            KeyInput::Char(character) => character,
            KeyInput::Space => ' ',
            KeyInput::Backspace => unreachable!(),
        };

        if self.input_lock {
            if self.whole_input_attempt == Some(true) && !self.is_word_correct(ctx) {
                events.push(TypingEvent::Notice(TypingNotice::DeleteReinput));
                return;
            }
            if is_space {
                if self.is_word_correct(ctx) {
                    if matches!(
                        ctx.practice_type,
                        WordPracticeType::FollowWrite | WordPracticeType::Spell
                    ) && ctx.settings.auto_next_word
                    {
                        return;
                    }
                    self.jump_deadline = None;
                    if self.word_completed_time > 0
                        && now - self.word_completed_time < ctx.settings.space_cooldown_ms as i64
                    {
                        return;
                    }
                    self.complete_type_word(false, ctx, now, events);
                    self.input_lock = false;
                } else if ctx.show_word_result {
                    self.press_number += 1;
                    if self.press_number >= 3 {
                        events.push(TypingEvent::Notice(TypingNotice::DeleteReinput));
                        self.press_number = 0;
                    }
                }
                return;
            }
            if self.is_word_correct(ctx) {
                self.press_number += 1;
                if self.press_number >= 3 {
                    events.push(TypingEvent::Notice(TypingNotice::SpaceContinue));
                    self.press_number = 0;
                }
                return;
            }
            events.push(TypingEvent::ShowWordResult(false));
            self.input_lock = false;
            self.input.clear();
            self.wrong.clear();
            self.type_key(key, ctx, now, events);
            return;
        }

        self.input_lock = true;

        if ctx.practice_type == WordPracticeType::Dictation {
            let target = ctx.target;
            if is_space
                && !self.input.is_empty()
                && (self.input.chars().count() >= target.chars().count() || !target.contains(' '))
            {
                if self.is_word_correct(ctx) {
                    if ctx.show_word_result {
                        events.push(TypingEvent::Completed);
                        return;
                    }
                    events.push(TypingEvent::ShowWordResult(true));
                } else {
                    events.push(TypingEvent::ShowWordResult(true));
                    self.typo(events);
                }
                return;
            }
            self.input.push(key_char);
            self.wrong.clear();
            self.input_lock = false;
            return;
        }

        if ctx.practice_type == WordPracticeType::Identify && !ctx.show_word_result {
            events.push(TypingEvent::ShowWordResult(true));
            self.typo(events);
        }

        if self.whole_input_attempt.is_none() {
            self.whole_input_attempt = Some(!ctx.is_word_masked);
        }

        let target_char = self.target_char(ctx);
        let letter = normalize_input_character(key_char, target_char);

        if self.whole_input_attempt == Some(true) {
            self.input.push(letter);
            self.wrong.clear();
            if self.input.chars().count() == ctx.target.chars().count() {
                if self.is_word_correct(ctx) {
                    self.complete_current_input(ctx, now, events);
                } else {
                    self.typo(events);
                }
            } else {
                self.input_lock = false;
            }
            return;
        }

        if is_input_char_correct(letter, target_char, ctx.settings.ignore_case) {
            self.input.push(letter);
            self.wrong.clear();
        } else {
            self.typo(events);
            self.wrong = letter.to_string();
            self.wrong_clear_word = Some(ctx.target.to_string());
            self.wrong_clear_deadline = Some(now + WRONG_CLEAR_DELAY_MS);
        }

        if self.is_word_correct(ctx) {
            self.complete_current_input(ctx, now, events);
        } else {
            self.input_lock = false;
        }
    }

    /// 定时器到期检查（由界面层每 tick 调用）。
    pub fn on_tick(&mut self, ctx: &TypingContext, now: i64) -> Vec<TypingEvent> {
        let mut events = Vec::new();

        if let Some(deadline) = self.jump_deadline
            && now >= deadline
        {
            self.jump_deadline = None;
            let matched = self
                .jump_word
                .as_deref()
                .map(|word| word == ctx.target)
                .unwrap_or(false);
            self.jump_word = None;
            if matched {
                events.push(TypingEvent::Completed);
            }
        }

        if let Some(deadline) = self.repeat_deadline
            && now >= deadline
        {
            self.repeat_deadline = None;
            let matched = self
                .repeat_word
                .as_deref()
                .map(|word| word == ctx.target)
                .unwrap_or(false);
            self.repeat_word = None;
            if matched {
                self.wrong.clear();
                self.input.clear();
                self.whole_input_attempt = None;
                self.word_repeat_count += 1;
                self.input_lock = false;
                events.push(TypingEvent::ResetForRepeat);
            }
        }

        if let Some(deadline) = self.wrong_clear_deadline
            && now >= deadline
        {
            self.wrong_clear_deadline = None;
            let matched = self
                .wrong_clear_word
                .as_deref()
                .map(|word| word == ctx.target)
                .unwrap_or(false);
            self.wrong_clear_word = None;
            if matched {
                if ctx.settings.input_wrong_clear {
                    self.input.clear();
                }
                self.wrong.clear();
                if self.input.is_empty() {
                    self.whole_input_attempt = None;
                }
            }
        }

        events
    }

    /// 单词切换 / 阶段切换时重置。
    pub fn reset(&mut self) {
        self.clear_timers();
        self.input.clear();
        self.wrong.clear();
        self.whole_input_attempt = None;
        self.word_repeat_count = 0;
        self.input_lock = false;
        self.word_completed_time = 0;
        self.press_number = 0;
    }

    /// 供自测面板：直接判定为正确（不经过键入）。
    pub fn mark_correct(&mut self, word: &str) {
        self.input_lock = true;
        self.input = word.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> TypingSettings {
        TypingSettings {
            ignore_case: true,
            repeat_count: 1,
            wait_ms: 300,
            space_cooldown_ms: 0,
            auto_next_word: false,
            input_wrong_clear: false,
        }
    }

    fn ctx<'a>(
        settings: &'a TypingSettings,
        target: &'a str,
        practice_type: WordPracticeType,
    ) -> TypingContext<'a> {
        TypingContext {
            target,
            practice_type,
            is_word_masked: false,
            show_word_result: false,
            settings,
        }
    }

    fn type_word(engine: &mut TypingEngine, ctx: &TypingContext, word: &str) -> Vec<TypingEvent> {
        let mut events = Vec::new();
        for character in word.chars() {
            events.extend(engine.handle_key(KeyInput::Char(character), ctx, 0));
        }
        events
    }

    #[test]
    fn follow_write_requires_space_to_advance() {
        let settings = settings();
        let ctx = ctx(&settings, "apple", WordPracticeType::FollowWrite);
        let mut engine = TypingEngine::new();
        let events = type_word(&mut engine, &ctx, "apple");
        assert!(!events.contains(&TypingEvent::Completed));
        assert!(engine.is_word_correct(&ctx));
        let events = engine.handle_key(KeyInput::Space, &ctx, 10);
        assert!(events.contains(&TypingEvent::Completed));
    }

    #[test]
    fn auto_next_word_completes_after_delay() {
        let mut settings = settings();
        settings.auto_next_word = true;
        let ctx = ctx(&settings, "apple", WordPracticeType::FollowWrite);
        let mut engine = TypingEngine::new();
        let events = type_word(&mut engine, &ctx, "apple");
        assert!(!events.contains(&TypingEvent::Completed));
        let events = engine.on_tick(&ctx, 300);
        assert!(events.contains(&TypingEvent::Completed));
    }

    #[test]
    fn masked_typing_marks_wrong_character() {
        let settings = settings();
        let ctx = TypingContext {
            target: "apple",
            practice_type: WordPracticeType::FollowWrite,
            is_word_masked: true,
            show_word_result: false,
            settings: &settings,
        };
        let mut engine = TypingEngine::new();
        let events = engine.handle_key(KeyInput::Char('x'), &ctx, 0);
        assert!(events.contains(&TypingEvent::Wrong));
        assert_eq!(engine.wrong, "x");
        assert_eq!(engine.input, "");
    }

    #[test]
    fn unmasked_typing_collects_full_word_then_marks_wrong() {
        let settings = settings();
        let ctx = ctx(&settings, "apple", WordPracticeType::FollowWrite);
        let mut engine = TypingEngine::new();
        let events = type_word(&mut engine, &ctx, "apxle");
        assert!(events.contains(&TypingEvent::Wrong));
        assert!(!engine.is_word_correct(&ctx));
    }

    #[test]
    fn dictation_reveals_then_completes() {
        let settings = settings();
        let mut ctx = TypingContext {
            target: "apple",
            practice_type: WordPracticeType::Dictation,
            is_word_masked: true,
            show_word_result: false,
            settings: &settings,
        };
        let mut engine = TypingEngine::new();
        type_word(&mut engine, &ctx, "apple");
        let events = engine.handle_key(KeyInput::Space, &ctx, 0);
        assert!(events.contains(&TypingEvent::ShowWordResult(true)));
        assert!(!events.contains(&TypingEvent::Completed));
        ctx.show_word_result = true;
        let events = engine.handle_key(KeyInput::Space, &ctx, 0);
        assert!(events.contains(&TypingEvent::Completed));
    }

    #[test]
    fn dictation_wrong_submit_marks_wrong() {
        let settings = settings();
        let ctx = TypingContext {
            target: "apple",
            practice_type: WordPracticeType::Dictation,
            is_word_masked: true,
            show_word_result: false,
            settings: &settings,
        };
        let mut engine = TypingEngine::new();
        type_word(&mut engine, &ctx, "appl");
        let events = engine.handle_key(KeyInput::Space, &ctx, 0);
        assert!(events.contains(&TypingEvent::Wrong));
        assert!(events.contains(&TypingEvent::ShowWordResult(true)));
    }

    #[test]
    fn backspace_removes_last_character() {
        let settings = settings();
        let ctx = ctx(&settings, "apple", WordPracticeType::FollowWrite);
        let mut engine = TypingEngine::new();
        engine.handle_key(KeyInput::Char('a'), &ctx, 0);
        engine.handle_key(KeyInput::Char('p'), &ctx, 0);
        engine.handle_key(KeyInput::Backspace, &ctx, 0);
        assert_eq!(engine.input, "a");
    }

    #[test]
    fn ignore_case_allows_uppercase() {
        let settings = settings();
        let ctx = ctx(&settings, "apple", WordPracticeType::FollowWrite);
        let mut engine = TypingEngine::new();
        type_word(&mut engine, &ctx, "APPLE");
        assert!(engine.is_word_correct(&ctx));
    }

    #[test]
    fn repeat_count_schedules_reset() {
        let mut settings = settings();
        settings.repeat_count = 2;
        let ctx = ctx(&settings, "apple", WordPracticeType::FollowWrite);
        let mut engine = TypingEngine::new();
        type_word(&mut engine, &ctx, "apple");
        let events = engine.handle_key(KeyInput::Space, &ctx, 0);
        assert!(!events.contains(&TypingEvent::Completed));
        let events = engine.on_tick(&ctx, 300);
        assert!(events.contains(&TypingEvent::ResetForRepeat));
        assert_eq!(engine.input, "");
        assert_eq!(engine.word_repeat_count, 1);
    }

    #[test]
    fn full_width_punctuation_maps_to_target() {
        let settings = settings();
        let ctx = ctx(&settings, "？", WordPracticeType::FollowWrite);
        let mut engine = TypingEngine::new();
        engine.handle_key(KeyInput::Char('/'), &ctx, 0);
        assert_eq!(engine.input, "？");
    }
}
