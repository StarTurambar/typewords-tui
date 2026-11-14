//! 内置词典（收藏 / 错词 / 已掌握）在词库页的可见性、切换与持久化。
//!
//! 词库页的条目 = 三本内置词典在前 + 词库资源在后；光标是**条目下标**，
//! 所以断言一律经 `App::dict_entry_at` / `dict_entry_of_study` 把位置换成
//! 条目，而不是直接索引 `Catalog`。持久化一律用 `FileStore::load_dict`
//! 重新读盘核对，不去读内存里的同一个对象。

mod common;

use std::fs;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Screen};
use typewords_tui::domain::constants::{DICT_WORD_COLLECT, DICT_WORD_KNOWN, DICT_WORD_WRONG};
use typewords_tui::domain::types::{BaseState, Word};
use typewords_tui::store::FileStore;
use unicode_width::UnicodeWidthStr;

const WIDE: (u16, u16) = (120, 40);

fn area_of(size: (u16, u16)) -> Rect {
    Rect::new(0, 0, size.0, size.1)
}

fn build(fixture: &common::Fixture, size: (u16, u16)) -> app::App {
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("build app");
    app.set_viewport(area_of(size));
    app
}

/// 建一个已经同步好视口的测试应用（Fixture 必须活得比 App 久，一起返回）。
fn test_app(size: (u16, u16)) -> (common::Fixture, app::App) {
    let fixture = common::Fixture::new();
    let app = build(&fixture, size);
    (fixture, app)
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// 打开词库页（主页按 `d`）。
fn open_dict_page(size: (u16, u16)) -> (common::Fixture, app::App) {
    let (fixture, mut app) = test_app(size);
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.screen, Screen::DictPicker);
    (fixture, app)
}

/// 把词库页光标移到第 `position` 行。
fn goto_entry(app: &mut app::App, position: usize) {
    while app.dict_cursor < position {
        press(app, KeyCode::Down);
    }
    while app.dict_cursor > position {
        press(app, KeyCode::Up);
    }
    assert_eq!(
        app.dict_cursor, position,
        "光标应停在词库页第 {position} 行"
    );
}

fn entry_id(app: &app::App, position: usize) -> String {
    app.dict_entry_at(position)
        .unwrap_or_else(|| panic!("词库页应有第 {position} 行"))
        .id
}

/// 给内置词典塞词条（模拟练习中收藏 / 记错 / 标记已掌握的结果）。
fn seed_system_dict(app: &mut app::App, id: &str, words: &[Word]) {
    let position = app
        .boot
        .base
        .word
        .book_list
        .iter()
        .position(|dict| dict.system && dict.id == id)
        .unwrap_or_else(|| panic!("存档里应有内置词典 {id}"));
    {
        let dict = &mut app.boot.base.word.book_list[position];
        dict.words = words.to_vec();
        dict.length = dict.words.len();
    }
    app.save_dict_now();
}

fn last_toast(app: &app::App) -> Option<String> {
    app.toasts.last().map(|toast| toast.message.clone())
}

/// 从磁盘重新装载 dict.json。
fn stored_base(fixture: &common::Fixture) -> BaseState {
    FileStore::new(fixture.home())
        .expect("store")
        .load_dict()
        .value
}

