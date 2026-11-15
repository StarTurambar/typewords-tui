//! 自定义练习流程：新建副本、编辑步骤、启用与持久化。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;
use typewords_tui::domain::flow::PracticeStepTemplateId;
use typewords_tui::domain::types::WordPracticeMode;
use typewords_tui::ui::focus;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut app::App, character: char) {
    app.on_key(KeyEvent::new(
        KeyCode::Char(character),
        KeyModifiers::CONTROL,
    ));
}

/// 流程列表里标 `●` 的流程 id。
fn active_flow_ids(app: &app::App) -> Vec<String> {
    app.flow_entries()
        .into_iter()
        .filter(|entry| entry.active)
        .map(|entry| entry.id)
        .collect()
}

/// 用 TestBackend 渲染一帧，返回带空格的文本（顶部模式标签等断言用）。
fn render_text(app: &app::App, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use unicode_width::UnicodeWidthStr;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut skip_cells = 0usize;
        for x in 0..width {
            if skip_cells > 0 {
                skip_cells -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                let symbol = cell.symbol();
                out.push_str(symbol);
                let display_width = UnicodeWidthStr::width(symbol);
                if display_width > 1 {
                    skip_cells = display_width - 1;
                }
            }
        }
        out.push('\n');
    }
    out
}

/// 流程页的 `●` 必须跟着当前练习模式走。
///
/// 早期实现拿「当前模式的流程 id」和「按模式取到的流程配置 id」比较——后者
/// 恒等于前者，于是没有自建流程时七行内置流程全是 `●`，用户看不出当前用哪条。
#[test]
fn the_flow_page_marks_only_the_current_practice_mode_as_active() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    assert_eq!(app.screen, app::Screen::Flows);

    assert_eq!(
        active_flow_ids(&app),
        vec!["system".to_string()],
        "默认「学习」模式下只有「学习」这一行是 ●"
    );

    let entries = app.flow_entries();
    let builtin: Vec<_> = entries.iter().filter(|entry| !entry.custom).collect();
    assert_eq!(
        builtin.len(),
        7,
        "流程页只列七个内置流程（单词测试与自定义流程没有内置 flow）"
    );
    let mut ids: Vec<&str> = builtin.iter().map(|entry| entry.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 7, "七个内置流程的 id 不应重复: {ids:?}");
    for entry in &builtin {
        assert_eq!(
            typewords_tui::domain::flow::get_flow_id_for_mode(entry.config.mode),
            entry.id,
            "内置流程「{}」的 mode 与 id 不对应，启用时就会写回错误的模式",
            entry.name
        );
    }
}

/// 在内置流程上按 Enter 必须真的切换练习模式，并把模式写回设置。
#[test]
fn activating_a_builtin_flow_switches_the_practice_mode() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');

    // 先建一个自建流程并启用：它只是「已启用」，练习模式仍是「学习」，
    // 所以下一轮跑的仍是内置「学习」流程，● 必须留在这一行。
    press(&mut app, KeyCode::Char('n'));
    let custom_id = app.flow_store.active_id.clone();
    assert!(!custom_id.is_empty(), "新建副本应启用它");
    assert_eq!(
        app.boot.settings.word_practice_mode,
        WordPracticeMode::System
    );
    assert_eq!(
        active_flow_ids(&app),
        vec!["system".to_string()],
        "模式仍是「学习」时 ● 不能跳到未生效的自建流程上"
    );
    let custom_entry = app
        .flow_entries()
        .into_iter()
        .find(|entry| entry.id == custom_id)
        .expect("应有自建流程");
    assert!(custom_entry.enabled, "自建流程应标记为已启用");
    assert!(!custom_entry.active, "未生效的自建流程不该是 active");

    // 光标上移一行到「复习」（自建流程排在最后），按 Enter 切回内置流程。
    let review = app
        .flow_entries()
        .iter()
        .position(|entry| entry.id == "review")
        .expect("应有「复习」内置流程");
    while app.flow_cursor > review {
        press(&mut app, KeyCode::Up);
    }
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        app.boot.settings.word_practice_mode,
        WordPracticeMode::Review,
        "启用「复习」后练习模式应切到 Review"
    );
    assert!(
        app.flow_store.active_id.is_empty(),
        "启用内置流程应清掉自建流程的启用位"
    );
    assert_eq!(
        active_flow_ids(&app),
        vec!["review".to_string()],
        "● 应随模式移到「复习」这一行"
    );

    // 模式要落盘：重启后仍然是「复习」。
    let store = typewords_tui::store::FileStore::new(fixture.home()).expect("store");
    assert_eq!(
        store.load_settings().value.word_practice_mode,
        WordPracticeMode::Review,
        "模式应写回 setting.json"
    );

    // 主页顶栏的「模式」标签同步变化。
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, app::Screen::Home);
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("模式 复习"),
        "主页顶栏模式标签应显示「复习」: {text}"
    );
}

