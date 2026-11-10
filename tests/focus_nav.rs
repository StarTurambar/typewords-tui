//! 主页方向键焦点导航的集成测试。
//!
//! 交互模型：
//! - 焦点存稳定身份（FocusId），不存列表下标；resize / 模式变化后重新解析。
//! - 内容区按视觉行分带：上下键在带间移动（永不反向），左右键在带内移动。
//! - 宽屏：侧栏竖向（上/下选、右进内容区、左在内容区到边时进导航）；
//!   窄屏：顶栏横向（左/右选、下进内容区、上在内容区到顶带时进导航）。
//! - 无焦点：下/右落「开始 / 继续学习」，上/左进导航栏。
//! - 到边不回头回绕；失效焦点 Enter 只清空并提示，不开练习。
//!
//! 所有用例都先 set_viewport 再按键，渲染用同一个尺寸。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Screen};
use typewords_tui::domain::types::WordPracticeMode;
use typewords_tui::ui::focus::{self, ContentId, FocusId, Pane};
use typewords_tui::ui::shell::{self, Nav};
use unicode_width::UnicodeWidthStr;

const WIDE: (u16, u16) = (120, 40);
const NARROW: (u16, u16) = (80, 24);
const MINIMUM: (u16, u16) = (60, 20);

fn area_of(size: (u16, u16)) -> Rect {
    Rect::new(0, 0, size.0, size.1)
}

/// 建一个已经同步好视口的测试应用（Fixture 必须活得比 App 久，一起返回）。
fn test_app(size: (u16, u16)) -> (common::Fixture, app::App) {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("build app");
    app.set_viewport(area_of(size));
    (fixture, app)
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn label(app: &app::App) -> Option<&'static str> {
    focus::focused_label(app, app.viewport)
}

fn rect(app: &app::App) -> Rect {
    focus::focused_rect(app, app.viewport).expect("当前应有焦点")
}

fn set_content(app: &mut app::App, id: ContentId) {
    app.focus = Some(FocusId::Content(id));
    app.last_content_focus = Some(id);
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

fn last_line(app: &app::App, size: (u16, u16)) -> String {
    render_text(app, size)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
}

/// 无焦点时方向键的确定落点：下/右 → 主按钮；上/左 → 导航栏。
#[test]
fn idle_arrows_have_a_predictable_landing() {
    for size in [WIDE, NARROW, MINIMUM] {
        let (_fixture, mut app) = test_app(size);
        press(&mut app, KeyCode::Down);
        assert_eq!(focus::pane(&app), Some(Pane::Content));
        assert_eq!(label(&app), Some("开始学习"), "{size:?} 下 ↓ 应落主按钮");

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Right);
        assert_eq!(label(&app), Some("开始学习"), "{size:?} 下 → 应落主按钮");

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Up);
        assert_eq!(
            focus::pane(&app),
            Some(Pane::Nav),
            "{size:?} 下 ↑ 应进导航栏"
        );
        assert_eq!(label(&app), Some("单词"));

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Left);
        assert_eq!(
            focus::pane(&app),
            Some(Pane::Nav),
            "{size:?} 下 ← 应进导航栏"
        );
        assert_eq!(label(&app), Some("单词"));
    }
}

/// 内容区连按 ↓：y 单调不降，到最后一个带后停住，不回绕。
#[test]
fn down_moves_downwards_and_never_reverses() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("开始学习"));
    let mut previous = rect(&app).y;
    for _ in 0..6 {
        press(&mut app, KeyCode::Down);
        let y = rect(&app).y;
        assert!(y >= previous, "↓ 不应向上跳：{previous} -> {y}");
        previous = y;
    }
    assert_eq!(label(&app), Some("更多词库"), "连续 ↓ 应停在最后一个带");
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("更多词库"), "到底后继续按 ↓ 仍停住");
}

/// 窄屏 60×20 下空间最小，方向语义同样成立：↓ 不向上，↑ 才向上。
#[test]
fn minimum_size_keeps_down_downwards() {
    let (_fixture, mut app) = test_app(MINIMUM);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("开始学习"));
    let start_y = rect(&app).y;
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("开始学习"), "↓ 到底不应跳到上面的目标");
    assert_eq!(rect(&app).y, start_y);
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app), Some("每日目标 -"));
    assert!(rect(&app).y < start_y);
}

/// 同一带内 ←/→ 移动；宽屏最左再按 ← 才进导航栏。
#[test]
fn left_and_right_move_inside_the_row() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("开始学习"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app), Some("自由练习"), "→ 应到同一行的右侧按钮");
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app), Some("自由练习"), "行尾继续 → 停住，不换栏");
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app), Some("开始学习"), "← 应回到同一行左侧");
    press(&mut app, KeyCode::Left);
    assert_eq!(
        focus::pane(&app),
        Some(Pane::Nav),
        "宽屏行首再 ← 才进导航栏"
    );
    assert_eq!(label(&app), Some("单词"));
}

