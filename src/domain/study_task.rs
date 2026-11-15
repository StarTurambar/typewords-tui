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
    // 进度分母与 `per_day()` / 结算推进同源。读作 `length` 时，元数据比实际
    // 词表大就永远判不出「最后一组」，整本学完也进不了总复习。
    let total = dict.effective_length();
    let start = dict.last_learn_index.min(total);
    let complete = dict.complete;
    // 「到头」= 新词已经全部学完。不能写成 `total - 1`：只剩最后一个词时
    // 那样会被判成「已到头」，新词列表恒空、`last_learn_index` 停在 `total - 1`，
    // 词典永远学不完（词数 ≡ 1 mod 每日词数时必现，例如 21 词 + 每日 20）。
    // 上游 `dict.ts` 的 `isEnd` 同样是 `length - 1`，这里刻意比上游正确。
    let is_end = start >= total;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rng::seeded;
    use crate::domain::types::{Dict, SettingState};

    fn base_with_words(count: usize, last_learn_index: usize, per_day: usize) -> BaseState {
        let mut base = BaseState::default();
        base.word.book_list.push(Dict {
            length: count,
            last_learn_index,
            per_day_study_number: per_day,
            words: (0..count)
                .map(|index| Word::new(format!("w{index}")))
                .collect(),
            ..Dict::default()
        });
        // 默认存档里已经有三本内置词典，当前词典是刚追加的那本。
        base.word.study_index = (base.word.book_list.len() - 1) as i32;
        base
    }

    /// 只剩最后一个词时也要把它出成新词：`is_end` 不能在 `total - 1` 就成立。
    #[test]
    fn the_last_word_is_still_offered_as_a_new_word() {
        let base = base_with_words(2, 1, 1);
        let mut rng = seeded(7);
        let task = get_current_study_word(&base, &SettingState::default(), &mut rng);
        assert_eq!(task.new.len(), 1, "还剩最后一个词，应该出题");
        assert_eq!(task.new[0].word, "w1");
    }

    /// 词表学完之后不再出新词，转入复习。
    #[test]
    fn a_finished_word_list_offers_no_new_words() {
        let base = base_with_words(2, 2, 1);
        let mut rng = seeded(7);
        let task = get_current_study_word(&base, &SettingState::default(), &mut rng);
        assert!(task.new.is_empty(), "学完之后不该再出新词");
    }
}
