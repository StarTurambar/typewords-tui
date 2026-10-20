//! 领域数据类型。
//!
//! 字段名与 Web 端存档（JSON）保持一致，便于将来与网页版数据互通：
//! - 大部分结构使用 camelCase 序列化（与前端对象属性一致）
//! - `Card` 使用 ts-fsrs 的 snake_case 字段名

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::constants::{
    DEFAULT_FSRS_W, DICT_ARTICLE_COLLECT, DICT_WORD_COLLECT, DICT_WORD_KNOWN, DICT_WORD_WRONG,
    SIMPLE_WORDS,
};
use crate::domain::flow::PracticeFlowCursor;

// ── 整数枚举（与 Web 的数值一致）────────────────────────────────────────────

macro_rules! int_enum {
    ($name:ident { $($variant:ident = $value:expr),+ $(,)? }) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_i32(self) -> i32 {
                match self { $(Self::$variant => $value),+ }
            }

            pub fn from_i32(value: i32) -> Option<Self> {
                match value { $($value => Some(Self::$variant),)+ _ => None }
            }

            pub fn all() -> Vec<Self> {
                vec![$(Self::$variant),+]
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_i32(self.as_i32())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = i32::deserialize(deserializer)?;
                Self::from_i32(value).ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        "无效的 {} 值: {}",
                        stringify!($name),
                        value
                    ))
                })
            }
        }
    };
}

int_enum!(WordPracticeMode {
    System = 0,
    Free = 1,
    IdentifyOnly = 2,
    DictationOnly = 3,
    ListenOnly = 4,
    Shuffle = 5,
    Review = 6,
    ShuffleWordsTest = 7,
    ReviewWordsTest = 8,
    Custom = 9,
});

int_enum!(WordPracticeType {
    FollowWrite = 0,
    Spell = 1,
    Identify = 2,
    Listen = 3,
    Dictation = 4,
});

int_enum!(WordPracticeStage {
    FollowWriteNewWord = 0,
    IdentifyNewWord = 1,
    ListenNewWord = 2,
    DictationNewWord = 3,
    FollowWriteReview = 4,
    IdentifyReview = 5,
    ListenReview = 6,
    DictationReview = 7,
    Shuffle = 12,
    Complete = 13,
});

/// FSRS 评级（与 ts-fsrs 的 Rating 一致）。
pub type Rating = u8;
pub const RATING_AGAIN: Rating = 1;
pub const RATING_HARD: Rating = 2;
pub const RATING_GOOD: Rating = 3;
pub const RATING_EASY: Rating = 4;

/// FSRS 卡片状态（与 ts-fsrs 的 State 一致）。
pub const CARD_STATE_NEW: u8 = 0;
pub const CARD_STATE_LEARNING: u8 = 1;
pub const CARD_STATE_REVIEW: u8 = 2;
pub const CARD_STATE_RELEARNING: u8 = 3;

// ── 通用反序列化助手 ───────────────────────────────────────────────────────

fn flex_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text,
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn deserialize_flex_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    Ok(flex_string(serde_json::Value::deserialize(deserializer)?))
}

fn deserialize_opt_flex_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(None),
        value => Ok(Some(flex_string(value))),
    }
}