/// 编辑流程时步骤光标要能单独移动，删除的才是选中的那一步。
///
/// 早期实现只有节点光标（↑/↓）能移动，步骤光标写死在 0 号或刚新增的那一步，
/// 于是 `d` / `s` / `g` 永远作用在错误的位置。
#[test]
fn the_flow_editor_moves_the_step_cursor() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    // 复制「学习」流程：第一个节点「新词」有 跟写 → 听写 → 默写 三个步骤。
    press(&mut app, KeyCode::Char('n'));
    let flow_id = app.flow_entries().last().expect("新流程").id.clone();
    press(&mut app, KeyCode::Char('e'));
    assert!(app.flow_editing, "应进入编辑模式");
    assert_eq!(app.flow_step_cursor, 0);

    let templates = |app: &app::App| -> Vec<PracticeStepTemplateId> {
        app.flow_store
            .flows
            .get(&flow_id)
            .expect("流程")
            .config
            .nodes[0]
            .steps
            .iter()
            .map(|step| step.template_id)
            .collect()
    };
    assert_eq!(
        templates(&app),
        vec![
            PracticeStepTemplateId::FollowWrite,
            PracticeStepTemplateId::Listen,
            PracticeStepTemplateId::Dictation
        ],
        "复制来的节点应有三个步骤"
    );

    press(&mut app, KeyCode::Char('l'));
    assert_eq!(app.flow_step_cursor, 1, "l 应把步骤光标移到第 2 步");
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(
        templates(&app),
        vec![
            PracticeStepTemplateId::FollowWrite,
            PracticeStepTemplateId::Dictation
        ],
        "删除的应是第 2 步（听写），而不是 0 号步骤"
    );
    assert_eq!(app.flow_step_cursor, 0, "删完光标回落到前一步");

    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.flow_step_cursor, 0, "h 在首步停住");
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Char('l'));
    assert_eq!(app.flow_step_cursor, 1, "l 在末步停住");
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.flow_step_cursor, 0, "h 移回上一步");
}

/// 流程编辑页的底栏必须提示 `] 换节点`，而不是早已改作换栏的 `Tab`。
///
/// `Tab` 改成「导航栏 ↔ 内容区换栏」后，换节点挪到了 `]`；底栏一度仍写着
/// 「Tab 换节点」，用户照着按只会换栏。这条测试同时锁住提示文案与键位行为，
/// 避免二者再次漂移。
#[test]
fn the_flow_editor_footer_teaches_the_bracket_key() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    // 内置流程不能编辑：先新建副本，再进编辑模式。
    press(&mut app, KeyCode::Char('n'));
    let flow_id = app.flow_entries().last().expect("新流程").id.clone();
    press(&mut app, KeyCode::Char('e'));
    assert!(app.flow_editing, "自建流程应能进入编辑模式");

    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("] 换节点"),
        "底栏应提示「] 换节点」，实际：\n{text}"
    );
    assert!(
        !text.contains("Tab 换节点"),
        "`Tab` 已改为换栏，底栏不应再写「Tab 换节点」：\n{text}"
    );

    // 提示与键位一致：`]` 换节点，`Tab` 只换栏、不动节点光标。
    let node_count = app
        .flow_store
        .flows
        .get(&flow_id)
        .expect("流程")
        .config
        .nodes
        .len();
    assert!(node_count > 1, "复制来的流程应有多个节点");
    let node_before = app.flow_node_cursor;
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(
        app.flow_node_cursor,
        (node_before + 1) % node_count,
        "`]` 应切到下一个节点"
    );

    let node_before = app.flow_node_cursor;
    let pane_before = focus::pane(&app);
    press(&mut app, KeyCode::Tab);
    assert_ne!(focus::pane(&app), pane_before, "`Tab` 应换栏");
    assert_eq!(app.flow_node_cursor, node_before, "`Tab` 不应再换节点");
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

    // 重启后停在主页；显式开始练习才恢复会话。
    let mut app2 = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    assert_eq!(app2.screen, app::Screen::Home, "启动应停在主页");
    app2.start_practice_with_mode(typewords_tui::domain::types::WordPracticeMode::Custom);
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

