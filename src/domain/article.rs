//! 文章背诵：文章模型、断句与分词。
//!
//! 对齐上游 `app/core/hooks/article.ts` 的 `parseSentence` 与
//! `genArticleSectionData`：文章按空行分段落、段落内按换行分句，每句再切成
//! 可逐个输入的片段（单词 / 数字 / 符号）。

use serde::{Deserialize, Serialize};

/// 片段类型（对应上游 `PracticeArticleWordType`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArticleWordKind {
    /// 符号
    Symbol,
    /// 数字
    Number,
    /// 单词
    Word,
}

/// 句子里一个可输入的片段。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArticleWord {
    /// 片段文本
    pub word: String,
    /// 与下一个片段之间是否有空格
    pub next_space: bool,
    pub kind: ArticleWordKind,
}

/// 一句话。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentence {
    pub text: String,
    pub translate: String,
    pub words: Vec<ArticleWord>,
}

/// 一篇文章（与资源 JSON 同构）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Article {
    pub id: i64,
    pub title: String,
    pub title_translate: String,
    pub text: String,
    pub text_translate: String,
    pub audio_src: String,
    #[serde(skip)]
    pub sections: Vec<Vec<Sentence>>,
}

/// 把一句切成可输入的片段。
///
/// 规则与上游 `parseSentence` 一致：先归一化智能引号，再依次匹配
/// 「货币+数字」「数字」「带点缩写」「单词」「单个符号」。
pub fn parse_sentence(sentence: &str) -> Vec<ArticleWord> {
    // 智能引号统一成直引号，避免匹配问题
    let normalized: String = sentence
        .chars()
        .map(|character| match character {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
            other => other,
        })
        .collect();

    let bytes = normalized.as_bytes();
    let mut tokens: Vec<(String, usize, ArticleWordKind)> = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let rest = &normalized[index..];
        let character = rest.chars().next().unwrap();
        if character.is_whitespace() {
            index += character.len_utf8();
            continue;
        }
        if let Some(length) = match_currency_number(rest) {
            tokens.push((rest[..length].to_string(), index, ArticleWordKind::Number));
            index += length;
            continue;
        }
        if let Some(length) = match_number(rest) {
            tokens.push((rest[..length].to_string(), index, ArticleWordKind::Number));
            index += length;
            continue;
        }
        if let Some(length) = match_abbreviation(rest) {
            tokens.push((rest[..length].to_string(), index, ArticleWordKind::Word));
            index += length;
            continue;
        }
        if let Some(length) = match_word(rest) {
            tokens.push((rest[..length].to_string(), index, ArticleWordKind::Word));
            index += length;
            continue;
        }
        // 其它可视符号：单字符处理
        tokens.push((character.to_string(), index, ArticleWordKind::Symbol));
        index += character.len_utf8();
    }

    // 片段之间是否有空格
    let mut words: Vec<ArticleWord> = Vec::with_capacity(tokens.len());
    for (position, (word, start, kind)) in tokens.iter().enumerate() {
        let end = start + word.len();
        let next_start = tokens.get(position + 1).map(|(_, start, _)| *start);
        let between = match next_start {
            Some(next_start) => &normalized[end..next_start],
            None => &normalized[end..],
        };
        words.push(ArticleWord {
            word: word.clone(),
            next_space: between.chars().any(|character| character.is_whitespace()),
            kind: *kind,
        });
    }
    words
}

/// 生成文章的段落与句子（对应上游 `genArticleSectionData`）。
pub fn gen_sections(article: &mut Article) {
    let text = article.text.trim().to_string();
    let mut sections: Vec<Vec<Sentence>> = Vec::new();
    for section_text in text.split("\n\n").filter(|part| !part.is_empty()) {
        let mut section: Vec<Sentence> = Vec::new();
        let lines: Vec<&str> = section_text
            .trim()
            .split('\n')
            .filter(|line| !line.is_empty())
            .collect();
        for (position, line) in lines.iter().enumerate() {
            let mut item = line.trim().to_string();
            // 除最后一句外补一个空格，保证相邻片段之间有间隔
            if position < lines.len() - 1 {
                item.push(' ');
            }
            section.push(Sentence {
                words: parse_sentence(&item),
                text: item,
                translate: String::new(),
            });
        }
        sections.push(section);
    }
    sections.retain(|section| !section.is_empty());

    // 译文按同样的段落 / 行结构对齐
    let translate_list: Vec<&str> = article.text_translate.split("\n\n").collect();
    for (index, section) in sections.iter_mut().enumerate() {
        let Some(section_translate) = translate_list.get(index) else {
            continue;
        };
        let lines: Vec<&str> = section_translate.split('\n').collect();
        for (position, sentence) in section.iter_mut().enumerate() {
            if let Some(translate) = lines.get(position) {
                let translate = translate.trim();
                if !translate.is_empty() {
                    sentence.translate = translate.to_string();
                }
            }
        }
    }
    article.sections = sections;
}

