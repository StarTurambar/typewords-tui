//! 主页方向键焦点导航的集成测试。
//!
//! 交互模型：主页页内的可点元素按固定巡览顺序排成一个环——开始学习 →
//! 自由练习 → 词库入口 → 每日目标 → 练习模式。↓ / → 走到下一项、↑ / ←
//! 走到上一项，到头回绕（没有死键）；Enter 触发当前项，Esc 取消。
//! 焦点用「selection 底色 + 左侧 accent 强调条」高亮（不再反显），底栏同步
//! 显示「选中 X」与操作提示。
//!
//! 所有用例都先 `app.set_viewport(...)` 再按键，渲染用同一个尺寸——焦点
//! 几何由布局算出，按键路径不经过渲染。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use typewords_tui::app::{self, Screen};
use typewords_tui::domain::types::WordPracticeMode;
use typewords_tui::ui::Theme;
use typewords_tui::ui::focus;
use typewords_tui::ui::shell::{self, Nav};
use unicode_width::UnicodeWidthStr;

const WIDE_WIDTH: u16 = 120;
const WIDE_HEIGHT: u16 = 40;
/// 宽屏（>= 120 列）侧栏 26 列 + 正文内边距 2 列：焦点矩形不应出现在此列之前。
const WIDE_CONTENT_X: u16 = 28;
const NARROW_WIDTH: u16 = 80;
const NARROW_HEIGHT: u16 = 24;
const MINIMUM_WIDTH: u16 = 60;
const MINIMUM_HEIGHT: u16 = 20;

fn wide_area() -> Rect {
    Rect::new(0, 0, WIDE_WIDTH, WIDE_HEIGHT)
}

/// 建一个已经同步好视口的测试应用（`Fixture` 必须活得比 `App` 久，一起返回）。
fn test_app(width: u16, height: u16) -> (common::Fixture, app::App) {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("build app");
    app.set_viewport(Rect::new(0, 0, width, height));
    (fixture, app)
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn focused_rect(app: &app::App, area: Rect) -> Rect {
    focus::focused_rect(app, area).expect("当前应有主页焦点")
}

/// 主页可聚焦目标里的 label 顺序（也就是方向键的巡览顺序）。
fn target_labels(app: &app::App, area: Rect) -> Vec<&'static str> {
    focus::targets(app, area)
        .iter()
        .map(|target| target.label.expect("可聚焦的主页目标都应带 label"))
        .collect()
}

/// 某个 label 在 `ui::focus::targets` 里的下标。
fn label_index(app: &app::App, area: Rect, label: &str) -> usize {
    focus::targets(app, area)
        .iter()
        .position(|target| target.label == Some(label))
        .unwrap_or_else(|| panic!("主页目标里应有「{label}」"))
}

fn render_terminal(app: &app::App, width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    terminal
}

/// 把一帧渲染成文本（宽字符按显示宽度跳过尾随格）。
fn render_text(app: &app::App, width: u16, height: u16) -> String {
    let terminal = render_terminal(app, width, height);
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut skip = 0usize;
        for x in 0..width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
                let display_width = UnicodeWidthStr::width(cell.symbol());
                if display_width > 1 {
                    skip = display_width - 1;
                }
            }
        }
        out.push('\n');
    }
    out
}

/// 底栏（最后一行）的文本。
fn last_line(app: &app::App, width: u16, height: u16) -> String {
    render_text(app, width, height)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
}

/// 全屏所有带反显（`Modifier::REVERSED`）的单元格。
fn reversed_cells(buffer: &Buffer, width: u16, height: u16) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = buffer.cell((x, y))
                && cell.modifier.contains(Modifier::REVERSED)
            {
                cells.push((x, y));
            }
        }
    }
    cells
}

/// 焦点矩形左侧的 accent 强调条（优先自身第 1 列，否则左邻 1 列）。
fn accent_bar(buffer: &Buffer, rect: Rect, theme: &Theme) -> Option<(u16, u16)> {
    for column in [rect.x, rect.x.saturating_sub(1)] {
        for y in rect.y..rect.bottom() {
            if let Some(cell) = buffer.cell((column, y))
                && cell.bg == theme.accent
            {
                return Some((column, y));
            }
        }
    }
    None
}

