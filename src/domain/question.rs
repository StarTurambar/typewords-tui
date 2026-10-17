//! 自测选择题生成（对应 Web 的 buildQuestion）。

use std::cmp::Ordering;
use std::collections::HashMap;

use rand::prelude::*;

use crate::domain::rng::{Rng, shuffle};
use crate::domain::types::{Candidate, Question, Word};

const CHARS_CONTAIN_SCORE: i64 = 1 << 2;
const CHARS_NOT_CONTAIN_SCORE: i64 = -1;
const TRAN_EQUALS_BASE_SCORE: i64 = 1 << 16;
const TRANS_CHARS_COMMON_FACTOR: i64 = 1;
const WORD_COMMON_FACTOR: i64 = 1 << 16;
const WORD_CONTAIN_SCORE: i64 = 1 << 20;
const FUZZ_SCORE_BASE: f64 = (1 << 18) as f64;

fn get_trans(word: &Word) -> Vec<(String, i64)> {
    let mut map: HashMap<String, i64> = HashMap::new();
    for item in &word.trans {
        let entry = map.entry(item.cn.clone()).or_insert(0);
        *entry += item.frequency.unwrap_or(0);
    }
    map.into_iter().collect()
}

fn cal_common(left: &str, right: &str) -> i64 {
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
    let word2_map: HashMap<&str, &(String, i64)> = word2_trans
        .iter()
        .map(|item| (item.0.as_str(), item))
        .collect();

    let mut similarity: i64 = 0;
    for item in &word1_trans {
        if let Some(other) = word2_map.get(item.0.as_str()) {
            similarity += item.1 * other.1 * TRAN_EQUALS_BASE_SCORE;
        }
    }

    let joined1: String = word1_trans.iter().map(|item| item.0.as_str()).collect();
    let joined2: String = word2_trans.iter().map(|item| item.0.as_str()).collect();
    similarity += cal_common(&joined1, &joined2) * TRANS_CHARS_COMMON_FACTOR;
    similarity += cal_common(&word1.word, &word2.word) * WORD_COMMON_FACTOR;

    if word1.word.contains(&word2.word) || word2.word.contains(&word1.word) {
        similarity += WORD_CONTAIN_SCORE;
    }

    let has_rel = |source: &Word, target: &Word| {
        source
            .rel_words
            .rels
            .iter()
            .any(|group| group.words.iter().any(|item| item.c == target.word))
    };
    if has_rel(word1, word2) || has_rel(word2, word1) {
        similarity += WORD_CONTAIN_SCORE;
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
