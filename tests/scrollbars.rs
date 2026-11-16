//! 滚动条与「没有溢出就不能滚」的行为测试。
//!
//! 覆盖三类契约：
//! - 有滚动功能的页面在内容溢出时画出右侧滚动条（轨道 `│` + 滑块 `█`）；
//! - 内容一屏放得下时**不画**滚动条，按键 / 滚轮也滚不动；
//! - 滚动上限按当前视口现算，光标列表滚动时行、命中区与滚动条一起移动。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use typewords_tui::app::{self, Action, Screen};
use typewords_tui::ui::shell::{self, Nav};

fn area_of(size: (u16, u16)) -> Rect {
    Rect::new(0, 0, size.0, size.1)
}

fn app_of(size: (u16, u16)) -> (common::Fixture, app::App) {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("build app");
    app.set_viewport(area_of(size));
    (fixture, app)
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut app::App, character: char) {
    app.on_key(KeyEvent::new(
        KeyCode::Char(character),
        KeyModifiers::CONTROL,
    ));
}

fn click(app: &mut app::App, area: Rect, rect: Rect) {
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + rect.width / 2,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
}

/// 当前页面内容区（外壳里正文所在区域）。
fn content_rect(app: &app::App) -> Rect {
    shell::layout(app.viewport, Nav::from_screen(app.screen)).content
}

/// 渲染后的字符格 `(列, 行, 字符)`；宽字符的续接单元跳过。
fn cells(app: &app::App, width: u16, height: u16) -> Vec<(u16, u16, char)> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut result = Vec::new();
    for y in 0..height {
        let mut skip = 0usize;
        for x in 0..width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            let symbol = cell.symbol();
            let symbol_width = UnicodeWidthStr::width(symbol);
            if symbol_width > 1 {
                skip = symbol_width - 1;
            }
            let mut characters = symbol.chars();
            if let (Some(character), None) = (characters.next(), characters.next()) {
                result.push((x, y, character));
            }
        }
    }
    result
}

/// 内容区里滚动条滑块（`█`）的格子；没有溢出时应为空。
///
/// 只统计内容列：侧栏的 `TW` 大字 logo 也用 `█`，不能算作滚动条。
fn content_thumbs(app: &app::App, width: u16, height: u16) -> Vec<(u16, u16)> {
    let content = content_rect(app);
    cells(app, width, height)
        .into_iter()
        .filter(|(column, _, symbol)| *column >= content.x && *symbol == '█')
        .map(|(column, row, _)| (column, row))
        .collect()
}

fn has_scroll_target(app: &app::App, area: Rect, down: bool) -> bool {
    let wanted = if down {
        Action::StatsScroll(1)
    } else {
        Action::StatsScroll(-1)
    };
    typewords_tui::ui::hit::targets(app, area)
        .iter()
        .any(|target| target.action == wanted)
}

fn render_text(app: &app::App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut skip = 0usize;
        for x in 0..width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                let symbol = cell.symbol();
                out.push_str(symbol);
                let symbol_width = UnicodeWidthStr::width(symbol);
                if symbol_width > 1 {
                    skip = symbol_width - 1;
                }
            }
        }
        out.push('\n');
    }
    out
}

/// 内容放得下时：不画滚动条，↑/↓ 与 PgUp/PgDn 都滚不动。
#[test]
fn pages_without_overflow_have_no_scrollbar_and_do_not_scroll() {
    // 120 × 40 的「快捷键」分页两栏正好一屏放下。
    let (_fixture, mut app) = app_of((120, 40));
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.screen, Screen::Help);
    assert_eq!(
        typewords_tui::ui::help::max_scroll(&app),
        0,
        "内容一屏放得下时滚动上限应为 0"
    );
    assert!(
        content_thumbs(&app, 120, 40).is_empty(),
        "没有溢出不该画滚动条"
    );
    for code in [KeyCode::Down, KeyCode::PageDown] {
        press(&mut app, code);
    }
    assert_eq!(app.help_scroll, 0, "没有溢出时按键不该滚动帮助正文");

    // 120 × 40 的统计页：日历 + 记录也在同一屏里。
    let (_fixture, mut app) = app_of((120, 40));
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Stats);
    assert_eq!(
        typewords_tui::ui::stats::max_scroll(&app),
        0,
        "内容一屏放得下时统计页滚动上限应为 0"
    );
    assert!(
        content_thumbs(&app, 120, 40).is_empty(),
        "没有溢出不该画滚动条"
    );
    for code in [KeyCode::Down, KeyCode::PageDown] {
        press(&mut app, code);
    }
    assert_eq!(app.stats_scroll, 0, "没有溢出时按键不该滚动统计正文");
}

