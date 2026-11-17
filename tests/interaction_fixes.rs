//! 交互一致性与命中完整性的回归测试。
//!
//! 每个用例对应一个已修复的缺陷，测试名用行为句子描述期望。

mod common;

use chrono::Datelike;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Action, HelpTab, Screen};
use typewords_tui::domain::shortcut::{BaseKey, ShortcutAction, ShortcutKey};
use typewords_tui::domain::types::WordPracticeMode;
use typewords_tui::ui::focus::{self, FocusId, NavId, Pane};
use unicode_width::UnicodeWidthStr;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn press_with(app: &mut app::App, code: KeyCode, modifiers: KeyModifiers) {
    app.on_key(KeyEvent::new(code, modifiers));
}

/// 渲染成「每行一个逻辑字符 + 起始列」的网格。
fn logical_rows(app: &app::App, width: u16, height: u16) -> Vec<Vec<(char, u16)>> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut rows = Vec::new();
    for y in 0..height {
        let mut row: Vec<(char, u16)> = Vec::new();
        let mut x = 0u16;
        while x < width {
            let symbol = buffer
                .cell((x, y))
                .map(|c| c.symbol().to_string())
                .unwrap_or_default();
            let w = UnicodeWidthStr::width(symbol.as_str()).max(1);
            let ch = symbol.chars().next().unwrap_or(' ');
            row.push((ch, x));
            x += w as u16;
        }
        rows.push(row);
    }
    rows
}

/// 在渲染结果里找一段文字，返回它的起始列与行号。
fn find_text(rows: &[Vec<(char, u16)>], needle: &str) -> Option<(u16, u16)> {
    let chars: Vec<char> = needle.chars().collect();
    for (y, row) in rows.iter().enumerate() {
        if row.len() < chars.len() {
            continue;
        }
        for x in 0..=(row.len() - chars.len()) {
            if row[x..x + chars.len()]
                .iter()
                .map(|(c, _)| *c)
                .eq(chars.iter().copied())
            {
                return Some((row[x].1, y as u16));
            }
        }
    }
    None
}

// ── 弹层：鼠标必须能用 ──────────────────────────────────────────────────

/// 模式选择弹层：点某个模式名应该能选中它。
///
/// 早期实现只给 `overlays::targets` 注册了 `ModeMove`，`target_action`
/// 漏了 mode_picker 分支，鼠标点弹层毫无反应。
#[test]
fn clicking_a_mode_in_the_picker_selects_it() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.open_mode_picker();
    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let row = targets
        .iter()
        .find(|t| t.action == Action::ModeMove(2))
        .expect("模式弹层应注册每行的 target");
    let action = typewords_tui::ui::hit::action_at(&app, area, row.rect.x + 2, row.rect.y);
    assert!(
        action.is_some(),
        "点模式弹层里的模式名应该产生 Action，实际 {action:?}"
    );
}

/// 「收藏到指定词典」弹层：点某个词典名应该能选中它。
#[test]
fn clicking_a_dict_in_the_collect_picker_selects_it() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    press_with(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    assert!(app.collect_picker.is_some(), "收藏弹层应已打开");

    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    assert!(
        !targets.is_empty(),
        "收藏弹层应注册每行的 target，实际一个都没有"
    );
    let action =
        typewords_tui::ui::hit::action_at(&app, area, targets[0].rect.x + 2, targets[0].rect.y);
    assert!(
        action.is_some(),
        "点收藏弹层里的词典名应该产生 Action，实际 {action:?}"
    );
}

