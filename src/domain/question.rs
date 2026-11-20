//! 自测选择题生成（对应 Web 的 buildQuestion）。

use std::cmp::Ordering;
use std::collections::HashMap;

use rand::prelude::*;

use crate::domain::rng::{Rng, shuffle};
use crate::domain::types::{Candidate, Question, Word};

// 相似度全部在 i128 里累加：`Trans.frequency` 直接来自词库 JSON（导入词库时
// 不做值域校验），两个 1e15 量级的频率相乘再乘 `TRAN_EQUALS_BASE_SCORE` 就装不进
// i64——release 构建静默回绕（只影响干扰项排序），debug 构建直接 panic。i128 足以
// 覆盖任意 i64 频率的两两乘积，再叠一层饱和运算兜住乘积之后的乘法：极端词库最多
// 让排序并列，不会 panic。最后统一 `as f64`，与原来的排序语义一致。
const CHARS_CONTAIN_SCORE: i128 = 1 << 2;
const CHARS_NOT_CONTAIN_SCORE: i128 = -1;
const TRAN_EQUALS_BASE_SCORE: i128 = 1 << 16;
const TRANS_CHARS_COMMON_FACTOR: i128 = 1;
const WORD_COMMON_FACTOR: i128 = 1 << 16;
const WORD_CONTAIN_SCORE: i128 = 1 << 20;
const FUZZ_SCORE_BASE: f64 = (1 << 18) as f64;

fn get_trans(word: &Word) -> Vec<(String, i128)> {
    let mut map: HashMap<String, i128> = HashMap::new();
    for item in &word.trans {
        let entry = map.entry(item.cn.clone()).or_insert(0);
        *entry += i128::from(item.frequency.unwrap_or(0));
    }
    map.into_iter().collect()
}

fn cal_common(left: &str, right: &str) -> i128 {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let set_left: std::collections::HashSet<char> = left.chars().collect();
    let set_right: std::collections::HashSet<char> = right.chars().collect();
    let mut score = 0;
    for character in &set_left {
        score += if set_right.contains(character) {
            CHARS_CONTAIN_SCORE
        } else {
            CHARS_NOT_CONTAIN_SCORE
        };
    }
    for character in &set_right {
        score += if set_left.contains(character) {
            CHARS_CONTAIN_SCORE
        } else {
            CHARS_NOT_CONTAIN_SCORE
        };
    }
    score
}

fn cal_similarity(left: &Word, right: &Word, rng: &mut Rng) -> f64 {
    let (word1, word2) = if right.trans.len() < left.trans.len() {
        (right, left)
    } else {
        (left, right)
    };
    let word1_trans = get_trans(word1);
    let word2_trans = get_trans(word2);
    let word2_map: HashMap<&str, &(String, i128)> = word2_trans
        .iter()
        .map(|item| (item.0.as_str(), item))
        .collect();

    let mut similarity: i128 = 0;
    for item in &word1_trans {
        if let Some(other) = word2_map.get(item.0.as_str()) {
            // 饱和乘法只是兜底：两个 `i64::MAX` 频率相乘后仍放得进 i128，
            // 再乘 `TRAN_EQUALS_BASE_SCORE` 才会越界。
            similarity = similarity.saturating_add(
                item.1
                    .saturating_mul(other.1)
                    .saturating_mul(TRAN_EQUALS_BASE_SCORE),
            );
        }
    }

    let joined1: String = word1_trans.iter().map(|item| item.0.as_str()).collect();
    let joined2: String = word2_trans.iter().map(|item| item.0.as_str()).collect();
    similarity =
        similarity.saturating_add(cal_common(&joined1, &joined2) * TRANS_CHARS_COMMON_FACTOR);
    similarity =
        similarity.saturating_add(cal_common(&word1.word, &word2.word) * WORD_COMMON_FACTOR);

    if word1.word.contains(&word2.word) || word2.word.contains(&word1.word) {
        similarity = similarity.saturating_add(WORD_CONTAIN_SCORE);
    }

    let has_rel = |source: &Word, target: &Word| {
        source
            .rel_words
            .rels
            .iter()
            .any(|group| group.words.iter().any(|item| item.c == target.word))
    };
    if has_rel(word1, word2) || has_rel(word2, word1) {
        similarity = similarity.saturating_add(WORD_CONTAIN_SCORE);
    }

    let fuzz = rng.random_range(0.0f64..1.0f64).powi(2) * FUZZ_SCORE_BASE;
    similarity as f64 + fuzz
}