fn render_text(app: &app::App, size: (u16, u16)) -> String {
    let backend = TestBackend::new(size.0, size.1);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..size.1 {
        let mut skip = 0usize;
        for x in 0..size.0 {
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

fn target_rect(app: &app::App, area: Rect, action: app::Action) -> Rect {
    typewords_tui::ui::hit::targets(app, area)
        .into_iter()
        .find(|target| target.action == action)
        .unwrap_or_else(|| panic!("应有动作 {action:?} 的点击目标"))
        .rect
}

fn click(app: &mut app::App, area: Rect, target: Rect) {
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.x + target.width / 2,
            row: target.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
}

// ── 列表 ─────────────────────────────────────────────────────────────────

/// 词库页列出三本内置词典：顺序、名称、system 标记，且都排在资源词库之前。
#[test]
fn system_dicts_are_listed_before_resource_dicts() {
    let (_fixture, app) = open_dict_page(WIDE);
    let entries = app.dict_entries();
    assert_eq!(entries.len(), 4, "夹具 = 3 本内置词典 + 1 本资源词库");

    let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            DICT_WORD_COLLECT,
            DICT_WORD_WRONG,
            DICT_WORD_KNOWN,
            // 夹具目录里 id 是数字 1，资源条目的 id 就是它的字符串形式。
            "1"
        ],
        "内置词典应固定排在最前，资源词库在后"
    );
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, vec!["收藏", "错词", "已掌握", "测试词库"]);

    for (position, entry) in entries[..3].iter().enumerate() {
        assert_eq!(
            entry.kind,
            app::DictEntryKind::System,
            "第 {position} 行应是内置词典"
        );
        assert!(entry.system, "内置词典应带 system 标记");
        assert_eq!(entry.length, 0, "新存档里内置词典还没有词条");
        assert!(!entry.current, "当前学习词典是资源词库");
        assert_eq!(entry.category, "内置词典");
        assert!(!entry.description.is_empty(), "内置词典应带用途说明");
    }

    let resource = &entries[3];
    assert_eq!(resource.kind, app::DictEntryKind::Resource);
    assert!(!resource.system, "资源词库不是内置词典");
    assert!(resource.current, "夹具启动后当前词典是资源词库");
    assert_eq!(resource.length, 12);
    assert_eq!(app.dict_entry_count(), 4);
    assert_eq!(
        app.dict_entry_of_study(),
        Some(3),
        "当前词典在词库页的第 4 行"
    );

    // 界面上确实画出了三本内置词典。
    let text = render_text(&app, WIDE);
    for name in ["收藏", "错词", "已掌握"] {
        assert!(text.contains(name), "词库页应显示内置词典「{name}」");
    }
    assert!(text.contains("内置"), "内置词典行应标注「内置」");
}

// ── 词条管理 ─────────────────────────────────────────────────────────────

/// 进入词库页光标停在当前学习词典；上移到内置词典行按 w 打开它的词条页。
#[test]
fn entering_the_dict_page_stops_at_the_current_dict_and_w_manages_builtin_entries() {
    let (fixture, mut app) = test_app(WIDE);
    let words = fixture.words();
    seed_system_dict(&mut app, DICT_WORD_COLLECT, &words[..2]);

    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.screen, Screen::DictPicker);
    assert_eq!(app.dict_cursor, 3, "光标应停在当前学习词典（第 4 行）");
    assert_eq!(entry_id(&app, 3), "1", "第 4 行是资源词库 toy");

    // 上移三次到「收藏」。
    goto_entry(&mut app, 0);
    assert_eq!(entry_id(&app, app.dict_cursor), DICT_WORD_COLLECT);

    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, Screen::Words, "w 应打开词条管理页");
    assert_eq!(
        app.boot.base.study_dict().map(|dict| dict.id.clone()),
        Some(DICT_WORD_COLLECT.to_string()),
        "w 应把收藏设为词条页的数据源"
    );
    assert!(app.words_is_system(), "收藏是系统词典");

    let indices = app.filtered_word_indices();
    let shown: Vec<String> = indices
        .iter()
        .filter_map(|index| app.boot.base.study_dict()?.words.get(*index))
        .map(|word| word.word.clone())
        .collect();
    assert_eq!(shown, vec!["word00", "word01"], "词条列表应显示收藏的词");

    // 页头标注系统词典，列表里确实画出了收藏进去的词。
    let text = render_text(&app, WIDE);
    assert!(text.contains("（系统词典）"), "词条页页头应标注系统词典");
    assert!(text.contains("word00"), "词条列表应显示收藏的词");

    // 打开词条页本身也把 studyIndex 落盘到收藏。
    assert_eq!(
        stored_base(&fixture).word.study_index,
        0,
        "dict.json 的 studyIndex 应指向收藏"
    );
}

/// 内置词典里删词只改 dict.json：资源目录里的词条文件一个字节都不动。
#[test]
fn removing_a_builtin_entry_updates_dict_json_without_touching_resource_files() {
    let (fixture, mut app) = test_app(WIDE);
    let words = fixture.words();
    seed_system_dict(&mut app, DICT_WORD_COLLECT, &words[..2]);
    let toy_path = fixture
        .assets()
        .join("dicts")
        .join("en")
        .join("word")
        .join("toy.json");
    let before = fs::read(&toy_path).expect("读 toy.json");

    press(&mut app, KeyCode::Char('d'));
    goto_entry(&mut app, 0);
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, Screen::Words);
    assert_eq!(app.filtered_word_indices().len(), 2);

    // 删除当前选中（第一行）的词条。
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.filtered_word_indices().len(), 1, "删掉一条后只剩一个词");

    // 重新装载 dict.json：词条变化落在存档里。
    let base = stored_base(&fixture);
    let collect = base.collect_word().expect("收藏词典");
    assert_eq!(collect.words.len(), 1, "删词应写进 dict.json");
    assert_eq!(collect.words[0].word, "word01");
    assert_eq!(collect.length, 1, "length 应跟着词条数更新");
    assert_eq!(base.word.study_index, 0, "词条页的数据源应落盘");

    // 资源目录里的词条文件一个字节都没变。
    let after = fs::read(&toy_path).expect("读 toy.json");
    assert_eq!(before, after, "内置词典的改动不应写进资源目录");
}

