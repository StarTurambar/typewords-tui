//! 词典进度口径：百分比与「N / M 词」必须用同一个分母——有效词数。
//!
//! 早期实现两套分母并存：`Dict::progress_percent()` 用
//! `length.max(words.len())`，`effective_length()` 却优先 `words.len()`。
//! 一旦 `length` 比实际词条多（词库目录元数据比词条文件旧、导入外部词库、
//! 手工裁剪词条文件），主页同屏的「已学 X%」与「N / M 词」、词库页的行、
//! 结算弹层的进度条与「N / M」、`tw task` 的百分比与分母就会各说各话；
//! 更糟的是结算推进的「学完判定」也拿假分母比，整本学完也到不了终点。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;
use typewords_tui::domain::types::{WordPracticeMode, WordPracticeType};

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn render_text(app: &app::App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..height {
        let mut skip_cells = 0usize;
        for x in 0..width {
            if skip_cells > 0 {
                skip_cells -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                let symbol = cell.symbol();
                text.push_str(symbol);
                // 宽字符（CJK）占两个单元，跳过其续接单元。
                let width = unicode_width::UnicodeWidthStr::width(symbol);
                if width > 1 {
                    skip_cells = width - 1;
                }
            }
        }
        text.push('\n');
    }
    text
}

fn line_with<'a>(text: &'a str, needle: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("没有任何一行包含 {needle:?}：\n{text}"))
}

/// 词库目录登记的 `length`（20）比词条文件里的实际词数（12）大。
///
/// 这不是人为构造的怪状态：导入外部词库、手工裁剪词条文件、上游存档的
/// 元数据与本资源文件不同步，都会留下这种记录。
fn stale_metadata_fixture() -> common::Fixture {
    let fixture = common::Fixture::new();
    let catalog = serde_json::json!([{
        "id": 1,
        "enName": "toy",
        "name": "测试词库",
        "description": "12 个测试词",
        "url": "toy.json",
        "length": 20,
        "language": "en",
        "translateLanguage": "zh_CN",
        "type": "word",
        "category": "测试",
        "tags": ["测试"]
    }]);
    std::fs::write(
        fixture.assets().join("list").join("word.json"),
        serde_json::to_string(&catalog).unwrap(),
    )
    .unwrap();
    fixture
}

/// 驱动单词会话直到结算（或没有会话）；超过 `limit` 步仍未结束视为卡住。
fn drive_to_settlement(app: &mut app::App, limit: usize) -> bool {
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
        for character in word.chars() {
            press(app, KeyCode::Char(character));
        }
        press(app, KeyCode::Char(' '));
    }
    false
}

/// 答完单词测试的全部题目。
fn finish_word_test(app: &mut app::App) {
    for _ in 0..200 {
        let Some(state) = app.word_test.as_ref() else {
            return;
        };
        if state.selected.is_none() {
            let question = state.question.as_ref().expect("题目");
            app.apply_action(app::Action::WordTestChoose(question.correct_index));
        }
        app.apply_action(app::Action::WordTestNext);
    }
}

/// 词表已经加载时，存档里的 `length` 只是元数据，必须校正成实际词数，
/// 否则它就是一个没有任何一处会再校正回来的假分母。
#[test]
fn the_stored_word_count_matches_the_loaded_word_list() {
    let fixture = stale_metadata_fixture();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let dict = app.boot.study_dict().expect("应选中测试词库");
    assert_eq!(dict.words.len(), 12, "测试词库共 12 词");
    assert_eq!(dict.effective_length(), 12, "有效词数应以已加载的词表为准");
    assert_eq!(
        dict.length, 12,
        "词表在手里时 length 应校正为 words.len()，不能留着目录里的 20"
    );
}

/// 主页同屏显示「已学 X%」与「N / M 词」：两者必须来自同一个分母。
#[test]
fn home_progress_percent_and_word_count_share_one_denominator() {
    let fixture = stale_metadata_fixture();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot
        .base
        .study_dict_mut()
        .expect("应选中测试词库")
        .last_learn_index = 6;
    let text = render_text(&app, 100, 32);
    let progress = line_with(&text, "当前进度");
    assert!(
        progress.contains("已学 50%"),
        "12 词学 6 词就是 50%，不能按目录里的 20 算成 30%：{progress:?}"
    );
    assert!(
        progress.contains("6 / 12 词"),
        "同屏的「N / M 词」应与百分比同源：{progress:?}"
    );
}

/// 词库页的行（词数 + 百分比）与主页用同一口径。
#[test]
fn dict_picker_row_uses_the_same_word_count_as_home() {
    let fixture = stale_metadata_fixture();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot
        .base
        .study_dict_mut()
        .expect("应选中测试词库")
        .last_learn_index = 6;
    let entry = app
        .dict_entries()
        .into_iter()
        .find(|entry| entry.current)
        .expect("当前词库应出现在词库页");
    assert_eq!(entry.length, 12, "词库页词数取有效词数");
    assert_eq!(entry.percent, 50, "词库页百分比与词数同源");
}

