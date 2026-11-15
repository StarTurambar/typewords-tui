//! 词库词数只有一个来源：词条数据。
//!
//! 词库页行 / 详情面板、主页词典磁贴与推荐列表过去各取各的字段
//! （目录登记值 vs 词条数），编辑或删除词条后三处就会各显示一个数。
//! 这里按用户可见的顺序（词库页 → 主页磁贴 → 切走 / 重启后 → 用户目录
//! 登记值）锁住同一个词数。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Screen};
use unicode_width::UnicodeWidthStr;

const SIZE: (u16, u16) = (120, 50);
/// 资源词库在词库页的位置：三本内置词典（收藏 / 错词 / 已掌握）在前。
const RESOURCE_ROW: usize = 3;
/// 夹具资源词库的 id（目录里登记成数字 1）。
const TOY_ID: &str = "1";

fn build(fixture: &common::Fixture) -> app::App {
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("build app");
    app.set_viewport(Rect::new(0, 0, SIZE.0, SIZE.1));
    app
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_str(app: &mut app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

fn render_text(app: &app::App) -> String {
    let backend = TestBackend::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..SIZE.1 {
        let mut skip = 0usize;
        for x in 0..SIZE.0 {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
                let width = UnicodeWidthStr::width(cell.symbol());
                if width > 1 {
                    skip = width - 1;
                }
            }
        }
        out.push('\n');
    }
    out
}

/// 词库页里某一本词典的行。
fn entry(app: &app::App, id: &str) -> app::DictEntry {
    app.dict_entries()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap_or_else(|| panic!("词库页应有 id 为 {id} 的条目"))
}

/// 用户词库目录里登记的 `length`。
fn registered_length(fixture: &common::Fixture, id: &str) -> Option<usize> {
    let text = std::fs::read_to_string(fixture.home().join("list").join("word.json")).ok()?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;
    entries
        .iter()
        .find(|entry| entry.get("id").and_then(|value| value.as_str()) == Some(id))
        .and_then(|entry| entry.get("length").and_then(|value| value.as_u64()))
        .map(|value| value as usize)
}

/// 打开当前资源词库的词条管理页并删掉第一条词条。
fn delete_first_resource_word(app: &mut app::App) {
    press(app, KeyCode::Char('d'));
    assert_eq!(app.screen, Screen::DictPicker);
    press(app, KeyCode::Char('w'));
    assert_eq!(app.screen, Screen::Words);
    app.words_cursor = 0;
    press(app, KeyCode::Char('d'));
    assert_eq!(
        app.boot
            .base
            .study_dict()
            .map(|dict| dict.words.len())
            .unwrap_or(0),
        11,
        "夹具词库 12 词，删一条后应剩 11 条"
    );
}

/// 新建一本自建词库并切过去，返回新词库的 id。
fn create_custom_dict(app: &mut app::App, name: &str) -> String {
    press(app, KeyCode::Char('n'));
    type_str(app, name);
    press(app, KeyCode::Enter);
    let id = app
        .boot
        .base
        .study_dict()
        .map(|dict| dict.id.clone())
        .expect("当前词库");
    assert!(
        id.starts_with("custom-"),
        "新建后应切到自建词库，实际是 {id}"
    );
    id
}

#[test]
fn home_tile_and_dict_page_agree_after_deleting_a_resource_word() {
    let fixture = common::Fixture::new();
    let mut app = build(&fixture);

    delete_first_resource_word(&mut app);

    assert_eq!(
        app.dict_entry_at(RESOURCE_ROW).map(|entry| entry.length),
        Some(11),
        "词库页词数应跟着词条数走，而不是目录里登记的 12"
    );

    // 回主页看磁贴。
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Home);
    let text = render_text(&app);
    assert!(text.contains("11词"), "主页磁贴应显示 11 词：\n{text}");
    assert!(
        !text.contains("12词"),
        "主页磁贴不应再显示登记的 12 词：\n{text}"
    );
}