// ── 设为当前词典 ─────────────────────────────────────────────────────────

/// Enter 把内置词典设为当前学习词典：回主页、Toast 提示、studyIndex 落盘。
#[test]
fn enter_switches_the_study_dict_to_a_builtin_one_and_persists_the_index() {
    let (fixture, mut app) = open_dict_page(WIDE);
    goto_entry(&mut app, 0);
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.screen, Screen::Home, "切换后应回主页");
    assert_eq!(
        last_toast(&app).as_deref(),
        Some("已切换到内置词典 收藏"),
        "内置词典的 Toast 文案"
    );
    assert_eq!(
        app.boot.base.study_dict().map(|dict| dict.id.clone()),
        Some(DICT_WORD_COLLECT.to_string())
    );
    assert_eq!(app.dict_entry_of_study(), Some(0), "词库页当前行应变成收藏");

    let base = stored_base(&fixture);
    assert_eq!(
        base.word.study_index, 0,
        "dict.json 的 studyIndex 应指向收藏"
    );
    assert_eq!(
        base.study_dict().map(|dict| dict.id.clone()),
        Some(DICT_WORD_COLLECT.to_string())
    );
}

/// 重启后当前词典仍是上次选的内置词典；空词典也不回退到别的词库。
#[test]
fn a_builtin_study_dict_survives_restart_even_when_empty() {
    let (fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('d'));
    goto_entry(&mut app, 1);
    press(&mut app, KeyCode::Enter);
    assert_eq!(last_toast(&app).as_deref(), Some("已切换到内置词典 错词"));
    assert_eq!(stored_base(&fixture).word.study_index, 1);
    drop(app);

    // 同一 paths / cli 再 build 一次 = 重启。
    let restarted = build(&fixture, WIDE);
    let study = restarted
        .boot
        .base
        .study_dict()
        .expect("重启后仍应有当前词典");
    assert_eq!(study.id, DICT_WORD_WRONG, "重启后应保持错词为当前词典");
    assert!(study.system, "错词是内置词典");
    assert!(study.words.is_empty(), "错词本里没有词也不应回退");
    assert_eq!(
        restarted.dict_entry_of_study(),
        Some(1),
        "词库页光标仍应指向错词"
    );
    assert_eq!(
        restarted.dict_entry_at(1).map(|entry| entry.name.clone()),
        Some("错词".to_string())
    );
}

/// `tw --dict <内置 id>` 与词库页 Enter 等价：启动即指向内置词典，未知 id 报错。
#[test]
fn cli_dict_flag_accepts_a_builtin_dict_and_rejects_unknown_ids() {
    let fixture = common::Fixture::new();
    let mut cli = fixture.cli();
    cli.dict = Some(DICT_WORD_COLLECT.to_string());
    let app = app::build_for_test(fixture.paths(), cli).expect("--dict 应接受内置词典");
    let study = app.boot.base.study_dict().expect("应有当前词典");
    assert_eq!(study.id, DICT_WORD_COLLECT, "--dict 应把学习词典设为收藏");
    assert!(study.system);
    assert_eq!(app.boot.base.word.study_index, 0);
    assert_eq!(app.dict_entry_of_study(), Some(0), "词库页光标指向收藏");
    assert!(study.words.is_empty(), "空词典也不回退");
    drop(app);

    let mut cli = fixture.cli();
    cli.dict = Some("no-such-dict".to_string());
    let err = match app::build_for_test(fixture.paths(), cli) {
        Ok(_) => panic!("未知词库 id 应返回错误"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("未找到词库"),
        "未知 id 的错误应点名词库：{err}"
    );
}

// ── 练习语义 ─────────────────────────────────────────────────────────────

/// 收藏里有词时，把它设为当前词典后「开始学习」能建会话。
#[test]
fn collect_dict_with_words_can_start_a_practice_session() {
    let (fixture, mut app) = test_app(WIDE);
    let words = fixture.words();
    seed_system_dict(&mut app, DICT_WORD_COLLECT, &words[..3]);

    press(&mut app, KeyCode::Char('d'));
    goto_entry(&mut app, 0);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.boot.base.study_dict().map(|dict| dict.id.clone()),
        Some(DICT_WORD_COLLECT.to_string())
    );

    // 主页主按钮：开始学习。
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Practice);
    let session = app.session.as_ref().expect("收藏有词时应能开始练习");
    let current = session.current_word().word;
    assert!(
        words[..3].iter().any(|word| word.word == current),
        "练习词应来自收藏词典，实际是 {current}"
    );
}

