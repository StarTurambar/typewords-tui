//! 词条管理页：搜索、编辑、批量标记与自定义词典。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn press(app: &mut typewords_tui::app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_str(app: &mut typewords_tui::app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

#[test]
fn words_screen_searches_and_edits_entries() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    // 从词库页进入词条管理
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.screen, typewords_tui::app::Screen::DictPicker);
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, typewords_tui::app::Screen::Words);

    // 搜索
    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "word03");
    let indices = app.filtered_word_indices();
    assert_eq!(indices.len(), 1, "应只匹配到一个词条");
    assert_eq!(
        app.boot
            .base
            .study_dict()
            .map(|dict| dict.words[indices[0]].word.clone()),
        Some("word03".to_string())
    );

    // 结束搜索（Esc 退出搜索框）后编辑该词条
    press(&mut app, KeyCode::Esc);
    assert!(!app.words_search_focus, "Esc 应退出搜索框");
    press(&mut app, KeyCode::Enter);
    assert!(app.word_edit.is_some(), "应打开词条编辑弹层");
    // 改单词字段：清空后输入新词
    for _ in 0..16 {
        app.apply_action(typewords_tui::app::Action::WordsEditBackspace);
    }
    type_str(&mut app, "renamed");
    app.apply_action(typewords_tui::app::Action::WordsEditSave);
    assert!(app.word_edit.is_none(), "保存后弹层应关闭");

    let dict = app.boot.base.study_dict().expect("当前词库");
    assert!(
        dict.words.iter().any(|word| word.word == "renamed"),
        "词条应被改名"
    );
}

#[test]
fn words_screen_adds_and_deletes_entries() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, typewords_tui::app::Screen::Words);

    let before = app
        .boot
        .base
        .study_dict()
        .map(|dict| dict.words.len())
        .unwrap_or(0);

    // 新增
    press(&mut app, KeyCode::Char('a'));
    assert!(app.word_edit.is_some(), "应打开新增弹层");
    type_str(&mut app, "brandnew");
    app.apply_action(typewords_tui::app::Action::WordsEditSave);
    let after_add = app
        .boot
        .base
        .study_dict()
        .map(|dict| dict.words.len())
        .unwrap_or(0);
    assert_eq!(after_add, before + 1, "应新增一个词条");

    // 删除第一个
    app.words_cursor = 0;
    press(&mut app, KeyCode::Char('d'));
    let after_delete = app
        .boot
        .base
        .study_dict()
        .map(|dict| dict.words.len())
        .unwrap_or(0);
    assert_eq!(after_delete, before, "应删除一个词条");
}

#[test]
fn words_screen_creates_a_custom_dictionary() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, typewords_tui::app::Screen::Words);

    press(&mut app, KeyCode::Char('n'));
    assert!(app.edit.is_some(), "应打开词典名称输入");
    type_str(&mut app, "我的收藏");
    app.apply_action(typewords_tui::app::Action::EditSave);
    assert!(app.edit.is_none(), "保存后输入弹层应关闭");

    // 新词典应登记到用户词库目录
    let catalog_file = fixture.home().join("list").join("word.json");
    let text = std::fs::read_to_string(&catalog_file).expect("用户词库目录");
    let entries: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let entries = entries.as_array().expect("数组");
    assert!(
        entries
            .iter()
            .any(|entry| entry.get("name").and_then(|name| name.as_str()) == Some("我的收藏")),
        "新词典应登记在用户词库目录"
    );
    // 词条文件也应落盘
    let words_file = fixture
        .home()
        .join("dicts")
        .join("en")
        .join("word")
        .join("custom-1.json");
    assert!(words_file.is_file(), "新词典应有词条文件");
}

#[test]
fn words_screen_marks_entries_in_bulk() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, typewords_tui::app::Screen::Words);

    // 勾选前两个
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(app.words_marked.len(), 2, "应勾选两个词条");

    // 标记为已掌握
    press(&mut app, KeyCode::Char('2'));
    let known = app.boot.base.known().expect("已掌握词典");
    assert_eq!(known.words.len(), 2, "两个词条应进入已掌握词典");
    assert!(app.words_marked.is_empty(), "标记后应清空勾选");
}
