//! 键盘焦点导航的集成测试。
//!
//! 交互模型（所有套外壳页面共用）：
//! - 焦点存稳定身份（FocusId），主页内容区是 ContentId，其它页面是 Page；
//!   页面自己的光标（词库行 / 设置行 / 日历日期 / 滚动位置）就是 Page 的身份。
//! - 主页内容区按视觉行分带：上下键在带间移动（永不反向），左右键在带内移动。
//! - 其它页面内容区：上下键移动页面光标，左右键执行页面的横向操作
//!   （文章页换栏、统计页选日期），动不了时按几何跨栏。
//! - 宽屏：侧栏竖向；窄屏：顶栏横向。宽屏内容区左边缘 ← 进侧栏，
//!   窄屏内容区最顶部 ↑ 进顶栏；Tab / Shift+Tab 在任何页面都能换栏。
//! - 进入任何页面默认选中内容区（主页是主按钮），Esc 在主页取消选中、
//!   在其它页面返回上一层。
//!
//! 所有用例都先 set_viewport 再按键，渲染用同一个尺寸。

mod common;

use chrono::Datelike;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Screen};
use typewords_tui::domain::types::WordPracticeMode;
use typewords_tui::ui::focus::{self, ContentId, FocusId, NavId, Pane};
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

fn press_ctrl(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::CONTROL));
}

/// 清掉主页的默认选中，回到「无焦点」状态。
///
/// 主页现在一打开就选中主按钮；历史用例验证的是「取消选中之后」的方向键
/// 语义，所以先显式 Esc 一次。
fn idle(app: &mut app::App) {
    press(app, KeyCode::Esc);
    assert!(app.focus.is_none(), "Esc 应清空默认选中");
}

fn label(app: &app::App) -> Option<String> {
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

fn buffer_of(app: &app::App, size: (u16, u16)) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(size.0, size.1);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    terminal.backend().buffer().clone()
}

// ── 主页：两栏与视觉行带 ────────────────────────────────────────────────

/// 无焦点时方向键的确定落点：下/右 → 主按钮；上/左 → 导航栏。
#[test]
fn idle_arrows_have_a_predictable_landing() {
    for size in [WIDE, NARROW, MINIMUM] {
        let (_fixture, mut app) = test_app(size);
        idle(&mut app);
        press(&mut app, KeyCode::Down);
        assert_eq!(focus::pane(&app), Some(Pane::Content));
        assert_eq!(
            label(&app).as_deref(),
            Some("开始学习"),
            "{size:?} 下 ↓ 应落主按钮"
        );

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Right);
        assert_eq!(
            label(&app).as_deref(),
            Some("开始学习"),
            "{size:?} 下 → 应落主按钮"
        );

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Up);
        assert_eq!(
            focus::pane(&app),
            Some(Pane::Nav),
            "{size:?} 下 ↑ 应进导航栏"
        );
        assert_eq!(label(&app).as_deref(), Some("单词"));

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Left);
        assert_eq!(
            focus::pane(&app),
            Some(Pane::Nav),
            "{size:?} 下 ← 应进导航栏"
        );
        assert_eq!(label(&app).as_deref(), Some("单词"));
    }
}

/// 初次打开界面：首帧就选中主按钮，整块高亮可见、底栏说明选中项。
#[test]
fn home_opens_with_the_primary_action_selected() {
    for size in [WIDE, NARROW, MINIMUM] {
        let (_fixture, app) = test_app(size);
        assert_eq!(
            app.focus,
            Some(FocusId::Content(ContentId::Start)),
            "{size:?} 启动应默认选中主按钮"
        );
        assert_eq!(label(&app).as_deref(), Some("开始学习"), "{size:?}");

        let theme = app.theme();
        let start = focus::content_target(&app, app.viewport, ContentId::Start).expect("主按钮");
        let buffer = buffer_of(&app, size);
        // 整行都应是强调色底。CJK 宽字符右侧的续接单元在 ratatui 的测试后端
        // 不参与 diff（真实终端由宽字形覆盖），保留默认样式是预期行为。
        let mut skipped = 0;
        for x in start.rect.x..start.rect.right() {
            let cell = buffer.cell((x, start.rect.y)).expect("cell");
            if cell.bg == theme.accent {
                continue;
            }
            assert_eq!(
                cell.bg,
                ratatui::style::Color::Reset,
                "{size:?} 高亮不应留下原底色"
            );
            skipped += 1;
        }
        assert!(
            skipped <= 8,
            "{size:?} 除宽字符续接单元外都应高亮（{skipped} 个空单元）"
        );

        let footer = last_line(&app, size);
        assert!(
            footer.contains("选中"),
            "{size:?} 底栏应说明选中：{footer:?}"
        );
        assert!(
            footer.contains("开始学习"),
            "{size:?} 底栏应显示选中项：{footer:?}"
        );
    }
}

