//! TestBackend 渲染快照（只断言关键文本与不 panic）。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;
use typewords_tui::app;
use unicode_width::UnicodeWidthStr;

fn render_text(app: &app::App, width: u16, height: u16) -> String {
    render_rows(app, width, height)
        .into_iter()
        .map(|(text, _)| text + "\n")
        .collect()
}

/// 渲染每一行：文本 + 该行字符格的前景色（用来断言某一行用了哪个主题色）。
fn render_rows(app: &app::App, width: u16, height: u16) -> Vec<(String, Vec<Color>)> {
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

/// 取包含 `needle` 的那一行：文本 + 该行用到的前景色。
fn rendered_row(app: &app::App, width: u16, height: u16, needle: &str) -> (String, Vec<Color>) {
    render_rows(app, width, height)
        .into_iter()
        .find(|(text, _)| text.contains(needle))
        .unwrap_or_else(|| panic!("没有任何一行包含 {needle:?}"))
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

/// 设置项必须按分区归位：主题 / 音标 / 大字单词属于「通用」，
/// 键音 / 发音属于「音频」。
///
/// 早期实现把 `SETTING_ROWS` 的顺序和 `SECTIONS` 的分区起点对不上，
/// 「主题」「音标」「大字单词」被划进了「音频」分区。
#[test]
fn settings_rows_follow_their_documented_sections() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let text = render_text(&app, 100, 40);
    let lines: Vec<&str> = text.lines().collect();

    // 找到每个分区标题所在行
    let section_row = |title: &str| lines.iter().position(|line| line.contains(title));

    let general = section_row("通用").expect("应有通用分区");
    let audio = section_row("音频").expect("应有音频分区");
    let review = section_row("复习").expect("应有复习分区");

    let row_of = |label: &str| {
        lines
            .iter()
            .position(|line| line.contains(label))
            .unwrap_or_else(|| panic!("设置页应有「{label}」行"))
    };

    assert!(
        general < row_of("主题") && row_of("主题") < audio,
        "主题应在通用分区"
    );
    assert!(
        general < row_of("音标") && row_of("音标") < audio,
        "音标应在通用分区"
    );
    assert!(
        general < row_of("大字单词") && row_of("大字单词") < audio,
        "大字单词应在通用分区"
    );
    assert!(
        audio < row_of("键音") && row_of("键音") < review,
        "键音应在音频分区"
    );
    assert!(
        audio < row_of("单词发音") && row_of("单词发音") < review,
        "单词发音应在音频分区"
    );
    assert!(
        audio < row_of("文章朗读")
            && row_of("单词发音") < row_of("文章朗读")
            && row_of("文章朗读") < review,
        "文章朗读应在音频分区，且排在单词发音之后"
    );
}

/// 两个破坏性动作的右侧提示必须各自成立：「清除所有数据」不可撤销，
/// 不能和「清空当前练习进度」共用一句「按 Enter 重置」被弱化成普通重置。
///
/// 早期实现 `setting_value` 对 `SettingKind::Action(_)` 用统一文案，
/// 用户从行内看不出两者严重性不同。
#[test]
fn destructive_action_rows_hint_their_own_consequence() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let theme = app.theme();
    // 一屏放下全部 39 行，两个动作项才都在画面里。
    let (reset_text, reset_colors) = rendered_row(&app, 100, 60, "清空当前练习进度");
    let (clear_text, clear_colors) = rendered_row(&app, 100, 60, "清除所有数据");

    assert!(
        reset_text.contains("按 Enter 重置"),
        "清空练习进度只重置断点缓存，仍提示重置：{reset_text}"
    );
    assert!(
        !reset_colors.contains(&theme.bad),
        "普通重置不该用危险色：{reset_text}"
    );

    assert!(
        clear_text.contains("按 Enter 清除"),
        "清除所有数据应提示「清除」而不是「重置」：{clear_text}"
    );
    assert!(
        !clear_text.contains("重置"),
        "不可撤销的清除不能写成重置：{clear_text}"
    );
    assert!(
        clear_text.contains("不可撤销"),
        "行内要点明该动作不可撤销：{clear_text}"
    );
    assert!(
        clear_colors.contains(&theme.bad),
        "清除所有数据是危险动作，提示应用 bad 色：{clear_text}"
    );

    // 最小终端（60×20）里滚到最后一屏：提示要整句显示，不能被截成半句。
    let last = app::SETTING_ROWS.len() - 1;
    while app.settings_cursor < last {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    let (narrow_text, _) = rendered_row(&app, 60, 20, "清除所有数据");
    assert!(
        narrow_text.contains("按 Enter 清除 · 不可撤销"),
        "60 列下提示被截断了：{narrow_text}"
    );
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
        "应有 accent_alt 的次强调数字"
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
    for theme_name in ["dark", "light", "high-contrast", "midnight", "paper", "ikb"] {
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

    app.confirm = Some(app::ConfirmState::new(
        "退出测试？",
        app::ConfirmAction::Quit,
    ));
    let text = render_text(&app, 100, 30);
    assert!(text.contains("确认"));
}

/// 文章跟打的结算与单词会话共用弹层，但文案、指标与按钮都是文章口径。
#[test]
fn article_summary_renders_article_labels() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.summary = Some(app::SummaryData {
        kind: app::SummaryKind::Article {
            article_id: 11,
            title: "第一篇".to_string(),
        },
        spend_ms: 95_000,
        accuracy: 80,
        wrong: 1,
        last_learn_index: 5,
        length: 5,
        complete: true,
        ..Default::default()
    });
    let text = render_text(&app, 100, 30);
    assert!(text.contains("文章跟打完成"), "文章结算标题不对: {text}");
    assert!(text.contains("第一篇"), "应显示文章标题: {text}");
    assert!(text.contains("完成 5 / 5 句"), "应显示句子进度: {text}");
    assert!(text.contains("错句 1"), "应显示错句数: {text}");
    assert!(text.contains("返回列表"), "主按钮应回文章列表: {text}");
    assert!(text.contains("重打本篇"), "重学按钮应重打本篇: {text}");
    assert!(
        !text.contains("今日任务完成"),
        "文章结算不该套用单词会话标题: {text}"
    );
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

/// 帮助页覆盖上游的 FAQ / 关于 / 反馈三个页面。
///
/// 分页标签常驻；FAQ 分页回答终端形态下的常见问题，关于与反馈分页给出
/// 项目 / 上游仓库、许可证、当前数据目录与提 issue 需要附带的诊断信息。
#[test]
fn help_tabs_cover_faq_and_about_feedback() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));

    let shortcuts = render_text(&app, 120, 40);
    for tab in ["快捷键", "常见问题", "关于与反馈"] {
        assert!(shortcuts.contains(tab), "分页条应列出「{tab}」");
    }
    assert!(shortcuts.contains("Ctrl+C"), "快捷键分页应列出练习页键位");

    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let faq = render_text(&app, 120, 40);
    assert!(
        faq.contains("数据保存在哪里"),
        "FAQ 应回答数据保存位置: {faq}"
    );
    assert!(faq.contains("会联网吗"), "FAQ 应回答联网问题");
    assert!(!faq.contains("Ctrl+I"), "FAQ 分页不该还渲染快捷键表: {faq}");

    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    // 「关于」分页显示的是 App::paths 里的真实数据目录；换一个短而唯一的
    // 路径，既避免临时路径折行，也能识别「固定显示默认路径 / 回退文案」。
    if let Some(paths) = app.paths.as_mut() {
        paths.home = std::path::PathBuf::from("D:/tw-about-home");
    }
    let about = render_text(&app, 120, 40);
    assert!(
        about.contains("https://github.com/StarTurambar/typewords-tui"),
        "关于分页应给出项目仓库地址: {about}"
    );
    assert!(
        about.contains(&format!("Type Words TUI v{}", env!("CARGO_PKG_VERSION"))),
        "关于分页应显示版本号（侧栏的 v 号不算）: {about}"
    );
    assert!(
        about.contains("数据目录：D:/tw-about-home"),
        "关于分页应显示 App::paths 里的真实数据目录: {about}"
    );
    assert!(
        !about.contains("未解析"),
        "关于分页不该回退到「数据目录未解析」: {about}"
    );
    assert!(
        about.contains("tw doctor"),
        "反馈分页应说明提 issue 要附的诊断信息"
    );
    assert!(about.contains("上游"), "关于分页应说明与上游项目的关系");
}

