//! 用合成按键驱动应用状态机。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// 按 key 定位设置行，避免依赖行号。
fn setting_row(key: &str) -> usize {
    app::SETTING_ROWS
        .iter()
        .position(|(row_key, _)| *row_key == key)
        .unwrap_or_else(|| panic!("设置页应有 {key} 行"))
}

/// 把设置页光标移到指定行。
fn goto_setting(app: &mut app::App, key: &str) {
    let target = setting_row(key);
    while app.settings_cursor < target {
        press(app, KeyCode::Down);
    }
    while app.settings_cursor > target {
        press(app, KeyCode::Up);
    }
    assert_eq!(app.settings_cursor, target, "光标应停在 {key} 行");
}

fn type_str(app: &mut app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

#[test]
fn start_practice_and_advance_by_typing() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home);

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    let session = app.session.as_ref().expect("session");
    let word = session.current_word().word;
    assert!(!word.is_empty());

    type_str(&mut app, &word);
    press(&mut app, KeyCode::Char(' '));

    let advanced = app
        .session
        .as_ref()
        .map(|session| session.data.index > 0 || session.complete)
        .unwrap_or(false);
    assert!(advanced, "正确输入后应推进到下一个单词");
}

#[test]
fn tab_skips_word() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    let before = app.session.as_ref().unwrap().data.index;
    press(&mut app, KeyCode::Tab);
    let after = app.session.as_ref().unwrap().data.index;
    assert!(after > before || app.session.as_ref().unwrap().complete);
}

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

#[test]
fn settings_screen_toggles_persist() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.screen, app::Screen::Settings);
    // 第一行是练习模式，Enter 打开模式选择弹层
    let before = app.boot.settings.word_practice_mode;
    press(&mut app, KeyCode::Enter);
    assert!(app.mode_picker.is_some(), "应打开模式选择弹层");
    app.apply_action(app::Action::ModeMove(1));
    app.apply_action(app::Action::ModeSelect);
    let after = app.boot.settings.word_practice_mode;
    assert_ne!(before, after, "选中别的模式后应生效");
    press(&mut app, KeyCode::Esc);

    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(reloaded.boot.settings.word_practice_mode, after);
}

#[test]
fn big_word_setting_toggles_and_persists() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    assert!(app.boot.settings.tui_big_word, "默认开启大字");
    press(&mut app, KeyCode::Char('s'));
    goto_setting(&mut app, "big_word");
    press(&mut app, KeyCode::Enter);
    assert!(!app.boot.settings.tui_big_word, "切换后应关闭大字");
    press(&mut app, KeyCode::Esc);

    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert!(!reloaded.boot.settings.tui_big_word, "设置应持久化");
}

#[test]
fn word_to_wrong_book_on_typo() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    let word = app.session.as_ref().unwrap().current_word().word;
    let wrong_char = if word.starts_with('x') { 'y' } else { 'x' };
    press(&mut app, KeyCode::Char(wrong_char));
    // 整词错误路径：补齐长度
    let remaining = word.chars().count().saturating_sub(1);
    for _ in 0..remaining {
        press(&mut app, KeyCode::Char('z'));
    }
    let wrong_count = app.boot.base.wrong_count();
    assert!(wrong_count > 0, "输入错误后应进入错词本");
}

#[test]
fn quit_confirmation_sets_quit_flag() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('q'));
    assert!(app.confirm.is_some());
    press(&mut app, KeyCode::Enter);
    assert!(app.quit);
}
#[test]
fn free_practice_starts_free_flow_without_changing_setting() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let before = app.boot.settings.word_practice_mode;
    press(&mut app, KeyCode::Char('f'));
    assert_eq!(app.screen, app::Screen::Practice);
    assert_eq!(
        app.session.as_ref().unwrap().mode,
        typewords_tui::domain::types::WordPracticeMode::Free
    );
    assert_eq!(app.boot.settings.word_practice_mode, before);
}

