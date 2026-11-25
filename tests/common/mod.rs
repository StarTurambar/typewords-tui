//! 测试共用：临时目录里生成一个最小词库与数据目录。
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use typewords_tui::app::App;
use typewords_tui::cli::Cli;
use typewords_tui::domain::types::{SettingState, TaskWords, Word};
use typewords_tui::paths::Paths;
use unicode_width::UnicodeWidthStr;

pub struct Fixture {
    pub root: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let assets = root.path().join("assets");
        let home = root.path().join("home");
        fs::create_dir_all(assets.join("list")).unwrap();
        fs::create_dir_all(assets.join("dicts").join("en").join("word")).unwrap();
        fs::create_dir_all(&home).unwrap();

        let words: Vec<serde_json::Value> = (0..12)
            .map(|index| {
                let word = format!("word{:02}", index);
                serde_json::json!({
                    "id": index + 1,
                    "word": word,
                    "phonetic0": format!("/{word}/"),
                    "phonetic1": format!("/{word}/"),
                    "trans": [
                        { "pos": "n.", "cn": format!("释义{index}") },
                        { "pos": "v.", "cn": format!("动词{index}") }
                    ],
                    "sentences": [{ "c": format!("This is {word}."), "cn": format!("这是{index}。") }],
                    "phrases": [{ "c": format!("{word} up"), "cn": "短语" }],
                    "synos": [{ "pos": "n.", "cn": "同义", "ws": [format!("syn{index}")] }],
                    "etymology": [{ "t": "来源", "d": "测试词源" }],
                    "relWords": { "root": word, "rels": [] }
                })
            })
            .collect();
        fs::write(
            assets
                .join("dicts")
                .join("en")
                .join("word")
                .join("toy.json"),
            serde_json::to_string(&words).unwrap(),
        )
        .unwrap();

        let catalog = serde_json::json!([{
            "id": 1,
            "enName": "toy",
            "name": "测试词库",
            "description": "12 个测试词",
            "url": "toy.json",
            "length": 12,
            "language": "en",
            "translateLanguage": "zh_CN",
            "type": "word",
            "category": "测试",
            "tags": ["测试"]
        }]);
        fs::write(
            assets.join("list").join("word.json"),
            serde_json::to_string(&catalog).unwrap(),
        )
        .unwrap();

        // 文章资源：一本书、两篇文章，每篇两句。
        fs::create_dir_all(assets.join("dicts").join("en").join("article")).unwrap();
        let article_book = serde_json::json!([{
            "id": 1,
            "enName": "toybook",
            "name": "测试文章集",
            "description": "测试用文章",
            "url": "toy.json",
            "length": 2,
            "language": "en",
            "translateLanguage": "zh_CN",
            "type": "article"
        }]);
        fs::write(
            assets.join("list").join("article.json"),
            serde_json::to_string(&article_book).unwrap(),
        )
        .unwrap();
        let articles = serde_json::json!([
            {
                "id": 11,
                "title": "第一篇",
                "titleTranslate": "Article One",
                "text": "Hello there.\n\nGeneral Kenobi.",
                "textTranslate": "你好。\n\n肯尼将军。",
                "audioSrc": ""
            },
            {
                "id": 12,
                "title": "第二篇",
                "titleTranslate": "Article Two",
                "text": "Good morning.\n\nGood night.",
                "textTranslate": "早上好。\n\n晚安。",
                "audioSrc": ""
            }
        ]);
        fs::write(
            assets
                .join("dicts")
                .join("en")
                .join("article")
                .join("toy.json"),
            serde_json::to_string(&articles).unwrap(),
        )
        .unwrap();

        Self { root }
    }

    pub fn assets(&self) -> PathBuf {
        self.root.path().join("assets")
    }

    pub fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    pub fn paths(&self) -> Paths {
        Paths {
            home: self.home(),
            assets: self.assets(),
        }
    }

    pub fn cli(&self) -> Cli {
        Cli {
            assets: Some(self.assets()),
            home: Some(self.home()),
            dict: None,
            mode: None,
            theme: None,
            seed: Some(42),
            reset_practice: false,
            log: None,
            no_log: false,
            log_level: typewords_tui::logging::Level::Info,
            command: None,
        }
    }

    pub fn words(&self) -> Vec<Word> {
        let text = fs::read_to_string(
            self.assets()
                .join("dicts")
                .join("en")
                .join("word")
                .join("toy.json"),
        )
        .unwrap();
        serde_json::from_str(&text).unwrap()
    }

    pub fn task_words(&self) -> TaskWords {
        let words = self.words();
        TaskWords {
            new: words[..6].to_vec(),
            review: words[6..].to_vec(),
        }
    }

    pub fn settings() -> SettingState {
        SettingState::default()
    }
}

