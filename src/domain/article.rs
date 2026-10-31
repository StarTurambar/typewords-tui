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

/// 句子里的一个字符与它的跟打下标。
///
/// `target` 为 `Some` 的字符需要逐个输入：字母、数字，以及词与词之间
/// 的那一个空格。标点、符号与多余空白为 `None`——只显示、不跟打，按下
/// 了也忽略。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypingCell {
    pub ch: char,
    pub target: Option<usize>,
}

impl Sentence {
    /// 逐字符的跟打映射：渲染与键入共用同一份「哪些字符要打」的规则。
    ///
    /// 规则：字母与数字是跟打内容；两段内容之间只要有空白，就要求输入
    /// 一个空格（落在该空白的第一个字符上）。句首 / 句尾的空白与连续
    /// 空白里的多余空格不跟打，标点与符号一律不跟打。
    pub fn typing_cells(&self) -> Vec<TypingCell> {
        let chars: Vec<char> = self.text.chars().collect();
        let mut cells: Vec<TypingCell> = chars
            .iter()
            .map(|character| TypingCell {
                ch: *character,
                target: None,
            })
            .collect();
        let mut next_target = 0usize;
        let mut previous: Option<usize> = None;
        for (index, character) in chars.iter().enumerate() {
            if !character.is_alphanumeric() {
                continue;
            }
            if let Some(end) = previous {
                let gap_start = end + 1;
                if gap_start < index
                    && let Some(offset) =
                        (gap_start..index).find(|position| chars[*position].is_whitespace())
                {
                    cells[offset].target = Some(next_target);
                    next_target += 1;
                }
            }
            cells[index].target = Some(next_target);
            next_target += 1;
            previous = Some(index);
        }
        cells
    }

    /// 这句话的跟打目标：字母、数字与词间空格按顺序排成的字符序列。
    ///
    /// 标点与符号不会出现在目标里；一句话可能没有任何可跟打内容
    /// （纯标点 / 空白），调用方需要跳过这种句子。
    pub fn typing_target(&self) -> Vec<char> {
        self.typing_cells()
            .into_iter()
            .filter_map(|cell| {
                cell.target.map(|_| {
                    if cell.ch.is_whitespace() {
                        ' '
                    } else {
                        cell.ch
                    }
                })
            })
            .collect()
    }
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

    #[test]
    fn typing_target_keeps_word_spaces_and_drops_punctuation() {
        let mut article = Article {
            text: "Hello, world! It's 3.14%.".to_string(),
            ..Article::default()
        };
        gen_sections(&mut article);
        let target: String = article.sections[0][0].typing_target().into_iter().collect();
        assert_eq!(target, "Hello world Its 314");
    }

    #[test]
    fn typing_cells_mark_punctuation_and_extra_space_as_not_typed() {
        let mut article = Article {
            text: "Hi,  you!".to_string(),
            ..Article::default()
        };
        gen_sections(&mut article);
        let cells = article.sections[0][0].typing_cells();
        let indices: Vec<Option<usize>> = cells.iter().map(|cell| cell.target).collect();
        // H i , 空格 空格 y o u !：只有一个词间空格跟打，
        // 逗号、感叹号与连续空白里的多余空格都不跟打。
        assert_eq!(
            indices,
            vec![
                Some(0),
                Some(1),
                None,
                Some(2),
                None,
                Some(3),
                Some(4),
                Some(5),
                None
            ]
        );
        let target: String = article.sections[0][0].typing_target().into_iter().collect();
        assert_eq!(target, "Hi you");
    }

    #[test]
    fn spaces_between_words_are_part_of_the_typing_target() {
        let mut article = Article {
            text: "Hello there.".to_string(),
            ..Article::default()
        };
        gen_sections(&mut article);
        let target = article.sections[0][0].typing_target();
        assert_eq!(target.iter().collect::<String>(), "Hello there");

        let mut typing = SentenceTyping::default();
        for character in "Hello".chars() {
            assert_eq!(typing.input(&target, character, true), Some(true));
        }
        // 句点不用打：忽略，光标停在空格前。
        assert_eq!(typing.input(&target, '.', true), None);
        assert_eq!(typing.cursor(), 5);
        // 空格是跟打内容：必须输入才继续。
        assert_eq!(typing.input(&target, ' ', true), Some(true));
        assert_eq!(typing.cursor(), 6);
        for character in "there".chars() {
            typing.input(&target, character, true);
        }
        assert!(typing.is_done(target.len()));
        assert!(!typing.wrong);
    }

