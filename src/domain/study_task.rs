//! 今日任务生成：新词 + FSRS 到期复习 + 随机复习填充。

use std::collections::{HashMap, HashSet};

use chrono::Local;

use crate::domain::rng::{Rng, shuffle};
use crate::domain::types::{BaseState, Card, SettingState, TaskWords, Word};

#[derive(Clone, Debug, Default)]
pub struct StudyTaskResult {
    pub task_words: TaskWords,
    pub due_review_count: usize,
    pub random_review_count: usize,
}

pub struct RandomReviewOptions<'a> {
    pub words: &'a [Word],
    pub last_learn_index: usize,
    pub per_day_study_number: usize,
    pub word_review_ratio: i64,
    pub ignore_set: &'a HashSet<String>,
    pub enabled: bool,
}

/// 无到期复习时，用已学词随机填充复习列表。
pub fn add_random_review_when_no_due(
    mut task_words: TaskWords,
    options: RandomReviewOptions<'_>,
    rng: &mut Rng,
) -> StudyTaskResult {
    let due_review_count = task_words.review.len();
    if !options.enabled || due_review_count > 0 {
        return StudyTaskResult {
            task_words,
            due_review_count,
            random_review_count: 0,
        };
    }

    let total_need =
        (options.per_day_study_number as i64 * options.word_review_ratio).max(0) as usize;
    if total_need == 0 {
        return StudyTaskResult {
            task_words,
            due_review_count,
            random_review_count: 0,
        };
    }

    let mut excluded: HashSet<String> = options.ignore_set.clone();
    excluded.extend(task_words.new.iter().map(|word| word.word.clone()));
    excluded.extend(task_words.review.iter().map(|word| word.word.clone()));

    let learned_end = options.last_learn_index.min(options.words.len());
    let mut candidates: Vec<Word> = options.words[..learned_end]
        .iter()
        .filter(|word| !excluded.contains(&word.word))
        .cloned()
        .collect();
    shuffle(rng, &mut candidates);
    let picked: Vec<Word> = candidates.into_iter().take(total_need).collect();
    let random_review_count = picked.len();
    task_words.review.extend(picked);

    StudyTaskResult {
        task_words,
        due_review_count,
        random_review_count,
    }
}

fn is_due_today(card: &Card, today: chrono::NaiveDate) -> bool {
    card.due.with_timezone(&Local).date_naive() <= today
}

/// 对应 Web 的 getCurrentStudyWord：新词 + 到期复习。
pub fn get_current_study_word(
    base: &BaseState,
    settings: &SettingState,
    rng: &mut Rng,
) -> TaskWords {
    let Some(dict) = base.study_dict() else {
        return TaskWords::default();
    };
    let words = &dict.words;
    if words.is_empty() {
        return TaskWords::default();
    }

    let ignore_set = base.ignore_words_set(settings.ignore_simple_word);
    let per_day = dict.per_day();
    let start = dict.last_learn_index.min(words.len());
    let complete = dict.complete;
    let is_end = start >= dict.length.saturating_sub(1) && dict.length != 1;
    let review_ratio = settings.word_review_ratio;

    let mut new_words: Vec<Word> = Vec::new();
    let mut end = start;
    if !is_end {
        for item in words.iter().skip(start) {
            if new_words.len() >= per_day {
                break;
            }
            if !ignore_set.contains(&item.lower()) {
                new_words.push(item.clone());
            }
            end += 1;
        }
    }

    let mut review_words: Vec<Word> = Vec::new();
    if review_ratio >= 1 || complete || is_end {
        let word_map: HashMap<&str, &Word> = words
            .iter()
            .map(|word| (word.word.as_str(), word))
            .collect();
        let ratio = if is_end {
            review_ratio.max(1)
        } else {
            review_ratio
        };
        let total_need = (per_day as i64 * ratio).max(0) as usize;
        let today = Local::now().date_naive();
        let new_keys: HashSet<&str> = new_words.iter().map(|word| word.word.as_str()).collect();

        let mut due: Vec<(&String, &Card)> = base
            .fsrs_data
            .iter()
            .filter(|(word, card)| {
                !ignore_set.contains(&word.to_lowercase())
                    && is_due_today(card, today)
                    && word_map.contains_key(word.as_str())
                    && !new_keys.contains(word.as_str())
            })
            .collect();
        due.sort_by_key(|(_, card)| card.due);

        review_words = due
            .into_iter()
            .take(total_need)
            .filter_map(|(word, _)| word_map.get(word.as_str()).map(|item| (*item).clone()))
            .collect();
        shuffle(rng, &mut review_words);
    }

    let _ = end;
    TaskWords {
        new: new_words,
        review: review_words,
    }
}

/// 生成今日任务（含随机复习填充）。
pub fn create_study_task(
    base: &BaseState,
    settings: &SettingState,
    rng: &mut Rng,
) -> StudyTaskResult {
    let Some(dict) = base.study_dict() else {
        return StudyTaskResult::default();
    };
    let task_words = get_current_study_word(base, settings, rng);
    let ignore_set = base.ignore_words_set(settings.ignore_simple_word);
    add_random_review_when_no_due(
        task_words,
        RandomReviewOptions {
            words: &dict.words,
            last_learn_index: dict.last_learn_index,
            per_day_study_number: dict.per_day(),
            word_review_ratio: settings.word_review_ratio,
            ignore_set: &ignore_set,
            enabled: settings.auto_add_random_review_when_no_due,
        },
        rng,
    )
}

/// 随机复习模式：从已学范围随机抽取。
pub fn create_shuffle_task(
    base: &BaseState,
    settings: &SettingState,
    total: Option<usize>,
    rng: &mut Rng,
) -> TaskWords {
    let Some(dict) = base.study_dict() else {
        return TaskWords::default();
    };
    let ignore_set = base.ignore_words_set(settings.ignore_simple_word);
    let end = if dict.last_learn_index == 0 {
        dict.words.len()
    } else {
        dict.last_learn_index.min(dict.words.len())
    };
    let total = total.unwrap_or_else(|| {
        (dict.per_day() as i64 * settings.word_review_ratio.max(1)).max(1) as usize
    });
    let mut candidates: Vec<Word> = dict.words[..end]
        .iter()
        .filter(|word| !ignore_set.contains(&word.lower()))
        .cloned()
        .collect();
    shuffle(rng, &mut candidates);
    TaskWords {
        new: Vec::new(),
        review: candidates.into_iter().take(total).collect(),
    }
}
