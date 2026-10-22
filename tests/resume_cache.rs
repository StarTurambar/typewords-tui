//! 断点续学：常规练习与文章跟打。
//!
//! 两处都要能「退出 → 重启 → 接着练」：
//! - 常规练习：词表、游标、统计、用时都要接上；
//! - 文章跟打：句子进度与用时都要接上。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// `Ctrl+C`——练习页的「保存并返回主页」。
fn press_ctrl_c(app: &mut app::App) {
    app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
}

/// 输入一整句，让跟打推进到下一句。
fn type_sentence(app: &mut app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

/// 常规练习：退出后重开，词表与游标都要接上。
///
/// 早期实现已有这套机制，但它只断言了 `index >= 1`——游标接上了但
/// 统计、用时是否接上没人管。
#[test]
fn practice_progress_persists_and_resumes() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        app.save_practice_now();
    }

    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Practice);
    let session = app.session.as_ref().expect("session restored");
    assert!(session.data.index >= 1);
}

/// 常规练习：恢复后统计快照也要接上。
///
/// 断点缓存里存了 `stat_store_data`（本轮的用时与计数），恢复时必须
/// 一起还原——否则恢复后计时从零开始，本轮已经练过的时间凭空消失。
#[test]
fn resuming_practice_restores_the_stat_snapshot() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let spend_before;
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        // 手动推进一点用时，模拟练了一会儿。
        app.stat.spend = 5000;
        app.save_practice_now();
        spend_before = app.stat.spend;
    }

    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Practice);
    assert_eq!(app.stat.spend, spend_before, "恢复后应保留断点里的用时");
}

/// 文章跟打：退出文章页后重开，句子进度与用时都要接上。
///
/// 早期 `ArticleCache` 结构定义了但没有任何地方使用——文章跟打中途
/// 退出，进度全丢。
#[test]
fn article_practice_progress_persists_and_resumes() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        app.open_articles();
        assert_eq!(app.screen, app::Screen::Articles);
        // 选中第一篇文章开始跟打。
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, app::Screen::ArticlePractice);
        // 打完第一句，进度应当推进到第二句。
        type_sentence(&mut app, "Hello there.");
        let practice = app.article_practice.as_ref().expect("practice");
        assert_eq!(practice.index, 1, "应推进到第二句");
        assert_eq!(practice.finished, 1, "应记下一句已完成");
        // Esc 回文章列表——此时应写入断点缓存。
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen, app::Screen::Articles);
    }

    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(
        app.screen,
        app::Screen::ArticlePractice,
        "重开应恢复文章跟打"
    );
    let practice = app.article_practice.as_ref().expect("practice restored");
    assert_eq!(practice.index, 1, "应从第二句接着练");
    assert_eq!(practice.finished, 1, "已完成句数要接上");
    assert_eq!(practice.article_id, 11, "应恢复同一篇文章");
}

/// 文章跟打：整篇练完后断点缓存要删掉。
///
/// 否则下次启动会「恢复」一篇已经练完的文章。
#[test]
fn finishing_an_article_clears_its_cache() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        app.open_articles();
        press(&mut app, KeyCode::Enter);
        type_sentence(&mut app, "Hello there.");
        type_sentence(&mut app, "General Kenobi.");
        // 第二篇的两句也打完——但当前是第一篇，两句就该结束。
        assert!(
            app.summary.is_some(),
            "整篇练完应弹结算，实际 summary={:?}",
            app.summary.is_some()
        );
    }

    // 缓存已删，重开应回到主页而不是恢复练习。
    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(
        app.screen,
        app::Screen::Home,
        "练完后重开应回主页，不该恢复"
    );
}

/// 文章跟打：缓存文件损坏时不该阻断启动。
#[test]
fn a_corrupt_article_cache_does_not_block_startup() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        app.open_articles();
        press(&mut app, KeyCode::Enter);
        type_sentence(&mut app, "Hello there.");
        press(&mut app, KeyCode::Esc);
    }
    // 写坏文件。
    std::fs::write(paths.home.join("practice-article.json"), b"{ not json").unwrap();

    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_ne!(
        app.screen,
        app::Screen::ArticlePractice,
        "缓存损坏时不该恢复练习"
    );
    assert!(app.fatal.is_none(), "缓存损坏不该是致命错误");
}

/// 清空练习进度会清掉两个断点缓存。
///
/// 「清空当前练习进度」与 `--reset-practice` 的用途都是「从干净状态开始」，
/// 留着半个文章的跟打进度没意义——两个缓存都要清。
#[test]
fn clearing_practice_progress_clears_both_caches() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        // 单词练习的断点。
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        press_ctrl_c(&mut app);
        // 文章跟打的断点。
        app.open_articles();
        press(&mut app, KeyCode::Enter);
        type_sentence(&mut app, "Hello there.");
        press(&mut app, KeyCode::Esc);
    }
    assert!(paths.home.join("practice-word.json").is_file());
    assert!(paths.home.join("practice-article.json").is_file());

    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    app.clear_practice_cache();
    assert!(
        !paths.home.join("practice-word.json").is_file(),
        "应删掉单词练习的断点缓存"
    );
    assert!(
        !paths.home.join("practice-article.json").is_file(),
        "应删掉文章跟打的断点缓存"
    );
}

/// 同一次运行内：退出练习页再按 Enter，要接着上次练，而不是重开一轮。
///
/// 早期实现总是 `create_study_task` 新建任务，用户中途退出再按 Enter
/// 会丢掉已练的部分从头再来。对齐上游 `nav(url, {}, practiceData)`
/// ——主页把现存的 `practiceData` 传给练习页，练习页优先
/// `applyPracticeCache`。
#[test]
fn pressing_enter_again_resumes_the_same_round() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    let index_before = app.session.as_ref().unwrap().data.index;
    assert!(index_before >= 1, "应已推进过");

    // 退出练习页（中途退出，进度写进断点缓存）。
    press_ctrl_c(&mut app);
    assert_eq!(app.screen, app::Screen::Home, "应回到主页");

    // 再按 Enter：应接着上次，而不是从头开始。
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice, "应重新进入练习");
    let index_after = app.session.as_ref().unwrap().data.index;
    assert!(
        index_after >= index_before,
        "续练的进度不该回退：之前 {index_before}，现在 {index_after}"
    );
}

/// 模式变了就不能续练——用「复习」模式去接「学习」模式的进度，
/// 流程阶段会对不上。
#[test]
fn a_different_practice_mode_does_not_resume_the_saved_round() {
    use typewords_tui::domain::types::WordPracticeMode;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    press_ctrl_c(&mut app);
    assert_eq!(app.screen, app::Screen::Home);

    // 换成「复习」模式再开始——应当开新一轮，而不是续「学习」的进度。
    app.boot.settings.word_practice_mode = WordPracticeMode::Review;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    let session = app.session.as_ref().unwrap();
    assert_eq!(
        session.mode,
        WordPracticeMode::Review,
        "应当以复习模式开新一轮"
    );
    assert_eq!(session.data.index, 0, "新一轮应从头开始");
}