    #[test]
    fn a_missing_word_space_counts_a_mistake_without_shifting_later_letters() {
        let target: Vec<char> = "Hello there".chars().collect();
        let mut typing = SentenceTyping::default();
        // 一口气打完、中间漏掉空格：补一次错误，但后面的字母不应全部错位。
        for character in "Hellothere".chars() {
            typing.input(&target, character, true);
        }
        assert!(typing.wrong);
        assert!(typing.is_done(target.len()));
        assert_eq!(typing.cursor(), target.len());
        // 自动补上的空格是正确位置，其余字母都不算错。
        assert!(!typing.is_mismatch(5, &target, true));
    }

    #[test]
    fn an_extra_space_is_ignored() {
        let target: Vec<char> = "Hi you".chars().collect();
        let mut typing = SentenceTyping::default();
        assert_eq!(typing.input(&target, 'H', true), Some(true));
        assert_eq!(typing.input(&target, 'i', true), Some(true));
        assert_eq!(typing.input(&target, ' ', true), Some(true));
        // 字母位置上的多余空格不是目标内容：忽略，不推进也不计错。
        assert_eq!(typing.input(&target, ' ', true), None);
        assert_eq!(typing.cursor(), 3);
        assert!(!typing.wrong);
    }

    #[test]
    fn typing_records_wrong_letters_at_their_own_position() {
        let target: Vec<char> = "Hello".chars().collect();
        let mut typing = SentenceTyping::default();
        assert_eq!(typing.input(&target, 'H', true), Some(true));
        assert_eq!(typing.input(&target, 'x', true), Some(false));
        assert_eq!(typing.cursor(), 2);
        assert!(typing.wrong);
        assert!(typing.is_mismatch(1, &target, true));
        assert!(!typing.is_mismatch(0, &target, true));

        // 继续正确输入后续字符，错误标记仍留在它发生的位置，不跟着光标走。
        assert_eq!(typing.input(&target, 'l', true), Some(true));
        assert_eq!(typing.input(&target, 'l', true), Some(true));
        assert_eq!(typing.input(&target, 'o', true), Some(true));
        assert!(typing.is_mismatch(1, &target, true));
        assert_eq!(typing.typed_char(1), Some('x'));
        assert!(typing.is_done(target.len()));
    }

    #[test]
    fn punctuation_keys_are_ignored() {
        let target: Vec<char> = "Hello".chars().collect();
        let mut typing = SentenceTyping::default();
        assert_eq!(typing.input(&target, ',', true), None);
        assert_eq!(typing.input(&target, '.', true), None);
        assert_eq!(typing.input(&target, '\'', true), None);
        assert_eq!(typing.cursor(), 0);
        assert!(!typing.wrong);
    }

    #[test]
    fn backspace_steps_back_but_keeps_the_wrong_mark() {
        let target: Vec<char> = "Hi".chars().collect();
        let mut typing = SentenceTyping::default();
        typing.input(&target, 'H', true);
        typing.input(&target, 'x', true);
        typing.backspace();
        assert_eq!(typing.cursor(), 1);
        assert!(typing.wrong, "本句出现过错误，退格也不该抹掉");
        assert_eq!(typing.input(&target, 'i', true), Some(true));
        assert!(typing.is_done(target.len()));
    }

