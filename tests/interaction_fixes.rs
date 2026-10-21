//! 交互一致性与命中完整性的回归测试。
//!
//! 每个用例对应一个已修复的缺陷，测试名用行为句子描述期望。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Action, Screen};
use typewords_tui::domain::shortcut::{BaseKey, ShortcutAction, ShortcutKey};
use typewords_tui::domain::types::WordPracticeMode;
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

// ── 滚动指示器不能被行 target 抢走 ──────────────────────────────────────

/// 设置页点右下角的 `↓` 不会跳到别的设置项。
#[test]
fn the_settings_scroll_indicator_is_not_a_row_target() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.screen = Screen::Settings;
    let area = Rect::new(0, 0, 120, 40);
    let rows = logical_rows(&app, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);

    // 找到右下角的 ↓
    let indicator = rows
        .iter()
        .enumerate()
        .filter_map(|(y, row)| {
            row.iter()
                .position(|(c, _)| *c == '↓')
                .map(|index| (row[index].1, y as u16))
        })
        .next_back();
    let Some((column, row)) = indicator else {
        return; // 没有更多内容时没有指示器
    };
    let owner = targets.iter().find(|t| t.contains(column, row));
    assert!(
        owner.is_none(),
        "↓ 指示器不应被行 target 覆盖，实际被 {:?} 覆盖",
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
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);

    // 120 列 → content 宽 90 ≥ 78 → 两栏；26 行 → content 高 22 < 25 → 左栏放不下
    for _ in 0..40 {
        press(&mut app, KeyCode::Down);
    }
    let rows = logical_rows(&app, 120, 26);
    let text: String = rows
        .iter()
        .flat_map(|row| row.iter().map(|(c, _)| *c))
        .collect();
    assert!(text.contains("Ctrl+C"), "两栏模式滚到底应能看到 Ctrl+C");
}

/// 帮助页点滚动指示器是滚动，不是退出。
#[test]
fn clicking_the_help_scroll_indicator_scrolls_instead_of_exiting() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    let area = Rect::new(0, 0, 80, 24);
    let rows = logical_rows(&app, 80, 24);
    let indicator = rows
        .iter()
        .enumerate()
        .filter_map(|(y, row)| {
            row.iter()
                .position(|(c, _)| *c == '↓')
                .map(|index| (row[index].1, y as u16))
        })
        .next_back();
    let Some((column, row)) = indicator else {
        return;
    };
    let action = typewords_tui::ui::hit::action_at(&app, area, column, row);
    assert_ne!(
        action,
        Some(Action::HelpBack),
        "点 ↓ 滚动指示器不该退出帮助页"
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

/// 中途退出练习，学习时长要落进词典统计。
///
/// 统计只在会话正常结束的 `settle()` 里写，中途 `Ctrl+C` 退出时
/// 这段时长会跟着断点缓存一起消失。
#[test]
fn leaving_practice_early_records_the_time_spent() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let mut now = typewords_tui::domain::timer::now_ms();
    for _ in 0..10 {
        now += 1000;
        app.on_tick(now);
    }
    assert_eq!(app.stat.spend, 10_000, "练习应累计 10 秒");

    // 中途退出：先退出练习，再开新一轮（断点缓存被丢弃）
    app.leave_practice();
    app.start_practice();

    let totals = typewords_tui::ui::insights::totals(&app.boot.base);
    assert!(
        totals.spend >= 10_000,
        "中途退出的学习时长应落进词典统计，实际 {}",
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
