//! TestBackend 渲染快照（只断言关键文本与不 panic）。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;
use unicode_width::UnicodeWidthStr;

fn render_text(app: &app::App, width: u16, height: u16) -> String {
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
                // 宽字符（CJK）占两个单元，跳过其续接单元
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

#[test]
fn home_screen_renders() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let text = render_text(&app, 100, 30);
    assert!(text.contains("Type Words"));
    assert!(text.contains("今日任务"));
    assert!(text.contains("测试词库"));
}

#[test]
fn practice_screen_renders_big_word() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let text = render_text(&app, 100, 30);
    // 大字字体用方块字符拼字
    assert!(
        text.contains('█') || text.contains('▀') || text.contains('▄'),
        "练习页应使用方块字符渲染大字单词"
    );
    assert!(text.contains("输入完成后按空格键继续"));
    assert!(text.contains("释义"));
}

#[test]
fn masked_mode_renders_placeholder_blocks() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_practice_mode =
        typewords_tui::domain::types::WordPracticeMode::DictationOnly;
    app.start_practice();
    let text = render_text(&app, 100, 30);
    let blocks = text.matches('█').count();
    assert!(blocks > 20, "默写模式应显示大块占位符（实际 {blocks} 个）");
}

#[test]
fn typed_letters_are_colored() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let word = app.session.as_ref().unwrap().current_word().word;
    let first = word.chars().next().unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char(first), KeyModifiers::NONE));
    let good = app.theme().good;
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let has_good = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .any(|cell| cell.fg == good);
    assert!(has_good, "正确输入的字母应使用主题 good 色");
}

#[test]
fn all_screens_render_without_panic() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    for (code, expected) in [
        (KeyCode::Char('t'), "统计"),
        (KeyCode::Char('d'), "词库"),
        (KeyCode::Char('s'), "设置"),
        (KeyCode::Char('?'), "帮助"),
    ] {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        for (width, height) in [(80u16, 24u16), (120, 40)] {
            let text = render_text(&app, width, height);
            assert!(text.contains(expected), "屏幕 {expected} 渲染失败");
        }
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    }
}

#[test]
fn shortcuts_screen_renders() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    app.screen = app::Screen::Shortcuts;
    for (width, height) in [(80u16, 24u16), (120, 40)] {
        let text = render_text(&app, width, height);
        assert!(
            text.contains("快捷键") || text.contains("改绑"),
            "快捷键页应渲染"
        );
        assert!(text.contains("显示单词"), "应列出动作名称");
    }
}

#[test]
fn tiny_terminal_shows_hint() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let text = render_text(&app, 40, 10);
    assert!(text.contains("终端太小"));
}

#[test]
fn usage_tips_can_be_hidden() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let with_tips = render_text(&app, 100, 30);
    assert!(with_tips.contains("提示："), "默认应显示使用提示");

    app.boot.settings.show_usage_tips = false;
    let without_tips = render_text(&app, 100, 30);
    assert!(!without_tips.contains("提示："), "关闭后不应显示使用提示");
}

#[test]
fn cjk_text_wraps_within_width() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let text = render_text(&app, 80, 24);
    // 每一行的显示宽度不应超过终端宽度
    for (index, line) in text.lines().enumerate() {
        let width = UnicodeWidthStr::width(line);
        assert!(width <= 80, "第 {index} 行超出宽度: {width}");
    }
}
#[test]
fn home_uses_theme_surfaces() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let theme = app.theme();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let cells = terminal.backend().buffer().content();
    assert!(cells.iter().any(|cell| cell.bg == theme.bg), "背景色应铺满");
    assert!(
        cells.iter().any(|cell| cell.bg == theme.panel),
        "hero 面板应使用 panel"
    );
    assert!(
        cells.iter().any(|cell| cell.fg == theme.accent_alt),
        "应有青色数字/强调"
    );
}

#[test]
fn home_compact_renders_at_min_size() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let text = render_text(&app, 60, 20);
    assert!(text.contains("Type Words"));
    assert!(text.contains("测试词库"));
    assert!(text.contains("今日任务"));
}

#[test]
fn all_builtin_themes_render_home() {
    for theme_name in ["dark", "light", "high-contrast"] {
        let fixture = common::Fixture::new();
        let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
        app.boot.settings.theme = theme_name.to_string();
        let theme = app.theme();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| cell.bg == theme.bg),
            "主题 {theme_name} 的背景应被渲染"
        );
    }
}