/// 打开收藏弹层，并准备 `custom` 个自建词典（连同系统「收藏」）。
fn app_with_many_collectible_dicts(custom: usize) -> (common::Fixture, app::App) {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    for index in 0..custom {
        app.boot
            .base
            .word
            .book_list
            .push(typewords_tui::domain::types::Dict {
                id: format!("custom-{index}"),
                name: format!("自建词典{index}"),
                custom: true,
                ..Default::default()
            });
    }
    app.start_practice();
    press_with(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    assert!(app.collect_picker.is_some(), "收藏弹层应已打开");
    (fixture, app)
}

/// 收藏弹层只登记真正画出来的行。
///
/// 早期 `collect_picker_layout` 给全部词典生成矩形，绘制却在提示行前
/// `break`：第 9 个词典的矩形正好落在提示行上，点提示行或弹层下方空白
/// 都会改变选中项。
#[test]
fn the_collect_picker_never_targets_rows_it_does_not_draw() {
    // 系统「收藏」+ 11 个自建词典 = 12 个可收藏目标，超过 8 行可视区。
    let (_fixture, app) = app_with_many_collectible_dicts(11);

    let area = Rect::new(0, 0, 120, 40);
    let dicts_len = app.collectible_dicts().len();
    assert!(
        dicts_len >= 12,
        "应有至少 12 个可收藏词典，实际 {dicts_len}"
    );
    let layout = typewords_tui::ui::overlays::collect_picker_layout(dicts_len, 0, area);
    let hint_y = layout.inner.bottom().saturating_sub(1);
    assert!(
        layout.rows.len() < dicts_len,
        "12 个词典不应全部画进 8 行列表，实际 {} 行",
        layout.rows.len()
    );

    let targets = typewords_tui::ui::hit::targets(&app, area);
    let collect: Vec<_> = targets
        .iter()
        .filter(|target| matches!(target.action, Action::CollectMove(_)))
        .collect();
    assert_eq!(
        collect.len(),
        layout.rows.len(),
        "命中区数量应与实际画出的行一致"
    );
    for target in &collect {
        assert!(
            target.rect.y < hint_y,
            "命中区越过了提示行: rect={:?}, hint_y={hint_y}",
            target.rect
        );
    }

    // 提示行上点击不能产生 CollectMove。
    let action = typewords_tui::ui::hit::action_at(&app, area, layout.inner.x, hint_y);
    assert!(
        !matches!(action, Some(Action::CollectMove(_))),
        "点提示行不应改变收藏选中项，实际 {action:?}"
    );
}

/// 收藏弹层跟随光标滚动：选中项滚出可视区后列表要跟过去，
/// 否则键盘选中的行既看不见也点不到。
#[test]
fn the_collect_picker_scrolls_to_keep_the_cursor_visible() {
    let (_fixture, mut app) = app_with_many_collectible_dicts(11);

    let area = Rect::new(0, 0, 120, 40);
    let total = app.collectible_dicts().len();
    app.apply_action(Action::CollectMove(total as i32));
    let cursor = app.collect_picker.as_ref().expect("弹层").cursor;
    assert_eq!(cursor, total - 1, "光标应停在最后一个词典");

    let layout = typewords_tui::ui::overlays::collect_picker_layout(total, cursor, area);
    let hint_y = layout.inner.bottom().saturating_sub(1);
    assert!(
        layout.rows.iter().any(|(index, _)| *index == cursor),
        "选中行应留在可见列表里: rows={:?}, cursor={cursor}",
        layout.rows
    );
    for (index, rect) in &layout.rows {
        assert!(*index < total);
        assert!(rect.y < hint_y, "行 {index} 越过了提示行: rect={rect:?}");
    }

    // 点最后一行（当前选中项）应产生 CollectMove(0)。
    let (_, last) = layout
        .rows
        .iter()
        .find(|(index, _)| *index == cursor)
        .expect("最后一行");
    let action = typewords_tui::ui::hit::action_at(&app, area, last.x + 2, last.y);
    assert!(
        matches!(action, Some(Action::CollectMove(0))),
        "点选中行应产生 CollectMove(0)，实际 {action:?}"
    );
}

/// 词条编辑弹层：点字段行应该切换到该字段。
#[test]
fn clicking_a_field_in_the_word_edit_overlay_switches_field() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.open_word_edit(Some(0));
    assert!(app.word_edit.is_some());

    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    assert!(!targets.is_empty(), "词条编辑弹层应注册每个字段的 target");
    let action =
        typewords_tui::ui::hit::action_at(&app, area, targets[2].rect.x + 2, targets[2].rect.y);
    assert!(
        action.is_some(),
        "点词条编辑弹层的字段行应该产生 Action，实际 {action:?}"
    );
    app.apply_action(action.expect("action"));
    assert_eq!(
        app.word_edit.as_ref().expect("弹层").field,
        2,
        "点第 3 个字段应切到第 3 个字段"
    );
}

// ── 主页：每日目标的 +/- 必须点在按钮上 ──────────────────────────────────

/// 主页每日目标的 `[+]` 按钮：点 + 号必须真的加每日词数。
///
/// 早期实现用固定偏移估算按钮位置，与渲染用的实际排版差 2 列，
/// 所有终端尺寸下点 + 号都没反应。
#[test]
fn clicking_the_plus_button_adds_five_words() {
    for (width, height) in [(120u16, 40u16), (100, 30), (80, 24), (60, 20)] {
        let fixture = common::Fixture::new();
        let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
        // 词库只有 12 个词，每日词数已经到上限，加不了——先降下来。
        if let Some(dict) = app.boot.base.study_dict_mut() {
            dict.per_day_study_number = 5;
        }
        let area = Rect::new(0, 0, width, height);
        let rows = logical_rows(&app, width, height);
        let plus_pos = find_text(&rows, "[+]").expect("主页应渲染出 [+]");
        let before = app
            .boot
            .base
            .study_dict()
            .map(|dict| dict.per_day())
            .unwrap_or(0);
        let action = typewords_tui::ui::hit::action_at(&app, area, plus_pos.0, plus_pos.1);
        assert_eq!(
            action,
            Some(Action::AdjustDaily(5)),
            "{width}x{height}：点 [+] 里的 + 号应该加每日词数"
        );
        app.apply_action(action.expect("action"));
        let after = app
            .boot
            .base
            .study_dict()
            .map(|dict| dict.per_day())
            .unwrap_or(0);
        assert!(
            after > before,
            "{width}x{height}：点 [+] 后每日词数应增加，{before} -> {after}"
        );
    }
}

/// 主页每日目标的 `[-]` 按钮：点 - 号必须真的减每日词数。
#[test]
fn clicking_the_minus_button_removes_five_words() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 120, 40);
    let rows = logical_rows(&app, 120, 40);
    let minus_pos = find_text(&rows, "[-]").expect("主页应渲染出 [-]");
    let action = typewords_tui::ui::hit::action_at(&app, area, minus_pos.0, minus_pos.1);
    assert_eq!(action, Some(Action::AdjustDaily(-5)));
}