/// 把词表里的词全部学完就该标记完成——分母若是目录里的 20，永远到不了。
#[test]
fn settling_the_last_group_completes_the_dictionary() {
    let fixture = stale_metadata_fixture();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_practice_mode = WordPracticeMode::System;
    app.start_practice();
    assert!(
        drive_to_settlement(&mut app, 5000),
        "12 个词的会话应在 5000 步内结束"
    );
    let summary = app.summary.clone().expect("会话结束后应给出结算");
    let dict = app.boot.study_dict().expect("应选中测试词库");
    assert!(
        dict.complete,
        "词表里的 12 词全学完了，应标记「已学完」而不是卡在 12/20"
    );
    assert_eq!(dict.last_learn_index, dict.effective_length());
    assert_eq!(summary.length, 12, "结算的分母应是有效词数");
    assert_eq!(summary.last_learn_index, 12, "结算的分子应夹在有效词数内");
}

/// 「继续」不能让进度跳两格：结算已经推进过一次，第二次推进会把中间整组词
/// 跳过去——它们永远不会出现在练习里，主页百分比却已经越过它们。
#[test]
fn continuing_after_the_summary_advances_the_dictionary_once() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot
        .base
        .study_dict_mut()
        .expect("应选中测试词库")
        .per_day_study_number = 4;
    app.boot.settings.word_practice_mode = WordPracticeMode::System;
    app.start_practice();
    assert!(drive_to_settlement(&mut app, 2000), "第一组应该能结束");
    assert_eq!(
        app.boot
            .base
            .study_dict()
            .expect("当前词典")
            .last_learn_index,
        4,
        "第一组学了 4 个新词"
    );
    app.continue_after_summary();
    let dict = app.boot.base.study_dict().expect("当前词典");
    assert_eq!(
        dict.last_learn_index, 4,
        "继续之后进度不该再涨一次（否则第 5~8 个词被跳过）"
    );
    let next_words: Vec<&str> = app
        .session
        .as_ref()
        .expect("继续之后应开下一轮")
        .task_words
        .new
        .iter()
        .map(|word| word.word.as_str())
        .collect();
    assert_eq!(
        next_words,
        vec!["word04", "word05", "word06", "word07"],
        "下一轮应接着第 5 个词，而不是被跳过 4 个后从第 9 个开始"
    );
}

/// 只剩最后一个词时不能被「已到头」吞掉：每日 1 词也要能把整本学完。
#[test]
fn the_last_word_is_taught_so_the_dictionary_can_complete() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot
        .base
        .study_dict_mut()
        .expect("应选中测试词库")
        .per_day_study_number = 1;
    app.boot.settings.word_practice_mode = WordPracticeMode::System;
    app.start_practice();
    let mut rounds = 0;
    while !app
        .boot
        .base
        .study_dict()
        .map(|dict| dict.complete)
        .unwrap_or(false)
    {
        rounds += 1;
        assert!(
            rounds <= 20,
            "12 词、每日 1 词应在 12 轮内学完（已跑 {rounds} 轮仍未完成）"
        );
        assert!(
            drive_to_settlement(&mut app, 2000),
            "第 {rounds} 轮应该能结束"
        );
        if app.boot.base.study_dict().expect("当前词典").complete {
            break;
        }
        app.continue_after_summary();
        assert!(app.session.is_some(), "继续之后应开下一轮");
    }
    let dict = app.boot.base.study_dict().expect("当前词典");
    assert!(dict.complete, "12 个词全学完后应标记「已学完」");
    assert_eq!(dict.last_learn_index, dict.effective_length());
}

/// 单词测试不推进词典进度，结算就该显示词典的真实进度，
/// 而不是拿「题目数」冒充总词数、再盖上「已完成」。
#[test]
fn word_test_summary_shows_dictionary_progress_not_its_question_count() {
    let fixture = stale_metadata_fixture();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot
        .base
        .study_dict_mut()
        .expect("应选中测试词库")
        .last_learn_index = 5;
    app.boot.settings.word_practice_mode = WordPracticeMode::ShuffleWordsTest;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::WordTest);
    finish_word_test(&mut app);
    let summary = app.summary.clone().expect("单词测试结束应给出结算");
    assert_eq!(
        summary.length, 12,
        "结算的分母应是词典有效词数，不是测试的题目数"
    );
    assert_eq!(
        summary.last_learn_index, 5,
        "单词测试不推进词典进度，结算应显示词典真实进度"
    );
    assert!(!summary.complete, "单词测试不该标词典「已完成」");

    let text = render_text(&app, 100, 32);
    let progress = line_with(&text, "学习进度");
    assert!(
        progress.contains("5 / 12"),
        "结算进度行应为「5 / 12」：{progress:?}"
    );
    assert!(
        !text.contains("已完成"),
        "单词测试没有学完词典，不该出现「已完成」：\n{text}"
    );
}
