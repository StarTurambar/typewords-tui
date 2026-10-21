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

    // 自建流程只在「自定义流程」模式下生效（对齐上游 getFlowIdForMode）
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Enter);
    let session = app.session.as_ref().expect("会话");
    assert_eq!(
        session.flow().id,
        "system",
        "普通学习模式不应被自建流程顶替"
    );

    // 切到自定义流程模式后，练习使用自建流程
    press(&mut app, KeyCode::Esc);
    app.start_practice_with_mode(typewords_tui::domain::types::WordPracticeMode::Custom);
    let session = app.session.as_ref().expect("会话");
    assert_eq!(session.flow().id, flow_id, "自定义流程模式应使用自建流程");
}

/// 损坏的自建流程存档不能让程序崩溃。
///
/// `flows.json` 是用户可编辑的文件，节点被清空后取步骤与推进游标都会越界。
#[test]
fn a_flow_with_no_nodes_degrades_instead_of_panicking() {
    use typewords_tui::domain::flow::{
        PracticeFlowConfig, PracticeFlowCursor, advance_step_cursor, get_step,
        is_valid_flow_config, resolve_flow_config_or_system,
    };

    let broken = PracticeFlowConfig {
        id: "custom-1".to_string(),
        version: 1,
        mode: typewords_tui::domain::types::WordPracticeMode::Custom,
        label: "broken".to_string(),
        nodes: Vec::new(),
    };
    assert!(!is_valid_flow_config(&broken), "空节点列表应判定为非法");

    // 取步骤不 panic
    let step = get_step(&broken, &PracticeFlowCursor::default());
    assert_eq!(
        step.template_id,
        typewords_tui::domain::flow::PracticeStepTemplateId::FollowWrite
    );

    // 推进游标不 panic（视为已完成）
    let (cursor, complete) = advance_step_cursor(&broken, &PracticeFlowCursor::default());
    assert!(complete, "空流程应视为已完成");
    assert_eq!(cursor.node_index, 0);

    // 非法配置回退到内置 system 流程
    let resolved = resolve_flow_config_or_system(&broken);
    assert_eq!(resolved.id, "system");
    assert!(is_valid_flow_config(&resolved));
}

/// 节点存在但步骤为空时同样不能越界。
#[test]
fn a_flow_with_an_empty_step_list_degrades_instead_of_panicking() {
    use typewords_tui::domain::flow::{
        PracticeFlowConfig, PracticeFlowCursor, PracticeFlowNode, PracticeWordsSource,
        advance_step_cursor, get_step, is_valid_flow_config,
    };

    let broken = PracticeFlowConfig {
        id: "custom-2".to_string(),
        version: 1,
        mode: typewords_tui::domain::types::WordPracticeMode::Custom,
        label: "broken".to_string(),
        nodes: vec![PracticeFlowNode {
            id: "n1".to_string(),
            label: "n1".to_string(),
            source: PracticeWordsSource::TaskNew,
            steps: Vec::new(),
        }],
    };
    assert!(!is_valid_flow_config(&broken));

    let _ = get_step(&broken, &PracticeFlowCursor::default());
    let (_, complete) = advance_step_cursor(&broken, &PracticeFlowCursor::default());
    assert!(complete);
}

/// 自建流程的练习进度在重启后要能恢复。
///
/// 恢复时按 `flowId` 查流程配置，内置表里没有自建流程的 id——早期实现直接
/// 用内置表覆盖，导致自建流程的断点进度每次重启都被静默丢弃。
#[test]
fn a_custom_flow_practice_resumes_after_a_restart() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    // 建一个自建流程并启用
    ctrl(&mut app, 'f');
    press(&mut app, KeyCode::Char('n'));
    let flow_id = app.flow_entries().last().expect("新流程").id.clone();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.flow_store.active_id, flow_id);

    // 用自定义流程模式开始一次练习并推进一个词
    press(&mut app, KeyCode::Esc);
    app.start_practice_with_mode(typewords_tui::domain::types::WordPracticeMode::Custom);
    assert!(app.session.is_some(), "应进入练习");
    press(&mut app, KeyCode::Tab); // 跳过当前词，产生进度
    app.save_practice_now();

    // 存档里的 flowId 应是自建流程的 id
    let cache = fixture.home().join("practice-word.json");
    let text = std::fs::read_to_string(&cache).expect("练习缓存应落盘");
    assert!(
        text.contains(&flow_id),
        "缓存应记录自建流程 id，实际：{text}"
    );

    // 重启后应恢复会话
    let app2 = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let session = app2.session.as_ref().expect("重启后应恢复会话");
    assert_eq!(session.flow().id, flow_id, "恢复的应是自建流程");
}