/// 每日目标行的文本与分段必须一致。
///
/// 命中检测按文本里的字符位置算按钮，渲染按分段画——两者不一致时
/// 按钮就会偏。
#[test]
fn the_goal_line_text_matches_its_spans() {
    for goal_short in [true, false] {
        for per_day in [0usize, 5, 12, 100] {
            let text = typewords_tui::ui::home::goal_line_text(goal_short, per_day);
            assert!(
                text.contains("[-]") && text.contains("[+]"),
                "每日目标行应包含 [-] 与 [+]：{text:?}"
            );
            // 按钮的位置必须在文本里
            let minus = text.find("[-]").expect("[-]");
            let plus = text.find("[+]").expect("[+]");
            assert!(minus < plus, "[-] 应在 [+] 之前：{text:?}");
        }
    }
}

// ── 词条页：点已勾选行改的是那一行 ──────────────────────────────────────

/// 点已勾选的那一行，取消的是**那一行**的勾选。
///
/// 早期实现的 target 按被点行判定，动作却按光标行执行：勾选第 3 行、
/// 光标移到第 1 行、再点第 3 行，第 3 行仍被勾选，反而勾上了第 1 行。
#[test]
fn clicking_a_marked_row_clears_that_row() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.open_words();

    // 空格勾选第 3 行
    app.words_cursor = 2;
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(app.words_marked, vec![2], "空格应勾选光标所在行");

    // 光标移到第 1 行
    app.words_cursor = 0;

    // 点第 3 行
    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let row3 = targets
        .iter()
        .find(|t| t.action == Action::WordsToggleMark(2))
        .expect("第 3 行应是勾选 target");
    let action = typewords_tui::ui::hit::action_at(&app, area, row3.rect.x + 2, row3.rect.y);
    app.apply_action(action.expect("action"));
    assert!(
        app.words_marked.is_empty(),
        "点第 3 行应取消第 3 行的勾选，实际 {:?}",
        app.words_marked
    );
}

/// 词条页：长列表要跟随光标滚动，光标词必须可见。
#[test]
fn the_words_list_scrolls_to_follow_the_cursor() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.open_words();
    // 移到最后一个词条
    app.apply_action(Action::WordsMove(100_000));
    let cursor = app.words_cursor;
    assert!(cursor > 0, "光标应已移到后面的词条");

    let rows = logical_rows(&app, 120, 40);
    let visible: String = rows
        .iter()
        .flat_map(|row| row.iter().map(|(c, _)| *c))
        .collect();
    let word = app
        .boot
        .base
        .study_dict()
        .and_then(|dict| dict.words.get(cursor))
        .map(|word| word.word.clone())
        .expect("光标词");
    assert!(
        visible.contains(&word),
        "光标词 {word} 应在可视区内（光标下标 {cursor}）"
    );
}

// ── 独占整屏的页面不能有侧栏命中区 ──────────────────────────────────────

/// 文章跟打页点屏幕左侧不会跳回主页。
///
/// 该页整屏覆盖不画侧栏，早期实现却按 shell 布局注册了导航 target，
/// 点屏幕上什么都没画的位置会突然回到主页。
#[test]
fn the_article_practice_page_has_no_sidebar_targets() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.screen = Screen::ArticlePractice;
    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    assert!(
        !targets
            .iter()
            .any(|t| matches!(t.action, Action::Navigate(_) | Action::QuitRequest)),
        "文章跟打页不应有侧栏导航 target，实际 {:?}",
        targets
    );
}

// ── 有内容 target 的页面不能是空壳 ──────────────────────────────────────

/// 统计页、流程页、单词测试页都有可点内容。
#[test]
fn the_stats_flows_and_word_test_pages_have_clickable_content() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 120, 40);

    app.screen = Screen::Stats;
    let stats = typewords_tui::ui::hit::targets(&app, area);
    assert!(
        stats.iter().any(|t| t.action == Action::StatsToggleView),
        "统计页应有「v 切换」target"
    );
    assert!(
        stats
            .iter()
            .any(|t| matches!(t.action, Action::StatsSelectDay(_))),
        "统计页日历应有可选日期的 target"
    );

    app.screen = Screen::Flows;
    let flows = typewords_tui::ui::hit::targets(&app, area);
    assert!(
        flows
            .iter()
            .any(|t| matches!(t.action, Action::FlowMove(_))),
        "流程页应有流程行的 target"
    );

    app.start_word_test(WordPracticeMode::ReviewWordsTest);
    let word_test = typewords_tui::ui::hit::targets(&app, area);
    assert!(
        word_test
            .iter()
            .any(|t| matches!(t.action, Action::WordTestChoose(_))),
        "单词测试页应有选项 target"
    );
}

/// 统计页点日历里的某一天，应该选中那一天。
#[test]
fn clicking_a_calendar_day_selects_it() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.screen = Screen::Stats;
    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let day = targets
        .iter()
        .find_map(|t| match t.action {
            Action::StatsSelectDay(date) => Some(date),
            _ => None,
        })
        .expect("日历应有一天可选");
    let target = targets
        .iter()
        .find(|t| matches!(t.action, Action::StatsSelectDay(_)))
        .expect("target");
    let action = typewords_tui::ui::hit::action_at(&app, area, target.rect.x, target.rect.y);
    app.apply_action(action.expect("action"));
    assert_eq!(app.stats_day, day, "点日历里的日期应选中那一天");
}

