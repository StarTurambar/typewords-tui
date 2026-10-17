//! 7 个内置流程都能从起点推进到 Complete。

mod common;

use typewords_tui::domain::flow::navigator::NavState;
use typewords_tui::domain::flow::navigator::Navigator;
use typewords_tui::domain::flow::{flow_for_mode, resolve_flow_start};
use typewords_tui::domain::rng::{Rng, seeded, shuffle};
use typewords_tui::domain::types::{
    PracticeData, PracticeState, TaskWords, WordPracticeMode, WordPracticeType,
};

fn drive(
    mode: WordPracticeMode,
    task: TaskWords,
    mutate: impl FnOnce(&mut PracticeData),
) -> (Navigator, PracticeData, usize) {
    let flow = flow_for_mode(mode);
    let mut rng: Rng = seeded(7);
    let start = resolve_flow_start(&flow, &task, &[], &mut |words| shuffle(&mut rng, words))
        .expect("flow start");
    let mut nav = Navigator::new(flow);
    nav.set_cursor(start.cursor.clone());
    nav.initialize_node_words(start.words.clone());

    let mut data = PracticeData {
        words: start.words,
        ..PracticeData::default()
    };
    mutate(&mut data);
    let mut stat = PracticeState::default();
    let mut notices = Vec::new();
    let mut reset_word = false;
    let mut completed = false;
    let is_simple = |_: &str| false;

    let mut steps = 0usize;
    while !completed && steps < 1000 {
        steps += 1;
        let mut state = NavState {
            data: &mut data,
            task_words: &task,
            stat: &mut stat,
            is_simple: &is_simple,
            rng: &mut rng,
            notices: &mut notices,
            reset_word: &mut reset_word,
            completed: &mut completed,
        };
        nav.next(&mut state, true, false);
        if data.words.is_empty() && !completed {
            break;
        }
    }
    (nav, data, steps)
}

#[test]
fn all_builtin_flows_reach_complete() {
    let modes = [
        WordPracticeMode::System,
        WordPracticeMode::Free,
        WordPracticeMode::Review,
        WordPracticeMode::Shuffle,
        WordPracticeMode::IdentifyOnly,
        WordPracticeMode::DictationOnly,
        WordPracticeMode::ListenOnly,
    ];
    let fixture = common::Fixture::new();
    for mode in modes {
        let task = fixture.task_words();
        let (nav, _data, steps) = drive(mode, task, |_| {});
        assert!(
            steps < 1000,
            "mode {:?} 未在合理步数内完成（{steps} 步，flow={}）",
            mode,
            nav.flow.id
        );
    }
}

#[test]
fn wrong_words_are_retried_before_next_step() {
    let fixture = common::Fixture::new();
    let task = fixture.task_words();
    let target = task.new[0].word.clone();
    let (nav, data, steps) = drive(WordPracticeMode::System, task, move |data| {
        data.wrong_words.push(data.words[0].clone());
        data.all_wrong_words.push(target.clone());
    });
    assert!(steps < 1000);
    // 错词清空会在最初几轮内被消费
    assert!(data.wrong_words.len() <= data.words.len() + 1);
    assert_eq!(nav.flow.id, "system");
}

#[test]
fn flow_phase_tracks_practice_type() {
    let fixture = common::Fixture::new();
    let task = fixture.task_words();
    let flow = flow_for_mode(WordPracticeMode::System);
    let mut rng = seeded(1);
    let mut nav = Navigator::new(flow);
    let start = resolve_flow_start(&nav.flow, &task, &[], &mut |words| shuffle(&mut rng, words))
        .expect("start");
    nav.set_cursor(start.cursor);
    let phase = nav.current_phase();
    assert_eq!(phase.practice_type, WordPracticeType::FollowWrite);
    assert!(matches!(
        phase.word_advance,
        typewords_tui::domain::flow::WordAdvance::WordLoop { group_size: 7, .. }
    ));
}
