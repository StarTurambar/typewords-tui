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

/// 编辑词条不应清空未被编辑的内容。
///
/// 例句、短语、近义词、词源、关联词、词性与 id 都不在弹层的编辑范围里，
/// 早期实现用 `..Default::default()` 重建整个词条，改一个词就把它们全丢了。
#[test]
fn editing_a_word_keeps_its_other_content() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));

    let before = app.boot.base.study_dict().expect("当前词库").words[0].clone();
    assert!(!before.sentences.is_empty(), "测试词条应有例句");
    assert!(!before.phrases.is_empty(), "测试词条应有短语");
    assert!(!before.synos.is_empty(), "测试词条应有近义词");
    assert!(!before.etymology.is_empty(), "测试词条应有词源");

    // 编辑第 0 个词条，只改音标
    app.words_cursor = 0;
    press(&mut app, KeyCode::Enter);
    assert!(app.word_edit.is_some(), "应打开词条编辑弹层");
    app.apply_action(typewords_tui::app::Action::WordsEditSave);

    let after = app.boot.base.study_dict().expect("当前词库").words[0].clone();
    assert_eq!(after.sentences, before.sentences, "例句应保留");
    assert_eq!(after.phrases, before.phrases, "短语应保留");
    assert_eq!(after.synos, before.synos, "近义词应保留");
    assert_eq!(after.etymology, before.etymology, "词源应保留");
    assert_eq!(after.rel_words, before.rel_words, "关联词应保留");
    assert_eq!(after.id, before.id, "id 应保留");
    assert_eq!(
        after.trans.first().map(|trans| trans.pos.clone()),
        before.trans.first().map(|trans| trans.pos.clone()),
        "原词性应保留"
    );
}

/// 词条改动不能改写资源目录里的内置词库。
///
/// 内置词库随发布包分发，被改写后用户无法恢复；`tw import-dict` 也只能写
/// 数据目录。改动应落到数据目录下的副本。
#[test]
fn editing_a_word_does_not_touch_the_builtin_asset_file() {
    let fixture = common::Fixture::new();
    let asset_file = fixture
        .assets()
        .join("dicts")
        .join("en")
        .join("word")
        .join("toy.json");
    let asset_before = std::fs::read(&asset_file).expect("内置词库");

    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    app.words_cursor = 0;
    press(&mut app, KeyCode::Enter);
    app.apply_action(typewords_tui::app::Action::WordsEditSave);

    let asset_after = std::fs::read(&asset_file).expect("内置词库");
    assert_eq!(asset_before, asset_after, "内置词库文件不应被改写");

    // 改动应落在数据目录的副本里
    let user_file = fixture
        .home()
        .join("dicts")
        .join("en")
        .join("word")
        .join("toy.json");
    assert!(user_file.is_file(), "改动应写入数据目录的词条文件");
}

/// 删除词条后，勾选的下标要跟着重映射。
///
/// 否则被勾选的词会「漂移」到别的词上——按「已掌握」后标错词。
#[test]
fn deleting_a_word_remaps_the_marked_indices() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));

    let words: Vec<String> = app
        .boot
        .base
        .study_dict()
        .expect("当前词库")
        .words
        .iter()
        .map(|word| word.word.clone())
        .collect();

    // 勾选第 0、2、4 个词条
    app.words_cursor = 0;
    press(&mut app, KeyCode::Char(' '));
    app.words_cursor = 2;
    press(&mut app, KeyCode::Char(' '));
    app.words_cursor = 4;
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(app.words_marked, vec![0, 2, 4], "应勾选三个词条");

    // 删除第 0 个词条
    app.words_cursor = 0;
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(
        app.words_marked,
        vec![1, 3],
        "删除后勾选下标应前移，且仍指向原来那两个词"
    );

    // 标记为已掌握后，进「已掌握」词典的应是原来勾选的两个词
    press(&mut app, KeyCode::Char('2'));
    let known = app.boot.base.known().expect("已掌握词典");
    let known_words: Vec<String> = known.words.iter().map(|word| word.word.clone()).collect();
    assert_eq!(
        known_words,
        vec![words[2].clone(), words[4].clone()],
        "已掌握的应是原来勾选的两个词"
    );
}

/// 勾选行点一下是取消勾选，不是移动光标。
///
/// 早期实现只比较 `words_marked.first()`，第二个及之后的勾选行点不动，
/// 而渲染层却把它们画成勾选态——显示与点击行为矛盾。
#[test]
fn clicking_any_marked_row_toggles_it_off() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));

    // 勾选第 2 行和第 5 行
    app.words_cursor = 1;
    press(&mut app, KeyCode::Char(' '));
    app.words_cursor = 4;
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(app.words_marked.len(), 2, "应勾选两个词条");

    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let toggle_rows: Vec<u16> = targets
        .iter()
        .filter(|target| target.action == typewords_tui::app::Action::WordsToggleMark)
        .map(|target| target.rect.y)
        .collect();
    assert_eq!(
        toggle_rows.len(),
        2,
        "两个勾选行都应是切换勾选，实际 {toggle_rows:?}"
    );
}

/// 搜索过滤后，点击某一行要选中**那一行显示的词条**。
///
/// 早期实现用原始词表下标算位移，过滤后每行点击都会偏移。
#[test]
fn clicking_a_filtered_row_selects_that_row() {
    let fixture = common::Fixture::new();
    let mut app = typewords_tui::app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));

    // 搜索 "1" 命中 word01 / word10 / word11——原始下标不连续，
    // 才能暴露「用原始下标算位移」的错误。
    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "1");
    press(&mut app, KeyCode::Esc);
    let indices = app.filtered_word_indices();
    assert_eq!(indices.len(), 3, "应过滤出三行");
    assert_ne!(indices[1], indices[0] + 1, "过滤后的原始下标应不连续");

    // 点击第 2 个可见行（可见位置 1）
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    // 词条列表的目标按可见顺序排列，取第 2 个
    let visible_row = 1usize;
    let target = targets
        .iter()
        .filter(|target| matches!(target.action, typewords_tui::app::Action::WordsMove(_)))
        .nth(visible_row)
        .expect("应有移动目标的行");
    let expected_index = indices[visible_row];
    let expected_word = app.boot.base.study_dict().expect("当前词库").words[expected_index]
        .word
        .clone();

    // 执行该动作
    if let typewords_tui::app::Action::WordsMove(delta) = target.action {
        app.apply_action(typewords_tui::app::Action::WordsMove(delta));
    }
    assert_eq!(app.words_cursor, visible_row, "光标应落在被点的行上");
    let selected = app.boot.base.study_dict().expect("当前词库").words[indices[app.words_cursor]]
        .word
        .clone();
    assert_eq!(selected, expected_word, "选中的应是那一行显示的词条");
}