/// 该列是否是宽字符的尾随格。
///
/// ratatui 的 diff 不更新「只改样式」的宽字符尾随格（终端上字形按首格样式
/// 整块绘制），所以 TestBackend 的 buffer 里它们会停留在默认样式。
fn is_wide_tail(buffer: &Buffer, x: u16, y: u16) -> bool {
    x > 0
        && buffer
            .cell((x - 1, y))
            .map(|cell| UnicodeWidthStr::width(cell.symbol()) > 1)
            .unwrap_or(false)
}

/// 首次按 ↓ / ↑ 分别选中第一项 / 最后一项（菜单式的进入方式）。
#[test]
fn the_first_down_selects_the_first_item_and_the_first_up_selects_the_last() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();
    let labels = target_labels(&app, area);
    assert!(labels.len() >= 3, "主页至少应有三项可聚焦目标：{labels:?}");
    assert_eq!(labels[0], "开始学习", "第一项应是「开始学习」");

    press(&mut app, KeyCode::Down);
    assert_eq!(app.focus, Some(0), "首次按 ↓ 应选中第一项");
    assert_eq!(focus::focused_label(&app, area), Some(labels[0]));
    assert_eq!(focused_rect(&app, area), focus::targets(&app, area)[0].rect);

    press(&mut app, KeyCode::Esc);
    assert!(app.focus.is_none(), "Esc 应清除焦点");

    press(&mut app, KeyCode::Up);
    assert_eq!(app.focus, Some(labels.len() - 1), "首次按 ↑ 应选中最后一项");
    assert_eq!(
        focus::focused_label(&app, area),
        Some(labels[labels.len() - 1])
    );
}

/// 连续 ↓ 按 targets() 的顺序逐项前进并在末尾回绕；↑ 反向逐项并回绕。
#[test]
fn down_tours_every_target_in_order_and_wraps_at_both_ends() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();
    let labels = target_labels(&app, area);
    let len = labels.len();
    assert!(len >= 3, "主页至少应有三项可聚焦目标：{labels:?}");

    press(&mut app, KeyCode::Down);
    assert_eq!(app.focus, Some(0));
    let mut index = 0usize;
    for step in 1..=len {
        press(&mut app, KeyCode::Down);
        index = (index + 1) % len;
        assert_eq!(app.focus, Some(index), "第 {step} 次 ↓ 应走到第 {index} 项");
        assert_eq!(
            focus::focused_label(&app, area),
            Some(labels[index]),
            "第 {step} 次 ↓ 的 label 应与 targets() 顺序一致"
        );
    }
    assert_eq!(app.focus, Some(0), "连按 {len} 次 ↓ 应回绕到第一项");

    // 第一项再按 ↑：回绕到最后一项（无死键）。
    press(&mut app, KeyCode::Up);
    assert_eq!(app.focus, Some(len - 1), "第一项再按 ↑ 应回绕到最后一项");
    index = len - 1;
    for step in 1..=len {
        press(&mut app, KeyCode::Up);
        index = (index + len - 1) % len;
        assert_eq!(app.focus, Some(index), "第 {step} 次 ↑ 应走到第 {index} 项");
        assert_eq!(
            focus::focused_label(&app, area),
            Some(labels[index]),
            "第 {step} 次 ↑ 的 label 应与 targets() 顺序反向一致"
        );
    }
}

/// → 等价于下一项，← 等价于上一项（同样到头回绕）。
#[test]
fn right_and_left_step_like_down_and_up() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();
    let labels = target_labels(&app, area);
    let len = labels.len();

    press(&mut app, KeyCode::Right);
    assert_eq!(app.focus, Some(0), "首次按 → 应像 ↓ 一样选中第一项");
    assert_eq!(focus::focused_label(&app, area), Some(labels[0]));

    press(&mut app, KeyCode::Right);
    assert_eq!(app.focus, Some(1), "→ 应前进一项");
    assert_eq!(focus::focused_label(&app, area), Some(labels[1]));

    press(&mut app, KeyCode::Left);
    assert_eq!(app.focus, Some(0), "← 应后退一项");

    press(&mut app, KeyCode::Left);
    assert_eq!(
        app.focus,
        Some(len - 1),
        "← 在第一项应回绕到最后一项（像 ↑ 一样）"
    );
    assert_eq!(focus::focused_label(&app, area), Some(labels[len - 1]));

    press(&mut app, KeyCode::Right);
    assert_eq!(app.focus, Some(0), "→ 在最后一项应回绕到第一项");
}

