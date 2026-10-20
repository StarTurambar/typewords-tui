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

/// 设置页「清除所有数据」对齐上游 `dataSyncPersistence.clear()`：
/// `dict.json` 与 `setting.json` 都恢复默认，断点缓存被删除，
/// 统计 / FSRS 卡片 / 已掌握 / 词典选择全部归零。
#[test]
fn clear_all_data_resets_dict_settings_and_cache() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let store = typewords_tui::store::FileStore::new(fixture.home()).unwrap();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();

    // 练完一轮写入 statistics 与 FSRS 卡片
    app.start_practice();
    assert!(drive_to_settlement(&mut app, 0, 3000), "应在限定步数内结算");
    assert!(!app.boot.base.fsrs_data.is_empty(), "结算应写入 FSRS 卡片");
    press(&mut app, KeyCode::Char('q')); // 结算页回主页

    // 再留一份断点缓存、一个已掌握词与一个收藏词
    let words = fixture.words();
    app.begin_session_with_task(fixture.task_words(), WordPracticeMode::System);
    app.boot
        .base
        .known_mut()
        .unwrap()
        .words
        .push(words[0].clone());
    app.boot
        .base
        .collect_word_mut()
        .unwrap()
        .words
        .push(words[1].clone());
    app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)); // 离开练习并落盘

    app.boot.settings.theme = "light".to_string();
    app.boot.settings.word_review_ratio = 9;
    app.save_settings_now();
    assert_eq!(app.boot.base.known_count(), 1, "应留下已掌握词");
    assert_eq!(app.boot.base.collect_count(), 1, "应留下收藏词");
    assert!(
        paths.home.join("practice-word.json").is_file(),
        "应留下断点缓存"
    );

    // 设置页移动到「清除所有数据」并确认
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.screen, app::Screen::Settings);
    let index = app::SETTING_ROWS
        .iter()
        .position(|(key, _)| *key == "clear_all_data")
        .expect("设置页应有 clear_all_data 行");
    for _ in 0..index {
        press(&mut app, KeyCode::Char('j'));
    }
    assert_eq!(app.settings_cursor, index);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.confirm.as_ref().map(|confirm| confirm.action),
        Some(app::ConfirmAction::ClearAllData),
        "动作行应弹出清除所有数据的确认框"
    );
    press(&mut app, KeyCode::Char('y'));

    // 内存状态：默认 base + 自动选中的学习词典
    assert!(app.boot.base.fsrs_data.is_empty(), "FSRS 卡片应清空");
    assert!(app.boot.base.note_data.is_empty(), "笔记应清空");
    assert_eq!(app.boot.base.known_count(), 0, "已掌握应清空");
    assert_eq!(app.boot.base.wrong_count(), 0, "错词应清空");
    assert_eq!(app.boot.base.collect_count(), 0, "收藏应清空");
    assert_eq!(app.boot.settings.theme, "auto", "主题应回默认");
    assert_eq!(app.boot.settings.word_review_ratio, 3, "复习比例应回默认");
    assert!(app.session.is_none(), "会话应结束");
    assert!(
        !paths.home.join("practice-word.json").is_file(),
        "断点缓存应删除"
    );
    assert!(
        app.current_toast(typewords_tui::domain::timer::now_ms())
            .is_some_and(|toast| toast.message == "清除成功"),
        "应提示清除成功"
    );

    // 落盘结果：重新装载两份存档，默认状态必须写下去
    let dict = store.load_dict();
    assert!(!dict.value.word.book_list.is_empty(), "应重新选中一本词典");
    let study = dict.value.study_dict().expect("应重新选中学习词典");
    assert!(study.statistics.is_empty(), "落盘的 statistics 应清空");
    assert_eq!(study.last_learn_index, 0, "落盘的学习进度应归零");
    assert!(dict.value.fsrs_data.is_empty(), "落盘的 FSRS 卡片应清空");
    let settings = store.load_settings();
    assert_eq!(settings.value.theme, "auto");
    assert_eq!(settings.value.word_review_ratio, 3);
}