/// 默认选中项的文案与按钮同源：有学习进度时是「继续学习」。
#[test]
fn the_initial_selection_follows_the_primary_label() {
    let (_fixture, mut app) = test_app(WIDE);
    app.boot
        .base
        .study_dict_mut()
        .expect("夹具应有学习词典")
        .last_learn_index = 3;
    assert_eq!(label(&app).as_deref(), Some("继续学习"));
    assert!(last_line(&app, WIDE).contains("继续学习"));
}

/// 内容区连按 ↓：y 单调不降，到最后一个带后停住，不回绕。
#[test]
fn down_moves_downwards_and_never_reverses() {
    let (_fixture, mut app) = test_app(WIDE);
    idle(&mut app);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    let mut previous = rect(&app).y;
    for _ in 0..6 {
        press(&mut app, KeyCode::Down);
        let y = rect(&app).y;
        assert!(y >= previous, "↓ 不应向上跳：{previous} -> {y}");
        previous = y;
    }
    assert_eq!(label(&app).as_deref(), Some("更多词库"));
    press(&mut app, KeyCode::Down);
    assert_eq!(
        label(&app).as_deref(),
        Some("更多词库"),
        "到底后继续 ↓ 仍停住"
    );
}

/// 窄屏 60×20 下空间最小，方向语义同样成立。
#[test]
fn minimum_size_keeps_down_downwards() {
    let (_fixture, mut app) = test_app(MINIMUM);
    idle(&mut app);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    let start_y = rect(&app).y;
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    assert_eq!(rect(&app).y, start_y);
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app).as_deref(), Some("每日目标 -"));
    assert!(rect(&app).y < start_y);
}

/// 同一带内 ←/→ 移动；宽屏最左再按 ← 才进导航栏。
#[test]
fn left_and_right_move_inside_the_row() {
    let (_fixture, mut app) = test_app(WIDE);
    idle(&mut app);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("自由练习"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("自由练习"), "行尾继续 → 停住");
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app).as_deref(), Some("单词"));
}

/// 跨栏会记住内容区上一次的选择：← 进导航、→ 回来仍是原来的按钮。
#[test]
fn returning_from_nav_restores_the_content_selection() {
    let (_fixture, mut app) = test_app(WIDE);
    idle(&mut app);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("词库页"));
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    press(&mut app, KeyCode::Right);
    assert_eq!(focus::pane(&app), Some(Pane::Content));
    assert_eq!(label(&app).as_deref(), Some("词库页"));
}

/// 宽屏侧栏：↑/↓ 走导航项（含「退出」），→ 回内容区。
#[test]
fn wide_nav_walks_every_item_including_quit() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app).as_deref(), Some("单词"));
    let expected = ["文章", "词库", "统计", "帮助", "设置", "退出"];
    for label_text in expected {
        press(&mut app, KeyCode::Down);
        assert_eq!(label(&app).as_deref(), Some(label_text), "宽屏侧栏 ↓ 顺序");
    }
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("退出"), "最后一项继续 ↓ 停住");
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app).as_deref(), Some("设置"));

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
    assert_eq!(label(&app).as_deref(), Some("单词"));
    app.toasts.clear();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Home);
    assert!(!app.toasts.is_empty(), "Enter 在当前页入口上应给出提示");
    assert_eq!(label(&app).as_deref(), Some("单词"), "焦点应保持");
}