/// 跨栏会记住内容区上一次的选择：← 进导航、→ 回来仍是原来的按钮。
#[test]
fn returning_from_nav_restores_the_content_selection() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("词库页"));
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    press(&mut app, KeyCode::Right);
    assert_eq!(focus::pane(&app), Some(Pane::Content));
    assert_eq!(label(&app), Some("词库页"), "回到内容区应恢复原选择");
}

/// 宽屏侧栏：↑/↓ 走导航项（含「退出」），→ 回内容区。
#[test]
fn wide_nav_walks_every_item_including_quit() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app), Some("单词"));
    let expected = ["文章", "词库", "统计", "帮助", "设置", "退出"];
    for label_text in expected {
        press(&mut app, KeyCode::Down);
        assert_eq!(label(&app), Some(label_text), "宽屏侧栏 ↓ 顺序");
    }
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("退出"), "最后一项继续 ↓ 停住");
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app), Some("设置"));

    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert!(app.confirm.is_some(), "Enter 在「退出」上应打开确认弹层");
    press(&mut app, KeyCode::Esc);
    assert!(app.confirm.is_none(), "Esc 应取消退出确认");
}

/// 导航栏 Enter 落在当前页入口：不切页，但要有可感知提示。
#[test]
fn enter_on_the_current_nav_entry_gives_feedback() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app), Some("单词"));
    app.toasts.clear();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Home);
    assert!(!app.toasts.is_empty(), "Enter 在当前页入口上应给出提示");
    assert_eq!(label(&app), Some("单词"), "焦点应保持");
}

/// 窄屏顶栏是横向的：←/→ 选标签，↓ 进内容区；内容区最顶带再 ↑ 才回顶栏。
#[test]
fn narrow_topbar_is_horizontal_and_down_enters_content() {
    let (_fixture, mut app) = test_app(NARROW);
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app), Some("单词"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app), Some("文章"), "窄屏 → 应向右移到下一个标签");
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app), Some("词库"));
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app), Some("文章"));

    press(&mut app, KeyCode::Down);
    assert_eq!(focus::pane(&app), Some(Pane::Content), "窄屏 ↓ 应进内容区");
    assert_eq!(label(&app), Some("开始学习"));
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app), Some("每日目标 -"), "内容区 ↑ 先走视觉行带");
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app), Some("练习模式"));
    press(&mut app, KeyCode::Up);
    assert_eq!(focus::pane(&app), Some(Pane::Nav), "顶带再 ↑ 才回顶栏");
    assert_eq!(label(&app), Some("单词"));
}

/// 窄屏顶栏到边停住，不回头回绕。
#[test]
fn narrow_topbar_stops_at_the_edges() {
    let (_fixture, mut app) = test_app(NARROW);
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app), Some("单词"));
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app), Some("单词"), "最左标签继续 ← 停住");
    for _ in 0..5 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(label(&app), Some("设置"), "最后一个标签");
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app), Some("设置"), "最右标签继续 → 停住");
}

/// 宽屏 resize 后元素消失：Enter 不再触发任何动作，只清焦点并提示。
#[test]
fn resize_that_removes_the_focused_target_cannot_trigger_actions() {
    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::Free);
    app.set_viewport(area_of(MINIMUM));
    assert_eq!(label(&app), None, "60×20 没有自由练习目标");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Home);
    assert!(app.session.is_none());
    assert!(app.focus.is_none());
    assert!(!app.toasts.is_empty(), "失效焦点应给出提示");
}

/// resize 后元素仍在（只是位置变了）：焦点身份保持，不会指到别人。
#[test]
fn resize_keeps_the_same_identity_when_the_target_still_exists() {
    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::DailyMinus);
    app.set_viewport(area_of(MINIMUM));
    assert_eq!(label(&app), Some("每日目标 -"), "身份应仍指向同一个按钮");
    let before = app
        .boot
        .base
        .study_dict()
        .expect("夹具应有学习词典")
        .per_day();
    press(&mut app, KeyCode::Enter);
    let after = app
        .boot
        .base
        .study_dict()
        .expect("夹具应有学习词典")
        .per_day();
    assert_eq!(label(&app), Some("每日目标 -"), "调整后焦点保持");
    assert_eq!(
        after,
        before.saturating_sub(5).max(1),
        "Enter 应仍是每日目标 -5"
    );
}

