//! 自定义练习流程：新建副本、编辑步骤、启用与持久化。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut app::App, character: char) {
    app.on_key(KeyEvent::new(
        KeyCode::Char(character),
        KeyModifiers::CONTROL,
    ));
}

#[test]
fn flow_editor_creates_a_duplicate_and_saves_it() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    // 主页 Ctrl+F 打开流程页
    ctrl(&mut app, 'f');
    assert_eq!(app.screen, app::Screen::Flows);
    let builtin = app.flow_entries().len();
    assert!(builtin >= 7, "应列出七个内置流程");

    // 新建副本
    press(&mut app, KeyCode::Char('n'));
    let entries = app.flow_entries();
    assert_eq!(entries.len(), builtin + 1, "应多出一条自建流程");
    assert!(entries.last().expect("最后一条").custom, "新流程应是自建的");

    // 存档已落盘
    let flows = fixture.home().join("flows.json");
    assert!(flows.is_file(), "流程存档应写入数据目录");
    let text = std::fs::read_to_string(&flows).expect("读存档");
    assert!(text.contains("activeId"), "存档结构与上游一致");
}

#[test]
fn flow_editor_edits_steps_and_persists() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    press(&mut app, KeyCode::Char('n'));
    let flow_id = app.flow_entries().last().expect("新流程").id.clone();

    // 进入编辑
    press(&mut app, KeyCode::Char('e'));
    assert!(app.flow_editing, "应进入编辑模式");

    let before = app
        .flow_store
        .flows
        .get(&flow_id)
        .expect("流程")
        .config
        .nodes
        .iter()
        .map(|node| node.steps.len())
        .sum::<usize>();
    press(&mut app, KeyCode::Char('a'));
    let after = app
        .flow_store
        .flows
        .get(&flow_id)
        .expect("流程")
        .config
        .nodes
        .iter()
        .map(|node| node.steps.len())
        .sum::<usize>();
    assert_eq!(after, before + 1, "应新增一个步骤");

    // 删除步骤
    press(&mut app, KeyCode::Char('d'));
    let removed = app
        .flow_store
        .flows
        .get(&flow_id)
        .expect("流程")
        .config
        .nodes
        .iter()
        .map(|node| node.steps.len())
        .sum::<usize>();
    assert_eq!(removed, before, "应删除一个步骤");
}

#[test]
fn activating_a_custom_flow_changes_the_practice_session() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    press(&mut app, KeyCode::Char('n'));
    let flow_id = app.flow_entries().last().expect("新流程").id.clone();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.flow_store.active_id, flow_id, "应启用新流程");

    // 开始的练习使用自建流程
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Enter);
    let session = app.session.as_ref().expect("会话");
    assert_eq!(session.flow().id, flow_id, "练习应使用自建流程");
}