/// 窄屏顶栏是横向的：←/→ 选标签，↓ 进内容区；内容区最顶带再 ↑ 才回顶栏。
#[test]
fn narrow_topbar_is_horizontal_and_down_enters_content() {
    let (_fixture, mut app) = test_app(NARROW);
    idle(&mut app);
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app).as_deref(), Some("单词"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("文章"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("词库"));
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app).as_deref(), Some("文章"));

    press(&mut app, KeyCode::Down);
    assert_eq!(focus::pane(&app), Some(Pane::Content));
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app).as_deref(), Some("每日目标 -"));
    press(&mut app, KeyCode::Up);
    assert_eq!(label(&app).as_deref(), Some("练习模式"));
    press(&mut app, KeyCode::Up);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app).as_deref(), Some("单词"));
}

/// 窄屏顶栏到边停住，不回头回绕。
#[test]
fn narrow_topbar_stops_at_the_edges() {
    let (_fixture, mut app) = test_app(NARROW);
    idle(&mut app);
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app).as_deref(), Some("单词"));
    press(&mut app, KeyCode::Left);
    assert_eq!(label(&app).as_deref(), Some("单词"));
    for _ in 0..5 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(label(&app).as_deref(), Some("设置"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("设置"));
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
    assert_eq!(label(&app).as_deref(), Some("每日目标 -"));
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
    assert_eq!(label(&app).as_deref(), Some("每日目标 -"));
    assert_eq!(after, before.saturating_sub(5).max(1));
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

/// 弹层打开时两栏都不画焦点（导航栏与内容区一致）。
#[test]
fn modal_hides_focus_on_both_panes() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Char('m'));
    assert!(app.mode_picker.is_some());
    assert!(focus::focused_rect(&app, app.viewport).is_none());
    assert!(focus::focused_label(&app, app.viewport).is_none());
    press(&mut app, KeyCode::Esc);

    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Esc);
    set_content(&mut app, ContentId::Start);
    press(&mut app, KeyCode::Char('m'));
    assert!(app.mode_picker.is_some());
    assert!(focus::focused_rect(&app, app.viewport).is_none());
}

/// 终端低于 60×20 时按键被忽略，不能盲操作看不见的主页。
#[test]
fn keys_are_ignored_below_the_minimum_terminal_size() {
    let (_fixture, mut app) = test_app((59, 19));
    let before = app.focus;
    press(&mut app, KeyCode::Down);
    assert_eq!(app.focus, before, "太小终端不应移动焦点");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Home);
    assert!(app.session.is_none());
    press(&mut app, KeyCode::Char('q'));
    assert!(app.confirm.is_none());
}

/// 主页就选中主按钮：底栏直接显示「选中 X / ↑↓←→ 移动 / enter 触发 / esc 取消」。
#[test]
fn footer_shows_the_selected_label_and_move_hint() {
    let (_fixture, app) = test_app(WIDE);
    let footer = last_line(&app, WIDE);
    assert!(footer.contains("选中"), "底栏应显示选中：{footer:?}");
    assert!(footer.contains("开始学习"));
    assert!(footer.contains("↑↓←→"));
    assert!(footer.contains("移动"));
    assert!(footer.contains("enter"));
    assert!(footer.contains("esc"));

    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("自由练习"));
    assert!(last_line(&app, WIDE).contains("自由练习"));
}

/// Esc 清掉默认选中，底栏恢复常规提示；此时方向键回到「无焦点」规则。
#[test]
fn escape_clears_focus_and_restores_the_regular_footer() {
    let (_fixture, mut app) = test_app(WIDE);
    assert!(app.focus.is_some());
    press(&mut app, KeyCode::Esc);
    assert!(app.focus.is_none());
    assert!(app.last_content_focus.is_none());
    let regular = last_line(&app, WIDE);
    assert!(!regular.contains("选中"));

    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    press(&mut app, KeyCode::Esc);
    assert_eq!(last_line(&app, WIDE), regular);
}

