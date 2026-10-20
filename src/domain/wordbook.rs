//! 词条管理：搜索过滤、自定义词典、批量标记。
//!
//! 对齐上游 `app/pages/(words)/dict.vue`、`components/word/WordMarkPickList.vue`
//! 与 `components/word/WordCollectPopover.vue` 的行为。

use crate::domain::types::{BaseState, Dict, Word};

/// 批量标记的种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// 认识：记一次 Good 评级，本轮不再出现
    Know,
    /// 已掌握：加入「已掌握」词典，本轮不再出现
    Mastered,
    /// 不认识：加入「错词」词典
    Unknown,
}

/// 按搜索词过滤词条，返回命中的下标。
///
/// 搜索不区分大小写；空搜索返回全部。匹配单词本身或任意一条释义。
pub fn filter_words(words: &[Word], filter: &str) -> Vec<usize> {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return (0..words.len()).collect();
    }
    words
        .iter()
        .enumerate()
        .filter(|(_, word)| {
            word.word.to_lowercase().contains(&needle)
                || word
                    .trans
                    .iter()
                    .any(|item| item.cn.to_lowercase().contains(&needle))
        })
        .map(|(index, _)| index)
        .collect()
}

/// 新建一本自定义词典，返回它在 `word.book_list` 里的下标。
///
/// 自定义词典的词条存在存档里（`custom: true`），保存时不会被清空。
pub fn create_custom_dict(base: &mut BaseState, name: &str, words: Vec<Word>) -> usize {
    let name = name.trim();
    let name = if name.is_empty() {
        "我的词库".to_string()
    } else {
        name.to_string()
    };
    let id = format!("custom-{}", base.word.book_list.len());
    let length = words.len();
    let dict = Dict {
        id: id.clone(),
        en_name: id,
        name: name.clone(),
        description: "自建词库".to_string(),
        url: String::new(),
        length,
        words,
        custom: true,
        complete: false,
        ..Dict::default()
    };
    base.word.book_list.push(dict);
    base.word.book_list.len() - 1
}

/// 把一本词典复制成自定义词典，返回新词典的下标。
pub fn copy_dict(base: &mut BaseState, index: usize) -> Option<usize> {
    let source = base.word.book_list.get(index)?;
    let words = source.words.clone();
    let name = format!("{} 副本", source.name);
    Some(create_custom_dict(base, &name, words))
}

/// 把一批词条加入「已掌握」词典。
pub fn mark_mastered(base: &mut BaseState, words: &[Word]) {
    let Some(known) = base.known_mut() else {
        return;
    };
    for word in words {
        if !known
            .words
            .iter()
            .any(|item| item.word.eq_ignore_ascii_case(&word.word))
        {
            known.words.push(word.clone());
        }
    }
    known.length = known.words.len();
}

/// 把一批词条加入「错词」词典。
pub fn mark_unknown(base: &mut BaseState, words: &[Word]) {
    let Some(wrong) = base.wrong_mut() else {
        return;
    };
    for word in words {
        if !wrong
            .words
            .iter()
            .any(|item| item.word.eq_ignore_ascii_case(&word.word))
        {
            wrong.words.push(word.clone());
        }
    }
    wrong.length = wrong.words.len();
}

/// 排序词条。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// 保持原样
    Normal,
    /// 按字母升序
    Ascending,
    /// 按字母降序
    Descending,
    /// 打乱
    Shuffle,
}

/// 按指定顺序重排词条。
pub fn sort_words(words: &mut [Word], order: SortOrder, rng: &mut crate::domain::rng::Rng) {
    match order {
        SortOrder::Normal => {}
        SortOrder::Ascending => {
            words.sort_by_key(|word| word.word.to_lowercase());
        }
        SortOrder::Descending => {
            words.sort_by_key(|word| std::cmp::Reverse(word.word.to_lowercase()));
        }
        SortOrder::Shuffle => crate::domain::rng::shuffle(rng, words),
    }
}

/// 排序方式的下一个。
impl SortOrder {
    pub fn next(self) -> Self {
        match self {
            SortOrder::Normal => SortOrder::Ascending,
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Shuffle,
            SortOrder::Shuffle => SortOrder::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortOrder::Normal => "原序",
            SortOrder::Ascending => "正序",
            SortOrder::Descending => "倒序",
            SortOrder::Shuffle => "随机",
        }
    }
}
