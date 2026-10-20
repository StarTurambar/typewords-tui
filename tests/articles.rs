//! 文章背诵：书本列表、断句与句子跟打。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// 把仓库里的文章资源复制到测试用的资源目录。
fn install_article_assets(assets_root: &std::path::Path) {
    let assets = assets_root;
    std::fs::create_dir_all(assets.join("list")).expect("list 目录");
    std::fs::create_dir_all(assets.join("dicts").join("en").join("article")).expect("article 目录");
    std::fs::copy(
        "assets/list/article.json",
        assets.join("list").join("article.json"),
    )
    .expect("复制文章目录");
    std::fs::copy(
        "assets/dicts/en/article/NCE_1.json",
        assets
            .join("dicts")
            .join("en")
            .join("article")
            .join("NCE_1.json"),
    )
    .expect("复制文章资源");
}

#[test]
fn article_books_are_listed_and_articles_can_be_practised() {
    let fixture = common::Fixture::new();
    install_article_assets(&fixture.assets());
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    // 主页按 a 打开文章
    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.screen, app::Screen::Articles);
    assert!(!app.article_books.is_empty(), "应读到文章书本列表");

    // 选中书本 → 载入文章
    press(&mut app, KeyCode::Down);
    assert!(!app.article_list.is_empty(), "应载入书本里的文章");

    // 开始练习
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert!(!practice.sentences.is_empty(), "应有句子");
    let expected = practice.sentences[0].text.clone();
    assert!(!expected.trim().is_empty(), "首句不应为空");

    // 逐字符跟打
    for character in expected.chars() {
        press(&mut app, KeyCode::Char(character));
    }
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "应完成第一句");
    assert!(!practice.typing.wrong, "正确输入不应算错");
}

#[test]
fn article_sentence_splitting_follows_upstream_rules() {
    use typewords_tui::domain::article::{ArticleWordKind, parse_sentence};

    let words = parse_sentence("Excuse me! Yes?");
    let texts: Vec<&str> = words.iter().map(|item| item.word.as_str()).collect();
    assert_eq!(texts, vec!["Excuse", "me", "!", "Yes", "?"]);
    assert_eq!(words[2].kind, ArticleWordKind::Symbol);
    assert!(!words[1].next_space, "me 与 ! 之间没有空格");
    assert!(words[2].next_space, "! 与 Yes 之间有空格");
}
