//! 与 Web 端保持一致的常量与默认值。

use crate::domain::types::{FsrsParameters, WordPracticeMode, WordPracticeStage};

/// 跟写分组大小（对应 Web 的 PRACTICE_WORD_GROUP_SIZE）。
pub const GROUP_SIZE: usize = 7;

/// 内置流程版本（对应 Web 的 CURRENT_FLOW_VERSION）。
pub const CURRENT_FLOW_VERSION: u32 = 6;

/// 内置词典 id（系统虚拟词典）。
pub const DICT_WORD_COLLECT: &str = "wordCollect";
pub const DICT_WORD_WRONG: &str = "wordWrong";
pub const DICT_WORD_KNOWN: &str = "wordKnown";
pub const DICT_ARTICLE_COLLECT: &str = "articleCollect";

/// 默认“简单词”列表，与 Web 的 getDefaultBaseState 一致。
pub const SIMPLE_WORDS: &[&str] = &[
    "a", "an", "i", "my", "me", "you", "your", "he", "his", "she", "her", "it", "what", "who",
    "where", "how", "when", "which", "be", "am", "is", "was", "are", "were", "do", "did", "can",
    "could", "will", "would", "the", "that", "this", "and", "not", "no", "yes", "to", "of", "for",
    "at", "in",
];

/// Web 的默认 FSRS-6 权重（21 个参数）。
pub const DEFAULT_FSRS_W: [f64; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];

pub fn default_fsrs_parameters() -> FsrsParameters {
    FsrsParameters {
        request_retention: 0.9,
        maximum_interval: 36500.0,
        w: DEFAULT_FSRS_W.to_vec(),
        enable_fuzz: false,
        enable_short_term: true,
        learning_steps: vec!["1m".to_string(), "10m".to_string()],
        relearning_steps: vec!["10m".to_string()],
    }
}

/// 练习模式名称（顺序与 Web 的 WordPracticeMode 一致）。
pub const PRACTICE_MODE_NAMES: &[(WordPracticeMode, &str)] = &[
    (WordPracticeMode::System, "学习"),
    (WordPracticeMode::Free, "自由练习"),
    (WordPracticeMode::IdentifyOnly, "自测"),
    (WordPracticeMode::DictationOnly, "默写"),
    (WordPracticeMode::ListenOnly, "听写"),
    (WordPracticeMode::Shuffle, "随机复习"),
    (WordPracticeMode::Review, "复习"),
    (WordPracticeMode::ShuffleWordsTest, "随机单词测试"),
    (WordPracticeMode::ReviewWordsTest, "单词测试"),
    (WordPracticeMode::Custom, "自定义流程"),
];

/// 全部可选的练习模式（顺序与上游 `WordPracticeMode` 一致）。
///
/// `Custom` 只在有已启用的自建流程时才有意义，但始终列在模式里——否则用户
/// 无法显式切到自建流程，也无法在选内置模式时避开它。
pub const PRACTICE_MODES: [WordPracticeMode; 10] = [
    WordPracticeMode::System,
    WordPracticeMode::Free,
    WordPracticeMode::IdentifyOnly,
    WordPracticeMode::DictationOnly,
    WordPracticeMode::ListenOnly,
    WordPracticeMode::Shuffle,
    WordPracticeMode::Review,
    WordPracticeMode::ShuffleWordsTest,
    WordPracticeMode::ReviewWordsTest,
    WordPracticeMode::Custom,
];

/// 该模式是否走「单词测试」流程（4 选 1 答题），而不是键入练习。
pub fn is_words_test_mode(mode: WordPracticeMode) -> bool {
    matches!(
        mode,
        WordPracticeMode::ShuffleWordsTest | WordPracticeMode::ReviewWordsTest
    )
}

pub fn practice_mode_name(mode: WordPracticeMode) -> &'static str {
    PRACTICE_MODE_NAMES
        .iter()
        .find(|(value, _)| *value == mode)
        .map(|(_, name)| *name)
        .unwrap_or("学习")
}

/// 阶段名称（顺序与 Web 的 WordPracticeStage 一致）。
pub fn stage_name(stage: WordPracticeStage) -> &'static str {
    match stage {
        WordPracticeStage::FollowWriteNewWord => "跟写新词",
        WordPracticeStage::IdentifyNewWord => "自测新词",
        WordPracticeStage::ListenNewWord => "听写新词",
        WordPracticeStage::DictationNewWord => "默写新词",
        WordPracticeStage::FollowWriteReview => "跟写旧词",
        WordPracticeStage::IdentifyReview => "自测旧词",
        WordPracticeStage::ListenReview => "听写旧词",
        WordPracticeStage::DictationReview => "默写旧词",
        WordPracticeStage::Shuffle => "随机复习",
        WordPracticeStage::Complete => "完成学习",
    }
}

/// 词典状态存档版本（Web 的 SAVE_DICT_KEY.version）。
pub const DICT_STATE_VERSION: u32 = 4;
/// 设置存档版本（Web 的 SAVE_SETTING_KEY.version）。
pub const SETTING_VERSION: u32 = 23;
/// 练习缓存版本（Web 的 PRACTICE_WORD_CACHE.version）。
pub const PRACTICE_CACHE_VERSION: u32 = 2;