/// 帮助页溢出：正文右侧画滚动条，滚动到上限停住，滑块上方 / 下方仍是可点轨道。
#[test]
fn overflowing_help_draws_a_scrollbar_and_stops_at_the_bottom() {
    let size = (80, 24);
    let area = area_of(size);
    let (_fixture, mut app) = app_of(size);
    press(&mut app, KeyCode::Char('?'));

    let max = typewords_tui::ui::help::max_scroll(&app);
    assert!(max > 0, "80 × 24 下帮助正文应超出视口");
    assert!(
        !content_thumbs(&app, size.0, size.1).is_empty(),
        "有溢出时右侧应出现滚动条滑块"
    );
    assert!(
        !typewords_tui::ui::hit::targets(&app, area)
            .iter()
            .any(|target| target.action == Action::HelpScroll(-1)),
        "在顶部时不应注册上滚轨道"
    );

    for _ in 0..40 {
        press(&mut app, KeyCode::PageDown);
    }
    assert_eq!(app.help_scroll, max, "PgDn 应停在滚动上限");
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.help_scroll, max, "到底后继续 PgDn 不该滚动");

    let targets = typewords_tui::ui::hit::targets(&app, area);
    assert!(
        !targets
            .iter()
            .any(|target| target.action == Action::HelpScroll(1)),
        "到底后不应再注册下滚轨道"
    );
    assert!(
        targets
            .iter()
            .any(|target| target.action == Action::HelpScroll(-1)),
        "到底后滑块上方应有可点的上滚轨道"
    );
}

/// 帮助页刚向下滚一行时，滚动条顶部仍要有可点的上滚轨道。
///
/// 按比例取整会让前几个偏移的滑块贴顶；旧的 `↑` 指示器在 offset > 0 时
/// 一直可点，换成滚动条后不能丢掉这条鼠标回滚路径。
#[test]
fn help_scrollbar_can_scroll_back_up_right_after_the_first_down() {
    let size = (80, 24);
    let area = area_of(size);
    let (_fixture, mut app) = app_of(size);
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.help_scroll, 1);

    let up = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::HelpScroll(-1))
        .expect("滚了一行后顶部应有上滚轨道");
    click(&mut app, area, up.rect);
    assert_eq!(app.help_scroll, 0, "点上滚轨道应回到顶部");
}

/// 统计页溢出：点滚动条轨道一次滚一行，滚动上限按视口现算。
#[test]
fn overflowing_stats_scrolls_and_its_track_moves_one_row() {
    let size = (60, 20);
    let area = area_of(size);
    let (_fixture, mut app) = app_of(size);
    press(&mut app, KeyCode::Char('4'));
    assert_eq!(app.screen, Screen::Stats);

    let max = typewords_tui::ui::stats::max_scroll(&app);
    assert!(max > 0, "60 × 20 下统计正文应超出视口");
    assert!(
        !content_thumbs(&app, size.0, size.1).is_empty(),
        "有溢出时右侧应出现滚动条滑块"
    );

    // 滑块下方的轨道可点：一次滚一行。
    let down = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::StatsScroll(1))
        .expect("应有下滚轨道");
    click(&mut app, area, down.rect);
    assert_eq!(app.stats_scroll, 1, "点下滚轨道应滚动一行");

    // 滚到底后停住；滑块上方的轨道可点上滚。
    for _ in 0..30 {
        press(&mut app, KeyCode::PageDown);
    }
    assert_eq!(app.stats_scroll, max, "PgDn 应停在滚动上限");
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.stats_scroll, max, "到底后继续 PgDn 不该滚动");
    assert!(has_scroll_target(&app, area, false), "到底后应有上滚轨道");
    let up = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::StatsScroll(-1))
        .expect("应有上滚轨道");
    click(&mut app, area, up.rect);
    assert_eq!(app.stats_scroll, max - 1, "点上滚轨道应回滚一行");
}

