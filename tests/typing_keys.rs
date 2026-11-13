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

// ── 底部动作按钮：方向键选中 / Enter 执行 / 鼠标点击 ────────────────────

/// 练习页底部按钮条：方向键在两行按钮里选中，Enter 执行选中的按钮。
///
/// 按钮条不抢键入——没有选中时 Enter 仍是「收藏」，方向键之外任何键都会
/// 取消选中（见 `typing_releases_the_button_selection`）。
#[test]
fn practice_buttons_are_selected_with_arrows_and_run_with_enter() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let word = app.session.as_ref().expect("练习会话").current_word().word;

    // 没有选中时任何方向键都落在第一个按钮上（显示单词）。
    press(&mut app, KeyCode::Right);
    assert_eq!(app.typing_button, Some(0), "→ 应选中第一个按钮");
    assert_eq!(app.screen, app::Screen::Practice, "选中按钮不该切页");

    // 第一行 5 个按钮：→ 逐个右移，到边停；↓ / ↑ 换行并保持列。
    press(&mut app, KeyCode::Right);
    assert_eq!(app.typing_button, Some(1), "第二个按钮是「跳过」");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.typing_button, Some(6), "↓ 应落到第二行的同一列");
    press(&mut app, KeyCode::Up);
    assert_eq!(app.typing_button, Some(1), "↑ 应回到第一行");
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.typing_button, Some(0), "到最左边就停住，不回绕");
    for _ in 0..4 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(app.typing_button, Some(4));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.typing_button, Some(9), "最后一列的 ↓ 也落在第二行");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.typing_button, Some(9), "第二行再按 ↓ 停住");

    // 行内到边停：第二行最左（换组）再按 ← 不动，↑ 才回到第一行。
    for _ in 0..4 {
        press(&mut app, KeyCode::Left);
    }
    assert_eq!(app.typing_button, Some(5), "← 逐列左移到本行最左");
    press(&mut app, KeyCode::Left);
    assert_eq!(app.typing_button, Some(5), "本行最左再按 ← 停住");
    press(&mut app, KeyCode::Up);
    assert_eq!(app.typing_button, Some(0), "↑ 应回到第一行的同一列");

    // 走到「跳过」并执行：Enter 等价于快捷键 Tab。
    press(&mut app, KeyCode::Right);
    assert_eq!(app.typing_button, Some(1));
    press(&mut app, KeyCode::Enter);
    let session = app.session.as_ref().expect("练习会话");
    assert!(
        session.data.exclude_words.contains(&word),
        "Enter 应执行选中的「跳过」"
    );
    assert_eq!(app.typing_button, Some(1), "执行后选中停在原按钮上");
}

/// 继续打字会取消按钮选中：Enter 回到「收藏」，不会一直执行上一个按钮。
#[test]
fn typing_releases_the_button_selection() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let word = app.session.as_ref().expect("练习会话").current_word().word;
    let first = word.chars().next().expect("首字母");

    press(&mut app, KeyCode::Right);
    assert_eq!(app.typing_button, Some(0));
    press(&mut app, KeyCode::Char(first));
    assert_eq!(app.typing_button, None, "输入字符应取消按钮选中");
    assert_eq!(app.typing.input, first.to_string(), "字符应照常进键入引擎");

    // 取消选中后 Enter 回到「收藏」的旧语义。
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.boot.base.collect_count(), 1, "Enter 应收藏当前词");
}

/// 鼠标点击底部按钮：点哪个就执行哪个，并把选中落在它上面。
#[test]
fn clicking_a_practice_button_runs_it() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let word = app.session.as_ref().expect("练习会话").current_word().word;

    let area = Rect::new(0, 0, 100, 30);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::TypingButton(1))
        .expect("练习页底部应有「跳过」按钮");
    // 命中行必须真的画着按钮文案（布局与绘制同源）。
    let text = render_text(&app, 100, 30);
    let row = text.lines().nth(target.rect.y as usize).unwrap_or_default();
    assert!(row.contains("跳过"), "按钮命中行应显示按钮文案: {row:?}");

    let column = target.rect.x + target.rect.width / 2;
    let row = target.rect.y;
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    let session = app.session.as_ref().expect("练习会话");
    assert!(
        session.data.exclude_words.contains(&word),
        "点击「跳过」应跳过当前词"
    );
    assert_eq!(app.typing_button, Some(1), "选中应落在被点的按钮上");
}