/// 匹配「货币 + 数字」（`$1,000.50` / `¥200` / `€100.5%`）。
fn match_currency_number(text: &str) -> Option<usize> {
    let first = text.chars().next()?;
    if !matches!(first, '$' | '¥' | '€' | '£') {
        return None;
    }
    let digits = match_number_digits(&text[first.len_utf8()..])?;
    Some(first.len_utf8() + digits)
}

/// 匹配纯数字（`100%` / `3.14` / `1,000.00`）。
fn match_number(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first().is_none_or(|byte| !byte.is_ascii_digit()) {
        return None;
    }
    match_number_digits(text)
}

/// `\d{1,3}(,\d{3})*(\.\d+)?%?` 的手写实现。
fn match_number_digits(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() && index - start < 3 {
        index += 1;
    }
    if index == start {
        return None;
    }
    // (,ddd)*
    while index + 3 < bytes.len()
        && bytes[index] == b','
        && bytes[index + 1].is_ascii_digit()
        && bytes[index + 2].is_ascii_digit()
        && bytes[index + 3].is_ascii_digit()
    {
        index += 4;
    }
    // (\.\d+)?
    if index < bytes.len() && bytes[index] == b'.' {
        let mut probe = index + 1;
        while probe < bytes.len() && bytes[probe].is_ascii_digit() {
            probe += 1;
        }
        if probe > index + 1 {
            index = probe;
        }
    }
    // %?
    if index < bytes.len() && bytes[index] == b'%' {
        index += 1;
    }
    Some(index)
}

/// 匹配带点缩写（`U.S.A.` / `e.g.` / `Ph.D.`）。
fn match_abbreviation(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
        index += 1;
    }
    if index == start {
        return None;
    }
    let mut groups = 0usize;
    loop {
        if index < bytes.len() && bytes[index] == b'.' {
            let mut probe = index + 1;
            let group_start = probe;
            while probe < bytes.len() && bytes[probe].is_ascii_alphabetic() {
                probe += 1;
            }
            if probe == group_start {
                break;
            }
            index = probe;
            groups += 1;
        } else {
            break;
        }
    }
    if groups == 0 {
        return None;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
    }
    Some(index)
}

/// 匹配单词（含撇号与连字符：`it's` / `o'clock` / `mother-in-law`）。
fn match_word(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
        index += 1;
    }
    if index == start {
        return None;
    }
    loop {
        if index < bytes.len() && (bytes[index] == b'\'' || bytes[index] == b'-') {
            let mut probe = index + 1;
            let group_start = probe;
            while probe < bytes.len() && bytes[probe].is_ascii_alphanumeric() {
                probe += 1;
            }
            if probe == group_start {
                break;
            }
            index = probe;
        } else {
            break;
        }
    }
    Some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_simple_sentence() {
        let words = parse_sentence("Excuse me!");
        let texts: Vec<&str> = words.iter().map(|item| item.word.as_str()).collect();
        assert_eq!(texts, vec!["Excuse", "me", "!"]);
        assert_eq!(words[0].kind, ArticleWordKind::Word);
        assert_eq!(words[2].kind, ArticleWordKind::Symbol);
        assert!(words[0].next_space);
        assert!(!words[1].next_space);
    }

    #[test]
    fn keeps_numbers_and_currency_together() {
        let words = parse_sentence("It costs $1,000.50 today.");
        let texts: Vec<&str> = words.iter().map(|item| item.word.as_str()).collect();
        assert_eq!(texts, vec!["It", "costs", "$1,000.50", "today", "."]);
        assert_eq!(words[2].kind, ArticleWordKind::Number);
    }

    #[test]
    fn keeps_abbreviations_and_hyphenated_words() {
        let words = parse_sentence("The U.S.A. mother-in-law is here.");
        let texts: Vec<&str> = words.iter().map(|item| item.word.as_str()).collect();
        assert_eq!(
            texts,
            vec!["The", "U.S.A.", "mother-in-law", "is", "here", "."]
        );
    }

    #[test]
    fn normalizes_smart_quotes() {
        let words = parse_sentence("\u{201C}Hi\u{201D}");
        let texts: Vec<&str> = words.iter().map(|item| item.word.as_str()).collect();
        assert_eq!(texts, vec!["\"", "Hi", "\""]);
    }

    #[test]
    fn builds_sections_and_aligns_translations() {
        let mut article = Article {
            text: "Excuse me! \n\nYes?".to_string(),
            text_translate: "对不起 \n\n什么事？".to_string(),
            ..Article::default()
        };
        gen_sections(&mut article);
        assert_eq!(article.sections.len(), 2);
        assert_eq!(article.sections[0][0].text.trim(), "Excuse me!");
        assert_eq!(article.sections[0][0].translate, "对不起");
        assert_eq!(article.sections[1][0].translate, "什么事？");
    }
}