#[test]
fn the_word_count_survives_a_restart() {
    let fixture = common::Fixture::new();
    let mut app = build(&fixture);
    delete_first_resource_word(&mut app);
    // 词条文件由删除动作写入；存档也要落盘，模拟正常退出。
    app.save_dict_now();
    drop(app);

    let restarted = build(&fixture);
    let words = restarted
        .boot
        .base
        .study_dict()
        .map(|dict| dict.words.len())
        .unwrap_or(0);
    assert_eq!(words, 11, "重启后应从词条文件加载 11 词");
    assert_eq!(
        entry(&restarted, TOY_ID).length,
        11,
        "重启后词库页也应显示 11 词"
    );
    let text = render_text(&restarted);
    assert!(text.contains("11词"), "重启后主页磁贴应是 11 词：\n{text}");
}

/// 切到别的词库后，旧词库的词表会从存档里清掉；重启后没有词表可数，
/// 词库页必须读存档里的真实词数，而不是回到目录登记的 12。
#[test]
fn a_switched_away_dict_keeps_its_real_count_after_restart() {
    let fixture = common::Fixture::new();
    let mut app = build(&fixture);
    delete_first_resource_word(&mut app);
    // 回到词库页，新建一本自建词库并切过去。
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::DictPicker);
    create_custom_dict(&mut app, "另一本");
    app.save_dict_now();
    drop(app);

    let restarted = build(&fixture);
    let study = restarted
        .boot
        .base
        .study_dict()
        .expect("重启后应有当前词库");
    assert!(study.words.is_empty(), "自建词库是空的");
    assert_eq!(
        entry(&restarted, TOY_ID).length,
        11,
        "切走的词库没有词表可数，应读存档里的 11 词"
    );
    // 内置资源目录只读：登记值保持原样，不能因为编辑就被改写。
    let assets_catalog = std::fs::read_to_string(fixture.assets().join("list").join("word.json"))
        .expect("资源词库目录");
    let assets: Vec<serde_json::Value> = serde_json::from_str(&assets_catalog).expect("JSON");
    assert_eq!(
        assets[0].get("length").and_then(|value| value.as_u64()),
        Some(12),
        "资源目录里的登记值不应被改写"
    );
}

/// 还没打开过的词库只有登记值可用：词库页与推荐列表都显示它，
/// 而且不会为了数词去读那本词库的词条文件。
#[test]
fn an_unowned_dict_uses_the_registered_count_in_recommendations() {
    let fixture = common::Fixture::new();
    let catalog_file = fixture.assets().join("list").join("word.json");
    let mut entries: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&catalog_file).expect("资源词库目录"))
            .expect("JSON");
    entries.push(serde_json::json!({
        "id": 2,
        "enName": "extra",
        "name": "推荐词库",
        "description": "还没打开过",
        "url": "extra.json",
        "length": 7,
        "language": "en",
        "translateLanguage": "zh_CN",
        "type": "word",
        "category": "测试",
        "tags": ["测试"]
    }));
    std::fs::write(
        &catalog_file,
        serde_json::to_string(&entries).expect("JSON"),
    )
    .expect("写入");

    let app = build(&fixture);
    assert_eq!(entry(&app, "2").length, 7, "没打开过的词库用登记值");
    let text = render_text(&app);
    assert!(text.contains("7 词"), "推荐列表应显示登记的 7 词：\n{text}");
}

/// 自建词库的词条文件改写时，用户词库目录里的 length 登记值也要跟着走：
/// 推荐列表这类「还没打开过这本词库」的界面读的就是它。
#[test]
fn editing_a_user_dict_keeps_the_registered_length_in_sync() {
    let fixture = common::Fixture::new();
    let mut app = build(&fixture);
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    let id = create_custom_dict(&mut app, "我的词库");
    assert_eq!(
        registered_length(&fixture, &id),
        Some(0),
        "新建词典登记 0 词"
    );

    // 加一个词。
    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "alpha");
    app.apply_action(app::Action::WordsEditSave);
    assert_eq!(
        registered_length(&fixture, &id),
        Some(1),
        "加词后登记值应变成 1"
    );

    // 删掉它，登记值要归零。
    app.words_cursor = 0;
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(
        registered_length(&fixture, &id),
        Some(0),
        "删空后登记值应归零"
    );
    assert_eq!(
        entry(&app, TOY_ID).length,
        12,
        "没被编辑过的资源词库仍按登记值显示"
    );
}