/// 模式名变长导致模式标签消失：焦点失效，Enter 不能进单词测试。
#[test]
fn disappearing_mode_target_does_not_start_a_words_test() {
    let (_fixture, mut app) = test_app(NARROW);
    app.boot.settings.word_practice_mode = WordPracticeMode::ShuffleWordsTest;
    set_content(&mut app, ContentId::Mode);
    assert_eq!(label(&app), None);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Home);
    assert!(app.word_test.is_none());
    assert!(app.focus.is_none());
    assert!(!app.toasts.is_empty());
}

/// 弹层打开时两栏都不画主页焦点（导航栏与内容区一致）。
#[test]
fn modal_hides_focus_on_both_panes() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Char('m'));
    assert!(app.mode_picker.is_some());
    assert!(
        focus::focused_rect(&app, app.viewport).is_none(),
        "导航栏焦点应隐藏"
    );
    assert!(focus::focused_label(&app, app.viewport).is_none());
    press(&mut app, KeyCode::Esc);

    press(&mut app, KeyCode::Esc);
    set_content(&mut app, ContentId::Start);
    press(&mut app, KeyCode::Char('m'));
    assert!(app.mode_picker.is_some());
    assert!(
        focus::focused_rect(&app, app.viewport).is_none(),
        "内容区焦点应隐藏"
    );
}

/// 终端低于 60×20 时按键被忽略，不能盲操作看不见的主页。
#[test]
fn keys_are_ignored_below_the_minimum_terminal_size() {
    let (_fixture, mut app) = test_app((59, 19));
    press(&mut app, KeyCode::Down);
    assert!(app.focus.is_none());
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Home);
    assert!(app.session.is_none());
    press(&mut app, KeyCode::Char('q'));
    assert!(app.confirm.is_none());
}

/// 底栏有焦点时显示「选中 X / ↑↓←→ 移动 / enter 触发 / esc 取消」。
#[test]
fn footer_shows_the_selected_label_and_move_hint() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Down);
    let footer = last_line(&app, WIDE);
    assert!(footer.contains("选中"), "底栏应显示选中：{footer:?}");
    assert!(footer.contains("开始学习"), "底栏应显示 label：{footer:?}");
    assert!(footer.contains("↑↓←→"), "底栏应提示方向键：{footer:?}");
    assert!(footer.contains("移动"), "底栏提示应为移动：{footer:?}");
    assert!(footer.contains("enter"));
    assert!(footer.contains("esc"));
}

/// Esc 清焦点，底栏恢复常规提示。
#[test]
fn escape_clears_focus_and_restores_the_regular_footer() {
    let (_fixture, mut app) = test_app(WIDE);
    let regular = last_line(&app, WIDE);
    press(&mut app, KeyCode::Down);
    assert!(app.focus.is_some());
    press(&mut app, KeyCode::Esc);
    assert!(app.focus.is_none());
    assert!(app.last_content_focus.is_none());
    assert_eq!(last_line(&app, WIDE), regular);
}

/// 焦点高亮 = 整块 accent + BOLD，且不溢出目标矩形。
#[test]
fn focus_highlight_fills_the_whole_target() {
    let (_fixture, mut app) = test_app(WIDE);
    let theme = app.theme();
    let plus = focus::content_target(&app, app.viewport, ContentId::DailyPlus).expect("目标");
    set_content(&mut app, ContentId::DailyPlus);
    let backend = TestBackend::new(WIDE.0, WIDE.1);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    for x in plus.rect.x..plus.rect.right() {
        let cell = buffer.cell((x, plus.rect.y)).expect("cell");
        assert_eq!(cell.bg, theme.accent, "焦点矩形应整块 accent");
        assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
    }
    let right_of = buffer.cell((plus.rect.right(), plus.rect.y)).expect("cell");
    assert_ne!(right_of.bg, theme.accent, "高亮不应溢出");
}

/// 导航目标：宽屏 7 项（含退出），窄屏 6 项。
#[test]
fn nav_targets_match_the_visible_navigation() {
    let (_fixture, app) = test_app(WIDE);
    let wide = focus::nav_targets(&app, app.viewport);
    assert_eq!(wide.len(), 7);
    assert_eq!(wide.last().and_then(|target| target.label), Some("退出"));

    let (_fixture, app) = test_app(NARROW);
    let narrow = focus::nav_targets(&app, app.viewport);
    assert_eq!(narrow.len(), 6);
    assert!(narrow.iter().all(|target| target.label != Some("退出")));
}

/// 内容区目标身份唯一且与 label 对应。
#[test]
fn content_targets_have_unique_ids() {
    let (_fixture, app) = test_app(WIDE);
    let targets = focus::content_targets(&app, app.viewport);
    let mut ids: Vec<_> = targets.iter().filter_map(|target| target.focus).collect();
    ids.sort_by_key(|id| format!("{id:?}"));
    ids.dedup();
    assert_eq!(ids.len(), targets.len());
}