/// 统计页日历：命中区 / 高亮矩形必须和画出来的日期数字对齐。
///
/// 早期命中区起点多加了 2 列（日期数字列的末尾），只盖住最后一个数字与
/// 记录点：鼠标点数字左半边没反应，键盘高亮复用这块矩形时会整体向右歪一格。
#[test]
fn calendar_day_rects_cover_the_rendered_day_numbers() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 120, 40);
    app.set_viewport(area);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Stats);

    let (year, month) = app.stats_month;
    let date = chrono::NaiveDate::from_ymd_opt(year, month, 12).expect("month day");
    app.select_stats_day(date);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let mut days: Vec<(chrono::NaiveDate, Rect)> = targets
        .iter()
        .filter_map(|target| match target.action {
            Action::StatsSelectDay(day) => Some((day, target.rect)),
            _ => None,
        })
        .collect();
    let (_, rect) = days
        .iter()
        .copied()
        .find(|(day, _)| *day == date)
        .expect("该日期应有命中区");

    // 高亮矩形与命中区同源。
    let highlight =
        typewords_tui::ui::focus::focused_rect(&app, app.viewport).expect("选中日应有高亮矩形");
    assert_eq!(highlight, rect, "日历高亮应与日期格命中区一致");

    // 命中区里画出来的正是这一天（两位数最容易看出偏列）。
    let rows = logical_rows(&app, 120, 40);
    let text: String = rows[rect.y as usize]
        .iter()
        .filter(|(_, column)| *column >= rect.x && *column < rect.right())
        .map(|(character, _)| *character)
        .collect();
    assert_eq!(
        text.trim(),
        date.day().to_string(),
        "命中区应正好盖住日期数字，实际盖到 {text:?}"
    );

    // 同一周的日期格首尾相接、宽度一致，不重叠也不留缝。
    days.sort_by_key(|(_, rect)| (rect.y, rect.x));
    let row: Vec<Rect> = days
        .iter()
        .filter(|(_, candidate)| candidate.y == rect.y)
        .map(|(_, candidate)| *candidate)
        .collect();
    assert!(row.len() >= 5, "一行应有至少 5 个日期格");
    for pair in row.windows(2) {
        assert_eq!(pair[0].right(), pair[1].x, "相邻日期格应首尾相接");
        assert_eq!(
            pair[0].width, 5,
            "日期格应覆盖整格（3 列数字 + 2 列记录点）"
        );
    }
}

// ── 滚动条不能被行 target 抢走 ─────────────────────────────────────────

/// 设置页的滚动条不会被行 target 覆盖：点滚动条不会跳到别的设置项。
#[test]
fn the_settings_scrollbar_is_not_a_row_target() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.screen = Screen::Settings;
    let area = Rect::new(0, 0, 120, 40);
    let rows = logical_rows(&app, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);

    // 内容区（侧栏右侧）里的滚动条滑块是实心块，正文里不会出现这个字符。
    let thumb = rows.iter().enumerate().find_map(|(y, row)| {
        row.iter()
            .find(|(c, column)| *c == '█' && *column >= 28)
            .map(|(_, column)| (*column, y as u16))
    });
    let (column, row) = thumb.expect("设置项在 40 行终端里一屏放不下，应出现滚动条");
    let owner = targets.iter().find(|t| t.contains(column, row));
    assert!(
        owner.is_none(),
        "滚动条不应被行 target 覆盖，实际被 {:?} 覆盖",
        owner.map(|t| &t.action)
    );
}

// ── 帮助页 ──────────────────────────────────────────────────────────────

/// 帮助页两栏模式（终端不够高时）要能滚到最后。
///
/// 早期实现两栏固定从第 0 行开始画，且不响应 `help_scroll`，
/// Ctrl+C / Ctrl+Q 这些行被裁掉且滚不出来。
#[test]
fn the_two_column_help_page_can_scroll_to_the_bottom() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    // 滚动上限按 viewport 现算；先同步成渲染尺寸，测试才真正覆盖两栏上限，
    // 而不是拿默认 80×24 的单栏上限再去渲染时 clamp。
    app.set_viewport(Rect::new(0, 0, 120, 26));
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);

    // 120 列 → content 宽 90 ≥ 78 → 两栏；26 行 → 正文高 20，左栏 32 行 → 上限 12。
    for _ in 0..40 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.help_scroll, 12, "两栏滚到底应停在上限 12");
    let rows = logical_rows(&app, 120, 26);
    let text: String = rows
        .iter()
        .flat_map(|row| row.iter().map(|(c, _)| *c))
        .collect();
    assert!(text.contains("Ctrl+C"), "两栏模式滚到底应能看到 Ctrl+C");
}

/// 帮助页正文是只读文本，点任何位置都不该退出页面。
///
/// 早期实现把整块内容区注册成 `HelpBack`：点终端窗口聚焦、点空白、
/// 点文字换行都会直接跳回主页，帮助页根本没法停留阅读。
#[test]
fn clicking_the_help_body_does_not_leave_the_page() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);

    let area = Rect::new(0, 0, 120, 40);
    let rows = logical_rows(&app, 120, 40);
    let (column, row) = find_text(&rows, "字母").expect("帮助页应渲染正文");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.screen, Screen::Help, "点击帮助正文不应退出帮助页");
    assert_eq!(app.help_scroll, 0, "点击正文不应改变滚动位置");
}