#[test]
fn daily_target_keys_adjust_and_persist() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    app.boot.base.study_dict_mut().unwrap().per_day_study_number = 2;
    press(&mut app, KeyCode::Char('+'));
    assert_eq!(app.boot.base.study_dict().unwrap().per_day_study_number, 7);
    press(&mut app, KeyCode::Char('-'));
    assert_eq!(app.boot.base.study_dict().unwrap().per_day_study_number, 2);
    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(
        reloaded
            .boot
            .base
            .study_dict()
            .unwrap()
            .per_day_study_number,
        2
    );
    let _ = app;
}

#[test]
fn number_shortcuts_navigate_shell_screens() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.screen, app::Screen::Stats);
    press(&mut app, KeyCode::Char('2'));
    assert_eq!(app.screen, app::Screen::DictPicker);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, app::Screen::Settings);
    press(&mut app, KeyCode::Char('5'));
    assert_eq!(app.screen, app::Screen::Help);
    // Help 的“任意键返回”优先
    press(&mut app, KeyCode::Char('1'));
    assert_eq!(app.screen, app::Screen::Home);
}

#[test]
fn mouse_click_starts_practice_from_home() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 100, 30);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::StartPractice)
        .expect("主页应有开始学习按钮");
    let column = target.rect.x + target.rect.width / 2;
    let row = target.rect.y + target.rect.height / 2;
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.screen, app::Screen::Practice);
}

#[test]
fn mouse_click_sidebar_navigates() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 100, 30);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::Navigate(app::Screen::DictPicker))
        .expect("侧栏应有词库入口");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x + target.rect.width / 2,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.screen, app::Screen::DictPicker);
}

#[test]
fn mouse_wheel_moves_settings_cursor() {
    use ratatui::crossterm::event::{MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('s'));
    let area = Rect::new(0, 0, 100, 30);
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 15,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.settings_cursor, 1);
}

#[test]
fn mouse_outside_confirm_cancels() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('q'));
    assert!(app.confirm.is_some());
    let area = Rect::new(0, 0, 100, 30);
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(app.confirm.is_none());
    assert!(!app.quit);
}

#[test]
fn custom_theme_cycles_and_renders() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    let fixture = common::Fixture::new();
    let themes = fixture.home().join("themes");
    std::fs::create_dir_all(&themes).unwrap();
    std::fs::write(
        themes.join("ocean.json"),
        r##"{
            "name": "Ocean",
            "base": "dark",
            "colors": { "bg": "#0b1020", "accent": "#6fd3c7" }
        }"##,
    )
    .unwrap();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    assert!(app.boot.themes.contains("ocean"), "应加载自定义主题");
    press(&mut app, KeyCode::Char('s'));
    goto_setting(&mut app, "theme");
    for _ in 0..4 {
        press(&mut app, KeyCode::Enter); // auto -> dark -> light -> high-contrast -> ocean
    }
    assert_eq!(app.boot.settings.theme, "ocean");
    assert_eq!(app.theme().bg, Color::Rgb(0x0b, 0x10, 0x20));
    app.screen = app::Screen::Home;
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.bg == Color::Rgb(0x0b, 0x10, 0x20)),
        "自定义主题背景应出现在渲染结果里"
    );
}
#[test]
fn mouse_click_inside_confirm_does_not_cancel() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('q'));
    let layout = typewords_tui::ui::overlays::confirm_layout(Rect::new(0, 0, 100, 30));
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: layout.rect.x + 2,
            row: layout.rect.y + 1,
            modifiers: KeyModifiers::NONE,
        },
        Rect::new(0, 0, 100, 30),
    );
    assert!(app.confirm.is_some(), "点击确认框内部不应取消");
}

#[test]
fn mouse_click_summary_home_button() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.summary = Some(app::SummaryData {
        spend_ms: 30_000,
        accuracy: 100,
        new_words: 3,
        review: 0,
        wrong: 0,
        last_learn_index: 3,
        length: 12,
        complete: false,
        ..Default::default()
    });
    let area = Rect::new(0, 0, 100, 30);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::SummaryHome)
        .expect("结算应有返回主页按钮");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x + target.rect.width / 2,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(app.summary.is_none());
    assert_eq!(app.screen, app::Screen::Home);
}
#[test]
fn cli_theme_override_is_applied() {
    use ratatui::style::Color;

    let fixture = common::Fixture::new();
    let mut cli = fixture.cli();
    cli.theme = Some("light".to_string());
    let app = app::build_for_test(fixture.paths(), cli).unwrap();
    assert_eq!(app.boot.settings.theme, "light");
    assert_eq!(app.theme().bg, Color::Rgb(0xf7, 0xf7, 0xf9));
}