// ── 单词 ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WordSubContent {
    /// 英文内容
    pub c: String,
    /// 中文说明
    pub cn: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Trans {
    pub pos: String,
    pub cn: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Syno {
    pub pos: String,
    pub cn: String,
    pub ws: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct RelGroup {
    pub pos: String,
    pub words: Vec<WordSubContent>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct RelWords {
    pub root: String,
    pub rels: Vec<RelGroup>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Etymology {
    /// 标题
    pub t: String,
    /// 描述
    pub d: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Word {
    #[serde(default, deserialize_with = "deserialize_opt_flex_string")]
    pub id: Option<String>,
    pub custom: bool,
    pub word: String,
    pub phonetic0: String,
    pub phonetic1: String,
    pub trans: Vec<Trans>,
    pub sentences: Vec<WordSubContent>,
    pub phrases: Vec<WordSubContent>,
    pub synos: Vec<Syno>,
    pub rel_words: RelWords,
    pub etymology: Vec<Etymology>,
}

impl Word {
    pub fn new(word: impl Into<String>) -> Self {
        Self {
            word: word.into(),
            ..Default::default()
        }
    }

    /// 小写单词，用于忽略大小写比较与在忽略列表中的查找。
    pub fn lower(&self) -> String {
        self.word.to_lowercase()
    }

    /// 是否包含任何可练习的内容（用于过滤空词条）。
    pub fn is_usable(&self) -> bool {
        !self.word.trim().is_empty()
    }

    /// 拼接所有中文释义（出题相似度计算用）。
    pub fn trans_joined(&self) -> String {
        self.trans.iter().map(|item| item.cn.as_str()).collect()
    }

    pub fn phonetic(&self, uk: bool) -> &str {
        if uk { &self.phonetic0 } else { &self.phonetic1 }
    }
}

// ── 统计 ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Statistics {
    /// 开始时间（毫秒）
    pub start_date: i64,
    /// 花费时间（毫秒）
    pub spend: i64,
    pub total: usize,
    #[serde(rename = "new")]
    pub new_words: usize,
    pub review: usize,
    pub wrong: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 本日实际学习时间片段，每项为 [startMs, endMs]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<[i64; 2]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_role: Option<String>,
}

// ── FSRS 卡片 ──────────────────────────────────────────────────────────────

/// 与 ts-fsrs 的 Card 序列化格式一致（字段为 snake_case）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Card {
    pub due: DateTime<Utc>,
    pub stability: f64,
    pub difficulty: f64,
    #[serde(default)]
    pub elapsed_days: f64,
    #[serde(default)]
    pub scheduled_days: f64,
    #[serde(default)]
    pub learning_steps: f64,
    #[serde(default)]
    pub reps: u32,
    #[serde(default)]
    pub lapses: u32,
    #[serde(default)]
    pub state: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_review: Option<DateTime<Utc>>,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            due: Utc::now(),
            stability: 0.0,
            difficulty: 0.0,
            elapsed_days: 0.0,
            scheduled_days: 0.0,
            learning_steps: 0.0,
            reps: 0,
            lapses: 0,
            state: CARD_STATE_NEW,
            last_review: None,
        }
    }
}

// ── 词典 ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Dict {
    #[serde(deserialize_with = "deserialize_flex_string")]
    pub id: String,
    #[serde(alias = "en_name")]
    pub en_name: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub length: usize,
    pub category: String,
    pub tags: Vec<String>,
    pub translate_language: String,
    pub language: String,
    pub last_learn_index: usize,
    pub per_day_study_number: usize,
    pub words: Vec<Word>,
    pub articles: Vec<serde_json::Value>,
    pub statistics: Vec<Statistics>,
    pub custom: bool,
    pub system: bool,
    pub source_id: String,
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Dict {
    pub fn system(id: &str, name: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            en_name: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            language: "en".to_string(),
            system: true,
            per_day_study_number: 20,
            ..Default::default()
        }
    }

    /// 词典的身份标识集合（id / enName 都可能被当作路由参数使用）。
    pub fn identity_list(&self) -> Vec<String> {
        let mut list = Vec::new();
        for value in [&self.id, &self.en_name] {
            if !value.is_empty() && !list.contains(value) {
                list.push(value.clone());
            }
        }
        list
    }

    pub fn matches_id(&self, id: &str) -> bool {
        !id.is_empty() && self.identity_list().iter().any(|value| value == id)
    }

    /// 有效词数（以 words 为准，兼容 length 字段缺省的数据）。
    pub fn effective_length(&self) -> usize {
        if self.words.is_empty() {
            self.length
        } else {
            self.words.len()
        }
    }

    pub fn progress_percent(&self) -> u32 {
        let length = self.length.max(self.words.len());
        if length == 0 {
            return 0;
        }
        if self.complete {
            return 100;
        }
        ((self.last_learn_index.min(length) as f64 / length as f64) * 100.0).round() as u32
    }

    pub fn per_day(&self) -> usize {
        if self.complete {
            return self.per_day_study_number.max(1);
        }
        let length = self.effective_length().max(1);
        self.per_day_study_number.clamp(1, length)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BookGroup {
    pub book_list: Vec<Dict>,
    pub study_index: i32,
}
// ── 设置 ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct FsrsParameters {
    pub request_retention: f64,
    pub maximum_interval: f64,
    pub w: Vec<f64>,
    pub enable_fuzz: bool,
    pub enable_short_term: bool,
    pub learning_steps: Vec<String>,
    pub relearning_steps: Vec<String>,
}

impl Default for FsrsParameters {
    fn default() -> Self {
        crate::domain::constants::default_fsrs_parameters()
    }
}

impl FsrsParameters {
    /// 有效权重（长度不为 21 时回退到默认权重）。
    pub fn effective_w(&self) -> Vec<f32> {
        if self.w.len() == DEFAULT_FSRS_W.len() && self.w.iter().all(|value| value.is_finite()) {
            self.w.iter().map(|value| *value as f32).collect()
        } else {
            DEFAULT_FSRS_W.iter().map(|value| *value as f32).collect()
        }
    }

    pub fn effective_retention(&self) -> f32 {
        if self.request_retention > 0.0 && self.request_retention < 1.0 {
            self.request_retention as f32
        } else {
            0.9
        }
    }

    pub fn effective_maximum_interval(&self) -> f64 {
        if self.maximum_interval >= 1.0 {
            self.maximum_interval
        } else {
            36500.0
        }
    }
}

/// 设置状态。只建模 TUI 需要读写的字段，其余字段通过 `extra` 原样保留。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SettingState {
    pub sound_type: String,
    pub word_review_ratio: i64,
    pub auto_add_random_review_when_no_due: bool,
    pub word_practice_mode: WordPracticeMode,
    pub word_practice_type: WordPracticeType,
    pub ignore_case: bool,
    pub input_wrong_clear: bool,
    pub auto_next_word: bool,
    pub repeat_count: i64,
    pub repeat_custom_count: Option<i64>,
    pub wait_time_for_change_word: i64,
    pub space_cooldown_time: i64,
    pub ignore_simple_word: bool,
    pub show_word_question: bool,
    pub show_etymology_and_rel_words: bool,
    pub show_near_word: bool,
    pub show_usage_tips: bool,
    pub always_show_note: bool,
    pub theme: String,
    pub dictation: bool,
    pub translate: bool,
    pub ignore_symbol: bool,
    pub fsrs_easy_limit: i64,
    pub fsrs_good_limit: i64,
    pub fsrs_hard_limit: i64,
    pub fsrs_parameters: FsrsParameters,
    /// 快捷键表：动作名 → 键位（上游 `shortcutKeyMap`，同样式 `Record<string,string>`）
    pub shortcut_key_map: BTreeMap<String, String>,
    /// TUI 专属：练习页是否使用 2 倍大字（空间足够时）
    #[serde(rename = "tuiBigWord")]
    pub tui_big_word: bool,
    #[serde(rename = "_ignoreWatch")]
    pub ignore_watch: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for SettingState {
    fn default() -> Self {
        Self {
            sound_type: "us".to_string(),
            word_review_ratio: 3,
            auto_add_random_review_when_no_due: false,
            word_practice_mode: WordPracticeMode::System,
            word_practice_type: WordPracticeType::FollowWrite,
            ignore_case: true,
            input_wrong_clear: false,
            auto_next_word: false,
            repeat_count: 1,
            repeat_custom_count: None,
            wait_time_for_change_word: 300,
            space_cooldown_time: 0,
            ignore_simple_word: false,
            show_word_question: true,
            show_etymology_and_rel_words: false,
            show_near_word: true,
            show_usage_tips: true,
            always_show_note: true,
            theme: "auto".to_string(),
            dictation: false,
            translate: true,
            ignore_symbol: true,
            fsrs_easy_limit: 0,
            fsrs_good_limit: 3,
            fsrs_hard_limit: 6,
            fsrs_parameters: FsrsParameters::default(),
            shortcut_key_map: BTreeMap::new(),
            tui_big_word: true,
            ignore_watch: false,
            extra: serde_json::Map::new(),
        }
    }
}

impl SettingState {
    /// 重复次数（100 表示使用自定义值，与 Web 一致）。
    pub fn effective_repeat_count(&self) -> i64 {
        let value = if self.repeat_count == 100 {
            self.repeat_custom_count.unwrap_or(0)
        } else {
            self.repeat_count
        };
        value.max(1)
    }

    pub fn wait_ms(&self) -> u64 {
        self.wait_time_for_change_word.clamp(0, 10_000) as u64
    }

    pub fn space_cooldown_ms(&self) -> u64 {
        self.space_cooldown_time.clamp(0, 10_000) as u64
    }

    /// 解析后的快捷键表（缺失或非法条目回退默认绑定）。
    pub fn shortcut_map(&self) -> crate::domain::shortcut::ShortcutMap {
        crate::domain::shortcut::ShortcutMap::from_stored(&self.shortcut_key_map)
    }
}

// ── 练习状态 / 数据 ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeState {
    pub stage: WordPracticeStage,
    pub start_date: i64,
    pub spend: i64,
    pub total: usize,
    pub new_word_number: usize,
    pub review_word_number: usize,
    pub input_word_number: usize,
    pub wrong: usize,
    pub timer_paused: bool,
    pub timer_pause_reason: Option<String>,
    pub segments: Vec<[i64; 2]>,
}