#[test]
fn overlay_screens_render() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    app.summary = Some(app::SummaryData {
        spend_ms: 65_000,
        accuracy: 90,
        new_words: 5,
        review: 2,
        wrong: 1,
        last_learn_index: 5,
        length: 12,
        complete: false,
        ..Default::default()
    });
    let text = render_text(&app, 100, 30);
    assert!(text.contains("今日任务完成"));
    app.summary = None;

    app.edit = Some(app::EditState {
        target: app::EditTarget::PerDay,
        buffer: "20".to_string(),
    });
    let text = render_text(&app, 100, 30);
    assert!(text.contains("修改"));
    app.edit = None;

    app.confirm = Some(app::ConfirmState {
        message: "退出测试？".to_string(),
        action: app::ConfirmAction::Quit,
    });
    let text = render_text(&app, 100, 30);
    assert!(text.contains("确认"));
}

#[test]
fn mouse_target_rects_match_rendered_text() {
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 100, 30);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let text = render_text(&app, 100, 30);
    let lines: Vec<&str> = text.lines().collect();

    let start = targets
        .iter()
        .find(|target| target.action == Action::StartPractice)
        .expect("start target");
    let row = lines.get(start.rect.y as usize).copied().unwrap_or("");
    assert!(
        row.contains("开始") || row.contains("继续"),
        "主按钮命中行应包含按钮文案: {row:?}"
    );

    let dicts = targets
        .iter()
        .find(|target| target.action == Action::Navigate(app::Screen::DictPicker))
        .expect("dict nav target");
    let row = lines.get(dicts.rect.y as usize).copied().unwrap_or("");
    assert!(row.contains("词库"), "词库入口命中行应有文案: {row:?}");
}
#[test]
fn help_scrolls_at_min_size() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    let first = render_text(&app, 80, 24);
    assert!(first.contains("练习"));
    app.help_scroll = 20;
    let scrolled = render_text(&app, 80, 24);
    assert!(scrolled.contains("Shift+拖拽") || scrolled.contains("Shift"));
    assert!(scrolled.contains("导航/查看"));
    assert!(scrolled.contains("↓") || scrolled.contains("↑"));
}

/// 词条编辑弹层打开时，鼠标点击不能穿透到下层页面。
///
/// `hit.rs` 三处弹层判定都漏了 `word_edit`，点击会落到词条列表和侧栏上，
/// 把弹层的编辑对象与下层光标搅乱。
#[test]
fn the_word_edit_overlay_blocks_mouse_clicks() {
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
    assert_eq!(app.screen, app::Screen::Words);

    app.open_word_edit(Some(0));
    assert!(app.word_edit.is_some());

    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    assert!(
        !targets
            .iter()
            .any(|target| matches!(target.action, Action::WordsMove(_))),
        "弹层打开时不应有词条列表的命中目标"
    );
    assert!(
        !targets
            .iter()
            .any(|target| matches!(target.action, Action::Navigate(_))),
        "弹层打开时不应有侧栏导航的命中目标"
    );
}

/// 模式选择弹层的命中区域必须落在它实际渲染的位置上。
///
/// 早期实现用整屏 `area` 的坐标构造命中矩形，与居中弹层的实际位置完全错位：
/// 点真正显示模式名的那一行没反应，点屏幕左上角反而会误触发。
#[test]
fn mode_picker_targets_match_the_rendered_rows() {
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.open_mode_picker();
    let area = Rect::new(0, 0, 120, 40);
    let text = render_text(&app, 120, 40);
    let lines: Vec<&str> = text.lines().collect();

    let targets = typewords_tui::ui::hit::targets(&app, area);
    assert!(!targets.is_empty(), "模式弹层应有命中目标");
    for target in &targets {
        assert!(
            matches!(target.action, Action::ModeMove(_)),
            "模式弹层的目标应是 ModeMove，实际 {:?}",
            target.action
        );
        let row = lines.get(target.rect.y as usize).copied().unwrap_or("");
        // 命中行必须真的显示着模式名，而不是弹层背景或空行。
        assert!(
            row.contains("学习")
                || row.contains("自由练习")
                || row.contains("自测")
                || row.contains("默写")
                || row.contains("听写")
                || row.contains("随机复习")
                || row.contains("复习")
                || row.contains("单词测试")
                || row.contains("自定义流程"),
            "命中行 {} 应显示模式名: {row:?}",
            target.rect.y
        );
    }
}
