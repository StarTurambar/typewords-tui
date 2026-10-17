//! 端到端：驱动一个完整会话直到结算。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn complete_full_session_and_settle() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_practice_mode = typewords_tui::domain::types::WordPracticeMode::System;
    app.start_practice();
    assert_eq!(app.screen, app::Screen::Practice);

    let mut steps = 0usize;
    while app.summary.is_none() && steps < 5000 {
        steps += 1;
        let Some(session) = app.session.as_ref() else {
            break;
        };
        if session.display.show_word_result {
            press(&mut app, KeyCode::Char(' '));
            continue;
        }
        let practice_type = session.practice_type();
        if practice_type == typewords_tui::domain::types::WordPracticeType::Identify {
            press(&mut app, KeyCode::Char('1'));
            continue;
        }
        let word = session.current_word().word;
        for character in word.chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Char(' '));
    }

    assert!(app.summary.is_some(), "会话应在 {steps} 步内完成");
    let summary = app.summary.clone().unwrap();
    assert_eq!(summary.new_words, 12, "新词数量应为词库全部 12 词");
    assert!(summary.accuracy > 0);

    // 结算后应写入统计与 FSRS 卡
    let dict = app.boot.base.study_dict().unwrap();
    assert!(!dict.statistics.is_empty(), "结算应写入 statistics");
    assert!(!app.boot.base.fsrs_data.is_empty(), "结算应写入 FSRS 卡片");
    assert!(dict.last_learn_index > 0, "学习进度应推进");

    // 结算后继续：要么进入下一组，要么因没有新词回到主页
    app.continue_after_summary();
    assert!(
        app.session.is_some() || app.screen == app::Screen::Home,
        "继续学习后应进入下一组或回到主页"
    );
}

#[test]
fn mastered_word_is_excluded_next_session() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let word = app.session.as_ref().unwrap().current_word().word;
    press(&mut app, KeyCode::Char('\u{60}'));
    assert!(app.boot.base.known_count() > 0, "应标记为已掌握");
    let known = app.boot.base.known().unwrap();
    assert!(known.words.iter().any(|item| item.word == word));
}