/// 构建 4 选 1 单选题。
pub fn build_question(
    word: &Word,
    list: &[Word],
    rng: &mut Rng,
    max_count: usize,
) -> Option<Question> {
    let max_count = max_count.max(2);
    let target_word = word.word.to_lowercase();
    let target_trans: String = word.trans.iter().map(|item| item.cn.as_str()).collect();

    let mut scored: Vec<(Word, f64)> = Vec::new();
    for item in list {
        let item_word = item.word.to_lowercase();
        if item_word == target_word {
            continue;
        }
        let item_trans: String = item.trans.iter().map(|entry| entry.cn.as_str()).collect();
        if item_trans == target_trans {
            continue;
        }
        let similarity = cal_similarity(word, item, rng);
        scored.push((item.clone(), similarity));
    }

    scored.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(Ordering::Equal));

    let take = (max_count - 1).min(scored.len());
    let mut candidates: Vec<Candidate> = scored
        .split_off(scored.len().saturating_sub(take))
        .into_iter()
        .map(|(item, similarity)| Candidate {
            word: item,
            similarity,
        })
        .collect();
    candidates.push(Candidate {
        word: word.clone(),
        similarity: f64::INFINITY,
    });
    shuffle(rng, &mut candidates);
    let correct_index = candidates
        .iter()
        .position(|candidate| candidate.word.word == word.word)
        .unwrap_or(0);
    Some(Question {
        candidates,
        correct_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rng::seeded;
    use crate::domain::types::Trans;

    fn word_with_trans(word: &str, cn: &str, frequency: i64) -> Word {
        Word {
            word: word.to_string(),
            trans: vec![Trans {
                pos: String::new(),
                cn: cn.to_string(),
                frequency: Some(frequency),
            }],
            ..Word::default()
        }
    }

    /// `frequency` 来自词库 JSON、导入时不校验值域：两个 1e15 量级的频率在 i64
    /// 里相乘就会溢出（debug 构建直接 panic），改 i128 累加后应只影响排序。
    #[test]
    fn extreme_frequencies_do_not_overflow_similarity() {
        let left = word_with_trans("alpha", "同一个释义", 1_000_000_000_000_000);
        let right = word_with_trans("beta", "同一个释义", 1_000_000_000_000_000);
        let mut rng = seeded(7);
        let similarity = cal_similarity(&left, &right, &mut rng);
        assert!(
            similarity.is_finite(),
            "相似度应为有限值，实际 {similarity}"
        );
        assert!(
            similarity > 0.0,
            "同释义的正频率应给出正相似度，实际 {similarity}"
        );
    }

    /// 频率写到 i64 上下界时，i128 里的乘积仍可能越界：饱和兜底保证不 panic、
    /// 结果仍是有限值，而不是回绕成负分。
    #[test]
    fn frequencies_at_i64_bounds_stay_finite() {
        let left = word_with_trans("alpha", "同一个释义", i64::MAX);
        let right = word_with_trans("beta", "同一个释义", i64::MAX);
        let mut rng = seeded(7);
        let similarity = cal_similarity(&left, &right, &mut rng);
        assert!(
            similarity.is_finite(),
            "相似度应为有限值，实际 {similarity}"
        );
        assert!(
            similarity > 0.0,
            "两个正频率的乘积应给出正相似度，实际 {similarity}"
        );
    }

    /// 负频率按算术走：不 panic，符号随乘积。
    #[test]
    fn negative_frequencies_do_not_panic() {
        let left = word_with_trans("alpha", "同一个释义", -1_000_000_000_000_000);
        let right = word_with_trans("beta", "同一个释义", 1_000_000_000_000_000);
        let mut rng = seeded(7);
        let similarity = cal_similarity(&left, &right, &mut rng);
        assert!(
            similarity.is_finite(),
            "相似度应为有限值，实际 {similarity}"
        );
        assert!(
            similarity < 0.0,
            "一正一负的频率相乘为负，相似度应低于纯字符分，实际 {similarity}"
        );
    }

    /// 正常频率下排序语义与改前一致：词形相同、只有释义频率不同的两个干扰项，
    /// 频率高的更相似（差距 9900 × 65536 远大于随机扰动上界 2^18）。
    #[test]
    fn normal_frequencies_keep_the_similarity_order() {
        let target = word_with_trans("alpha", "同一个释义", 100);
        let near = word_with_trans("beta", "同一个释义", 100);
        let far = word_with_trans("beta", "同一个释义", 1);
        for seed in 0..16 {
            let mut rng = seeded(seed);
            let near_score = cal_similarity(&target, &near, &mut rng);
            let far_score = cal_similarity(&target, &far, &mut rng);
            assert!(
                near_score > far_score,
                "seed {seed}：频率高的干扰项应更相似（{near_score} vs {far_score}）"
            );
        }
    }
}