/// 帮助页分页标签的命中矩形必须与绘制同源：画在哪就能点在哪。
#[test]
fn help_tab_targets_match_rendered_labels() {
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();

    for tab in app::HelpTab::ALL {
        let target = targets
            .iter()
            .find(|target| target.action == Action::HelpSelectTab(tab))
            .unwrap_or_else(|| panic!("{tab:?} 应有命中区"));
        // 标签文字从 rect.x + 1 开始（两侧各留 1 列）。
        let first = tab.label().chars().next().expect("label");
        let symbol = buffer
            .cell((target.rect.x + 1, target.rect.y))
            .map(|cell| cell.symbol())
            .unwrap_or("");
        assert!(
            symbol.starts_with(first),
            "{tab:?} 的命中区与绘制文字漂移: rect={:?}, cell={symbol:?}",
            target.rect
        );
    }
}

/// FAQ 是折行后的长文：滚动上限必须按屏幕行算，最后一条答案要能滚出来。
#[test]
fn faq_scrolls_to_the_last_answer() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    // 80×24 单栏：FAQ 折行后远超一屏；PgDn 连按到上限。
    for _ in 0..12 {
        app.on_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    }
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    assert!(
        !typewords_tui::ui::hit::targets(&app, area)
            .iter()
            .any(|target| target.action == app::Action::HelpScroll(1)),
        "滚到底后不应再注册 ↓ 命中区"
    );
    let text = render_text(&app, 80, 24);
    assert!(
        text.contains("手机 / 平板能用吗"),
        "FAQ 最后一条问题应能滚到: {text}"
    );
    assert!(
        text.contains("触屏没有对应的输入方式"),
        "FAQ 最后一条答案应能滚到: {text}"
    );
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
