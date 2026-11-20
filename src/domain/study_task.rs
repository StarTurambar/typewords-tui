//! 今日任务生成：新词 + FSRS 到期复习 + 随机复习填充。

use std::collections::{HashMap, HashSet};

use chrono::Local;

use crate::domain::rng::{Rng, shuffle};
use crate::domain::scheduler::Scheduler;
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

/// 复习比例在界面上的上限（设置页编辑时 `clamp(0, 10)`）。
const REVIEW_RATIO_MAX: i64 = 10;

/// 复习比例的有效值：界面把 `wordReviewRatio` 夹在 0~10，这里按同一口径兜底
/// 手改存档 / 导入网页备份带进来的极端值——`per_day × ratio` 直接在 i64 里相乘时，
/// `wordReviewRatio = i64::MAX` 会让 release 静默回绕（任务数量算错）、debug panic。
fn effective_review_ratio(settings: &SettingState) -> i64 {
    clamp_review_ratio(settings.word_review_ratio)
}

/// 只拿到裸比例的调用点（`RandomReviewOptions`）复用同一口径。
fn clamp_review_ratio(ratio: i64) -> i64 {
    ratio.clamp(0, REVIEW_RATIO_MAX)
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

    let total_need = (options.per_day_study_number as i64)
        .saturating_mul(clamp_review_ratio(options.word_review_ratio))
        .max(0) as usize;
    if total_need == 0 {
        return StudyTaskResult {
            task_words,
            due_review_count,
            random_review_count: 0,
        };
    }

    let mut excluded: HashSet<String> = options.ignore_set.clone();
    excluded.extend(task_words.new.iter().map(Word::lower));
    excluded.extend(task_words.review.iter().map(Word::lower));

    let learned_end = options.last_learn_index.min(options.words.len());
    let mut candidates: Vec<Word> = options.words[..learned_end]
        .iter()
        // 忽略集合里全是小写键（`ignore_words_set`），这里也必须按小写比较：
        // `Christ` 这类首字母大写的已掌握 / 简单词不能重新抽进随机复习。
        .filter(|word| !excluded.contains(&word.lower()))
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
    // 学完判定只看词表（`words`，即实际词数），不看词库目录里的登记值
    // `dict.length`：删过词 / 换过更短的词库文件后登记值会大于真实词数，
    // 按它判「是否到末尾」就永远到不了，`complete` 永远不置位（主页会一直
    // 停在预计完成日期）。
    //
    // 「已到末尾」= 扫描起点之后已没有可学的新词（剩余词全在忽略集合里也算）。
    // 不能按位置判 `start >= 实际词数 - 1`：词表刚好只剩最后一个词时那个词
    // 永远取不到，而 `advance_dict_progress` 按「已学词数到实际词数」判完成，
    // `complete` 也就永远不置位——症状与登记值偏大时一模一样。
    let has_learnable_new = words[start..]
        .iter()
        .any(|word| !ignore_set.contains(&word.lower()));
    let is_end = !has_learnable_new;
    let review_ratio = effective_review_ratio(settings);

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
        let total_need = (per_day as i64).saturating_mul(ratio).max(0) as usize;
        let today = Local::now().date_naive();
        let new_keys: HashSet<&str> = new_words.iter().map(|word| word.word.as_str()).collect();

        let mut due: Vec<(&String, &Card)> = base
            .fsrs_data
            .iter()
            .filter(|(word, card)| {
                !ignore_set.contains(&word.to_lowercase())
                    && Scheduler::is_due_today(card, today)
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
        (dict.per_day() as i64)
            .saturating_mul(effective_review_ratio(settings).max(1))
            .max(1) as usize
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
    use crate::domain::types::Dict;

    /// 构造一本「目录登记词数」与词表实际词数不一致的学习词典。
    ///
    /// 词库目录里的 `length` 只是登记值：用户删过词、或词库文件被换成
    /// 更短的版本后，登记值会大于词表里的实际词数。
    fn base_with(length: usize, words: usize, last_learn_index: usize) -> BaseState {
        let mut base = BaseState::default();
        let words: Vec<Word> = (0..words)
            .map(|index| Word::new(format!("word{index:02}")))
            .collect();
        base.word.book_list.push(Dict {
            id: "toy".to_string(),
            en_name: "toy".to_string(),
            name: "测试词库".to_string(),
            words,
            length,
            last_learn_index,
            per_day_study_number: 20,
            ..Dict::default()
        });
        let index = base.word.book_list.len() - 1;
        base.set_study_index(index);
        base
    }

    /// 学完判定按词表实际词数走：登记值偏大时，实际词表已到尾就不再取新词，
    /// 也不会因为还没追平登记值而把任务判成未完成。
    #[test]
    fn study_task_stops_new_words_at_the_actual_word_count() {
        // 登记 8 词、实际 5 词，5 个实际词都已学完。
        let base = base_with(8, 5, 5);
        let settings = SettingState::default();
        let mut rng = seeded(7);
        let task = create_study_task(&base, &settings, &mut rng).task_words;
        assert!(
            task.new.is_empty(),
            "实际词表已到尾，不应再取新词，实际取了 {:?}",
            task.new
                .iter()
                .map(|word| word.word.clone())
                .collect::<Vec<_>>()
        );
    }

    /// 只剩最后一个实际词时仍要把它当新词取出来：按位置判「末尾」（`start >=
    /// length - 1`）会跳过它，那个词永远学不到，`complete` 也就永远不置位。
    #[test]
    fn the_last_remaining_word_is_still_offered_as_a_new_word() {
        // 12 词、已学 11 个，只剩最后一个。
        let base = base_with(12, 12, 11);
        let settings = SettingState::default();
        let mut rng = seeded(7);
        let task = create_study_task(&base, &settings, &mut rng).task_words;
        assert_eq!(task.new.len(), 1, "最后一个词应作为新词出现");
        assert_eq!(task.new[0].word, "word11");
    }

    /// 剩余词全在忽略集合里时直接转入复习：不能因为取不到新词就给出空任务，
    /// 那会让会话无法结算、`complete` 永远不置位。
    #[test]
    fn study_task_falls_back_to_review_when_every_remaining_word_is_ignored() {
        let mut base = base_with(5, 5, 3);
        let ignored: Vec<Word> = base.study_dict().expect("当前词典").words[3..].to_vec();
        base.known_mut().expect("已掌握词典").words = ignored;
        // 已学过的词都到期（`Card::default().due` 是现在），复习分支才有的可抽。
        for word in ["word00", "word01", "word02"] {
            base.fsrs_data.insert(word.to_string(), Card::default());
        }
        let settings = SettingState {
            word_review_ratio: 0,
            ..SettingState::default()
        };
        let mut rng = seeded(7);
        let task = create_study_task(&base, &settings, &mut rng).task_words;
        assert!(task.new.is_empty(), "剩余词都被忽略，不应再取新词");
        assert!(
            !task.review.is_empty(),
            "剩余词全被忽略时应转入复习，而不是给出空任务"
        );
    }

    /// 反过来，登记值偏小时也不能提前停止：学完判定不能被偏小的登记值截断。
    #[test]
    fn study_task_ignores_a_length_registry_smaller_than_the_word_list() {
        // 登记 2 词、实际 5 词，已学 1 个：还剩 4 个实际新词可学。
        let base = base_with(2, 5, 1);
        let settings = SettingState::default();
        let mut rng = seeded(7);
        let task = create_study_task(&base, &settings, &mut rng).task_words;
        assert_eq!(task.new.len(), 4, "应按实际词表继续取新词");
    }

    /// 随机复习填充同样按小写比较忽略集合：`Christ` 这类大写已掌握词不能被抽中。
    #[test]
    fn random_review_skips_capitalized_ignored_words() {
        let mut base = base_with_words(4, 4, 20);
        base.study_dict_mut().expect("当前词典").words[1] = Word::new("Christ");
        base.known_mut()
            .expect("已掌握词典")
            .words
            .push(Word::new("Christ"));
        let settings = SettingState {
            auto_add_random_review_when_no_due: true,
            word_review_ratio: 1,
            ..SettingState::default()
        };
        let mut rng = seeded(7);
        let task = create_study_task(&base, &settings, &mut rng).task_words;
        assert!(
            !task.review.iter().any(|word| word.word == "Christ"),
            "已掌握的大写词不应进入随机复习: {:?}",
            task.review
                .iter()
                .map(|word| word.word.clone())
                .collect::<Vec<_>>()
        );
    }

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

    /// 手改 `wordReviewRatio` 为 `i64::MAX` 时到期复习数量不能溢出：比例按界面
    /// 口径夹到 10，数量再受词表上限约束（debug 构建下旧代码会在这里 panic）。
    #[test]
    fn extreme_review_ratio_does_not_overflow_due_review_count() {
        let mut base = base_with_words(100, 50, 20);
        for index in 0..50 {
            base.fsrs_data.insert(format!("w{index}"), Card::default());
        }
        let settings = SettingState {
            word_review_ratio: i64::MAX,
            auto_add_random_review_when_no_due: true,
            ..SettingState::default()
        };
        let mut rng = seeded(7);
        let task = create_study_task(&base, &settings, &mut rng).task_words;
        assert_eq!(task.new.len(), 20, "新词仍按每日词数取满");
        assert!(
            task.review.len() <= 50,
            "复习词不能超过已学词数，实际 {} 个",
            task.review.len()
        );
        assert!(
            task.review.len() <= 20 * 10,
            "复习词不能超过每日词数 × 界面比例上限，实际 {} 个",
            task.review.len()
        );
    }

    /// 无到期复习时的随机填充同样按夹取后的比例算：`i64::MAX` 只能拿到
    /// `per_day × 10` 与已学词数里较小的那个。
    #[test]
    fn extreme_review_ratio_does_not_overflow_random_review_fill() {
        let base = base_with_words(100, 50, 20);
        let settings = SettingState {
            word_review_ratio: i64::MAX,
            auto_add_random_review_when_no_due: true,
            ..SettingState::default()
        };
        let mut rng = seeded(7);
        let result = create_study_task(&base, &settings, &mut rng);
        assert_eq!(result.due_review_count, 0, "没有到期卡片时才走随机填充");
        assert_eq!(
            result.random_review_count, 50,
            "已学 50 词，夹取后的 200 个名额只取得到这 50 个"
        );
    }

    /// 比例 10 是界面允许的上限：到期复习按 `per_day × 10` 取，不多取。
    #[test]
    fn review_ratio_ten_is_the_upper_bound() {
        let mut base = base_with_words(100, 100, 2);
        for index in 0..100 {
            base.fsrs_data.insert(format!("w{index}"), Card::default());
        }
        let settings = SettingState {
            word_review_ratio: 10,
            ..SettingState::default()
        };
        let mut rng = seeded(7);
        let task = create_study_task(&base, &settings, &mut rng).task_words;
        assert_eq!(
            task.review.len(),
            20,
            "每日 2 词 × 比例 10 应取 20 个到期复习词"
        );
    }

    /// 比例 0 仍是「不补随机复习」：夹取不能把 0 变成 1。
    #[test]
    fn review_ratio_zero_still_disables_random_fill() {
        let base = base_with_words(10, 5, 5);
        let settings = SettingState {
            word_review_ratio: 0,
            auto_add_random_review_when_no_due: true,
            ..SettingState::default()
        };
        let mut rng = seeded(7);
        let result = create_study_task(&base, &settings, &mut rng);
        assert_eq!(result.random_review_count, 0, "比例 0 不应随机补复习");
    }
}