/// 焦点在「开始学习」上按 Enter：进入练习页。
#[test]
fn enter_on_the_start_button_enters_the_practice_screen() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();
    let index = label_index(&app, area, "开始学习");
    app.focus = Some(index);

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.screen, Screen::Practice, "应进入练习页");
    assert!(app.session.is_some(), "应创建练习会话");
}

/// 焦点在「自由练习」上按 Enter：进入练习页，且本轮是自由模式。
#[test]
fn enter_on_the_free_practice_button_starts_a_free_session() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();
    let index = label_index(&app, area, "自由练习");
    app.focus = Some(index);

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.screen, Screen::Practice, "应进入练习页");
    let session = app.session.as_ref().expect("应创建练习会话");
    assert_eq!(session.mode, WordPracticeMode::Free, "应是自由练习模式");
}

/// 三个词库入口按 Enter 都切到词库页，并清空焦点。
#[test]
fn enter_on_each_dict_entry_opens_the_dict_picker_and_clears_the_focus() {
    for label in ["词库页", "管理词典", "更多词库"] {
        let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
        let area = wide_area();
        let index = label_index(&app, area, label);
        app.focus = Some(index);

        press(&mut app, KeyCode::Enter);

        assert_eq!(app.screen, Screen::DictPicker, "「{label}」应切到词库页");
        assert!(app.focus.is_none(), "「{label}」切页后焦点应清空");
    }
}

/// 每日目标 -/+ 按 Enter：词数按 clamp 变化，焦点保持。
#[test]
fn enter_on_the_daily_goal_buttons_clamps_the_target_and_keeps_the_focus() {
    for (label, delta, start) in [("每日目标 -", -5i64, 7usize), ("每日目标 +", 5i64, 2usize)]
    {
        let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
        let area = wide_area();
        app.boot
            .base
            .study_dict_mut()
            .expect("夹具应有学习词典")
            .per_day_study_number = start;
        let cap = app
            .boot
            .base
            .study_dict()
            .expect("夹具应有学习词典")
            .effective_length()
            .max(1) as i64;
        let before = app
            .boot
            .base
            .study_dict()
            .expect("夹具应有学习词典")
            .per_day() as i64;
        let expected = (before + delta).clamp(1, cap);
        assert_ne!(before, expected, "夹具应让 {label} 真的改变数值");

        let index = label_index(&app, area, label);
        app.focus = Some(index);
        press(&mut app, KeyCode::Enter);

        let after = app
            .boot
            .base
            .study_dict()
            .expect("夹具应有学习词典")
            .per_day() as i64;
        assert_eq!(after, expected, "「{label}」应把每日词数改成 clamp 后的值");
        assert_eq!(app.focus, Some(index), "「{label}」触发后焦点应保持");
        assert_eq!(app.screen, Screen::Home, "「{label}」不应离开主页");
    }
}

/// 焦点在「练习模式」上按 Enter：打开模式弹层，焦点保持。
#[test]
fn enter_on_the_mode_label_opens_the_mode_picker_and_keeps_the_focus() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();
    let index = label_index(&app, area, "练习模式");
    app.focus = Some(index);

    press(&mut app, KeyCode::Enter);

    assert!(app.mode_picker.is_some(), "应打开模式选择弹层");
    assert_eq!(app.focus, Some(index), "打开弹层后焦点应保持");
    assert_eq!(app.screen, Screen::Home);
}

/// Esc 清除焦点，底栏恢复常规快捷键提示。
#[test]
fn escape_clears_the_focus_and_restores_the_regular_footer() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let regular_footer = last_line(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        !regular_footer.contains("选中"),
        "无焦点时底栏不应出现「选中」：{regular_footer:?}"
    );

    press(&mut app, KeyCode::Down);
    let focused_footer = last_line(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        focused_footer.contains("选中"),
        "有焦点时底栏应显示「选中」：{focused_footer:?}"
    );

    press(&mut app, KeyCode::Esc);
    assert!(app.focus.is_none(), "Esc 应清除焦点");
    assert_eq!(app.screen, Screen::Home, "Esc 不应离开主页");
    let footer_after = last_line(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        !footer_after.contains("选中"),
        "Esc 后底栏应恢复常规提示：{footer_after:?}"
    );
    assert_eq!(footer_after, regular_footer, "底栏应恢复成无焦点时的样子");
}