#[test]
fn mouse_click_daily_plus_adjusts_target() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.base.study_dict_mut().unwrap().per_day_study_number = 2;
    let area = Rect::new(0, 0, 100, 30);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::AdjustDaily(5))
        .expect("主页应有每日词数 + 按钮");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x + target.rect.width / 2,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.boot.base.study_dict().unwrap().per_day_study_number, 7);
}
#[test]
fn help_screen_scrolls_without_losing_any_key() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, app::Screen::Help);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.help_scroll, 1);
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.help_scroll, 6);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, app::Screen::Home);
}
#[test]
fn newer_setting_version_is_not_overwritten() {
    let fixture = common::Fixture::new();
    let raw = r#"{"version":99,"val":{"wordReviewRatio":5}}"#;
    std::fs::write(fixture.home().join("setting.json"), raw).unwrap();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    assert_eq!(
        app.boot.settings.word_review_ratio, 3,
        "版本过高时回退默认值"
    );

    app.boot.settings.word_review_ratio = 7;
    app.save_settings_now();
    app.boot.settings.theme = "light".to_string();
    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Char('y'));

    let after = std::fs::read_to_string(fixture.home().join("setting.json")).unwrap();
    assert_eq!(after, raw, "版本过高的 setting.json 不应被覆盖");
}

#[test]
fn repeat_custom_count_can_be_edited_and_persists() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    let index = app::SETTING_ROWS
        .iter()
        .position(|(key, _)| *key == "repeat_custom_count")
        .expect("设置页应有 repeat_custom_count 行");
    app.apply_action(app::Action::ActivateSetting(index));
    assert!(app.edit.is_some(), "数字项应打开输入弹层");
    app.apply_action(app::Action::EditBackspace);
    app.apply_action(app::Action::EditChar('5'));
    app.apply_action(app::Action::EditSave);
    assert_eq!(app.boot.settings.repeat_custom_count, Some(5));

    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(reloaded.boot.settings.repeat_custom_count, Some(5));
    assert_eq!(reloaded.boot.settings.effective_repeat_count(), 1);

    let mut settings = reloaded.boot.settings.clone();
    settings.repeat_count = 100;
    assert_eq!(settings.effective_repeat_count(), 5, "100 是自定义次数标记");
}

#[test]
fn cross_day_statistics_count_once_in_today_stats() {
    use typewords_tui::domain::types::Statistics;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let today_ms = chrono::Local::now().timestamp_millis();
    let make = |spend: i64, role: &str| Statistics {
        start_date: today_ms,
        spend,
        total: 10,
        new_words: 6,
        review: 4,
        wrong: 2,
        session_role: Some(role.to_string()),
        ..Statistics::default()
    };
    let dict = app.boot.base.study_dict_mut().unwrap();
    dict.statistics.push(make(60_000, "start"));
    dict.statistics.push(make(30_000, "end"));

    app.refresh_stats();
    assert_eq!(app.today_spend, 90_000, "两天的时长都要计入");
    assert_eq!(app.today_new, 6, "新词计数只应算一次");
    assert_eq!(app.today_review, 4, "复习计数只应算一次");
    assert_eq!(app.today_wrong, 2, "错词计数只应算一次");
}

#[test]
fn clicking_word_list_row_jumps_to_that_word() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    // 词表侧栏默认关闭，Ctrl+L 打开
    app.on_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));

    let area = Rect::new(0, 0, 120, 40);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::PracticeGotoWord(2))
        .expect("词表侧栏应有点击目标");
    let column = target.rect.x + target.rect.width / 2;
    let row = target.rect.y + target.rect.height / 2;
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );

    let session = app.session.as_ref().expect("session");
    assert_eq!(session.data.index, 2, "点击词表应跳到该词");
}