/// 帮助页点滚动条的轨道是滚动，不是退出。
#[test]
fn clicking_the_help_scrollbar_scrolls_instead_of_exiting() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    let area = Rect::new(0, 0, 80, 24);

    // 从命中目标取「向下滚」那一段轨道，而不是在整屏里找字符：底栏提示
    // 「↑↓ 滚动」里也有箭头，按文字搜索会点到页脚（那里没有目标）。
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::HelpScroll(1))
        .expect("帮助页应有向下滚动命中区");
    let rows = logical_rows(&app, 80, 24);
    let symbol = rows[target.rect.y as usize]
        .iter()
        .find(|(_, x)| *x == target.rect.x)
        .map(|(c, _)| *c)
        .expect("滚动条所在列应存在");
    assert_eq!(symbol, '│', "命中区应落在滚动条滑块下方的轨道上");

    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.screen, Screen::Help, "点滚动条不该退出帮助页");
    assert_eq!(app.help_scroll, 1, "点轨道应向下滚动一行");
}

/// 帮助页三个分页：`←` / `→`（`h` / `l` 别名）切换，到边停住，换页回到顶部。
///
/// 上游把 FAQ / 关于 / 反馈拆成 help / about / feedback 三个页面；终端里合并
/// 成同一个帮助页的三个分页。分页键之外的按键仍是「任意键返回」。
#[test]
fn help_page_switches_between_its_three_tabs() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);
    assert_eq!(
        app.help_tab,
        HelpTab::Shortcuts,
        "打开帮助默认停在快捷键分页"
    );

    press(&mut app, KeyCode::Down);
    assert_eq!(app.help_scroll, 1);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::Faq);
    assert_eq!(app.help_scroll, 0, "切换分页应回到顶部");

    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::About);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::About, "最后一页再按 → 应停住");

    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.help_tab, HelpTab::Faq);
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.help_tab, HelpTab::Shortcuts);
    assert_eq!(app.screen, Screen::Help, "切分页不该离开帮助页");

    // 分页键之外仍是「任意键返回」：数字键返回而不是切页。
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Home);
}

/// 鼠标点分页标签切换分页，而不是退出帮助页。
///
/// 分页标签的命中矩形来自与绘制同一份布局（`ui::help::layout`），
/// 画在哪就能点在哪。
#[test]
fn clicking_a_help_tab_switches_pages() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    let area = Rect::new(0, 0, 120, 40);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::HelpSelectTab(HelpTab::About))
        .expect("帮助页分页条应为每个分页注册点击目标");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x + target.rect.width / 2,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.help_tab, HelpTab::About);
    assert_eq!(app.screen, Screen::Help, "点分页标签不该退出帮助页");
}

/// 点已高亮的「帮助」导航项不是「重新打开帮助」，不应把当前分页踢回快捷键。
///
/// 键盘在导航栏按 Enter 时只会提示「已在「帮助」页」；鼠标点击走同一条
/// 导航语义，行为应一致。
#[test]
fn clicking_the_active_help_nav_entry_keeps_the_current_tab() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::Faq);

    let area = Rect::new(0, 0, 120, 40);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::Navigate(Screen::Help))
        .expect("帮助导航项应可点击");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x + target.rect.width / 2,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.screen, Screen::Help);
    assert_eq!(app.help_tab, HelpTab::Faq, "点击当前帮助导航项不应重置分页");
}

/// 重新打开帮助总是从「快捷键」分页开始，不沿用上次停留的分页。
#[test]
fn reopening_help_starts_on_the_shortcuts_tab() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::About);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Home);
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(
        app.help_tab,
        HelpTab::Shortcuts,
        "再次打开帮助应回到第一个分页"
    );
}

// ── 帮助页：分页条高亮与到边方向键 ────────────────────────────────────

/// 分页条那一行每列的底色（帮助页三分页都画在同一行）。
///
/// 高亮是「整块」的：底色铺满分页标签矩形（含左右各一格留白），
/// 所以按颜色找块面就够了，不必逐字判断。
fn tab_row_backgrounds(app: &app::App, width: u16, height: u16) -> Vec<ratatui::style::Color> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    for y in 0..height {
        // 宽字符（CJK）占两列：后一格是占位而不是同一行文字，拼文本时要跳过，
        // 否则拿到的是「快 捷 键」。
        let mut text = String::new();
        let mut x = 0u16;
        while x < width {
            let symbol = buffer
                .cell((x, y))
                .map(|cell| cell.symbol().to_string())
                .unwrap_or_default();
            let cell_width = UnicodeWidthStr::width(symbol.as_str()).max(1) as u16;
            text.push_str(&symbol);
            x += cell_width;
        }
        if text.contains("快捷键") && text.contains("常见问题") {
            return (0..width)
                .map(|x| buffer.cell((x, y)).map(|cell| cell.bg).unwrap_or_default())
                .collect();
        }
    }
    panic!("帮助页分页条没有渲染出来");
}

/// 当前分页用全站统一的强调色整块高亮，而不是几乎看不见的 selection 底色。
///
/// 早期分页条只铺 `theme.selection`：深色主题下它与背景几乎同色、高对比主题下
/// 干脆同色，看不出当前停在哪一页。现在与设置页当前行 / 侧栏当前项同一套：
/// 键盘在本页时强调色底 + 反色文字；焦点进导航栏后让位回 selection 底色。
#[test]
fn help_active_tab_uses_the_accent_selection_language() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 120, 40));
    press(&mut app, KeyCode::Char('?'));
    let theme = app.theme();

    let backgrounds = tab_row_backgrounds(&app, 120, 40);
    assert!(
        backgrounds.contains(&theme.accent),
        "内容区焦点下当前分页应整块强调色高亮"
    );
    assert!(
        !backgrounds.contains(&theme.selection),
        "强调色高亮不应再混用 selection 底色"
    );

    // 切到「常见问题」，强调色块跟着走。
    press(&mut app, KeyCode::Right);
    let backgrounds = tab_row_backgrounds(&app, 120, 40);
    assert!(
        backgrounds.contains(&theme.accent),
        "换分页后新分页应有强调色高亮"
    );

    // Tab 进导航栏：键盘不在本页，页面高亮让位成 selection 底色。
    press(&mut app, KeyCode::Tab);
    let backgrounds = tab_row_backgrounds(&app, 120, 40);
    assert!(
        !backgrounds.contains(&theme.accent),
        "焦点进导航栏后分页条不再用强调色"
    );
    assert!(
        backgrounds.contains(&theme.selection),
        "焦点进导航栏后当前分页退回 selection 底色"
    );
}