/// 关掉工具栏（Ctrl+B）时按钮条不画，方向键也不该选中看不见的按钮。
#[test]
fn hiding_the_toolbar_removes_the_practice_buttons() {
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    press_ctrl(&mut app, KeyCode::Char('b'));
    assert!(!app.show_toolbar, "Ctrl+B 应关掉工具栏");

    press(&mut app, KeyCode::Right);
    assert_eq!(app.typing_button, None, "工具栏关闭时没有可见的按钮可选中");
    let area = Rect::new(0, 0, 100, 30);
    assert!(
        !typewords_tui::ui::hit::targets(&app, area)
            .iter()
            .any(|target| matches!(target.action, Action::TypingButton(_))),
        "工具栏关闭时不该留下按钮命中区"
    );
}

/// 跟打页的按钮：朗读 / 译文 / 主题走键位表里两页共用的动作。
#[test]
fn article_practice_buttons_run_the_shared_actions() {
    audio::record_speech();
    let _ = audio::take_recorded_speech();
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);
    // 进入跟打页会自动朗读首句，先清掉。
    let _ = audio::take_recorded_speech();

    // 0 朗读
    press(&mut app, KeyCode::Right);
    assert_eq!(app.typing_button, Some(0));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        audio::take_recorded_speech(),
        vec!["Hello there.".to_string()],
        "Enter 应执行选中的「朗读」"
    );

    // 1 译文
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);
    assert!(
        !app.article_practice
            .as_ref()
            .expect("跟打状态")
            .show_translate,
        "「译文」按钮应关掉当前句译文"
    );

    // 2 主题
    press(&mut app, KeyCode::Right);
    let theme = app.theme_label();
    press(&mut app, KeyCode::Enter);
    assert_ne!(theme, app.theme_label(), "「主题」按钮应切换主题");
}

/// 鼠标点跟打页的「返回」：先写断点缓存再回文章列表（与 Ctrl+C 同义）。
#[test]
fn clicking_the_article_return_button_saves_and_leaves() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_article(&mut app);
    type_text(&mut app, "Hello");

    let area = Rect::new(0, 0, 100, 30);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::TypingButton(5))
        .expect("跟打页底部应有「返回」按钮");
    let column = target.rect.x + target.rect.width / 2;
    let row = target.rect.y;
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.screen, app::Screen::Articles, "应回到文章列表");
    assert!(
        fixture.home().join("practice-article.json").is_file(),
        "离开前应先写断点缓存"
    );
}

/// 选中的按钮用主题 accent 铺底（与页面焦点、弹层按钮同一套高亮）。
#[test]
fn the_selected_button_is_highlighted() {
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let theme = app.theme();
    let area = Rect::new(0, 0, 100, 30);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::TypingButton(1))
        .expect("练习页底部应有「跳过」按钮");

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let cell = terminal
        .backend()
        .buffer()
        .cell((target.rect.x + 1, target.rect.y))
        .expect("按钮所在单元");
    assert_eq!(cell.bg, theme.element, "未选中的按钮是次级底色");

    // 选中「跳过」后同一个单元换成强调色底。
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.typing_button, Some(1));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let cell = terminal
        .backend()
        .buffer()
        .cell((target.rect.x + 1, target.rect.y))
        .expect("按钮所在单元");
    assert_eq!(cell.bg, theme.accent, "选中的按钮应用 accent 铺底");
}

