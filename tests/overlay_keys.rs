//! 弹层按键回归测试。
//!
//! 确认弹层（退出 / 清空练习进度 / 清除所有数据）与结算弹层的按钮原来只能
//! 用固定快捷键：确认弹层「y/Enter 确认、其它任意键取消」，结算弹层
//! 「Enter 继续、r 重学、q 主页」。方向键在两个弹层里都没有作用，确认弹层里
//! 按方向键甚至会直接取消。
//!
//! 修复后两个弹层都能用方向键 / Tab / j/k 移动选中，Enter / 空格执行当前
//! 选中的按钮，y / n / Esc / r / q 旧快捷键保留。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Color;
use typewords_tui::app::{self, Action, ConfirmAction, ConfirmFocus, SummaryButton};
use typewords_tui::ui::overlays;
use unicode_width::UnicodeWidthStr;

/// 固定测试尺寸：足够宽，两个弹层都完整可见，且达到可交互下限。
fn viewport() -> Rect {
    Rect::new(0, 0, 100, 30)
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn new_app(fixture: &common::Fixture) -> app::App {
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("构建 App");
    app.set_viewport(viewport());
    app
}

/// 在主页按 q 打开退出确认弹层。
fn open_confirm(app: &mut app::App) {
    press(app, KeyCode::Char('q'));
    assert!(app.confirm.is_some(), "主页按 q 应打开退出确认弹层");
}

fn confirm_focus(app: &app::App) -> ConfirmFocus {
    app.confirm.as_ref().expect("确认弹层应打开").focus
}

fn summary_focus(app: &app::App) -> SummaryButton {
    app.summary.as_ref().expect("结算弹层应打开").focus
}

/// 按 key 定位设置行，避免依赖行号。
fn setting_row(key: &str) -> usize {
    app::SETTING_ROWS
        .iter()
        .position(|(row_key, _)| *row_key == key)
        .unwrap_or_else(|| panic!("设置页应有 {key} 行"))
}

fn buffer_of(app: &app::App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    terminal.backend().buffer().clone()
}

fn render_text(app: &app::App, width: u16, height: u16) -> String {
    let buffer = buffer_of(app, width, height);
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
                // 宽字符（CJK）占两个单元，跳过它的续接单元。
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

fn cell_bg(buffer: &Buffer, x: u16, y: u16) -> Color {
    buffer.cell((x, y)).expect("缓冲区单元格应存在").bg
}

/// 按钮文字居中，取矩形里第一个非空单元格的 (底, 字) 颜色。
fn label_cell(buffer: &Buffer, rect: Rect) -> (Color, Color) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            if let Some(cell) = buffer.cell((x, y))
                && cell.symbol() != " "
            {
                return (cell.bg, cell.fg);
            }
        }
    }
    panic!("按钮内应有文字");
}

// ── 确认弹层 ─────────────────────────────────────────────────────────────

#[test]
fn confirm_dialog_default_focus_is_accept() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    open_confirm(&mut app);

    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Accept,
        "确认弹层默认应选中「确认」"
    );
    press(&mut app, KeyCode::Enter);
    assert!(app.quit, "默认焦点在「确认」时 Enter 应执行退出");
    assert!(app.confirm.is_none(), "执行后应关闭弹层");
}

#[test]
fn confirm_dialog_arrows_move_between_buttons() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    open_confirm(&mut app);

    press(&mut app, KeyCode::Right);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Cancel,
        "→ 应选中「取消」"
    );
    press(&mut app, KeyCode::Left);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Accept,
        "← 应选中「确认」"
    );
    press(&mut app, KeyCode::Down);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Cancel,
        "↓ 应选中「取消」"
    );
    press(&mut app, KeyCode::Up);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Accept,
        "↑ 应选中「确认」"
    );
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Cancel,
        "Tab 应移到「取消」"
    );
    press(&mut app, KeyCode::BackTab);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Accept,
        "Shift+Tab 应移到「确认」"
    );
    // j/k 是全站通用的 ↓/↑ 别名，弹层里不能落进「其它任意键取消」。
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Cancel,
        "j 应移到「取消」"
    );
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Accept,
        "k 应移到「确认」"
    );

    assert!(app.confirm.is_some(), "移动焦点不应关闭弹层");
    assert!(!app.quit, "移动焦点不应执行退出");
}

#[test]
fn confirm_dialog_enter_cancels_when_cancel_is_selected() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    open_confirm(&mut app);

    press(&mut app, KeyCode::Right);
    assert_eq!(confirm_focus(&app), ConfirmFocus::Cancel);
    press(&mut app, KeyCode::Left);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Accept,
        "← 应能回到「确认」"
    );
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);

    assert!(app.confirm.is_none(), "焦点在「取消」时 Enter 应关闭弹层");
    assert!(!app.quit, "焦点在「取消」时 Enter 不应执行退出");
}