/// 数字键切页后焦点与「上次内容选择」都清空。
#[test]
fn switching_screens_clears_focus_and_remembered_selection() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app), Some("自由练习"));
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.screen, Screen::DictPicker);
    assert!(app.focus.is_none());
    assert!(app.last_content_focus.is_none());
}

/// 其它页面的方向键语义不受影响：统计页仍是滚动，不产生主页焦点。
#[test]
fn stats_arrows_still_scroll_instead_of_moving_focus() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Stats);
    assert!(focus::pane(&app).is_none());
    let max_scroll = typewords_tui::ui::stats::body_line_count(&app).saturating_sub(1) as u16;
    assert!(max_scroll >= 1);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.stats_scroll, 1.min(max_scroll));
    assert!(app.focus.is_none());
}

/// 焦点在弹层关闭后恢复；模式弹层关闭后 Enter 仍是「练习模式」。
#[test]
fn focus_survives_overlay_roundtrip() {
    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::Mode);
    press(&mut app, KeyCode::Enter);
    assert!(app.mode_picker.is_some());
    press(&mut app, KeyCode::Esc);
    assert!(app.mode_picker.is_none());
    assert_eq!(label(&app), Some("练习模式"));
    press(&mut app, KeyCode::Enter);
    assert!(app.mode_picker.is_some(), "再次 Enter 应重新打开模式弹层");
}

/// 宽屏内容区最左项不会把「上一项」解释成换栏以外的语义：← 就是进导航。
#[test]
fn left_from_the_first_content_band_enters_nav_not_wraps() {
    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::Start);
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app), Some("单词"));
    assert!(
        !shell::layout(app.viewport, Nav::Home).narrow,
        "120×40 应是宽屏布局"
    );
}

/// 鼠标点击可聚焦元素会把焦点同步过去；点空白清空焦点。
#[test]
fn mouse_click_syncs_the_focus_to_the_clicked_target() {
    let (_fixture, mut app) = test_app(WIDE);
    let target = focus::content_target(&app, app.viewport, ContentId::DailyMinus).expect("目标");
    let before = app
        .boot
        .base
        .study_dict()
        .expect("夹具应有学习词典")
        .per_day();
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x + 1,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        app.viewport,
    );
    assert_eq!(label(&app), Some("每日目标 -"), "点击后焦点应跟到被点元素");
    let after = app
        .boot
        .base
        .study_dict()
        .expect("夹具应有学习词典")
        .per_day();
    assert_eq!(after, before.saturating_sub(5).max(1));

    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        app.viewport,
    );
    assert!(app.focus.is_none(), "点空白应清空焦点");
}

fn buffer_of(app: &app::App, size: (u16, u16)) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(size.0, size.1);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    terminal.backend().buffer().clone()
}

/// 焦点在主按钮上时必须有可见变化：常态 element 底，选中 accent 底。
#[test]
fn focus_on_the_primary_button_is_visible() {
    let (_fixture, mut app) = test_app(WIDE);
    let theme = app.theme();
    let start = focus::content_target(&app, app.viewport, ContentId::Start).expect("目标");
    let normal = buffer_of(&app, WIDE);
    let cell = normal.cell((start.rect.x + 1, start.rect.y)).expect("cell");
    assert_eq!(cell.bg, theme.element, "主按钮常态应是 element 底");

    set_content(&mut app, ContentId::Start);
    let focused = buffer_of(&app, WIDE);
    let cell = focused
        .cell((start.rect.x + 1, start.rect.y))
        .expect("cell");
    assert_eq!(cell.bg, theme.accent, "选中的主按钮应是 accent 底");
}

/// Enter 仍然触发各内容区动作（开始 / 自由 / 词库 / 每日目标）。
#[test]
fn enter_still_triggers_each_content_action() {
    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::Start);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Practice);
    assert!(app.session.is_some());

    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::Free);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Practice);
    let session = app.session.as_ref().expect("会话");
    assert_eq!(session.mode, WordPracticeMode::Free);

    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::DictPage);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::DictPicker);
    assert!(app.focus.is_none(), "切页应清焦点");

    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::DailyMinus);
    let before = app
        .boot
        .base
        .study_dict()
        .expect("夹具应有学习词典")
        .per_day();
    press(&mut app, KeyCode::Enter);
    let after = app
        .boot
        .base
        .study_dict()
        .expect("夹具应有学习词典")
        .per_day();
    assert_eq!(after, before.saturating_sub(5).max(1));
    assert_eq!(label(&app), Some("每日目标 -"), "调整后焦点保持");
}

/// 导航栏 Enter 仍然切页（文章），并清空焦点。
#[test]
fn enter_on_a_nav_item_navigates() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app), Some("文章"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Articles);
    assert!(app.focus.is_none());
}