// ── 资源加载 ───────────────────────────────────────────────────────────────

use anyhow::{Context, Result};

use crate::paths::Paths;

/// 一本文章书（对应上游 `public/list/article.json` 的一项）。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ArticleBook {
    #[serde(alias = "en_name")]
    pub en_name: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub length: usize,
    pub category: String,
    pub tags: Vec<String>,
}

/// 读取资源目录里的文章书列表。
pub fn load_books(paths: &Paths) -> Result<Vec<ArticleBook>> {
    let file = paths.assets.join("list").join("article.json");
    if !file.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("读取文章目录失败: {}", file.display()))?;
    let books: Vec<ArticleBook> = serde_json::from_str(&text)
        .with_context(|| format!("解析文章目录失败: {}", file.display()))?;
    Ok(books)
}

/// 读取一本文章书里的所有文章，并生成段落与句子。
pub fn load_articles(paths: &Paths, book: &ArticleBook) -> Result<Vec<Article>> {
    let file = paths
        .assets
        .join("dicts")
        .join("en")
        .join("article")
        .join(&book.url);
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("读取文章失败: {}", file.display()))?;
    let mut articles: Vec<Article> =
        serde_json::from_str(&text).with_context(|| format!("解析文章失败: {}", file.display()))?;
    for article in &mut articles {
        gen_sections(article);
    }
    Ok(articles)
}

// ── 句子跟打 ───────────────────────────────────────────────────────────────

/// 句子跟打状态：逐字符比对。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SentenceTyping {
    /// 已正确输入的字符数
    pub typed: usize,
    /// 本句是否出现过错误
    pub wrong: bool,
}

impl SentenceTyping {
    /// 输入一个字符。
    ///
    /// 返回 `true` 表示正确推进（`typed` 加一），`false` 表示输错了
    /// （`wrong` 置位，光标不前进）。
    pub fn input(&mut self, expected: &str, character: char, ignore_case: bool) -> bool {
        let Some(target) = expected.chars().nth(self.typed) else {
            return false;
        };
        let matched = if ignore_case {
            character.eq_ignore_ascii_case(&target)
                || character.to_lowercase().eq(target.to_lowercase())
        } else {
            character == target
        };
        if matched {
            self.typed += 1;
            true
        } else {
            self.wrong = true;
            false
        }
    }

    /// 是否已经输完整句。
    pub fn is_done(&self, expected: &str) -> bool {
        self.typed >= expected.chars().count()
    }

    /// 已输入的内容（用于界面回显）。
    pub fn typed_text(&self, expected: &str) -> String {
        expected.chars().take(self.typed).collect()
    }
}

/// 文章练习的断点缓存（对应上游 `PracticeSaveArticle` v1）。
///
/// 上游的 `practiceData` 记的是「段落 / 句 / 词」三级游标；TUI 的文章
/// 在 `gen_sections` 后把段落展开成扁平句子列表，跟打时也按扁平下标推进，
/// 因此这里只存一个句子下标——够用且不会因段落结构变化而失效。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ArticleCache {
    /// 文章 id
    pub article_id: i64,
    /// 当前句子的下标（扁平后的全局下标）
    pub sentence_index: usize,
    /// 已完成的句子数
    pub finished: usize,
    /// 出现错误的句子数
    pub wrong_sentences: usize,
    /// 总句子数
    pub total: usize,
    /// 累计时长（毫秒）
    pub spend: i64,
}

/// 文章练习缓存的落盘格式（对齐上游 `PracticeSaveArticle` 的信封内容）。
///
/// 上游是 `{ practiceData: { sectionIndex, sentenceIndex, wordIndex },
/// statStoreData: PracticeState }`；TUI 的字段更少，但外层信封
/// （`{ val, version, updated_at }`）与 key 名保持一致，便于将来互通。
///
/// 不派生 `PartialEq`：`PracticeState` 没实现它（`PracticeCacheCompact`
/// 同理），`Option<PracticeState>` 也就没法比较。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ArticleCacheEnvelope {
    pub practice_data: ArticleCache,
    /// 与上游同名的统计字段；TUI 只用 `spend`，其余留空。
    pub stat_store_data: Option<crate::domain::types::PracticeState>,
}