/// 渲染一帧，返回每一行的「文本 + 该行字符格的前景色」。
///
/// 文本按**字符**拼接：CJK 宽字符只占一个字符串元素、它后面的续接列被跳过，
/// 因此 `text.contains("今日任务")` 这类断言才成立。要做「命中矩形是否落在
/// 画出来的内容上」这种按**显示列**的几何断言，用 [`Cells`]。
pub fn render_rows(app: &App, width: u16, height: u16) -> Vec<(String, Vec<Color>)> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut rows = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut skip_cells = 0usize;
        let mut text = String::new();
        let mut colors = Vec::new();
        for x in 0..width {
            if skip_cells > 0 {
                skip_cells -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                let symbol = cell.symbol();
                text.push_str(symbol);
                // 宽字符（CJK）占两个单元，跳过其续接单元
                let display_width = UnicodeWidthStr::width(symbol);
                if display_width > 1 {
                    skip_cells = display_width - 1;
                }
                colors.push(cell.fg);
            }
        }
        rows.push((text, colors));
    }
    rows
}

pub fn render_text(app: &App, width: u16, height: u16) -> String {
    render_rows(app, width, height)
        .into_iter()
        .map(|(text, _)| text + "\n")
        .collect()
}

/// 取包含 `needle` 的那一行：文本 + 该行用到的前景色。
pub fn rendered_row(app: &App, width: u16, height: u16, needle: &str) -> (String, Vec<Color>) {
    render_rows(app, width, height)
        .into_iter()
        .find(|(text, _)| text.contains(needle))
        .unwrap_or_else(|| panic!("没有任何一行包含 {needle:?}"))
}

/// 一帧的**单元格**视图：`symbols[y][x]` 是第 y 行第 x 个显示列上的符号。
///
/// 与 [`render_rows`] 的区别：宽字符只写在它起始的那一列，续接列是一个空格。
/// 因此这里的下标就是终端的真实显示列，可以和 `Target::rect` 直接对照——
/// 用字符下标去查矩形会把每一行都算偏（每遇到一个 CJK 就偏一列）。
pub struct Cells {
    pub width: u16,
    pub height: u16,
    symbols: Vec<Vec<String>>,
}

impl Cells {
    /// 整行的文本（宽字符后面会跟一个占位的空格）。
    pub fn row(&self, y: u16) -> String {
        self.symbols
            .get(y as usize)
            .map(|row| row.concat())
            .unwrap_or_default()
    }

    /// 整帧的文本，每行一个 `\n`。
    pub fn text(&self) -> String {
        (0..self.height).map(|y| self.row(y) + "\n").collect()
    }

    /// 矩形首行（`rect.y`）在矩形范围内的文本。
    pub fn rect_text(&self, rect: Rect) -> String {
        self.symbols
            .get(rect.y as usize)
            .map(|row| {
                row.iter()
                    .skip(rect.x as usize)
                    .take(rect.width as usize)
                    .cloned()
                    .collect::<String>()
            })
            .unwrap_or_default()
    }

    /// 矩形范围内是否一个非空白字符都没有。
    pub fn rect_is_blank(&self, rect: Rect) -> bool {
        !self
            .symbols
            .iter()
            .skip(rect.y as usize)
            .take(rect.height as usize)
            .any(|row| {
                row.iter()
                    .skip(rect.x as usize)
                    .take(rect.width as usize)
                    .any(|symbol| !symbol.trim().is_empty())
            })
    }
}

pub fn render_cells(app: &App, width: u16, height: u16) -> Cells {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut symbols = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut row = Vec::with_capacity(width as usize);
        for x in 0..width {
            row.push(
                buffer
                    .cell((x, y))
                    .map(|cell| cell.symbol().to_string())
                    .unwrap_or_else(|| " ".to_string()),
            );
        }
        symbols.push(row);
    }
    Cells {
        width,
        height,
        symbols,
    }
}