/// 焦点高亮 = 整块 accent + BOLD，且不溢出目标矩形。
#[test]
fn focus_highlight_fills_the_whole_target() {
    let (_fixture, mut app) = test_app(WIDE);
    let theme = app.theme();
    let plus = focus::content_target(&app, app.viewport, ContentId::DailyPlus).expect("目标");
    set_content(&mut app, ContentId::DailyPlus);
    let buffer = buffer_of(&app, WIDE);
    for x in plus.rect.x..plus.rect.right() {
        let cell = buffer.cell((x, plus.rect.y)).expect("cell");
        assert_eq!(cell.bg, theme.accent);
        assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
    }
    let right_of = buffer.cell((plus.rect.right(), plus.rect.y)).expect("cell");
    assert_ne!(right_of.bg, theme.accent);
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

/// 数字键切页后：焦点交给新页面的内容区，主页的「上次内容选择」清空。
#[test]
fn switching_screens_hands_focus_to_the_new_page() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("自由练习"));
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.screen, Screen::DictPicker);
    assert_eq!(app.focus, Some(FocusId::Page));
    assert!(app.last_content_focus.is_none());
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
    assert_eq!(label(&app).as_deref(), Some("练习模式"));
    press(&mut app, KeyCode::Enter);
    assert!(app.mode_picker.is_some());
}

/// 宽屏内容区最左项：← 就是进导航，不回绕。
#[test]
fn left_from_the_first_content_band_enters_nav_not_wraps() {
    let (_fixture, mut app) = test_app(WIDE);
    set_content(&mut app, ContentId::Start);
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app).as_deref(), Some("单词"));
    assert!(!shell::layout(app.viewport, Nav::Home).narrow);
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
    assert_eq!(label(&app).as_deref(), Some("每日目标 -"));
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

/// 焦点在主按钮上时必须有可见变化：常态 element 底，选中 accent 底。
#[test]
fn focus_on_the_primary_button_is_visible() {
    let (_fixture, mut app) = test_app(WIDE);
    let theme = app.theme();
    let start = focus::content_target(&app, app.viewport, ContentId::Start).expect("目标");
    press(&mut app, KeyCode::Esc);
    let normal = buffer_of(&app, WIDE);
    let cell = normal.cell((start.rect.x + 1, start.rect.y)).expect("cell");
    assert_eq!(cell.bg, theme.element);

    set_content(&mut app, ContentId::Start);
    let focused = buffer_of(&app, WIDE);
    let cell = focused
        .cell((start.rect.x + 1, start.rect.y))
        .expect("cell");
    assert_eq!(cell.bg, theme.accent);
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
    // 切页后焦点交给词库页自己的内容区，而不是留一个失效的主页身份。
    assert_eq!(app.focus, Some(FocusId::Page));

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
    assert_eq!(label(&app).as_deref(), Some("每日目标 -"));
}

/// 导航栏 Enter 仍然切页，并把焦点交给新页面的内容区。
#[test]
fn enter_on_a_nav_item_navigates() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("文章"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Articles);
    assert_eq!(app.focus, Some(FocusId::Page));
}

/// 从其它页面回到主页：重新选中主按钮，而不是留下一个没有高亮的界面。
#[test]
fn returning_to_home_selects_the_primary_action() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Stats);
    assert_eq!(app.focus, Some(FocusId::Page), "统计页应选中自己的内容区");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
    assert!(last_line(&app, WIDE).contains("选中"));

    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.screen, Screen::DictPicker);
    assert_eq!(app.focus, Some(FocusId::Page));
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
}

/// 练习页 Ctrl+C 直接回主页（不经过 apply_action 的切屏分支）也要选中主按钮。
#[test]
fn leaving_practice_returns_with_the_primary_action_selected() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Practice);
    assert!(app.session.is_some());
    assert!(app.focus.is_none(), "进入练习应清焦点");

    press_ctrl(&mut app, KeyCode::Char('c'));
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(app.focus, Some(FocusId::Content(ContentId::Start)));
    let expected = typewords_tui::ui::home::primary_label(&app);
    assert_eq!(label(&app).as_deref(), Some(expected));
    assert!(app.session.is_none());
}