    #[test]
    fn ignore_case_applies_to_input_comparison() {
        let target: Vec<char> = "Hi".chars().collect();
        let mut typing = SentenceTyping::default();
        assert_eq!(typing.input(&target, 'h', true), Some(true));
        assert_eq!(typing.input(&target, 'I', true), Some(true));
        assert!(!typing.wrong);
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

/// 判断一个字符是否可能成为跟打内容（字母、数字、空格）。
///
/// 标点与符号不是跟打内容，按下直接忽略；空格是跟打内容，只有出现在
/// 词与词之间时才要求输入（见 `Sentence::typing_cells`）。
pub fn is_typing_char(character: char) -> bool {
    character.is_alphanumeric() || character.is_whitespace()
}

/// 字符比较：`ignore_case` 时按小写比较。
pub fn chars_match(typed: char, expected: char, ignore_case: bool) -> bool {
    if typed == expected {
        return true;
    }
    ignore_case && typed.to_lowercase().eq(expected.to_lowercase())
}

/// 句子跟打状态：记录实际按下的字符序列。
///
/// 与旧模型的区别：旧模型只记「已正确输入的字符数」，打错时既不前进也不
/// 留痕，红色只能钉在进度最前面。现在每次按键都记到光标处（打错也前移），
/// 错误标记留在它真正发生的位置上，界面据此显示实际打出的字符。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SentenceTyping {
    /// 实际按下的字符（含打错的字符），长度即当前光标位置
    pub input: Vec<char>,
    /// 本句是否出现过错误（打错后即使退格修正也算出现过）
    pub wrong: bool,
}

impl SentenceTyping {
    /// 当前光标位置（已输入的字符数）。
    pub fn cursor(&self) -> usize {
        self.input.len()
    }

    /// 记录一次按键。
    ///
    /// - 标点 / 符号不是跟打内容：任何位置都直接忽略，返回 `None`；
    /// - 空格是跟打内容：该按时没按会被记一次错误，并自动补空格继续，
    ///   不至于让后面的字母全部错位；不该按时按了则忽略；
    /// - 字母与数字记到光标处并前移，返回 `Some(是否与目标一致)`。
    ///
    /// 打错也照样前移：错误只在原位置标红，不把光标钉在进度最前面。
    pub fn input(&mut self, target: &[char], character: char, ignore_case: bool) -> Option<bool> {
        let position = self.input.len();
        if position >= target.len() {
            return None;
        }
        let expected = target[position];
        if expected.is_whitespace() {
            if character.is_whitespace() {
                self.input.push(' ');
                return Some(true);
            }
            // 标点不是跟打内容：忽略，光标留在空格上。
            if !is_typing_char(character) {
                return None;
            }
            // 漏打词间空格：补上空缺并记一次错误，再把这次按键用于下一个
            // 目标字符，避免后面的字母全部错位。
            self.input.push(' ');
            self.wrong = true;
            return self.input(target, character, ignore_case);
        }
        // 字母 / 数字位置：多余的空格不是目标内容，忽略。
        if character.is_whitespace() {
            return None;
        }
        // 标点不是跟打内容：按错位置时忽略，不推进也不计错。
        if !is_typing_char(character) {
            return None;
        }
        let matched = chars_match(character, expected, ignore_case);
        self.input.push(character);
        if !matched {
            self.wrong = true;
        }
        Some(matched)
    }

    /// 退格：删掉最后输入的字符；本句的 `wrong` 标记不因此清除。
    pub fn backspace(&mut self) {
        self.input.pop();
    }

    /// 是否已经输完整句（输入长度达到目标长度，打错也算完成）。
    pub fn is_done(&self, target_len: usize) -> bool {
        self.input.len() >= target_len
    }

    /// 第 `index` 个目标位置实际打出的字符。
    pub fn typed_char(&self, index: usize) -> Option<char> {
        self.input.get(index).copied()
    }

    /// 第 `index` 个目标位置是否打错（尚未输入或输入正确都返回 false）。
    pub fn is_mismatch(&self, index: usize, target: &[char], ignore_case: bool) -> bool {
        match (self.input.get(index), target.get(index)) {
            (Some(typed), Some(expected)) => !chars_match(*typed, *expected, ignore_case),
            _ => false,
        }
    }

    /// 清空本句的输入状态。
    pub fn reset(&mut self) {
        *self = Self::default();
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
