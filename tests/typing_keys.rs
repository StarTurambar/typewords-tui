//! 两个键入练习页（背单词 / 文章跟打）的键位统一。
//!
//! 约定：**同一个功能用同一个键**。
//!
//! - 朗读、翻译、主题走同一张键位表，改绑后两页一起变；
//! - 暂停计时（`Ctrl+,`）、保存并离开（`Ctrl+C`）、帮助（`?`）是两页共用的
//!   TUI 专有键位；
//! - `Esc` 在跟打页是页面级返回（练习页的 `Esc` 被上游绑成「显示单词」，
//!   不参与这条约定）；
//! - 键位表里那些跟打页没有对应物的动作（跳过 / 收藏 / 自测……）不能抢走
//!   跟打内容：空格、字母、数字仍然是输入。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;
use typewords_tui::audio;
use typewords_tui::domain::shortcut::{BaseKey, ShortcutAction, ShortcutKey};
use typewords_tui::domain::timer;
use unicode_width::UnicodeWidthStr;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn press_ctrl(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::CONTROL));
}

/// 打开文章页并开始第一篇（fixture 的文章 11：Hello there. / General Kenobi.）。
fn start_article(app: &mut app::App) {
    app.open_articles();
    press(app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice, "应进入文章跟打页");
}

fn type_text(app: &mut app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

/// 渲染一帧并拼成纯文本，用于断言界面上出现的提示与面板。
fn render_text(app: &app::App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut skip = 0u16;
        for x in 0..width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
                let display_width = UnicodeWidthStr::width(cell.symbol());
                if display_width > 1 {
                    skip = (display_width - 1) as u16;
                }
            }
        }
        out.push('\n');
    }
    out
}

/// 两个练习页都用 `Ctrl+C` 保存并离开：练习页回主页、跟打页回文章列表。
#[test]
fn both_typing_pages_save_and_leave_on_ctrl_c() {
    let fixture = common::Fixture::new();

    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    assert_eq!(app.screen, app::Screen::Practice);
    press_ctrl(&mut app, KeyCode::Char('c'));
    assert_eq!(app.screen, app::Screen::Home, "练习页 Ctrl+C 应回主页");
    assert!(app.session.is_none(), "练习会话应被清空");

    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);
    type_text(&mut app, "Hello");
    press_ctrl(&mut app, KeyCode::Char('c'));
    assert_eq!(
        app.screen,
        app::Screen::Articles,
        "跟打页 Ctrl+C 应回到文章列表"
    );
    assert!(app.article_practice.is_none(), "跟打状态应被清空");
    assert!(
        fixture.home().join("practice-article.json").is_file(),
        "Ctrl+C 离开前应先写断点缓存"
    );
    let cache = app.article_resume.as_ref().expect("应刷新断点缓存");
    assert_eq!(cache.article_id, 11);
    assert_eq!(cache.sentence_index, 0, "没打完的句子仍从本句续打");
}

/// 两个练习页都用 `Ctrl+,` 暂停计时，暂停期间不累计用时。
#[test]
fn article_practice_pauses_the_timer_on_the_same_key_as_word_practice() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);

    let base = timer::now_ms();
    app.on_tick(base + 1_000);
    let spend = app.article_practice.as_ref().expect("跟打状态").spend;
    assert!(spend > 0, "跟打页应累计用时");

    press_ctrl(&mut app, KeyCode::Char(','));
    assert!(
        app.article_practice.as_ref().expect("跟打状态").paused,
        "Ctrl+, 应暂停计时"
    );
    app.on_tick(base + 10_000);
    assert_eq!(
        app.article_practice.as_ref().expect("跟打状态").spend,
        spend,
        "暂停期间不该累计用时"
    );

    press_ctrl(&mut app, KeyCode::Char(','));
    assert!(
        !app.article_practice.as_ref().expect("跟打状态").paused,
        "再按一次 Ctrl+, 应恢复计时"
    );
    app.on_tick(base + 20_000);
    assert!(
        app.article_practice.as_ref().expect("跟打状态").spend > spend,
        "恢复后应继续累计用时"
    );
}