#[test]
fn confirm_dialog_focus_stops_at_both_ends() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    open_confirm(&mut app);

    for step in 0..3 {
        press(&mut app, KeyCode::Left);
        assert_eq!(
            confirm_focus(&app),
            ConfirmFocus::Accept,
            "在「确认」再按 ←（第 {step} 次）应停住"
        );
    }
    for _ in 0..3 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(confirm_focus(&app), ConfirmFocus::Cancel);
    press(&mut app, KeyCode::Right);
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Cancel,
        "在「取消」再按 → 应停住"
    );
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(confirm_focus(&app), ConfirmFocus::Cancel);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(confirm_focus(&app), ConfirmFocus::Accept);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(
        confirm_focus(&app),
        ConfirmFocus::Accept,
        "在「确认」再按 k 应停住"
    );
}

#[test]
fn confirm_dialog_keeps_y_and_esc_shortcuts() {
    let fixture = common::Fixture::new();

    for key in ['y', 'Y'] {
        let mut app = new_app(&fixture);
        open_confirm(&mut app);
        press(&mut app, KeyCode::Char(key));
        assert!(app.confirm.is_none(), "「{key}」应直接关闭确认弹层");
        assert!(app.quit, "「{key}」仍应是直接确认");
    }

    for key in ['n', 'N'] {
        let mut app = new_app(&fixture);
        open_confirm(&mut app);
        press(&mut app, KeyCode::Char(key));
        assert!(app.confirm.is_none(), "「{key}」应关闭确认弹层");
        assert!(!app.quit, "「{key}」仍应是直接取消");
    }

    let mut app = new_app(&fixture);
    open_confirm(&mut app);
    press(&mut app, KeyCode::Esc);
    assert!(app.confirm.is_none(), "Esc 应关闭确认弹层");
    assert!(!app.quit, "Esc 仍应是直接取消");
}

#[test]
fn confirm_dialog_shows_the_arrow_hint() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    open_confirm(&mut app);

    let text = render_text(&app, 100, 30);
    assert!(
        text.contains("←/→ 选择 · Enter 确认 · Esc 取消"),
        "确认弹层应提示方向键选择：\n{text}"
    );
    assert!(text.contains("确认"), "应有「确认」按钮：\n{text}");
    assert!(text.contains("取消"), "应有「取消」按钮：\n{text}");
}

#[test]
fn confirm_dialog_highlights_the_selected_button() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    open_confirm(&mut app);

    let theme = app.theme();
    let layout = overlays::confirm_layout(viewport());
    let buffer = buffer_of(&app, 100, 30);
    assert_eq!(
        cell_bg(&buffer, layout.accept.x + 1, layout.accept.y),
        theme.accent,
        "选中的「确认」应是 accent 底"
    );
    let (accept_bg, accept_fg) = label_cell(&buffer, layout.accept);
    assert_eq!(accept_bg, theme.accent, "选中的「确认」应是 accent 底");
    assert_eq!(accept_fg, theme.bg, "选中的「确认」文字应是 bg 色");
    assert_eq!(
        cell_bg(&buffer, layout.cancel.x + 1, layout.cancel.y),
        theme.element,
        "未选中的「取消」应是 element 底"
    );
    assert_eq!(
        label_cell(&buffer, layout.cancel).1,
        theme.fg,
        "未选中的「取消」文字应是 fg 色"
    );

    press(&mut app, KeyCode::Right);
    let buffer = buffer_of(&app, 100, 30);
    let (cancel_bg, cancel_fg) = label_cell(&buffer, layout.cancel);
    assert_eq!(cancel_bg, theme.accent, "焦点移到「取消」后应是 accent 底");
    assert_eq!(cancel_fg, theme.bg, "焦点移到「取消」后文字应是 bg 色");
    assert_eq!(
        cell_bg(&buffer, layout.accept.x + 1, layout.accept.y),
        theme.element,
        "「确认」失去焦点后应回到 element 底"
    );
}

#[test]
fn reset_practice_confirm_can_be_cancelled_with_arrows() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    app.stat.spend = 12_345;

    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.screen, app::Screen::Settings);

    let index = setting_row("reset_practice_progress");
    app.apply_action(Action::ActivateSetting(index));
    let confirm = app
        .confirm
        .as_ref()
        .expect("「清空练习进度」应弹出确认弹层");
    assert_eq!(confirm.action, ConfirmAction::ResetPractice);
    assert_eq!(confirm_focus(&app), ConfirmFocus::Accept);

    press(&mut app, KeyCode::Right);
    assert_eq!(confirm_focus(&app), ConfirmFocus::Cancel);
    press(&mut app, KeyCode::Enter);

    assert!(app.confirm.is_none(), "Enter 应执行「取消」并关闭弹层");
    assert!(!app.quit);
    assert_eq!(app.screen, app::Screen::Settings, "取消后应留在设置页");
    assert_eq!(app.stat.spend, 12_345, "取消不应清空练习进度");
}