/// 结算弹层关闭时 screen 可能已经是 Home，摘要关闭要恢复默认选中。
#[test]
fn closing_a_summary_on_home_selects_the_primary_action() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Esc);
    assert!(app.focus.is_none());
    app.summary = Some(app::SummaryData::default());
    press(&mut app, KeyCode::Char('q'));
    assert!(app.summary.is_none());
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(label(&app).as_deref(), Some("开始学习"));
}

// ── 其它页面：同一套两栏焦点 ─────────────────────────────────────────────

/// 每个左侧标签页进入后都默认选中「内容区」，底栏显示选中项。
#[test]
fn every_tab_lands_with_its_content_selected() {
    let tabs = [
        (KeyCode::Char('1'), Screen::Home),
        (KeyCode::Char('2'), Screen::Articles),
        (KeyCode::Char('3'), Screen::DictPicker),
        (KeyCode::Char('4'), Screen::Stats),
        (KeyCode::Char('5'), Screen::Help),
        (KeyCode::Char('6'), Screen::Settings),
    ];
    for (key, screen) in tabs {
        let (_fixture, mut app) = test_app(WIDE);
        press(&mut app, key);
        assert_eq!(app.screen, screen);
        assert_eq!(
            focus::pane(&app),
            Some(Pane::Content),
            "{screen:?} 应选中内容区"
        );
        assert!(app.focus.is_some(), "{screen:?} 应有焦点");
    }
}

/// Tab / Shift+Tab 在所有页面都能在导航栏与内容区之间切换。
#[test]
fn tab_switches_panes_on_every_page() {
    for key in [
        KeyCode::Char('2'),
        KeyCode::Char('3'),
        KeyCode::Char('4'),
        KeyCode::Char('5'),
        KeyCode::Char('6'),
    ] {
        let (_fixture, mut app) = test_app(WIDE);
        press(&mut app, key);
        assert_eq!(focus::pane(&app), Some(Pane::Content));
        press(&mut app, KeyCode::Tab);
        assert_eq!(focus::pane(&app), Some(Pane::Nav), "Tab 应进导航栏");
        assert_eq!(
            label(&app).as_deref(),
            Some(Nav::from_screen(app.screen).label()),
            "导航栏落点应是当前页入口"
        );
        press(&mut app, KeyCode::Tab);
        assert_eq!(focus::pane(&app), Some(Pane::Content), "再 Tab 回内容区");
    }
}

/// 宽屏内容区左边缘按 ← 进侧栏，Enter 可以继续切页。
#[test]
fn wide_left_from_page_content_enters_sidebar() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('3'));
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app).as_deref(), Some("词库"));
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("统计"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Stats);
    assert_eq!(app.focus, Some(FocusId::Page));
}

/// 窄屏内容区最顶部再按 ↑ 回顶栏；↓ 回内容区。
#[test]
fn narrow_up_from_page_content_enters_topbar() {
    let (_fixture, mut app) = test_app(NARROW);
    press(&mut app, KeyCode::Char('3'));
    press(&mut app, KeyCode::Up);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app).as_deref(), Some("词库"));
    press(&mut app, KeyCode::Down);
    assert_eq!(focus::pane(&app), Some(Pane::Content));
    assert_eq!(app.focus, Some(FocusId::Page));
}

/// 词库页：进入后底栏显示当前词库名；↑/↓ 仍走词库光标（只读安全）。
#[test]
fn dict_picker_shows_the_current_dict_label() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.focus, Some(FocusId::Page));
    let name = app.boot.catalog.resources()[0].name.clone();
    assert_eq!(label(&app).as_deref(), Some(name.as_str()));
    assert!(last_line(&app, WIDE).contains(name.as_str()));
    press(&mut app, KeyCode::Down);
    // 夹具只有一本词库：到边停住，不越界，也不跨栏。
    assert_eq!(app.dict_cursor, 0);
    assert_eq!(focus::pane(&app), Some(Pane::Content));
}

/// 设置页：↑/↓ 移动设置光标，Enter 触发该项。
#[test]
fn settings_arrows_move_the_cursor_and_enter_activates() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.focus, Some(FocusId::Page));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.settings_cursor, 1);
    let text = typewords_tui::domain::i18n::tr(app::SETTING_ROWS[1].0);
    assert_eq!(label(&app).as_deref(), Some(text.as_str()));
    // 第 0 行是练习模式：回到行首后 Enter 打开模式弹层。
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Enter);
    assert!(app.mode_picker.is_some());
}