/// 流程页的 `●` 只能是「下一轮练习真正会跑的流程」。
///
/// 早期 `●` 同时表示「已启用」和「将实际使用」：启用自建流程后它立刻跳到
/// 自建行，而练习模式不是「自定义流程」时下一轮跑的仍是内置流程。这里把
/// 7 个有内置流程的模式 × {无 / 有已启用自建流程} 逐个开练，比对两者。
#[test]
fn flow_marker_matches_the_flow_the_next_practice_actually_uses() {
    for with_custom in [false, true] {
        for mode in typewords_tui::domain::flow::BUILTIN_FLOW_MODES {
            let fixture = common::Fixture::new();
            let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
            ctrl(&mut app, 'f');
            if with_custom {
                press(&mut app, KeyCode::Char('n'));
            }
            app.boot.settings.word_practice_mode = mode;

            let marked = active_flow_ids(&app);
            assert_eq!(
                marked.len(),
                1,
                "{mode:?}（自建流程 {with_custom}）应恰好标一条流程：{marked:?}"
            );

            app.start_practice_with_mode(mode);
            let session = app
                .session
                .as_ref()
                .unwrap_or_else(|| panic!("{mode:?} 应能开始练习"));
            assert_eq!(
                marked[0],
                session.flow().id,
                "{mode:?}（自建流程 {with_custom}）标了 ● 的流程与实际生效的流程不一致"
            );
        }
    }
}

/// 自建流程「已启用」但未生效时：只标文字标签，页头说明怎么切过去。
#[test]
fn an_enabled_but_inactive_custom_flow_is_labelled_not_marked() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(
        app.boot.settings.word_practice_mode,
        WordPracticeMode::System
    );

    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("当前：学习"),
        "页头「当前」应是实际生效的内置流程：{text}"
    );
    assert!(
        !text.contains("当前：学习 副本"),
        "未生效的自建流程不能出现在「当前」里：{text}"
    );
    assert!(
        text.contains("已启用「学习 副本」，切到「自定义流程」后生效"),
        "页头应说明它为什么还没生效：{text}"
    );
    assert!(
        text.contains("学习 副本 （已启用）"),
        "列表里应标「（已启用）」而不是抢 ●：{text}"
    );
}

/// 「自定义流程」模式还没启用自建流程时，没有任何流程会执行。
///
/// 此时既不该标 `●`（会让人以为点开就会按它编排），页头也不能写一个
/// 具体流程名；开练会被拒绝并提示先启用。
#[test]
fn custom_mode_without_an_enabled_flow_marks_nothing() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    app.boot.settings.word_practice_mode = WordPracticeMode::Custom;

    assert_eq!(app.effective_flow_id(), None, "没有可用的流程");
    assert!(
        active_flow_ids(&app).is_empty(),
        "没有可用流程时不该标 ●：{:?}",
        active_flow_ids(&app)
    );
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("自定义流程模式尚未启用自建流程"),
        "页头应说明当前没有可用的流程：{text}"
    );

    press(&mut app, KeyCode::Esc);
    app.start_practice_with_mode(WordPracticeMode::Custom);
    assert!(app.session.is_none(), "没有可用流程时不应开始练习");
    let toast = app
        .current_toast(typewords_tui::domain::timer::now_ms())
        .map(|toast| toast.message.clone())
        .unwrap_or_default();
    assert!(
        toast.contains("自定义流程"),
        "应提示先启用自建流程，实际：{toast}"
    );
}

/// 已启用的自建流程配置损坏时回退内置流程：`●` 必须跟着回到实际生效的那一行。
#[test]
fn an_enabled_flow_with_a_broken_config_falls_back_to_the_builtin_flow() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    ctrl(&mut app, 'f');
    press(&mut app, KeyCode::Char('n'));
    let custom_id = app.flow_store.active_id.clone();
    // 模拟手工改坏 flows.json：节点被清空。
    app.flow_store
        .flows
        .get_mut(&custom_id)
        .expect("自建流程")
        .config
        .nodes
        .clear();
    app.boot.settings.word_practice_mode = WordPracticeMode::Custom;

    assert_eq!(
        app.effective_flow_id().as_deref(),
        Some("system"),
        "非法配置应回退到内置 system 流程"
    );
    assert_eq!(
        active_flow_ids(&app),
        vec!["system".to_string()],
        "回退后 ● 应回到「学习」这一行"
    );
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("「学习 副本」的配置不可用，已回退到内置流程"),
        "页头应说明已经回退：{text}"
    );
}