/// 设置页：内容一屏放得下不画滚动条；放不下才画，并跟随光标滚动。
#[test]
fn settings_scrollbar_appears_only_when_the_list_overflows() {
    // 120 × 60：42 行条目一屏放得下，光标移到最后一行的偏移仍是 0。
    let (_fixture, mut app) = app_of((120, 60));
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.screen, Screen::Settings);
    app.settings_cursor = app::SETTING_ROWS.len() - 1;
    let content = content_rect(&app);
    let layout = typewords_tui::ui::settings::layout(&app, content);
    assert_eq!(layout.offset, 0, "内容放得下时光标移到底也不该滚动");
    assert!(
        content_thumbs(&app, 120, 60).is_empty(),
        "没有溢出不该画滚动条"
    );

    // 120 × 40：放不下 → 有滚动条，光标到底时列表跟到底。
    let (_fixture, mut app) = app_of((120, 40));
    press(&mut app, KeyCode::Char('s'));
    app.settings_cursor = app::SETTING_ROWS.len() - 1;
    let content = content_rect(&app);
    let layout = typewords_tui::ui::settings::layout(&app, content);
    assert!(layout.offset > 0, "光标移到底部时列表应跟着滚");
    assert!(
        !content_thumbs(&app, 120, 40).is_empty(),
        "有溢出时右侧应出现滚动条滑块"
    );
    let last = layout
        .rows
        .iter()
        .find(|(entry, _)| *entry == Some(app.settings_cursor))
        .expect("最后一行应可见");
    assert!(
        last.1.y < content.bottom(),
        "选中行滚出视口就等于没跟光标滚"
    );
}

/// 词条管理页：长列表跟随光标滚动，右侧滚动条同源。
#[test]
fn the_word_list_follows_the_cursor_and_shows_a_scrollbar() {
    let size = (80, 24);
    let area = area_of(size);
    let (_fixture, mut app) = app_of(size);
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.screen, Screen::Words);
    // fixture 只有 12 个词，一屏放得下；进页后再补到 52 个（进页会按 id
    // 从词条文件重新装载词表，进页前塞进去的词会被覆盖）。
    {
        let words = &mut app.boot.base.study_dict_mut().expect("应有学习词典").words;
        let template = words[0].clone();
        for index in 0..40 {
            let mut word = template.clone();
            word.word = format!("extra{index:02}");
            words.push(word);
        }
    }
    app.words_cursor = app.filtered_word_indices().len() - 1;

    let content = content_rect(&app);
    let layout = typewords_tui::ui::words::layout(&app, content);
    assert!(layout.offset > 0, "光标到底时词表应跟着滚");
    assert!(
        !content_thumbs(&app, size.0, size.1).is_empty(),
        "词表溢出时应出现滚动条"
    );
    let row = app.words_cursor - layout.offset;
    assert!(row < layout.list.height as usize, "光标词必须在视口里");
    assert!(
        render_text(&app, size.0, size.1).contains("extra39"),
        "滚动后应显示最后一条词"
    );

    // 行目标跟随同一份偏移：点最后一行会落到最后一条词。
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::WordsMove(0))
        .expect("最后一行应有命中目标");
    assert_eq!(target.rect.y, layout.list.y + row as u16);
}