#[test]
fn practice_shortcuts_follow_the_configured_key_map() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);

    // 默认绑定：Ctrl+Z 切换翻译
    let before = app
        .session
        .as_ref()
        .map(|session| session.view_state().is_show_translate)
        .unwrap_or(false);
    app.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    let after = app
        .session
        .as_ref()
        .map(|session| session.view_state().is_show_translate)
        .unwrap_or(false);
    assert_ne!(before, after, "Ctrl+Z 应切换翻译显示");
}

#[test]
fn rebinding_a_shortcut_takes_effect_immediately() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);

    // 把「切换翻译」改绑到 Ctrl+K
    app.boot
        .settings
        .shortcut_key_map
        .insert("ToggleShowTranslate".to_string(), "Ctrl+K".to_string());
    app.refresh_shortcuts();

    let before = app
        .session
        .as_ref()
        .map(|session| session.view_state().is_show_translate)
        .unwrap_or(false);
    app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    let after = app
        .session
        .as_ref()
        .map(|session| session.view_state().is_show_translate)
        .unwrap_or(false);
    assert_ne!(before, after, "改绑后 Ctrl+K 应切换翻译");

    // 原键位不再生效
    let after_rebind = app
        .session
        .as_ref()
        .map(|session| session.view_state().is_show_translate)
        .unwrap_or(false);
    app.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    let unchanged = app
        .session
        .as_ref()
        .map(|session| session.view_state().is_show_translate)
        .unwrap_or(false);
    assert_eq!(after_rebind, unchanged, "原键位应失效");
}

#[test]
fn shortcut_key_map_survives_a_restart() {
    let fixture = common::Fixture::new();
    {
        let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
        app.boot
            .settings
            .shortcut_key_map
            .insert("ShowWord".to_string(), "Ctrl+K".to_string());
        app.save_settings_now();
    }

    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    assert_eq!(
        app.boot
            .settings
            .shortcut_key_map
            .get("ShowWord")
            .map(String::as_str),
        Some("Ctrl+K"),
        "快捷键表应写入 setting.json"
    );
    assert_eq!(
        app.shortcuts
            .key_for(typewords_tui::domain::shortcut::ShortcutAction::ShowWord)
            .map(|key| key.to_upstream()),
        Some("Ctrl+K".to_string()),
        "重启后应从存档恢复改绑"
    );
}

#[test]
fn shortcut_editor_rebinds_and_persists() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();

    press(&mut app, KeyCode::Char('s'));
    goto_setting(&mut app, "shortcut_settings");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Shortcuts, "应进入快捷键编辑页");

    // 选中「显示单词」并改绑到 Ctrl+K
    let index = typewords_tui::domain::shortcut::ShortcutAction::ALL
        .iter()
        .position(|action| *action == typewords_tui::domain::shortcut::ShortcutAction::ShowWord)
        .expect("应有显示单词动作");
    while app.shortcut_cursor < index {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.shortcut_cursor, index);

    press(&mut app, KeyCode::Enter);
    assert!(app.shortcut_capturing, "Enter 应进入捕获状态");
    app.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert!(!app.shortcut_capturing, "捕获后应退出捕获状态");
    assert_eq!(
        app.shortcuts
            .key_for(typewords_tui::domain::shortcut::ShortcutAction::ShowWord)
            .map(|key| key.to_upstream()),
        Some("Ctrl+K".to_string())
    );

    press(&mut app, KeyCode::Esc);
    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(
        reloaded
            .shortcuts
            .key_for(typewords_tui::domain::shortcut::ShortcutAction::ShowWord)
            .map(|key| key.to_upstream()),
        Some("Ctrl+K".to_string()),
        "改绑应持久化"
    );
}

#[test]
fn shortcut_editor_rejects_a_conflicting_key() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('s'));
    goto_setting(&mut app, "shortcut_settings");
    press(&mut app, KeyCode::Enter);

    let show_word = typewords_tui::domain::shortcut::ShortcutAction::ShowWord;
    let existing = app
        .shortcuts
        .key_for(show_word)
        .expect("默认键位")
        .to_upstream();

    // 把「显示单词」改绑到已被「收藏」占用的 Enter
    press(&mut app, KeyCode::Enter);
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.shortcuts
            .key_for(show_word)
            .map(|key| key.to_upstream()),
        Some(existing),
        "冲突时不应改绑"
    );
}