/// 键位表占用了方向键时优先：把「跳过单词」绑到 `→` 后，练习页的 `→` 是跳过，
/// 不再用来选中按钮（用户显式绑定优先于按钮条的选中键）。
#[test]
fn a_shortcut_bound_to_an_arrow_key_wins_over_the_button_bar() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let word = app.session.as_ref().expect("练习会话").current_word().word;
    app.shortcuts
        .set(ShortcutAction::Ignore, ShortcutKey::new(BaseKey::Right))
        .expect("改绑「跳过单词」到 →");

    press(&mut app, KeyCode::Right);
    assert_eq!(
        app.typing_button, None,
        "方向键已被键位表占用，不该选中按钮"
    );
    assert!(
        app.session
            .as_ref()
            .expect("练习会话")
            .data
            .exclude_words
            .contains(&word),
        "→ 应执行绑定的「跳过」"
    );
}

// ── 收藏反馈：Toast + 单词上方的徽标 + 按钮星标 ──────────────────────────

/// 收藏 / 取消收藏都要当场看得见。
///
/// 收藏表存在存档里、按 Enter 不推进词，早期实现画面上什么都没变，用户会以为
/// 没生效。现在有三处反馈：底部 Toast、单词上方的「★ 已收藏」徽标、收藏按钮
/// 上的星标；取消收藏时三处一起消失。
#[test]
fn collecting_a_word_shows_feedback() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();
    let word = app.session.as_ref().expect("练习会话").current_word().word;

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.boot.base.collect_count(), 1, "Enter 应收藏当前词");
    let toast = app
        .current_toast(timer::now_ms())
        .map(|toast| toast.message.clone())
        .unwrap_or_default();
    assert!(toast.contains("已收藏"), "收藏后应给出提示，实际 {toast:?}");
    let text = render_text(&app, 100, 30);
    assert!(text.contains("★ 已收藏"), "单词上方应标出已收藏:\n{text}");
    assert!(text.contains("★ 收藏"), "收藏按钮应带星标:\n{text}");
    assert!(
        text.contains(&format!("已收藏「{word}」")),
        "底部应显示收藏提示:\n{text}"
    );

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.boot.base.collect_count(), 0, "再按一次应取消收藏");
    let toast = app
        .current_toast(timer::now_ms())
        .map(|toast| toast.message.clone())
        .unwrap_or_default();
    assert!(
        toast.contains("已取消收藏"),
        "取消收藏后应给出提示，实际 {toast:?}"
    );
    let text = render_text(&app, 100, 30);
    assert!(!text.contains("★ 已收藏"), "取消后徽标应消失:\n{text}");
    assert!(!text.contains("★ 收藏"), "取消后按钮星标应消失:\n{text}");
}

/// 「收藏到指定词典」的提示要区分收藏与取消收藏。
#[test]
fn collect_to_dict_reports_when_it_uncollects() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.start_practice();

    let collect_to_dict = || KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
    app.on_key(collect_to_dict());
    assert!(
        app.collect_picker.is_some(),
        "Shift+Enter 应打开收藏目标弹层"
    );
    app.apply_action(app::Action::CollectSelect);
    let toast = app
        .current_toast(timer::now_ms())
        .map(|toast| toast.message.clone())
        .unwrap_or_default();
    assert!(
        toast.contains("已收藏到"),
        "应提示收藏到哪个词典，实际 {toast:?}"
    );
    assert_eq!(app.boot.base.collect_count(), 1);

    // 再收藏一次同一个词 = 从该词典取消收藏。
    app.on_key(collect_to_dict());
    app.apply_action(app::Action::CollectSelect);
    let toast = app
        .current_toast(timer::now_ms())
        .map(|toast| toast.message.clone())
        .unwrap_or_default();
    assert!(
        toast.contains("取消收藏"),
        "取消收藏的提示不能还说「已收藏到」，实际 {toast:?}"
    );
    assert_eq!(app.boot.base.collect_count(), 0);
}
