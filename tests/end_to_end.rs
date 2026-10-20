//! 端到端：驱动一个完整会话直到结算。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;
use typewords_tui::domain::types::{TaskWords, WordPracticeMode, WordPracticeType};

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// 驱动会话直到结算（或没有会话）；`wrong_budget` 指定前几个词各打错一次。
/// 返回是否正常结束（结算或退出），超过 `limit` 步仍未结束说明卡住。
fn drive_to_settlement(app: &mut app::App, wrong_budget: usize, limit: usize) -> bool {
    let mut wrong_left = wrong_budget;
    for _ in 0..limit {
        if app.summary.is_some() || app.session.is_none() {
            return true;
        }
        let Some(session) = app.session.as_ref() else {
            return true;
        };
        if session.display.show_word_result {
            press(app, KeyCode::Char(' '));
            continue;
        }
        if session.practice_type() == WordPracticeType::Identify {
            press(app, KeyCode::Char('1'));
            continue;
        }
        let word = session.current_word().word;
        if word.is_empty() {
            return true;
        }
        let characters: Vec<char> = word.chars().collect();
        if wrong_left > 0 && characters.len() > 1 {
            wrong_left -= 1;
            // 正确前缀 + 一个错字（长度凑齐才会计一次错误），再退格改对
            for character in &characters[..characters.len() - 1] {
                press(app, KeyCode::Char(*character));
            }
            press(app, KeyCode::Char('~'));
            press(app, KeyCode::Backspace);
            press(app, KeyCode::Char(characters[characters.len() - 1]));
        } else {
            for character in characters {
                press(app, KeyCode::Char(character));
            }
        }
        press(app, KeyCode::Char(' '));
    }
    false
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

/// 错词巩固退回组头时若只剩同一个单词，单词键不变；
/// 键入状态不重置会让该词永远无法完成，练习卡在巩固阶段。
#[test]
fn wrong_word_clear_with_single_word_settles() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_practice_mode = WordPracticeMode::System;
    app.start_practice();
    assert!(
        drive_to_settlement(&mut app, 1, 3000),
        "只有 1 个错词时错词巩固应能结束并结算"
    );
    assert!(app.summary.is_some(), "应进入结算");
}

/// 小组只有 1 个词时，进入组内巩固与切到下一步骤都是同一个单词，
/// 两种情况都必须重置键入状态。
#[test]
fn single_word_group_settles() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let task = TaskWords {
        new: vec![fixture.words()[0].clone()],
        review: Vec::new(),
    };
    app.begin_session_with_task(task, WordPracticeMode::System);
    assert!(
        drive_to_settlement(&mut app, 0, 2000),
        "单词组应能走完组内巩固并结算"
    );
    assert!(app.summary.is_some(), "应进入结算");
}