/// 底栏有焦点时显示「选中 {label}」与焦点操作键。
#[test]
fn the_footer_shows_the_selected_label_and_the_focus_keys() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();

    for label in ["开始学习", "自由练习"] {
        let index = label_index(&app, area, label);
        app.focus = Some(index);
        let footer = last_line(&app, WIDE_WIDTH, WIDE_HEIGHT);
        assert!(footer.contains("选中"), "底栏应显示「选中」：{footer:?}");
        assert!(
            footer.contains(label),
            "底栏应显示当前 label「{label}」：{footer:?}"
        );
        assert!(footer.contains("↑↓←→"), "底栏应提示方向键：{footer:?}");
        assert!(footer.contains("enter"), "底栏应提示 enter：{footer:?}");
        assert!(footer.contains("esc"), "底栏应提示 esc：{footer:?}");
    }
}

/// 焦点高亮 = selection 底色 + BOLD + 左侧 accent 强调条（不再用反显）。
#[test]
fn the_focus_highlight_uses_a_selection_background_and_an_accent_bar() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();
    let theme = app.theme();

    // 无焦点：全屏没有反显。
    let terminal = render_terminal(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        reversed_cells(terminal.backend().buffer(), WIDE_WIDTH, WIDE_HEIGHT).is_empty(),
        "无焦点时全屏不应有反显单元格"
    );

    // 「每日目标 +」的文字全是 ASCII：高亮与强调条可以逐格核对。
    let plus_index = label_index(&app, area, "每日目标 +");
    let plus_rect = focus::targets(&app, area)[plus_index].rect;
    app.focus = Some(plus_index);
    let terminal = render_terminal(&app, WIDE_WIDTH, WIDE_HEIGHT);
    let buffer = terminal.backend().buffer();
    for x in plus_rect.x..plus_rect.right() {
        let cell = buffer.cell((x, plus_rect.y)).expect("cell");
        assert_eq!(
            cell.bg, theme.selection,
            "焦点矩形 ({x}, {}) 应有 selection 底色",
            plus_rect.y
        );
        assert!(
            cell.modifier.contains(Modifier::BOLD),
            "焦点矩形 ({x}, {}) 应加粗",
            plus_rect.y
        );
    }
    let plus_bar = accent_bar(buffer, plus_rect, &theme).expect("焦点矩形旁应有 accent 强调条");
    assert!(
        plus_bar.0 == plus_rect.x || plus_bar.0 + 1 == plus_rect.x,
        "强调条应贴着焦点矩形：{plus_bar:?} vs {plus_rect:?}"
    );
    assert_eq!(buffer.cell(plus_bar).expect("bar cell").bg, theme.accent);
    assert!(
        reversed_cells(buffer, WIDE_WIDTH, WIDE_HEIGHT).is_empty(),
        "焦点高亮不应再使用 REVERSED"
    );
    let right_of_rect = buffer.cell((plus_rect.right(), plus_rect.y)).expect("cell");
    assert_ne!(
        right_of_rect.bg, theme.selection,
        "selection 底色不应溢出焦点矩形"
    );

    // 中文主按钮：宽字符尾随格在 buffer 里可能仍是默认样式，跳过它们。
    let start_index = label_index(&app, area, "开始学习");
    let start_rect = focus::targets(&app, area)[start_index].rect;
    app.focus = Some(start_index);
    let terminal = render_terminal(&app, WIDE_WIDTH, WIDE_HEIGHT);
    let buffer = terminal.backend().buffer();
    let start_bar =
        accent_bar(buffer, start_rect, &theme).expect("「开始学习」旁应有 accent 强调条");
    assert!(
        start_bar.0 == start_rect.x || start_bar.0 + 1 == start_rect.x,
        "强调条应贴着焦点矩形：{start_bar:?} vs {start_rect:?}"
    );
    let mut checked = 0usize;
    for y in start_rect.y..start_rect.bottom() {
        for x in start_rect.x..start_rect.right() {
            if is_wide_tail(buffer, x, y) {
                continue;
            }
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            if x == start_bar.0 {
                assert_eq!(cell.bg, theme.accent, "强调条 ({x}, {y}) 应有 accent 底色");
                continue;
            }
            assert_eq!(
                cell.bg, theme.selection,
                "焦点矩形内 ({x}, {y}) 应有 selection 底色"
            );
            assert!(
                cell.modifier.contains(Modifier::BOLD),
                "焦点矩形内 ({x}, {y}) 应加粗"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "「开始学习」矩形内应有可核对的高亮单元格");
    assert!(
        reversed_cells(buffer, WIDE_WIDTH, WIDE_HEIGHT).is_empty(),
        "焦点高亮不应再使用 REVERSED"
    );

    // Esc 之后：之前的选中底色与强调条都应消失。
    press(&mut app, KeyCode::Esc);
    assert!(app.focus.is_none(), "Esc 应清除焦点");
    let terminal = render_terminal(&app, WIDE_WIDTH, WIDE_HEIGHT);
    let buffer = terminal.backend().buffer();
    for x in plus_rect.x..plus_rect.right() {
        let cell = buffer.cell((x, plus_rect.y)).expect("cell");
        assert_ne!(
            cell.bg, theme.selection,
            "Esc 后 ({x}, {}) 不应再有选中底色",
            plus_rect.y
        );
    }
    assert_ne!(
        buffer.cell(plus_bar).expect("bar cell").bg,
        theme.accent,
        "Esc 后强调条应消失"
    );
    assert!(
        reversed_cells(buffer, WIDE_WIDTH, WIDE_HEIGHT).is_empty(),
        "Esc 后全屏不应有反显单元格"
    );
}

/// 120×40 / 80×24 / 60×20 下反复按四个方向键都不 panic，焦点始终在正文区。
#[test]
fn focus_navigation_stays_inside_the_content_area_on_every_terminal_size() {
    for (width, height) in [
        (WIDE_WIDTH, WIDE_HEIGHT),
        (NARROW_WIDTH, NARROW_HEIGHT),
        (MINIMUM_WIDTH, MINIMUM_HEIGHT),
    ] {
        let (_fixture, mut app) = test_app(width, height);
        let area = Rect::new(0, 0, width, height);
        let content_x = shell::layout(area, Nav::Home).content.x;
        if width == WIDE_WIDTH {
            assert_eq!(content_x, WIDE_CONTENT_X, "120 宽时正文应起于第 28 列");
        }
        let targets = focus::targets(&app, area);
        assert!(!targets.is_empty(), "{width}x{height} 主页应有可聚焦目标");

        let directions = [KeyCode::Down, KeyCode::Right, KeyCode::Up, KeyCode::Left];
        for (step, direction) in directions.iter().cycle().take(12).enumerate() {
            press(&mut app, *direction);
            let rect = focus::focused_rect(&app, area)
                .unwrap_or_else(|| panic!("{width}x{height} 下第 {} 次按键后应有焦点", step + 1));
            assert!(
                rect.width > 0 && rect.height > 0,
                "{width}x{height} 下焦点矩形不应为空：{rect:?}"
            );
            assert!(
                rect.x >= content_x,
                "{width}x{height} 下焦点落进了侧栏 / 顶栏：{rect:?}（正文起于第 {content_x} 列）"
            );
            assert!(
                rect.right() <= area.right() && rect.y >= area.y && rect.bottom() <= area.bottom(),
                "{width}x{height} 下焦点越出了视口：{rect:?}"
            );
        }
    }
}

/// 其它页面的方向键语义不变：统计页上下键仍滚动画布，不产生焦点。
#[test]
fn arrow_keys_on_the_stats_page_still_scroll_instead_of_moving_focus() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    let area = wide_area();

    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Stats, "数字键 4 应切到统计页");
    assert!(
        focus::targets(&app, area).is_empty(),
        "非主页不应有可聚焦目标"
    );
    assert!(app.focus.is_none(), "统计页不应出现主页焦点");

    let max_scroll = typewords_tui::ui::stats::body_line_count(&app).saturating_sub(1) as u16;
    assert!(max_scroll >= 1, "统计页正文应至少有可滚动的一行");

    press(&mut app, KeyCode::Down);
    assert_eq!(
        app.stats_scroll,
        1.min(max_scroll),
        "下方向键应滚动统计页，而不是移动焦点"
    );
    assert!(app.focus.is_none(), "统计页滚动不应产生主页焦点");

    press(&mut app, KeyCode::Up);
    assert_eq!(app.stats_scroll, 0, "上方向键应把统计页滚回顶部");
    assert!(app.focus.is_none());
}

/// 用数字键切页同样清空焦点（离开主页后旧下标就没有意义了）。
#[test]
fn switching_screens_clears_the_focus() {
    let (_fixture, mut app) = test_app(WIDE_WIDTH, WIDE_HEIGHT);
    press(&mut app, KeyCode::Down);
    assert!(app.focus.is_some(), "按方向键后应有焦点");

    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.screen, Screen::DictPicker, "数字键 3 应切到词库页");
    assert!(app.focus.is_none(), "切换页面后焦点应清空");
}