// ── 结算弹层 ─────────────────────────────────────────────────────────────

#[test]
fn summary_dialog_enter_runs_the_selected_button() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    // 结算通常出现在练习页；把当前页设成非主页，才能验证「主页」按钮真的切屏。
    app.screen = app::Screen::Practice;
    app.summary = Some(app::SummaryData::default());

    assert_eq!(
        summary_focus(&app),
        SummaryButton::Continue,
        "结算弹层默认应选中「继续」"
    );
    press(&mut app, KeyCode::Right);
    assert_eq!(
        summary_focus(&app),
        SummaryButton::Relearn,
        "→ 应选中「重学」"
    );
    press(&mut app, KeyCode::Right);
    assert_eq!(summary_focus(&app), SummaryButton::Home, "→ 应选中「主页」");
    press(&mut app, KeyCode::Enter);

    assert!(
        app.summary.is_none(),
        "Enter 应执行选中的「主页」并关闭结算"
    );
    assert_eq!(app.screen, app::Screen::Home, "「主页」应回到主页");
    assert!(!app.quit);
}

#[test]
fn summary_dialog_focus_stops_at_both_ends() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    app.summary = Some(app::SummaryData::default());

    for step in 0..3 {
        press(&mut app, KeyCode::Left);
        assert_eq!(
            summary_focus(&app),
            SummaryButton::Continue,
            "在「继续」再按 ←（第 {step} 次）应停住"
        );
    }
    for _ in 0..5 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(summary_focus(&app), SummaryButton::Home);
    press(&mut app, KeyCode::Right);
    assert_eq!(
        summary_focus(&app),
        SummaryButton::Home,
        "在「主页」再按 → 应停住"
    );
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(summary_focus(&app), SummaryButton::Home);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(summary_focus(&app), SummaryButton::Relearn);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(summary_focus(&app), SummaryButton::Continue);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(
        summary_focus(&app),
        SummaryButton::Continue,
        "在「继续」再按 k 应停住"
    );
}

#[test]
fn summary_dialog_keeps_r_and_q_shortcuts() {
    let fixture = common::Fixture::new();

    // 空结算（没有练习会话）里 r 会走「退出练习」兜底，与旧行为一致。
    let mut app = new_app(&fixture);
    app.summary = Some(app::SummaryData::default());
    press(&mut app, KeyCode::Char('r'));
    assert!(app.summary.is_none(), "r 仍应是「重学」");
    assert_eq!(app.screen, app::Screen::Home);

    let mut app = new_app(&fixture);
    app.summary = Some(app::SummaryData::default());
    press(&mut app, KeyCode::Char('q'));
    assert!(app.summary.is_none(), "q 仍应是「主页」");
    assert_eq!(app.screen, app::Screen::Home);
}

#[test]
fn summary_buttons_drop_the_key_prefixes() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    app.summary = Some(app::SummaryData::default());

    let text = render_text(&app, 100, 30);
    for label in ["继续", "重学", "主页", "←/→ 选择 · Enter 确认"] {
        assert!(text.contains(label), "结算弹层应包含 {label:?}：\n{text}");
    }
    for stale in ["Enter 继续", "Enter 重学", "Enter 主页", "r 重学", "q 主页"] {
        assert!(
            !text.contains(stale),
            "按钮文案不应再带键名前缀 {stale:?}：\n{text}"
        );
    }
}

#[test]
fn summary_dialog_highlights_the_selected_button() {
    let fixture = common::Fixture::new();
    let mut app = new_app(&fixture);
    app.summary = Some(app::SummaryData::default());

    let theme = app.theme();
    // 空结算没有错词 Top10，弹层只多出「空行 + 本周记录」两行。
    let layout = overlays::summary_layout(viewport(), 2);
    let buffer = buffer_of(&app, 100, 30);
    assert_eq!(
        cell_bg(&buffer, layout.continue_btn.x + 1, layout.continue_btn.y),
        theme.accent,
        "默认选中的「继续」应是 accent 底"
    );
    assert_eq!(
        cell_bg(&buffer, layout.relearn_btn.x + 1, layout.relearn_btn.y),
        theme.element,
        "未选中的「重学」应是 element 底"
    );
    assert_eq!(
        cell_bg(&buffer, layout.home_btn.x + 1, layout.home_btn.y),
        theme.element,
        "未选中的「主页」应是 element 底"
    );

    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    let buffer = buffer_of(&app, 100, 30);
    assert_eq!(
        cell_bg(&buffer, layout.home_btn.x + 1, layout.home_btn.y),
        theme.accent,
        "焦点移到「主页」后它应变成 accent 底"
    );
    assert_eq!(
        cell_bg(&buffer, layout.continue_btn.x + 1, layout.continue_btn.y),
        theme.element,
        "「继续」失去焦点后应回到 element 底"
    );
}