/// 清除绑定后该动作不再响应任何键位。
///
/// 早期实现直接 `remove()` 存档里的条目，读回来时又回退到默认键位——
/// 按 `Delete` 等于「恢复默认」，而不是「取消绑定」。
#[test]
fn clearing_a_shortcut_binding_unbinds_the_action() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('s'));
    goto_setting(&mut app, "shortcut_settings");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Shortcuts);

    let show_word = typewords_tui::domain::shortcut::ShortcutAction::ShowWord;
    let index = typewords_tui::domain::shortcut::ShortcutAction::ALL
        .iter()
        .position(|action| *action == show_word)
        .expect("应有显示单词动作");
    while app.shortcut_cursor < index {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.shortcut_cursor, index);

    press(&mut app, KeyCode::Delete);
    assert!(
        app.shortcuts.key_for(show_word).is_none(),
        "清除后不应再有键位"
    );
    assert!(
        app.shortcuts
            .actions_for(&typewords_tui::domain::shortcut::ShortcutKey::new(
                typewords_tui::domain::shortcut::BaseKey::Esc
            ))
            .is_empty(),
        "反查也不该再命中该动作"
    );

    // 重启后仍然是「未绑定」，而不是回到默认键位
    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert!(
        reloaded.shortcuts.key_for(show_word).is_none(),
        "清除绑定应持久化"
    );
    assert_eq!(
        reloaded
            .boot
            .settings
            .shortcut_key_map
            .get("ShowWord")
            .map(String::as_str),
        Some(""),
        "存档里应写空串（与上游一致）"
    );
}

/// 改绑不能撞上终端兼容别名。
///
/// 别名键（`Ctrl+N` = 下一词、`Ctrl+J` = 下一阶段）不落盘，但照样参与
/// 反查；绑上去会让同一个键位同时触发两个动作。
#[test]
fn rebinding_rejects_a_key_used_by_a_terminal_alias() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('s'));
    goto_setting(&mut app, "shortcut_settings");
    press(&mut app, KeyCode::Enter);

    let show_word = typewords_tui::domain::shortcut::ShortcutAction::ShowWord;
    let index = typewords_tui::domain::shortcut::ShortcutAction::ALL
        .iter()
        .position(|action| *action == show_word)
        .expect("应有显示单词动作");
    while app.shortcut_cursor < index {
        press(&mut app, KeyCode::Down);
    }

    // Ctrl+N 是「下一词」的终端兼容别名
    press(&mut app, KeyCode::Enter);
    app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
    assert_eq!(
        app.shortcuts
            .key_for(show_word)
            .map(|key| key.to_upstream()),
        Some("Escape".to_string()),
        "与别名冲突时不应改绑"
    );
}

#[test]
fn group_picker_jumps_to_the_selected_group() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    // 12 个词、每组 4 个 → 3 组
    if let Some(dict) = app.boot.base.study_dict_mut() {
        dict.per_day_study_number = 4;
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    assert_eq!(app.total_groups(), 3);

    app.apply_action(app::Action::OpenGroupPicker);
    assert!(app.group_picker.is_some(), "应打开跳组弹层");
    app.apply_action(app::Action::GroupMove(2));
    app.apply_action(app::Action::GroupSelect);
    assert!(app.group_picker.is_none(), "选择后弹层应关闭");

    let dict = app.boot.base.study_dict().expect("当前词库");
    assert_eq!(dict.last_learn_index, 8, "跳到第 3 组应把进度指到该组开头");
}

#[test]
fn concise_mode_toggles_toolbar_and_panel() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    assert!(app.show_toolbar, "默认显示工具栏");

    app.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(!app.show_toolbar, "简洁模式应隐藏工具栏");
    assert!(!app.show_panel, "简洁模式应隐藏词表面板");

    app.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(app.show_toolbar, "再次触发应恢复工具栏");
}

