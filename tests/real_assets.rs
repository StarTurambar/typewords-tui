//! 使用仓库内置 CET-4 词库做一次真实数据校验（资源不存在时跳过）。

use typewords_tui::bootstrap;
use typewords_tui::domain::rng;
use typewords_tui::domain::rng::shuffle;
use typewords_tui::domain::study_task::create_study_task;
use typewords_tui::paths::{Paths, locate_assets};

#[test]
fn loads_real_cet4_dict_and_builds_task() {
    let Some(assets) = locate_assets(None) else {
        return;
    };
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
        assets,
    };
    let mut boot = bootstrap::load(&paths, None).expect("bootstrap");
    boot.ensure_study_dict().expect("study dict");

    let dict = boot.study_dict().expect("dict");
    assert_eq!(dict.words.len(), 2607, "CET-4 词库应有 2607 个单词");
    assert!(dict.words.iter().all(|word| !word.word.is_empty()));

    let first = &dict.words[0];
    assert_eq!(first.word, "cancel");
    assert!(!first.trans.is_empty());
    assert!(!first.sentences.is_empty());

    let mut rng = rng::seeded(42);
    let task = create_study_task(&boot.base, &boot.settings, &mut rng);
    assert_eq!(task.task_words.new.len(), 20);
    assert_eq!(task.task_words.review.len(), 0);

    // 随机洗牌后不应丢失单词
    let mut words = dict.words.clone();
    let before: Vec<String> = words.iter().map(|word| word.word.clone()).collect();
    shuffle(&mut rng, &mut words);
    let after: Vec<String> = words.iter().map(|word| word.word.clone()).collect();
    assert_eq!(before.len(), after.len());
    let mut sorted_before = before.clone();
    sorted_before.sort();
    let mut sorted_after = after.clone();
    sorted_after.sort();
    assert_eq!(sorted_before, sorted_after);
}

#[test]
fn real_dict_session_saves_and_reloads() {
    let Some(assets) = locate_assets(None) else {
        return;
    };
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
        assets,
    };

    let cli = typewords_tui::cli::Cli {
        assets: Some(paths.assets.clone()),
        home: Some(paths.home.clone()),
        dict: None,
        mode: Some("free".to_string()),
        theme: None,
        seed: Some(7),
        reset_practice: false,
        command: None,
    };

    let mut app = typewords_tui::app::build_for_test(paths.clone(), cli.clone()).expect("app");
    app.start_practice();
    assert!(app.session.is_some(), "free 模式应能基于真实词库开始练习");
    app.save_practice_now();

    let restored = typewords_tui::app::build_for_test(paths, cli).expect("app");
    assert_eq!(restored.screen, typewords_tui::app::Screen::Practice);
    assert!(restored.session.is_some());
}
