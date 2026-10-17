//! 大字像素字体：用半块字符把单词拼成大号字形。
//!
//! 每个字形是 5×7 的点阵，渲染时每两个纵向像素合并进一个终端单元
//! （▀ / ▄ / █ / 空格），因此每个字母占 5 列 × 4 行。
//! 遮罩状态使用实心方块占位，不泄露字形。

use std::ops::Range;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub const GLYPH_WIDTH: usize = 5;
const GLYPH_HEIGHT: usize = 7;
type Glyph = [&'static str; GLYPH_HEIGHT];

/// 遮罩占位：空心方框，只表达“这里有一个字母”。
const PLACEHOLDER: Glyph = [
    "#####", "#...#", "#...#", "#...#", "#...#", "#...#", "#####",
];

/// 每个字母的显示颜色与是否遮罩。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BigLetter {
    pub color: Color,
    pub masked: bool,
}

impl BigLetter {
    pub fn new(color: Color, masked: bool) -> Self {
        Self { color, masked }
    }
}

fn glyph(character: char) -> Option<Glyph> {
    let glyph = match character.to_ascii_uppercase() {
        'A' => [
            ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
        'B' => [
            "####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.",
        ],
        'C' => [
            ".####", "#....", "#....", "#....", "#....", "#....", ".####",
        ],
        'D' => [
            "####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####.",
        ],
        'E' => [
            "#####", "#....", "#....", "####.", "#....", "#....", "#####",
        ],
        'F' => [
            "#####", "#....", "#....", "####.", "#....", "#....", "#....",
        ],
        'G' => [
            ".####", "#....", "#....", "#.###", "#...#", "#...#", ".###.",
        ],
        'H' => [
            "#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
        'I' => [
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####",
        ],
        'J' => [
            "..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..",
        ],
        'K' => [
            "#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#",
        ],
        'L' => [
            "#....", "#....", "#....", "#....", "#....", "#....", "#####",
        ],
        'M' => [
            "#...#", "##.##", "#.#.#", "#...#", "#...#", "#...#", "#...#",
        ],
        'N' => [
            "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#",
        ],
        'O' => [
            ".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
        'P' => [
            "####.", "#...#", "#...#", "####.", "#....", "#....", "#....",
        ],
        'Q' => [
            ".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#",
        ],
        'R' => [
            "####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#",
        ],
        'S' => [
            ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
        ],
        'T' => [
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
        'U' => [
            "#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
        'V' => [
            "#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..",
        ],
        'W' => [
            "#...#", "#...#", "#...#", "#...#", "#.#.#", "##.##", "#...#",
        ],
        'X' => [
            "#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#",
        ],
        'Y' => [
            "#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#..",
        ],
        'Z' => [
            "#####", "....#", "...#.", "..#..", ".#...", "#....", "#####",
        ],
        '0' => [
            ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
        ],
        '1' => [
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", "#####",
        ],
        '2' => [
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
        '3' => [
            "####.", "....#", "....#", ".###.", "....#", "....#", "####.",
        ],
        '4' => [
            "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
        ],
        '5' => [
            "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
        ],
        '6' => [
            ".###.", "#....", "#....", "####.", "#...#", "#...#", ".###.",
        ],
        '7' => [
            "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
        ],
        '8' => [
            ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
        ],
        '9' => [
            ".###.", "#...#", "#...#", ".####", "....#", "....#", ".###.",
        ],
        '-' => [
            ".....", ".....", ".....", "#####", ".....", ".....", ".....",
        ],
        '\'' => [
            "..#..", "..#..", "..#..", ".....", ".....", ".....", ".....",
        ],
        '.' => [
            ".....", ".....", ".....", ".....", ".....", ".##..", ".##..",
        ],
        ',' => [
            ".....", ".....", ".....", ".....", ".##..", ".##..", ".#...",
        ],
        '/' => [
            "....#", "...#.", "...#.", "..#..", ".#...", ".#...", "#....",
        ],
        '?' => [
            ".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#..",
        ],
        ':' => [
            ".....", ".##..", ".##..", ".....", ".##..", ".##..", ".....",
        ],
        '!' => [
            "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#..",
        ],
        '(' => [
            "...#.", "..#..", ".#...", ".#...", ".#...", "..#..", "...#.",
        ],
        ')' => [
            ".#...", "..#..", "...#.", "...#.", "...#.", "..#..", ".#...",
        ],
        ' ' => [
            ".....", ".....", ".....", ".....", ".....", ".....", ".....",
        ],
        _ => return None,
    };
    Some(glyph)
}

fn cell_character(top: bool, bottom: bool) -> char {
    match (top, bottom) {
        (true, true) => '█',
        (true, false) => '▀',
        (false, true) => '▄',
        (false, false) => ' ',
    }
}

/// 放大后每个字母实际占用的终端行数。
fn rows_for(scale: usize) -> usize {
    (GLYPH_HEIGHT * scale.max(1)).div_ceil(2)
}

fn push_chunk_rows(
    glyphs: &[Glyph],
    letters: &[BigLetter],
    range: Range<usize>,
    spacing: usize,
    scale: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let scale = scale.max(1);
    let rows = rows_for(scale);
    for row in 0..rows {
        // 像素级放大：先把点阵纵向放大 scale 倍，再把相邻两行合并进一个终端单元
        let top_pixel = (row * 2) / scale;
        let bottom_pixel = (row * 2 + 1) / scale;
        let mut spans: Vec<Span<'static>> = Vec::new();
        let count = range.len();
        for (offset, index) in range.clone().enumerate() {
            let letter = letters
                .get(index)
                .copied()
                .unwrap_or(BigLetter::new(Color::Reset, false));
            let pattern = if letter.masked {
                &PLACEHOLDER
            } else {
                &glyphs[index]
            };
            let mut text = String::with_capacity(GLYPH_WIDTH * scale);
            for column in 0..GLYPH_WIDTH {
                let top_on = top_pixel < GLYPH_HEIGHT
                    && pattern[top_pixel].as_bytes().get(column) == Some(&b'#');
                let bottom_on = bottom_pixel < GLYPH_HEIGHT
                    && pattern[bottom_pixel].as_bytes().get(column) == Some(&b'#');
                let character = cell_character(top_on, bottom_on);
                for _ in 0..scale {
                    text.push(character);
                }
            }
            spans.push(Span::styled(text, Style::default().fg(letter.color)));
            if spacing > 0 && offset + 1 < count {
                spans.push(Span::raw(" ".repeat(spacing)));
            }
        }
        lines.push(Line::from(spans));
    }
}

/// 渲染大字单词；遇到没有字形的字符返回 None（由调用方回退普通文本）。
///
/// `max_width` / `max_height` 描述可用空间：空间足够时自动使用 2 倍像素（字更大、更粗），
/// 否则退回 1 倍；仍然超宽时按字母拆行。
pub fn render_word(
    word: &str,
    letters: &[BigLetter],
    max_width: usize,
    max_height: usize,
) -> Option<Vec<Line<'static>>> {
    let characters: Vec<char> = word.chars().collect();
    if characters.is_empty() || max_width < GLYPH_WIDTH {
        return None;
    }
    let mut glyphs: Vec<Glyph> = Vec::with_capacity(characters.len());
    for character in &characters {
        glyphs.push(glyph(*character)?);
    }

    let letters = if letters.len() < characters.len() {
        let mut filled = letters.to_vec();
        filled.resize(characters.len(), BigLetter::new(Color::Reset, false));
        filled
    } else {
        letters.to_vec()
    };

    let expected_width = |scale: usize, spacing: usize| {
        characters.len() * GLYPH_WIDTH * scale + characters.len().saturating_sub(1) * spacing
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    for scale in [2usize, 1] {
        let spacing = scale;
        let height = rows_for(scale);
        // 1 倍始终允许（否则极小窗口无法渲染）；2 倍需要同时满足宽高
        let fits_height = scale == 1 || height <= max_height;
        if expected_width(scale, spacing) <= max_width && fits_height {
            push_chunk_rows(
                &glyphs,
                &letters,
                0..characters.len(),
                spacing,
                scale,
                &mut lines,
            );
            return Some(lines);
        }
    }

    // 1 倍仍超宽：压缩字距，必要时按字母拆成多行
    let mut spacing = 1usize;
    if expected_width(1, spacing) > max_width {
        spacing = 0;
    }
    if expected_width(1, spacing) <= max_width {
        push_chunk_rows(
            &glyphs,
            &letters,
            0..characters.len(),
            spacing,
            1,
            &mut lines,
        );
        return Some(lines);
    }

    let per_line = (max_width / (GLYPH_WIDTH + 1)).max(1);
    let mut start = 0;
    while start < characters.len() {
        let end = (start + per_line).min(characters.len());
        push_chunk_rows(&glyphs, &letters, start..end, spacing, 1, &mut lines);
        start = end;
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_five_by_seven_glyphs_as_four_rows() {
        let letters = vec![BigLetter::new(Color::Green, false); 5];
        let lines = render_word("wreck", &letters, 80, 3).expect("glyphs");
        assert_eq!(lines.len(), 4);
        for row in &lines {
            let width: usize = row
                .spans
                .iter()
                .map(|span| unicode_width::UnicodeWidthStr::width(span.content.as_ref()))
                .sum();
            assert!(width <= 80, "行宽 {width} 超出");
        }
    }

    #[test]
    fn masked_letters_render_hollow_placeholder() {
        let letters = vec![BigLetter::new(Color::DarkGray, true)];
        let lines = render_word("a", &letters, 80, 3).expect("glyphs");
        assert_eq!(lines.len(), 4);
        let text: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.to_string())
            .collect();
        assert!(text.contains('█'), "占位符应有边框");
        // 中间应有空心部分，而不是整块实心
        assert!(text.contains(' '), "占位符应为空心方框: {text:?}");
        assert!(
            text.chars()
                .all(|character| matches!(character, '█' | '▀' | '▄' | ' ')),
            "占位符不应包含字形像素: {text:?}"
        );
    }

    #[test]
    fn scales_up_when_space_allows() {
        let letters = vec![BigLetter::new(Color::Green, false); 5];
        let small = render_word("wreck", &letters, 80, 3).expect("glyphs");
        let large = render_word("wreck", &letters, 120, 20).expect("glyphs");
        assert_eq!(small.len(), 4);
        assert_eq!(large.len(), 7);
        let large_width: usize = large[0]
            .spans
            .iter()
            .map(|span| unicode_width::UnicodeWidthStr::width(span.content.as_ref()))
            .sum();
        assert!(large_width > 40, "2 倍像素应明显更宽: {large_width}");
    }

    #[test]
    fn unicode_words_fall_back() {
        assert!(render_word("café", &[BigLetter::new(Color::Reset, false)], 80, 20).is_none());
    }

    #[test]
    fn long_words_split_into_multiple_lines() {
        let word = "characteristically";
        let letters = vec![BigLetter::new(Color::Reset, false); word.chars().count()];
        let lines = render_word(word, &letters, 40, 4).expect("glyphs");
        assert!(lines.len() > 4, "长单词应拆成多行");
    }
}