/// `flows.json` 的字段名必须与上游 `PracticeFlowV2` 一致。
///
/// 上游把自建流程存在浏览器 localStorage 的 `PracticeFlowV2` 键下，结构是
/// `{ activeId, flows: { [id]: { config, name, createdAt, updatedAt } } }`，
/// 流程配置里的字段是 `groupSize` / `subSteps` / `templateId`。字段名对不上，
/// 两端就互相读不了存档。
#[test]
fn the_flow_archive_uses_the_upstream_field_names() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    press(&mut app, KeyCode::Char('n'));

    let file = fixture.home().join("flows.json");
    let text = std::fs::read_to_string(&file).expect("流程存档");
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");

    assert!(value.get("activeId").is_some(), "顶层应有 activeId");
    let flows = value.get("flows").expect("顶层应有 flows");
    let entry = flows
        .as_object()
        .and_then(|map| map.values().next())
        .expect("应有一条流程");
    assert!(entry.get("config").is_some(), "流程条目应有 config");
    assert!(entry.get("name").is_some(), "流程条目应有 name");
    assert!(entry.get("createdAt").is_some(), "流程条目应有 createdAt");
    assert!(entry.get("updatedAt").is_some(), "流程条目应有 updatedAt");

    // 步骤里的字段名
    let step = entry
        .get("config")
        .and_then(|config| config.get("nodes"))
        .and_then(|nodes| nodes.get(0))
        .and_then(|node| node.get("steps"))
        .and_then(|steps| steps.get(0))
        .expect("应至少有一个步骤");
    assert!(
        step.get("templateId").is_some(),
        "步骤应有 templateId，实际：{step}"
    );
    assert!(
        step.get("wordAdvance").is_some(),
        "步骤应有 wordAdvance，实际：{step}"
    );

    // wordLoop 的字段名与上游一致
    let word_advance = step.get("wordAdvance").expect("wordAdvance");
    assert_eq!(
        word_advance.get("type").and_then(|value| value.as_str()),
        Some("wordLoop")
    );
    assert!(
        word_advance.get("groupSize").is_some(),
        "wordLoop 应有 groupSize，实际：{word_advance}"
    );
    assert!(
        word_advance.get("subSteps").is_some(),
        "wordLoop 应有 subSteps，实际：{word_advance}"
    );
    // 上游的 subSteps 是 `{ templateId }` 对象数组，不是裸模板 id
    let sub_step = word_advance
        .get("subSteps")
        .and_then(|value| value.get(0))
        .expect("应有子步骤");
    assert!(
        sub_step.get("templateId").is_some(),
        "子步骤应是 {{ templateId }} 对象，实际：{sub_step}"
    );

    // 错词清空动作的字段名
    let on_end = step
        .get("onEnd")
        .and_then(|value| value.get(0))
        .expect("应有 onEnd");
    assert_eq!(
        on_end.get("type").and_then(|value| value.as_str()),
        Some("wrongWordClear")
    );
    assert!(
        on_end.get("templateId").is_some(),
        "wrongWordClear 应有 templateId，实际：{on_end}"
    );
}

/// 损坏的流程存档要被隔离，不能静默吞掉。
///
/// `flows.json` 是用户可编辑的文件；早期实现直接 `unwrap_or_default()`，
/// 解析失败时文件原样留着，用户看不出存档已经坏了。
#[test]
fn a_corrupt_flow_archive_is_quarantined() {
    let fixture = common::Fixture::new();
    let file = fixture.home().join("flows.json");
    std::fs::write(&file, "{ not json").unwrap();

    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    assert!(app.flow_store.flows.is_empty(), "损坏的存档应回退到空集合");

    let quarantined: Vec<_> = std::fs::read_dir(fixture.home())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
        .collect();
    assert_eq!(quarantined.len(), 1, "损坏文件应被隔离保留");
}