/// 文章页：两个列表都跟随各自光标滚动，右侧各有一条滚动条。
#[test]
fn the_article_list_follows_the_cursor_and_shows_a_scrollbar() {
    let size = (120, 20);
    let area = area_of(size);
    let (_fixture, mut app) = app_of(size);
    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.screen, Screen::Articles);

    // 复制成 30 篇，保证右栏放不下。
    let template = app.article_list.first().cloned().expect("fixture 应有文章");
    app.article_list = (0..30)
        .map(|index| {
            let mut article = template.clone();
            article.id = 100 + index;
            article.title = format!("文章{index:02}");
            article
        })
        .collect();
    app.article_pane_left = false;
    app.article_cursor = app.article_list.len() - 1;

    let content = content_rect(&app);
    let layout = typewords_tui::ui::articles::layout(&app, content);
    let pane = &layout.articles_list;
    assert!(pane.offset > 0, "文章光标到底时右栏应跟着滚");
    let row = app.article_cursor - pane.offset;
    assert!(row < pane.visible, "选中文章必须在视口里");
    assert!(
        !content_thumbs(&app, size.0, size.1).is_empty(),
        "右栏溢出时应出现滚动条"
    );
    assert!(
        render_text(&app, size.0, size.1).contains("文章29"),
        "滚动后应显示最后一篇文章"
    );

    // 命中区与绘制同源：最后一行的目标 y 就是它在视口里的行。
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::ArticleMove(0))
        .expect("最后一行应有命中目标");
    assert_eq!(target.rect.y, pane.list.y + row as u16);
}

/// 快捷键页与高级 FSRS 页：列表溢出时同样有滚动条，并跟随光标。
#[test]
fn shortcuts_and_adv_fsrs_draw_scrollbars_when_their_lists_overflow() {
    // 快捷键页 48 个动作，60 × 20 放不下。
    let size = (60, 20);
    let area = area_of(size);
    let (_fixture, mut app) = app_of(size);
    app.screen = Screen::Shortcuts;
    app.shortcut_cursor = 0;
    let content = content_rect(&app);
    let layout = typewords_tui::ui::shortcuts::layout(&app, content);
    assert_eq!(layout.offset, 0);
    assert!(
        !content_thumbs(&app, size.0, size.1).is_empty(),
        "快捷键列表溢出时应出现滚动条"
    );
    app.shortcut_cursor = typewords_tui::domain::shortcut::ShortcutAction::ALL.len() - 1;
    let layout = typewords_tui::ui::shortcuts::layout(&app, content);
    assert!(layout.offset > 0, "快捷键光标到底时列表应跟着滚");
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::ShortcutCapture)
        .expect("选中的最后一行应有改绑命中目标");
    let row = app.shortcut_cursor - layout.offset;
    assert_eq!(target.rect.y, content.y + 1 + row as u16);

    // 高级 FSRS 页 30 行条目，60 × 20 放不下。
    let (_fixture, mut app) = app_of(size);
    app.screen = Screen::AdvFsrs;
    app.adv_fsrs_cursor = 0;
    let content = content_rect(&app);
    let layout = typewords_tui::ui::adv_fsrs::layout(&app, content);
    assert_eq!(layout.offset, 0);
    assert!(
        !content_thumbs(&app, size.0, size.1).is_empty(),
        "高级 FSRS 列表溢出时应出现滚动条"
    );
    app.adv_fsrs_cursor = app::AdvFsrsRow::ALL.len() - 1;
    let layout = typewords_tui::ui::adv_fsrs::layout(&app, content);
    assert!(layout.offset > 0, "FSRS 光标到底时列表应跟着滚");
    let last = layout
        .rows
        .iter()
        .find(|(entry, _)| *entry == Some(app.adv_fsrs_cursor))
        .expect("最后一行应可见");
    assert!(last.1.y < content.bottom());
}

/// 流程页：列表跟随光标滚动，溢出时出现滚动条。
#[test]
fn the_flow_list_follows_the_cursor_and_shows_a_scrollbar() {
    let size = (60, 20);
    let area = area_of(size);
    let (_fixture, mut app) = app_of(size);
    ctrl(&mut app, 'f');
    assert_eq!(app.screen, Screen::Flows);
    // 内置流程之外再复制若干条，让 60 × 20 下的列表放不下。
    for _ in 0..20 {
        press(&mut app, KeyCode::Char('n'));
    }
    app.flow_cursor = app.flow_entries().len() - 1;

    let content = content_rect(&app);
    let layout = typewords_tui::ui::flows::layout(&app, content);
    assert!(layout.offset > 0, "光标到底时流程列表应跟着滚");
    assert!(
        !content_thumbs(&app, size.0, size.1).is_empty(),
        "流程列表溢出时应出现滚动条"
    );
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::FlowMove(0))
        .expect("最后一行应有命中目标");
    let row = app.flow_cursor - layout.offset;
    assert_eq!(target.rect.y, layout.list.y + row as u16);
}
