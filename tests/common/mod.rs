//! 测试共用：临时目录里生成一个最小词库与数据目录。
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use typewords_tui::cli::Cli;
use typewords_tui::domain::types::{SettingState, TaskWords, Word};
use typewords_tui::paths::Paths;

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