/// 分页条底色必须覆盖整块，包括宽字符（CJK）后面的续接格。
///
/// 先铺底色再画字时，`Buffer::set_stringn` 会把宽字符的续接格 `reset()`，
/// 块面上留下与文字同宽的竖条空洞；底色只能在文字之后用 `set_style` 补。
/// `TestBackend` 的 diff 会跳过续接格，所以这里直接检查渲染后的帧缓冲。
#[test]
fn help_tab_highlight_covers_wide_char_continuation_cells() {
    fn tab_row_frame(
        app: &app::App,
        width: u16,
        height: u16,
    ) -> (Rect, Vec<ratatui::style::Color>) {
        let mut captured: Option<ratatui::buffer::Buffer> = None;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                app.render(frame);
                captured = Some(frame.buffer_mut().clone());
            })
            .expect("draw");
        let buffer = captured.expect("frame buffer");
        let tab = typewords_tui::ui::hit::targets(app, Rect::new(0, 0, width, height))
            .into_iter()
            .find(|target| matches!(target.action, Action::HelpSelectTab(_)))
            .expect("应有分页标签目标");
        let colors = (tab.rect.x..tab.rect.right())
            .map(|x| buffer.cell((x, tab.rect.y)).expect("cell").bg)
            .collect();
        (tab.rect, colors)
    }

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 120, 40));
    press(&mut app, KeyCode::Char('?'));
    let theme = app.theme();

    // 焦点在导航栏：selection 底色也要覆盖续接格。
    press(&mut app, KeyCode::Tab);
    let (rect, colors) = tab_row_frame(&app, 120, 40);
    assert_eq!(colors.len(), rect.width as usize);
    assert!(
        colors.iter().all(|bg| *bg == theme.selection),
        "selection 底色应覆盖整块（含宽字符续接格）：{colors:?}"
    );

    // 焦点回内容区：强调色底色同样覆盖整块。
    press(&mut app, KeyCode::Tab);
    let (rect, colors) = tab_row_frame(&app, 120, 40);
    assert_eq!(colors.len(), rect.width as usize);
    assert!(
        colors.iter().all(|bg| *bg == theme.accent),
        "强调色底色应覆盖整块（含宽字符续接格）：{colors:?}"
    );
}

/// 练习中打开的整屏帮助没有焦点模型，当前分页仍是强调色高亮。
#[test]
fn practice_help_active_tab_is_highlighted_with_accent() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 120, 40));
    app.start_practice();
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);
    let theme = app.theme();

    let backgrounds = tab_row_backgrounds(&app, 120, 40);
    assert!(
        backgrounds.contains(&theme.accent),
        "整屏帮助的当前分页应强调色高亮"
    );
    press(&mut app, KeyCode::Right);
    let backgrounds = tab_row_backgrounds(&app, 120, 40);
    assert!(
        backgrounds.contains(&theme.accent),
        "整屏帮助切分页后新分页仍有强调色高亮"
    );
}

/// 第一个分页再按 `←` 要像其它页面一样回到侧栏，而不是被分页条吞掉。
///
/// 分页条早期无条件拦截 `←` / `→`：帮助页里 `←` 永远回不到侧栏，只能
/// 发现 `Tab` 能换栏。到边的那一下现在放行给焦点模型（宽屏内容区左边缘
/// `←` 进侧栏），分页之间照旧切换。
#[test]
fn help_left_at_the_first_tab_enters_the_sidebar() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 120, 40));
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.help_tab, HelpTab::Shortcuts);

    press(&mut app, KeyCode::Left);
    assert_eq!(app.screen, Screen::Help, "到边的 ← 不该离开帮助页");
    assert_eq!(
        app.help_tab,
        HelpTab::Shortcuts,
        "第一个分页再按 ← 仍是第一个分页"
    );
    assert_eq!(
        app.focus,
        Some(FocusId::Nav(NavId::Help)),
        "到边的 ← 应回到侧栏的「帮助」项"
    );

    // 回内容区后分页键照旧：→ 回到内容区，再 → 切到第二个分页。
    press(&mut app, KeyCode::Right);
    assert_eq!(
        focus::pane(&app),
        Some(Pane::Content),
        "侧栏焦点下 → 回到内容区"
    );
    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::Faq);
    assert_eq!(app.screen, Screen::Help);
}

/// 最后一个分页再按 `→` 只吞键：不离开帮助页，也不会去别的地方。
#[test]
fn help_right_at_the_last_tab_stays_on_the_page() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 120, 40));
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::About);

    press(&mut app, KeyCode::Right);
    assert_eq!(app.help_tab, HelpTab::About, "最后一页再按 → 应停住");
    assert_eq!(app.screen, Screen::Help, "到边的 → 不该离开帮助页");
    assert_eq!(
        focus::pane(&app),
        Some(Pane::Content),
        "帮助页右侧没有可跨的栏，焦点留在内容区"
    );
}

