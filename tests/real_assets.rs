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
        log: None,
        no_log: true,
        log_level: typewords_tui::logging::Level::Info,
        command: None,
    };

    let mut app = typewords_tui::app::build_for_test(paths.clone(), cli.clone()).expect("app");
    app.start_practice();
    assert!(app.session.is_some(), "free 模式应能基于真实词库开始练习");
    app.save_practice_now();

    let mut restored = typewords_tui::app::build_for_test(paths, cli).expect("app");
    assert_eq!(
        restored.screen,
        typewords_tui::app::Screen::Home,
        "启动应停在主页"
    );
    restored.start_practice();
    assert_eq!(restored.screen, typewords_tui::app::Screen::Practice);
    assert!(restored.session.is_some());
}

/// 仓库内置文章书目必须和文章文件对得上：书本行写「N 篇」，就要能列出 N 篇。
///
/// 曾经 `assets/list/article.json` 登记 72 篇、`assets/dicts/en/article/NCE_1.json`
/// 只有 5 篇：文章页的左栏写「72 篇」，右栏却只列出 5 条。
#[test]
fn real_article_catalog_matches_the_article_file() {
    let Some(assets) = locate_assets(None) else {
        return;
    };
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
        assets,
    };

    // 书目文件里登记的 length 必须就是文章文件里的篇数（数据维护规则）。
    let catalog_file = paths.assets.join("list").join("article.json");
    let catalog: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&catalog_file).expect("文章书目"))
            .expect("解析");
    let books = typewords_tui::domain::article::load_books(&paths).expect("文章书目");
    assert_eq!(books.len(), catalog.len(), "书目与解析结果条数应一致");
    for (book, entry) in books.iter().zip(catalog.iter()) {
        let articles = typewords_tui::domain::article::load_articles(&paths, book)
            .unwrap_or_else(|error| panic!("读取《{}》的文章失败: {error:#}", book.name));
        assert_eq!(
            book.length,
            Some(articles.len()),
            "《{}》显示 {:?} 篇，但文件里有 {} 篇",
            book.name,
            book.length,
            articles.len()
        );
        assert_eq!(
            entry["length"].as_u64().map(|value| value as usize),
            Some(articles.len()),
            "《{}》在 article.json 里登记的 length 应与文章文件一致",
            book.name
        );
    }

    let nce1 = books
        .iter()
        .find(|book| book.en_name == "nce1")
        .expect("应内置《新概念英语1》");
    assert_eq!(nce1.length, Some(5), "《新概念英语1》内置 5 篇文章");
}