/// 统计页：↑/↓ 滚动正文，←/→ 选日期，Enter 切换视图。
#[test]
fn stats_arrows_scroll_and_pick_dates() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(focus::pane(&app), Some(Pane::Content));
    // 日历里选中的那一天同样有整块高亮。
    let theme = app.theme();
    let day_rect = focus::focused_rect(&app, app.viewport).expect("日历选中日应有高亮矩形");
    let buffer = buffer_of(&app, WIDE);
    assert_eq!(
        buffer.cell((day_rect.x, day_rect.y)).expect("cell").bg,
        theme.accent,
        "日历选中日应整块 accent 高亮"
    );
    let max_scroll = typewords_tui::ui::stats::body_line_count(&app).saturating_sub(1) as u16;
    assert!(max_scroll >= 1);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.stats_scroll, 1.min(max_scroll));

    let (year, month) = app.stats_month;
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("month start");
    app.select_stats_day(first);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.stats_day.day(), 2, "→ 应选下一天");
    press(&mut app, KeyCode::Left);
    assert_eq!(app.stats_day.day(), 1, "← 应回上一天");
    press(&mut app, KeyCode::Left);
    assert_eq!(focus::pane(&app), Some(Pane::Nav), "月初再 ← 应进侧栏");
    assert_eq!(label(&app).as_deref(), Some("统计"));

    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('4'));
    assert!(!app.stats_by_dict);
    press(&mut app, KeyCode::Enter);
    assert!(app.stats_by_dict, "统计页 Enter 应切换视图");
}

/// 帮助页：内容区方向键滚动，Tab 进导航栏；Enter 返回上一层。
#[test]
fn help_page_scrolls_and_returns() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);
    assert_eq!(app.focus, Some(FocusId::Page));
    assert_eq!(label(&app).as_deref(), Some("帮助"));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.help_scroll, 1);
    press(&mut app, KeyCode::Tab);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    press(&mut app, KeyCode::Tab);
    assert_eq!(focus::pane(&app), Some(Pane::Content));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Home);
    assert_eq!(app.focus, Some(FocusId::Content(ContentId::Start)));
}

/// Esc 在其它页面仍是返回上一层，并回到主页默认选中。
#[test]
fn escape_on_a_tab_returns_home() {
    for key in [
        KeyCode::Char('2'),
        KeyCode::Char('3'),
        KeyCode::Char('4'),
        KeyCode::Char('6'),
    ] {
        let (_fixture, mut app) = test_app(WIDE);
        press(&mut app, key);
        assert_ne!(app.screen, Screen::Home);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.focus, Some(FocusId::Content(ContentId::Start)));
    }
}

/// 窄屏顶栏横向选择后 Enter 切页，落点仍是新页面内容区。
#[test]
fn narrow_topbar_enter_switches_pages() {
    let (_fixture, mut app) = test_app(NARROW);
    press(&mut app, KeyCode::Char('4'));
    press(&mut app, KeyCode::Tab);
    assert_eq!(label(&app).as_deref(), Some("统计"));
    press(&mut app, KeyCode::Right);
    assert_eq!(label(&app).as_deref(), Some("帮助"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Help);
    assert_eq!(app.focus, Some(FocusId::Page));
}

/// 单词测试页是专用答题页：不参与两栏焦点，方向键不改变页面。
#[test]
fn word_test_page_keeps_its_own_keys() {
    let (_fixture, mut app) = test_app(WIDE);
    app.boot.settings.word_practice_mode = WordPracticeMode::ShuffleWordsTest;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::WordTest);
    assert!(app.focus.is_none());
    press(&mut app, KeyCode::Down);
    assert_eq!(app.screen, Screen::WordTest);
    assert!(app.focus.is_none());
}

