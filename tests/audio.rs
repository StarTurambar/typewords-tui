//! 朗读触发：单词练习换词自动朗读、单词测试自动朗读与 `Ctrl+P` 重播。
//!
//! 音频没有可断言的落盘结果，这里用 `audio::record_speech` 记录被朗读的文本。
//! `App::build_for_test` 已把真正的系统 TTS 静音，所以这些用例不会出声。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;
use typewords_tui::audio;
use typewords_tui::domain::types::WordPracticeMode;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// 开始记录本线程的朗读文本，并清掉可能残留的记录。
fn start_recording() {
    let _ = audio::take_recorded_speech();
    audio::record_speech();
}

fn current_word(app: &app::App) -> String {
    app.session
        .as_ref()
        .expect("练习会话应存在")
        .current_word()
        .word
}

/// 练习页的换词：进入练习与每次切词都要自动朗读新词。
#[test]
fn word_practice_auto_pronounces_every_new_word() {
    start_recording();
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    press(&mut app, KeyCode::Enter);
    let first = current_word(&app);
    assert_eq!(
        audio::take_recorded_speech(),
        vec![first.clone()],
        "进入练习应立即朗读第一个新词"
    );

    // 正确敲完当前词并确认，推进到下一个词
    for character in first.chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Char(' '));
    let second = current_word(&app);
    assert_ne!(second, first, "确认后应切换到下一个词");
    assert_eq!(
        audio::take_recorded_speech(),
        vec![second],
        "切词后应朗读新词"
    );
}

/// 关掉「单词发音」后，换词不再自动朗读。
#[test]
fn disabling_word_sound_silences_auto_pronunciation() {
    start_recording();
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_sound = false;

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    assert!(
        audio::take_recorded_speech().is_empty(),
        "关掉「单词发音」后不应自动朗读"
    );
}

/// 默写阶段不该自动朗读（否则直接念出答案）；手动 `Ctrl+P` 仍然可用。
#[test]
fn dictation_mode_does_not_auto_pronounce_the_answer() {
    start_recording();
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    app.start_practice_with_mode(WordPracticeMode::DictationOnly);
    assert_eq!(app.screen, app::Screen::Practice);
    assert!(
        audio::take_recorded_speech().is_empty(),
        "默写阶段不应自动朗读答案"
    );

    app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(
        audio::take_recorded_speech().len(),
        1,
        "手动 Ctrl+P 不受默写阶段限制"
    );
}

/// 单词测试页：进入题目 / 切题自动朗读答案词，`Ctrl+P` 可重播。
#[test]
fn word_test_auto_pronounces_and_ctrl_p_replays() {
    start_recording();
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    app.start_word_test(WordPracticeMode::ReviewWordsTest);
    assert_eq!(app.screen, app::Screen::WordTest);
    let first = app
        .word_test
        .as_ref()
        .and_then(|state| state.answer_word())
        .map(str::to_string)
        .expect("单词测试应有当前题");
    assert_eq!(
        audio::take_recorded_speech(),
        vec![first.clone()],
        "进入单词测试应朗读当前题"
    );

    app.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(
        audio::take_recorded_speech(),
        vec![first.clone()],
        "单词测试页 Ctrl+P 应重播当前词"
    );

    press(&mut app, KeyCode::Char('1'));
    press(&mut app, KeyCode::Char(' '));
    let next = app
        .word_test
        .as_ref()
        .and_then(|state| state.answer_word())
        .map(str::to_string)
        .expect("应切到下一题");
    assert_eq!(
        audio::take_recorded_speech(),
        vec![next],
        "切题后应朗读下一题"
    );
}