#[test]
fn theme_shortcut_cycles_the_theme() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    let before = app.boot.settings.theme.clone();

    app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    assert_ne!(app.boot.settings.theme, before, "Ctrl+Q 应切换主题");
}

#[test]
fn repeat_chapter_restarts_the_current_group() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    if let Some(dict) = app.boot.base.study_dict_mut() {
        dict.per_day_study_number = 4;
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);

    // 先推进两个词，再重学本组
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    let index = app.session.as_ref().expect("session").data.index;
    assert!(index > 0, "应已推进");

    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
    let session = app.session.as_ref().expect("session");
    assert_eq!(session.data.index, 0, "重学本组应回到组首");
}

#[test]
fn collect_to_dict_picker_collects_into_the_chosen_dict() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    let word = app.session.as_ref().expect("session").current_word().word;

    // Shift+Enter 打开「收藏到指定词典」
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert!(app.collect_picker.is_some(), "应打开收藏目标弹层");
    app.apply_action(app::Action::CollectSelect);
    assert!(app.collect_picker.is_none(), "选择后弹层应关闭");

    let collect = app.boot.base.collect_word().expect("收藏词典");
    assert!(
        collect.words.iter().any(|item| item.word == word),
        "词应进入收藏词典"
    );
}

#[test]
fn typing_resumes_the_timer_after_an_idle_pause() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);

    // 模拟空闲 3 分钟：推进 tick 触发自动暂停
    let start = app.last_activity;
    app.on_tick(start + 100_000);
    assert!(!app.stat.timer_paused, "未到空闲阈值时不应暂停");
    app.on_tick(start + 200_000);
    assert!(app.stat.timer_paused, "超过空闲阈值应自动暂停");
    assert_eq!(app.stat.timer_pause_reason.as_deref(), Some("auto_idle"));

    // 一有输入就恢复
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(!app.stat.timer_paused, "输入后应自动恢复计时");
    assert_eq!(app.stat.timer_pause_reason, None);
}

#[test]
fn manual_pause_is_not_resumed_by_typing() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    app.on_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::CONTROL));
    assert!(app.stat.timer_paused, "Ctrl+, 应暂停计时");
    assert_eq!(app.stat.timer_pause_reason.as_deref(), Some("manual"));

    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(app.stat.timer_paused, "手动暂停不应被输入自动恢复");
}

#[test]
fn word_note_can_be_edited_and_persists() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    let word = app.session.as_ref().expect("session").current_word().word;

    // Ctrl+O 打开笔记编辑
    app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.edit.is_some(), "应打开输入弹层");
    for character in "记住这个词".chars() {
        app.apply_action(app::Action::EditChar(character));
    }
    app.apply_action(app::Action::EditSave);
    assert!(app.edit.is_none(), "保存后弹层应关闭");

    assert_eq!(
        app.boot.base.note_data.get(&word).map(String::as_str),
        Some("记住这个词"),
        "笔记应写入词典状态"
    );

    // 重启后仍在
    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(
        reloaded.boot.base.note_data.get(&word).map(String::as_str),
        Some("记住这个词"),
        "笔记应持久化"
    );
}

#[test]
fn clearing_a_note_removes_it() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    let word = app.session.as_ref().expect("session").current_word().word;
    app.boot
        .base
        .note_data
        .insert(word.clone(), "旧笔记".to_string());

    app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    for _ in 0..3 {
        app.apply_action(app::Action::EditBackspace);
    }
    app.apply_action(app::Action::EditSave);
    assert!(
        !app.boot.base.note_data.contains_key(&word),
        "清空后应删除笔记"
    );
}