/// 鼠标点内容区会把焦点交给页面内容区；点导航栏则同步到导航栏。
#[test]
fn mouse_click_on_page_syncs_focus() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('s'));
    press(&mut app, KeyCode::Tab);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));

    let shell = shell::layout(app.viewport, Nav::Settings);
    let content = shell.content;
    // 第二行是「主题」：点未选中的行只移动光标，不会打开弹层。
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: content.x + 2,
            row: content.y + 2,
            modifiers: KeyModifiers::NONE,
        },
        app.viewport,
    );
    assert_eq!(app.focus, Some(FocusId::Page));
    assert!(app.mode_picker.is_none(), "点未选中行不应打开弹层");

    let nav = focus::nav_targets(&app, app.viewport)
        .into_iter()
        .find(|target| target.focus == Some(FocusId::Nav(NavId::Settings)))
        .expect("应有「设置」导航目标");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: nav.rect.x + 1,
            row: nav.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        app.viewport,
    );
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
}

/// 非主页的页面内容区：当前选中行同样用整块 accent 高亮；焦点进导航栏后
/// 页面行让位给导航项，回到内容区再恢复。
#[test]
fn page_focus_highlights_the_selected_row() {
    for key in [KeyCode::Char('3'), KeyCode::Char('s')] {
        let (_fixture, mut app) = test_app(WIDE);
        press(&mut app, key);
        let theme = app.theme();
        let rect = focus::focused_rect(&app, app.viewport).expect("页面选中行应有高亮矩形");
        let buffer = buffer_of(&app, WIDE);
        let mut previous_width = 1usize;
        for x in rect.x..rect.right() {
            if previous_width == 2 {
                // CJK 宽字符的续接单元由前一个宽字形覆盖，测试后端保留默认样式。
                previous_width = 1;
                continue;
            }
            let cell = buffer.cell((x, rect.y)).expect("cell");
            previous_width = UnicodeWidthStr::width(cell.symbol());
            assert_eq!(
                cell.bg, theme.accent,
                "{key:?} 页面选中行应整块 accent 高亮（x={x}）"
            );
        }

        press(&mut app, KeyCode::Tab);
        assert_eq!(focus::pane(&app), Some(Pane::Nav));
        let buffer = buffer_of(&app, WIDE);
        let cell = buffer.cell((rect.x + 1, rect.y)).expect("cell");
        assert_ne!(
            cell.bg, theme.accent,
            "{key:?} 导航栏聚焦时页面行不应继续整块高亮"
        );
    }
}

/// 帮助页的「任意键返回」仍然优先：不在练习中时按数字键也返回，不切页。
#[test]
fn help_page_returns_on_digit_keys_too() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Home, "帮助页数字键应返回，不应切到统计");
}

/// 设置页的空格与 Enter 同义：跟着焦点走，不会隔着焦点去改下面的设置项。
#[test]
fn space_on_settings_follows_the_focused_pane() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('s'));
    press(&mut app, KeyCode::Char(' '));
    assert!(
        app.mode_picker.is_some(),
        "内容区焦点 + 空格应打开当前设置项"
    );
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Tab);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    press(&mut app, KeyCode::Down);
    assert_eq!(label(&app).as_deref(), Some("退出"));
    press(&mut app, KeyCode::Char(' '));
    assert!(app.confirm.is_some(), "导航栏焦点 + 空格应触发导航项");
}

/// 词条搜索框获得焦点时，方向键属于输入框，不移动焦点导航。
#[test]
fn searching_words_disables_focus_navigation() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, Screen::Words);
    press(&mut app, KeyCode::Char('/'));
    assert!(app.words_search_focus);
    assert!(focus::pane(&app).is_none(), "搜索时不应有焦点导航");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.words_cursor, 0, "搜索时方向键不改光标");
}

/// 子页面（词条管理）也共用同一套：进页选中内容区，Tab / ← 进导航。
#[test]
fn subpages_share_the_same_focus_model() {
    let (_fixture, mut app) = test_app(WIDE);
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, Screen::Words);
    assert_eq!(app.focus, Some(FocusId::Page));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.words_cursor, 1);
    press(&mut app, KeyCode::Tab);
    assert_eq!(focus::pane(&app), Some(Pane::Nav));
    assert_eq!(label(&app).as_deref(), Some("词库"));
}