impl Default for PracticeState {
    fn default() -> Self {
        Self {
            stage: WordPracticeStage::FollowWriteNewWord,
            start_date: 0,
            spend: 0,
            total: 0,
            new_word_number: 0,
            review_word_number: 0,
            input_word_number: 0,
            wrong: 0,
            timer_paused: false,
            timer_pause_reason: None,
            segments: Vec::new(),
        }
    }
}

impl PracticeState {
    pub fn reset_session(&mut self) {
        self.wrong = 0;
        self.input_word_number = 0;
        self.new_word_number = 0;
        self.review_word_number = 0;
        self.spend = 0;
        self.segments.clear();
        self.start_date = crate::domain::timer::now_ms();
        self.timer_paused = false;
        self.timer_pause_reason = None;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Candidate {
    pub word: Word,
    pub similarity: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Question {
    pub candidates: Vec<Candidate>,
    pub correct_index: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeData {
    pub index: usize,
    pub words: Vec<Word>,
    pub wrong_words: Vec<Word>,
    pub exclude_words: Vec<String>,
    pub all_wrong_words: Vec<String>,
    pub is_typing_wrong_word: bool,
    /// word -> wrongTimes
    pub wrong_times_map: HashMap<String, i64>,
    pub wrong_times: i64,
    pub rating_map: HashMap<String, Rating>,
    pub question: Option<Question>,
}

impl PracticeData {
    pub fn current_word(&self) -> Option<&Word> {
        self.words.get(self.index)
    }

    pub fn current_word_or_default(&self) -> Word {
        self.words.get(self.index).cloned().unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TaskWords {
    pub new: Vec<Word>,
    pub review: Vec<Word>,
}

impl TaskWords {
    pub fn total(&self) -> usize {
        self.new.len() + self.review.len()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

// ── 基础状态 ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct BaseState {
    pub simple_words: Vec<String>,
    pub load: bool,
    pub word: BookGroup,
    pub article: BookGroup,
    pub dict_list_version: u32,
    /// word -> FSRS 卡片
    pub fsrs_data: HashMap<String, Card>,
    /// word -> 个人笔记
    pub note_data: HashMap<String, String>,
    #[serde(rename = "_ignoreWatch")]
    pub ignore_watch: bool,
}

impl Default for BaseState {
    fn default() -> Self {
        Self {
            simple_words: SIMPLE_WORDS.iter().map(|word| word.to_string()).collect(),
            load: false,
            word: BookGroup {
                book_list: vec![
                    Dict::system(DICT_WORD_COLLECT, "收藏", ""),
                    Dict::system(DICT_WORD_WRONG, "错词", ""),
                    Dict::system(DICT_WORD_KNOWN, "已掌握", "已掌握后的单词不会出现在练习中"),
                ],
                study_index: -1,
            },
            article: BookGroup {
                book_list: vec![Dict::system(DICT_ARTICLE_COLLECT, "收藏", "")],
                study_index: -1,
            },
            dict_list_version: 1,
            fsrs_data: HashMap::new(),
            note_data: HashMap::new(),
            ignore_watch: false,
        }
    }
}

impl BaseState {
    fn find_system(&self, id: &str) -> Option<&Dict> {
        self.word
            .book_list
            .iter()
            .find(|dict| dict.system && dict.matches_id(id))
    }

    fn find_system_mut(&mut self, id: &str) -> Option<&mut Dict> {
        self.word
            .book_list
            .iter_mut()
            .find(|dict| dict.system && dict.matches_id(id))
    }

    pub fn collect_word(&self) -> Option<&Dict> {
        self.find_system(DICT_WORD_COLLECT)
    }

    pub fn wrong(&self) -> Option<&Dict> {
        self.find_system(DICT_WORD_WRONG)
    }

    pub fn known(&self) -> Option<&Dict> {
        self.find_system(DICT_WORD_KNOWN)
    }

    pub fn collect_word_mut(&mut self) -> Option<&mut Dict> {
        self.find_system_mut(DICT_WORD_COLLECT)
    }

    pub fn wrong_mut(&mut self) -> Option<&mut Dict> {
        self.find_system_mut(DICT_WORD_WRONG)
    }

    pub fn known_mut(&mut self) -> Option<&mut Dict> {
        self.find_system_mut(DICT_WORD_KNOWN)
    }

    pub fn known_words(&self) -> Vec<String> {
        self.known()
            .map(|dict| dict.words.iter().map(Word::lower).collect())
            .unwrap_or_default()
    }

    pub fn known_words_set(&self) -> std::collections::HashSet<String> {
        self.known()
            .map(|dict| dict.words.iter().map(Word::lower).collect())
            .unwrap_or_default()
    }

    pub fn all_ignore_words(&self) -> Vec<String> {
        let mut list = self.known_words();
        list.extend(self.simple_words.iter().map(|word| word.to_lowercase()));
        list
    }

    pub fn all_ignore_words_set(&self) -> std::collections::HashSet<String> {
        self.all_ignore_words().into_iter().collect()
    }

    /// 当前忽略集合：是否忽略简单词由设置决定（与 Web 的 getIgnoreWordsSet 一致）。
    pub fn ignore_words_set(&self, ignore_simple_word: bool) -> std::collections::HashSet<String> {
        if ignore_simple_word {
            self.all_ignore_words_set()
        } else {
            self.known_words_set()
        }
    }

    /// 当前学习词典。
    pub fn study_dict(&self) -> Option<&Dict> {
        if self.word.study_index < 0 {
            return None;
        }
        self.word.book_list.get(self.word.study_index as usize)
    }

    pub fn study_dict_mut(&mut self) -> Option<&mut Dict> {
        if self.word.study_index < 0 {
            return None;
        }
        self.word.book_list.get_mut(self.word.study_index as usize)
    }

    /// 设置当前学习词典：已存在则就地更新元数据，否则追加。
    pub fn set_study_dict(&mut self, dict: Dict) {
        if let Some(index) = self.word.book_list.iter().position(|item| {
            item.matches_id(&dict.id)
                || (!dict.en_name.is_empty() && item.matches_id(&dict.en_name))
        }) {
            let existing = &mut self.word.book_list[index];
            let words = if dict.words.is_empty() {
                std::mem::take(&mut existing.words)
            } else {
                dict.words
            };
            let last_learn_index = dict.last_learn_index;
            let per_day = dict.per_day_study_number;
            let complete = dict.complete;
            existing.words = words;
            existing.length = dict.length.max(existing.words.len());
            existing.last_learn_index = last_learn_index;
            existing.per_day_study_number = per_day;
            existing.complete = complete;
            existing.name = dict.name;
            existing.en_name = dict.en_name;
            self.word.study_index = index as i32;
        } else {
            let mut dict = dict;
            dict.length = dict.length.max(dict.words.len());
            self.word.book_list.push(dict);
            self.word.study_index = (self.word.book_list.len() - 1) as i32;
        }
    }

    /// 清理非系统词典的词表（存档只保留元数据，词表按需从资源加载）。
    pub fn shake_common_dict(&mut self) {
        for dict in &mut self.word.book_list {
            if !dict.custom && !dict.system {
                dict.words.clear();
            }
        }
        for dict in &mut self.article.book_list {
            if !dict.system {
                dict.articles.clear();
            }
        }
    }

    pub fn study_progress(&self) -> u32 {
        self.study_dict()
            .map(|dict| dict.progress_percent())
            .unwrap_or(0)
    }

    pub fn collect_count(&self) -> usize {
        self.collect_word()
            .map(|dict| dict.words.len())
            .unwrap_or(0)
    }

    pub fn wrong_count(&self) -> usize {
        self.wrong().map(|dict| dict.words.len()).unwrap_or(0)
    }

    pub fn known_count(&self) -> usize {
        self.known().map(|dict| dict.words.len()).unwrap_or(0)
    }
}

// ── 练习缓存（与 Web 的 PracticeSaveWord compact v2 对应）─────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TaskWordsStr {
    pub new: Vec<String>,
    pub review: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeDataCompact {
    pub index: usize,
    pub exclude_words: Vec<String>,
    pub all_wrong_words: Vec<String>,
    pub is_typing_wrong_word: bool,
    pub wrong_times_map: HashMap<String, i64>,
    pub wrong_times: i64,
    pub rating_map: HashMap<String, Rating>,
    pub words_str: Vec<String>,
    pub wrong_words_str: Vec<String>,
    pub question: Option<serde_json::Value>,
}

/// 会话快照（用于断点续练）。
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeSessionSnapshot {
    pub flow_id: String,
    pub cursor: PracticeFlowCursor,
    pub node_working_word_keys: Option<Vec<String>>,
}

/// 内存态的练习缓存（词表为完整的 Word，落盘时转字符串）。
#[derive(Clone, Debug, Default)]
pub struct PracticeCache {
    pub task_words: TaskWords,
    pub practice_data: PracticeData,
    pub stat_store_data: PracticeState,
    pub session_snapshot: PracticeSessionSnapshot,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeCacheCompact {
    pub task_words_str: TaskWordsStr,
    pub practice_data: Option<PracticeDataCompact>,
    pub stat_store_data: Option<PracticeState>,
    pub session_snapshot: Option<PracticeSessionSnapshot>,
}