#[test]
fn word_test_mode_asks_questions_and_records_wrong_answers() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    // 切到「随机单词测试」模式
    app.boot.settings.word_practice_mode =
        typewords_tui::domain::types::WordPracticeMode::ShuffleWordsTest;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::WordTest, "应进入单词测试页");
    let (wrong_index, answer) = {
        let state = app.word_test.as_ref().expect("测试状态");
        assert!(!state.words.is_empty(), "应有题目");
        let question = state.question.as_ref().expect("题目");
        let wrong_index = (question.correct_index + 1) % question.candidates.len();
        let answer = question.candidates[question.correct_index]
            .word
            .word
            .clone();
        (wrong_index, answer)
    };
    app.apply_action(app::Action::WordTestChoose(wrong_index));
    assert_eq!(app.word_test.as_ref().expect("测试状态").wrong, 1);

    // 正确答案应进入错词本
    let wrong_dict = app.boot.base.wrong().expect("错词本");
    assert!(
        wrong_dict.words.iter().any(|item| item.word == answer),
        "答错的正确答案应进入错词本"
    );

    // 空格进入下一题
    app.apply_action(app::Action::WordTestNext);
    let state = app.word_test.as_ref().expect("测试状态");
    assert_eq!(state.index, 1, "应进入下一题");
    assert_eq!(state.selected, None, "新的一题不应有选择");
}

#[test]
fn word_test_finishes_with_a_summary() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_practice_mode =
        typewords_tui::domain::types::WordPracticeMode::ShuffleWordsTest;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::WordTest);

    // 走完所有题目
    for _ in 0..200 {
        let Some(state) = app.word_test.as_ref() else {
            break;
        };
        if state.selected.is_none() {
            let question = state.question.as_ref().expect("题目");
            app.apply_action(app::Action::WordTestChoose(question.correct_index));
        }
        app.apply_action(app::Action::WordTestNext);
    }
    assert!(app.word_test.is_none(), "答完后应结束测试");
    assert!(app.summary.is_some(), "应给出结算");
}

#[test]
fn stats_page_switches_views_and_navigates_the_calendar() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(app.screen, app::Screen::Stats);

    let day = app.stats_day;
    press(&mut app, KeyCode::Right);
    assert_eq!(app.stats_day, day + chrono::Duration::days(1));
    press(&mut app, KeyCode::Left);
    assert_eq!(app.stats_day, day);

    let (year, month) = app.stats_month;
    press(&mut app, KeyCode::Char('.'));
    assert_eq!(app.stats_month, (year, month + 1), "应翻到下个月");

    assert!(!app.stats_by_dict);
    press(&mut app, KeyCode::Char('v'));
    assert!(app.stats_by_dict, "v 应切换到按词典视图");
    press(&mut app, KeyCode::Char('v'));
    assert!(!app.stats_by_dict, "再按一次应切回日历视图");
}

#[test]
fn audio_settings_toggle_and_persist() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('s'));

    goto_setting(&mut app, "keyboard_sound");
    press(&mut app, KeyCode::Enter);
    assert!(!app.boot.settings.keyboard_sound, "键音应被关闭");

    goto_setting(&mut app, "word_sound");
    press(&mut app, KeyCode::Enter);
    assert!(!app.boot.settings.word_sound, "单词发音应被关闭");

    goto_setting(&mut app, "tts_source");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.boot.settings.tts_source, "youdao", "应切到有道发音");

    press(&mut app, KeyCode::Esc);
    let reloaded = app::build_for_test(paths, fixture.cli()).unwrap();
    assert!(!reloaded.boot.settings.keyboard_sound);
    assert!(!reloaded.boot.settings.word_sound);
    assert_eq!(reloaded.boot.settings.tts_source, "youdao");
}

/// 单词测试结算后要回到可用的页面。
///
/// 早期实现把 `screen` 设成 `Screen::Practice`，但单词测试没有练习会话，
/// 结算弹层一关就落到一个空白页——没有任何提示，也没有退路（Esc 在练习页
/// 是「显示单词」）。
#[test]
fn word_test_summary_lands_on_a_usable_screen() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_practice_mode =
        typewords_tui::domain::types::WordPracticeMode::ShuffleWordsTest;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::WordTest);

    for _ in 0..200 {
        let Some(state) = app.word_test.as_ref() else {
            break;
        };
        if state.selected.is_none() {
            let question = state.question.as_ref().expect("题目");
            app.apply_action(app::Action::WordTestChoose(question.correct_index));
        }
        app.apply_action(app::Action::WordTestNext);
    }
    assert!(app.summary.is_some(), "应给出结算");

    // 关掉结算弹层（Esc → 回主页）
    app.apply_action(app::Action::SummaryHome);
    assert_eq!(
        app.screen,
        app::Screen::Home,
        "结算后应回到主页，而不是没有会话的空白练习页"
    );
    assert!(app.session.is_none(), "单词测试不应留下练习会话");
}