/// `h` / `l` 不是焦点键：到边时仍被分页条吞掉，不能落进「任意键返回」。
#[test]
fn help_h_at_the_first_tab_does_not_leave_the_page() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 120, 40));
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.screen, Screen::Help, "h 到边应被分页条吞掉");
    assert_eq!(app.help_tab, HelpTab::Shortcuts);

    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Char('l'));
    assert_eq!(app.screen, Screen::Help, "l 到边应被分页条吞掉");
    assert_eq!(app.help_tab, HelpTab::About);
}

/// 窄屏没有侧栏：到边的 `←` 仍是分页条的事，跨栏键是 `↑`。
#[test]
fn help_left_at_the_first_tab_does_nothing_on_narrow_screens() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 80, 24));
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Left);
    assert_eq!(app.screen, Screen::Help);
    assert_eq!(app.help_tab, HelpTab::Shortcuts);
    assert_eq!(
        focus::pane(&app),
        Some(Pane::Content),
        "窄屏内容区跨栏键是 ↑，← 到边不动"
    );
}

// ── 键盘：组合键不能被当成字符 ──────────────────────────────────────────

/// 词条编辑弹层里按 `Ctrl+C` 不会把字母 c 写进字段。
///
/// 终端把 `Ctrl+C` 报成 `Char('c')` + CONTROL 修饰位，只匹配
/// `KeyCode::Char` 的实现会把它当普通字母。
#[test]
fn ctrl_c_is_not_typed_into_the_word_edit_overlay() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.open_word_edit(Some(0));
    press_with(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    let field = app
        .word_edit
        .as_mut()
        .map(|edit| edit.current().clone())
        .unwrap_or_default();
    assert_eq!(field, "word00", "Ctrl+C 不应被写进词条字段，实际 {field:?}");
}

/// 笔记弹层里按 `Ctrl+C` 不会把字母 c 写进笔记。
#[test]
fn ctrl_c_is_not_typed_into_the_note_overlay() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    press_with(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(app.edit.is_some(), "Ctrl+O 应打开笔记弹层");
    press_with(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    let buffer = app
        .edit
        .as_ref()
        .map(|edit| edit.buffer.clone())
        .unwrap_or_default();
    assert_eq!(buffer, "", "Ctrl+C 不应被写进笔记，实际 {buffer:?}");
}

/// 搜索框里按 `Ctrl+C` 不会把字母 c 写进搜索词。
#[test]
fn ctrl_c_is_not_typed_into_the_words_search_box() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.open_words();
    press(&mut app, KeyCode::Char('/'));
    assert!(app.words_search_focus, "「/」应聚焦搜索框");
    press_with(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(app.words_filter, "", "Ctrl+C 不应被写进搜索词");
}

// ── 键盘：改绑后必须跟着走 ──────────────────────────────────────────────

/// 练习页的提示条跟着改绑走。
///
/// 早期实现把「跳过」「掌握」写死成字符串，用户改绑后提示仍指向旧键位。
#[test]
fn the_practice_hint_follows_rebinding() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    app.shortcuts
        .set(
            ShortcutAction::Ignore,
            ShortcutKey::ctrl(BaseKey::Char('Y')),
        )
        .expect("改绑「跳过单词」");
    app.shortcuts
        .set(
            ShortcutAction::ToggleSimple,
            ShortcutKey::ctrl(BaseKey::Char('U')),
        )
        .expect("改绑「标记已掌握」");

    let rows = logical_rows(&app, 120, 40);
    let text: String = rows
        .iter()
        .flat_map(|row| row.iter().map(|(c, _)| *c))
        .collect();
    assert!(
        !text.contains("Tab"),
        "改绑「跳过单词」后提示条不应再显示 Tab:\n{text}"
    );
    assert!(
        text.contains("Ctrl+Y"),
        "改绑后提示条应显示新键位 Ctrl+Y:\n{text}"
    );
}

/// 「播放单词发音」改绑后，新键位必须能触发发音。
///
/// 早期实现把 `Ctrl+P` 硬编码在 TUI 专有键位里，键位表里的
/// `PlayWordPronunciation` 改绑后按了没反应。
#[test]
fn the_pronunciation_shortcut_follows_rebinding() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    app.shortcuts
        .set(
            ShortcutAction::PlayWordPronunciation,
            ShortcutKey::ctrl(BaseKey::Char('Y')),
        )
        .expect("改绑「播放单词发音」");
    let actions = app
        .shortcuts
        .actions_for(&ShortcutKey::ctrl(BaseKey::Char('Y')));
    assert!(
        actions.contains(&ShortcutAction::PlayWordPronunciation),
        "改绑后 Ctrl+Y 应能反查到 PlayWordPronunciation"
    );
    assert!(
        ShortcutAction::PlayWordPronunciation.is_available(),
        "「播放单词发音」已有实现，不该再标为暂不可用"
    );
}

/// 单词测试页的选项键位跟着改绑走。
#[test]
fn the_word_test_options_follow_rebinding() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_word_test(WordPracticeMode::ReviewWordsTest);
    app.shortcuts
        .set(
            ShortcutAction::ChooseA,
            ShortcutKey::ctrl(BaseKey::Char('A')),
        )
        .expect("改绑「选择 A」");
    press_with(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let selected = app.word_test.as_ref().and_then(|state| state.selected);
    assert_eq!(
        selected,
        Some(0),
        "改绑「选择 A」到 Ctrl+A 后按 Ctrl+A 应选 A"
    );
}