/// 已掌握的词在忽略集合里：以它为当前词典「开始学习」不建会话。
#[test]
fn known_dict_as_study_dict_starts_no_session() {
    let (fixture, mut app) = test_app(WIDE);
    let words = fixture.words();
    seed_system_dict(&mut app, DICT_WORD_KNOWN, &words[..2]);

    press(&mut app, KeyCode::Char('d'));
    goto_entry(&mut app, 2);
    press(&mut app, KeyCode::Enter);
    assert_eq!(last_toast(&app).as_deref(), Some("已切换到内置词典 已掌握"));
    assert_eq!(app.screen, Screen::Home);

    press(&mut app, KeyCode::Enter);
    assert!(
        app.session.is_none(),
        "已掌握的词都在忽略集合里，不应建会话"
    );
    assert_eq!(app.screen, Screen::Home, "没有可学单词时应留在主页");
    // 具体文案不在此锁定（设计语义可能调整措辞），只要求有反馈。
    assert!(last_toast(&app).is_some(), "应给出提示");
    assert_eq!(
        app.boot.base.study_dict().map(|dict| dict.id.clone()),
        Some(DICT_WORD_KNOWN.to_string()),
        "不建会话不应顺带换掉当前词典"
    );
}

/// 资源词库仍可切回：Toast 是「已切换到词库」，进度元数据保留。
#[test]
fn switching_back_to_a_resource_dict_keeps_its_progress_metadata() {
    let (fixture, mut app) = test_app(WIDE);
    {
        let dict = app.boot.base.study_dict_mut().expect("toy 是当前词典");
        dict.last_learn_index = 5;
        dict.per_day_study_number = 6;
    }
    app.save_dict_now();

    // 先切到收藏。
    press(&mut app, KeyCode::Char('d'));
    goto_entry(&mut app, 0);
    press(&mut app, KeyCode::Enter);
    assert_eq!(last_toast(&app).as_deref(), Some("已切换到内置词典 收藏"));

    // 再回词库页：光标停在收藏（当前词典），下移三行回到资源词库。
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.dict_cursor, 0, "光标应停在当前词典（收藏）");
    goto_entry(&mut app, 3);
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.screen, Screen::Home);
    assert_eq!(
        last_toast(&app).as_deref(),
        Some("已切换到词库 测试词库"),
        "资源词库保持原文案"
    );
    let study = app.boot.base.study_dict().expect("当前词典");
    assert_eq!(study.name, "测试词库");
    assert!(!study.system, "切回后当前词典应是资源词库");
    assert_eq!(study.last_learn_index, 5, "学习进度应保留");
    assert_eq!(study.per_day_study_number, 6, "每日词数应保留");
    assert_eq!(
        stored_base(&fixture).word.study_index,
        3,
        "studyIndex 应回到资源词库所在的下标"
    );
}

// ── 鼠标 ─────────────────────────────────────────────────────────────────

/// 点内置词典行：第一次点只选中，第二次点才设为当前词典。
#[test]
fn clicking_a_builtin_dict_row_selects_then_activates_it() {
    let (_fixture, mut app) = open_dict_page(WIDE);
    let area = area_of(WIDE);
    assert_eq!(app.dict_cursor, 3, "光标初始停在当前学习词典");

    // 第一次点：只选中，不切词库。
    let row = target_rect(&app, area, app::Action::SelectDict(0));
    click(&mut app, area, row);
    assert_eq!(app.dict_cursor, 0, "第一次点应把光标移到收藏");
    assert_eq!(app.screen, Screen::DictPicker, "第一次点不应切页");
    assert_eq!(
        app.boot.base.study_dict().map(|dict| dict.id.clone()),
        Some("1".to_string()),
        "第一次点不应换当前词典"
    );

    // 第二次点：设为当前词典并回主页。
    let row = target_rect(&app, area, app::Action::ActivateDict(0));
    click(&mut app, area, row);
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(
        app.boot.base.study_dict().map(|dict| dict.id.clone()),
        Some(DICT_WORD_COLLECT.to_string())
    );
    assert_eq!(last_toast(&app).as_deref(), Some("已切换到内置词典 收藏"));
}