/// 单词测试的用时不应沿用上一轮练习的残留值。
#[test]
fn word_test_tracks_its_own_elapsed_time() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    // 先跑一轮普通练习，把 stat.spend 弄脏
    app.boot.settings.word_practice_mode = typewords_tui::domain::types::WordPracticeMode::System;
    press(&mut app, KeyCode::Enter);
    let base = typewords_tui::domain::timer::now_ms();
    app.on_tick(base);
    app.on_tick(base + 3_000);
    assert!(app.stat.spend > 0, "普通练习应累计用时");

    // 离开练习后再开单词测试
    app.leave_practice();
    app.boot.settings.word_practice_mode =
        typewords_tui::domain::types::WordPracticeMode::ShuffleWordsTest;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::WordTest);
    assert_eq!(app.word_test_spend, 0, "单词测试应从零开始计时");

    // 计时器按秒推进：每个满 1000ms 的 tick 记 1000ms。
    let base = typewords_tui::domain::timer::now_ms();
    app.on_tick(base);
    app.on_tick(base + 1_000);
    app.on_tick(base + 2_000);
    assert_eq!(app.word_test_spend, 2000, "单词测试应累计自己的用时");
}

/// 「重学本组」要重学当前这一组，而不是下一组。
///
/// 早期实现的 `current_group()` 把本轮任务的新词总数加进了词典进度，
/// 于是正在学第 1 组时按 `Ctrl+Enter` 会跳到第 2 组的词——「重学」变成了
/// 「跳到下一组」。
#[test]
fn repeating_the_group_repeats_the_current_one() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    // 每天学 5 个词 -> 12 个词分 3 组
    if let Some(dict) = app.boot.base.study_dict_mut() {
        dict.per_day_study_number = 5;
    }

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    let before: Vec<String> = app
        .session
        .as_ref()
        .expect("会话")
        .data
        .words
        .iter()
        .map(|word| word.word.clone())
        .collect();
    assert_eq!(before.len(), 5, "第一组应有 5 个词");
    assert_eq!(app.current_group(), 1, "刚开始学的是第 1 组");
    assert_eq!(app.total_groups(), 3, "12 个词每天 5 个应分 3 组");

    // 重学本组（上游 `Ctrl+Enter` → `RepeatChapter`）
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
    let after: Vec<String> = app
        .session
        .as_ref()
        .expect("会话")
        .data
        .words
        .iter()
        .map(|word| word.word.clone())
        .collect();
    assert_eq!(after, before, "重学本组应回到当前组的词表，而不是下一组");
    assert_eq!(app.current_group(), 1, "重学后仍在第 1 组");
}

/// 跳组弹层的光标要停在当前组。
///
/// 弹层用 `current_group() - 1` 当光标初值，当前组算错时光标就落在别的组上，
/// 打开弹层再直接回车会跳到不相干的一组。
#[test]
fn the_group_picker_opens_on_the_current_group() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    if let Some(dict) = app.boot.base.study_dict_mut() {
        dict.per_day_study_number = 5;
    }

    press(&mut app, KeyCode::Enter);
    app.open_group_picker();
    let picker = app.group_picker.as_ref().expect("跳组弹层");
    assert_eq!(picker.cursor, 0, "弹层光标应停在第 1 组");

    // 回车即跳到当前组，词表不变
    let before: Vec<String> = app
        .session
        .as_ref()
        .expect("会话")
        .data
        .words
        .iter()
        .map(|word| word.word.clone())
        .collect();
    app.apply_action(app::Action::GroupSelect);
    let after: Vec<String> = app
        .session
        .as_ref()
        .expect("会话")
        .data
        .words
        .iter()
        .map(|word| word.word.clone())
        .collect();
    assert_eq!(after, before, "跳到当前组不应换词表");
}
