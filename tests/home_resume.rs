//! 主页对断点状态的呈现。
//!
//! 对齐上游 `words.vue` 的 `isSaveData`：有未完成的一轮时，标题从
//! 「今日任务」变成「上次任务」，主按钮从「开始学习」变成「继续学习」。

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
                out.push_str(cell.symbol());
                let display_width = UnicodeWidthStr::width(cell.symbol());
                if display_width > 1 {
                    skip_cells = display_width - 1;
                }
            }
        }
        out.push('\n');
    }
    out
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// 没有断点时，主页显示「今日任务」与「开始学习」。
#[test]
fn home_shows_today_task_without_a_saved_round() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let text = render_text(&app, 120, 40);
    assert!(text.contains("今日任务"), "无断点时应显示「今日任务」");
    assert!(
        text.contains("开始学习"),
        "无断点时应显示「开始学习」: {text}"
    );
}

/// 有断点时，主页显示「上次任务」与「继续学习」。
///
/// 对齐上游 `words.vue` 的 `isSaveData ? $t('last_task') : $t('today_task')`
/// 与 `isSaveData ? '继续' + 模式名 : '开始' + 模式名`。
#[test]
fn home_shows_the_saved_round_when_a_cache_exists() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        // 退出练习页——断点缓存写盘，但程序不退出。
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(app.screen, app::Screen::Home);
    }

    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    // 恢复练习后停在练习页，这里要看的是「有缓存但不在练习中」的状态。
    let mut app = app;
    app.screen = app::Screen::Home;
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("上次任务"),
        "有断点时应显示「上次任务」: {text}"
    );
    assert!(
        text.contains("继续学习"),
        "有断点时应显示「继续学习」: {text}"
    );
}

/// 练完一轮并结算后，断点缓存被删——主页回到「今日任务」。
#[test]
fn finishing_a_round_restores_the_today_task_title() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        press(&mut app, KeyCode::Enter);
        // 跳过所有词直到结算。
        for _ in 0..60 {
            if app.summary.is_some() {
                break;
            }
            press(&mut app, KeyCode::Tab);
        }
        assert!(app.summary.is_some(), "应弹出结算");
    }

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    app.screen = app::Screen::Home;
    let text = render_text(&app, 120, 40);
    assert!(
        !text.contains("上次任务"),
        "练完后不该再显示「上次任务」: {text}"
    );
    assert!(text.contains("今日任务"), "应显示「今日任务」");
}