/// 跟打页的 `?` 打开整屏帮助，返回时回到跟打页而不是主页。
#[test]
fn article_practice_help_returns_to_the_typing_page() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);
    type_text(&mut app, "Hell");
    let cursor = app
        .article_practice
        .as_ref()
        .expect("跟打状态")
        .typing
        .cursor();

    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, app::Screen::Help, "? 应打开帮助");
    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("帮助（Esc 返回跟打）"),
        "跟打中打开的帮助是整屏页，标题要写清返回哪: \n{text}"
    );
    assert!(
        !typewords_tui::ui::focus::enabled(&app),
        "跟打中的帮助页不该让键盘焦点导航接手"
    );

    press(&mut app, KeyCode::Esc);
    assert_eq!(
        app.screen,
        app::Screen::ArticlePractice,
        "帮助页返回应回到跟打页"
    );
    let practice = app.article_practice.as_ref().expect("跟打状态应保留");
    assert_eq!(practice.typing.cursor(), cursor, "帮助页不该动跟打进度");
}

/// 跟打页的 `Ctrl+Z` 切换当前句译文（与练习页的「切换翻译」同一个键）。
#[test]
fn article_practice_toggles_the_translation_panel_on_ctrl_z() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);
    assert!(
        render_text(&app, 120, 40).contains("当前句译文"),
        "默认显示译文面板"
    );

    press_ctrl(&mut app, KeyCode::Char('z'));
    assert!(
        !app.article_practice
            .as_ref()
            .expect("跟打状态")
            .show_translate,
        "Ctrl+Z 应关掉译文"
    );
    assert!(
        !render_text(&app, 120, 40).contains("当前句译文"),
        "关掉译文后不该再画译文面板"
    );

    press_ctrl(&mut app, KeyCode::Char('z'));
    assert!(
        app.article_practice
            .as_ref()
            .expect("跟打状态")
            .show_translate,
        "再按一次应恢复译文"
    );
}

/// 跟打页的朗读与主题跟着键位表走：改绑后新键位生效、旧键位失效。
#[test]
fn article_practice_speech_and_theme_follow_the_shortcut_table() {
    audio::record_speech();
    let _ = audio::take_recorded_speech();
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);
    // 进入跟打页会自动朗读首句，先把它清掉。
    let _ = audio::take_recorded_speech();

    press_ctrl(&mut app, KeyCode::Char('p'));
    assert_eq!(
        audio::take_recorded_speech(),
        vec!["Hello there.".to_string()],
        "Ctrl+P 应朗读当前句"
    );

    let label = app.theme_label();
    press_ctrl(&mut app, KeyCode::Char('q'));
    assert_ne!(label, app.theme_label(), "Ctrl+Q 应切换主题");

    // 改绑「播放单词发音」：新键位生效，旧键位不再朗读。
    app.shortcuts
        .set(
            ShortcutAction::PlayWordPronunciation,
            ShortcutKey::ctrl(BaseKey::Char('Y')),
        )
        .expect("改绑「播放单词发音」");
    press_ctrl(&mut app, KeyCode::Char('p'));
    assert!(
        audio::take_recorded_speech().is_empty(),
        "改绑后旧键位 Ctrl+P 不该再朗读"
    );
    press_ctrl(&mut app, KeyCode::Char('y'));
    assert_eq!(
        audio::take_recorded_speech(),
        vec!["Hello there.".to_string()],
        "改绑后新键位 Ctrl+Y 应朗读当前句"
    );
}

/// 键位表里的页面动作不能抢走跟打内容：空格照打、`Enter` 不推进。
#[test]
fn article_practice_keeps_typing_the_keys_bound_in_the_shortcut_table() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);

    // 空格在键位表里是「单词测试：下一题」，在跟打页必须仍是输入。
    type_text(&mut app, "Hello");
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(
        app.article_practice
            .as_ref()
            .expect("跟打状态")
            .typing
            .cursor(),
        6,
        "空格应作为跟打内容输入，而不是被键位表消费"
    );

    // Enter 在键位表里是「收藏」：跟打页没有收藏，也不该退出页面。
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    assert_eq!(
        app.article_practice
            .as_ref()
            .expect("跟打状态")
            .typing
            .cursor(),
        6,
        "Enter 不是跟打内容，也不该改变光标"
    );

    // Esc 是页面级返回（跟打页没有「显示单词」这层遮罩）。
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, app::Screen::Articles, "Esc 应返回文章列表");
}