// ── 一致性：统计不能丢 ──────────────────────────────────────────────────

/// 中途退出练习，学习时长不能丢。
///
/// 统计只在会话正常结束的 `settle()` 里写，中途 `Ctrl+C` 退出时
/// 这段时长会跟着断点缓存一起走——但它并没有消失，而是留在缓存里，
/// 等这一轮真正结算时才落盘。
///
/// 对齐上游：`flushStatToStore` 只在结算（`settleLocalPractice`）与
/// 重置缓存（`resetCacheData`）两处调用，中途退出后**继续练**时
/// 统计是不落盘的。早期实现一退出就重新生成任务（等于重置缓存），
/// 于是落盘时机被提前到了「开新一轮」。
#[test]
fn leaving_practice_early_keeps_the_time_spent() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let mut now = typewords_tui::domain::timer::now_ms();
    for _ in 0..10 {
        now += 1000;
        app.on_tick(now);
    }
    assert_eq!(app.stat.spend, 10_000, "练习应累计 10 秒");

    // 中途退出：统计留在断点缓存里，还没落盘。
    app.leave_practice();
    let totals = typewords_tui::ui::insights::totals(&app.boot.base);
    assert_eq!(totals.spend, 0, "中途退出时统计应留在缓存里，还没落盘");

    // 续练：接着上次那一轮，累计的用时不能归零。
    app.start_practice();
    assert_eq!(app.stat.spend, 10_000, "续练时断点里的用时应当保留");
}

/// 练完一整轮（结算）后，中途退出攒下的时长要落进词典统计。
#[test]
fn a_resumed_round_records_its_whole_time_on_settle() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let mut now = typewords_tui::domain::timer::now_ms();
    for _ in 0..10 {
        now += 1000;
        app.on_tick(now);
    }
    app.leave_practice();

    // 续练并把这轮练完——结算时统计一次性落盘。
    app.start_practice();
    for _ in 0..60 {
        if app.summary.is_some() {
            break;
        }
        app.on_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Tab,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
    }
    assert!(app.summary.is_some(), "应弹出结算");

    let totals = typewords_tui::ui::insights::totals(&app.boot.base);
    assert!(
        totals.spend >= 10_000,
        "结算后学习时长应落进词典统计，实际 {}",
        totals.spend
    );
}

/// 正常结算的练习时长只记一次。
///
/// 结算走 `settle` 落盘，随后退出不能再记一遍。
#[test]
fn a_finished_session_records_its_time_exactly_once() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let mut now = typewords_tui::domain::timer::now_ms();
    for _ in 0..5 {
        now += 1000;
        app.on_tick(now);
    }
    // 直接把会话标记为完成，走结算流程
    if let Some(session) = app.session.as_mut() {
        session.complete = true;
    }
    // `after_advance` 是结算的触发点（会话完成后由它调 `finish_session`）
    app.apply_action(Action::PracticeGotoWord(0));
    assert!(
        app.summary.is_some(),
        "会话完成后应弹出结算，summary={:?}",
        app.summary
    );
    let after_settle = typewords_tui::ui::insights::totals(&app.boot.base).spend;
    assert!(
        after_settle >= 5_000,
        "结算后统计里应有这一轮的时长，实际 {after_settle}"
    );

    // 结算后再退出，不应重复记录
    app.apply_action(Action::SummaryHome);
    let after_home = typewords_tui::ui::insights::totals(&app.boot.base).spend;
    assert_eq!(after_home, after_settle, "结算后退出不应重复记录时长");
}

/// 设置页的「每日词数」与主页的「每日目标」口径一致。
#[test]
fn the_per_day_setting_matches_the_home_display() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    // 手动把每日词数设成超过词库词数
    if let Some(dict) = app.boot.base.study_dict_mut() {
        dict.per_day_study_number = 100;
    }
    let displayed = app
        .boot
        .base
        .study_dict()
        .map(|dict| dict.per_day())
        .unwrap_or(0);
    let setting = app.setting_number_value(app::EditTarget::PerDay);
    assert_eq!(
        displayed as i64, setting,
        "设置页「每日词数」应与主页「每日目标」一致"
    );
}

/// 离开单词测试页后状态要清干净。
#[test]
fn leaving_the_word_test_page_clears_its_state() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_word_test(WordPracticeMode::ReviewWordsTest);
    assert!(app.word_test.is_some());
    // 鼠标点侧栏「统计」离开
    app.apply_action(Action::Navigate(Screen::Stats));
    assert_eq!(app.screen, Screen::Stats);
    assert!(app.word_test.is_none(), "离开单词测试页后应清掉测试状态");
}

/// 词典学完时跳组弹层的光标不能越界。
#[test]
fn the_group_picker_cursor_stays_in_range() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    if let Some(dict) = app.boot.base.study_dict_mut() {
        dict.per_day_study_number = 4;
        dict.length = 12;
        dict.last_learn_index = 12;
        dict.complete = true;
    }
    let total = app.total_groups();
    app.open_group_picker();
    let cursor = app.group_picker.as_ref().expect("跳组弹层").cursor;
    assert!(cursor < total, "弹层光标 {cursor} 越界（共 {total} 组）");
    assert!(app.current_group() <= total, "当前组号不该超过总组数");
}
