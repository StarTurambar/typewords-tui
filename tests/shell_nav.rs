//! 外壳导航（侧栏 / 顶栏）与主页可点元素的回归测试。
//!
//! 早期侧栏只有「单词 / 词库 / 统计 / 帮助」，文章页虽然已经实现，却只能
//! 从主页按 `a` 进入：侧栏没有入口、数字键跳不到、文章页的高亮还错指
//! 「单词」。这里钉住这几条不变量。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Action, Screen};
use unicode_width::UnicodeWidthStr;

fn render_text(app: &app::App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        let mut skip_cells = 0usize;
        for x in 0..width {
            if skip_cells > 0 {
                skip_cells -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                out.push_str(cell.symbol());
                let display_width = UnicodeWidthStr::width(cell.symbol());
                if display_width > 1 {
                    skip_cells = display_width - 1;
                }
            }
        }
        out.push('\n');
    }
    out
}

/// 取一行里落在 `[start, start + width)` 显示列区间内的文字。
///
/// 按**显示宽度**切，不是按字符个数——中文占两列，直接按字符切会错位。
fn slice_by_columns(line: &str, start: u16, width: u16) -> String {
    let mut out = String::new();
    let mut column = 0u16;
    for character in line.chars() {
        let char_width = UnicodeWidthStr::width(character.to_string().as_str()) as u16;
        if column >= start && column < start + width {
            out.push(character);
        }
        column += char_width;
        if column >= start + width {
            break;
        }
    }
    out
}

/// 宽屏侧栏里「文章」这一项必须在。
///
/// 上游侧栏是「单词 / 文章 / 反馈 / 文档 / 帮助 / 设置」，文章是与单词
/// 并列的一级页面。TUI 缺了它，文章功能就成了只有 `a` 键的隐藏入口。
#[test]
fn wide_sidebar_lists_the_articles_page() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let text = render_text(&app, 120, 40);
    let sidebar: Vec<&str> = text.lines().take(40).collect();
    assert!(
        sidebar.iter().any(|line| line.contains("文章")),
        "侧栏应有「文章」入口"
    );
}

/// 侧栏的编号必须连续，不能跳号。
///
/// 早期「统计 3」后面直接是「帮助 5」——「4」消失了，因为设置被塞在
/// 底部固定区，和主导航区不在同一组。
#[test]
fn wide_sidebar_number_keys_are_consecutive() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let text = render_text(&app, 120, 40);
    let mut numbers: Vec<String> = Vec::new();
    for line in text.lines() {
        // 侧栏只占左边 26 列，按显示宽度切（中文占两列）。
        let side = slice_by_columns(line, 0, 25);
        let trimmed = side.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        let Some((_, key)) = trimmed.rsplit_once(' ') else {
            continue;
        };
        if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) {
            numbers.push(key.to_string());
        }
    }
    assert!(
        numbers.len() >= 6,
        "侧栏应有 6 个带数字键的入口，实际 {numbers:?}"
    );
    for (index, key) in numbers.iter().enumerate() {
        assert_eq!(
            key,
            &(index + 1).to_string(),
            "侧栏数字键应连续，实际 {numbers:?}"
        );
    }
}

/// 文章页的高亮要落在「文章」上，不能错指「单词」。
///
/// 早期 `Nav::from_screen` 把 `Articles` 映射成 `Nav::Home`，用户站在
/// 文章页却看到「单词」被选中。
#[test]
fn articles_page_highlights_the_articles_nav_item() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.screen = Screen::Articles;
    let theme = app.theme();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();

    // 找出侧栏里带 selection 背景的行，确认它是「文章」那一行。
    let mut selected_rows: Vec<u16> = Vec::new();
    for y in 0..40u16 {
        let mut highlighted = false;
        for x in 0..25u16 {
            if let Some(cell) = buffer.cell((x, y))
                && cell.bg == theme.selection
            {
                highlighted = true;
                break;
            }
        }
        if highlighted {
            selected_rows.push(y);
        }
    }
    assert!(!selected_rows.is_empty(), "文章页应有高亮的侧栏项");
    let row = selected_rows[0];
    let line: String = (0..25)
        .filter_map(|x| buffer.cell((x, row)).map(|cell| cell.symbol().to_string()))
        .collect();
    assert!(
        line.contains('文') && line.contains('章'),
        "高亮行应是「文章」，实际 {line:?}"
    );
}

/// 窄屏顶栏也要有「文章」标签。
#[test]
fn narrow_topbar_lists_the_articles_page() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let text = render_text(&app, 90, 30);
    let top = text.lines().next().unwrap();
    assert!(top.contains("文章"), "窄屏顶栏应有「文章」标签: {top:?}");
}

/// 「+ / d 管理」卡片要能点。
///
/// 卡片上写着「d」，点它就该去词库页——早期它只画不注册命中区，是个
/// 死字。命中矩形是整张卡（「d 管理」文字只是卡上的一行），所以判定条件
/// 是矩形**覆盖**文字所在行。
#[test]
fn the_manage_dict_tile_is_clickable() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let text = render_text(&app, 120, 40);
    let lines: Vec<&str> = text.lines().collect();

    let manage_row = lines
        .iter()
        .position(|line| line.contains("d 管理"))
        .expect("应渲染出「d 管理」卡片");
    // 卡片在正文区（侧栏右侧），标题栏的「d 词库」不是这张卡。
    let hit = targets.iter().any(|target| {
        target.rect.x > 25
            && target.rect.y <= manage_row as u16
            && (manage_row as u16) < target.rect.y + target.rect.height
    });
    assert!(hit, "「d 管理」卡片（第 {manage_row} 行）应有命中目标");
}

/// 命中目标必须真的落在卡片上，不能只对上行号就算数。
#[test]
fn the_manage_dict_tile_target_matches_its_rendered_rect() {
    let fixture = common::Fixture::new();
    let app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let text = render_text(&app, 120, 40);
    let lines: Vec<&str> = text.lines().collect();

    let manage_row = lines
        .iter()
        .position(|line| line.contains("d 管理"))
        .expect("应渲染出「d 管理」卡片");
    let target = targets
        .iter()
        .find(|target| {
            target.rect.x > 25
                && target.rect.y <= manage_row as u16
                && (manage_row as u16) < target.rect.y + target.rect.height
        })
        .expect("「d 管理」卡片应有命中目标");

    // 按显示宽度切出命中矩形覆盖的文本，确认落在「d 管理」上。
    let covered = slice_by_columns(lines[manage_row], target.rect.x, target.rect.width);
    assert!(
        covered.contains("d") || covered.contains("管理"),
        "命中矩形应盖住「d 管理」文字，实际 {covered:?}"
    );
}

/// 主页的「模式 X ▾」标签不能被截断。
///
/// 早期宽度写死成「模式 学习 ▾」，模式名一变长就被截成「模式 随机…」，
/// 连下拉三角 `▾` 都没了——用户看不出那是个可点的下拉菜单。
#[test]
fn the_mode_label_is_not_truncated() {
    use typewords_tui::domain::types::WordPracticeMode;

    for mode in [
        WordPracticeMode::System,
        WordPracticeMode::Free,
        WordPracticeMode::Shuffle,
        WordPracticeMode::ShuffleWordsTest,
        WordPracticeMode::Custom,
    ] {
        let fixture = common::Fixture::new();
        let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
        app.boot.settings.word_practice_mode = mode;
        let text = render_text(&app, 120, 40);
        let line = text
            .lines()
            .find(|line| line.contains("今日任务"))
            .expect("应渲染出「今日任务」标题行");
        assert!(
            line.contains('▾'),
            "{mode:?} 的模式标签应保留下拉三角: {line:?}"
        );
        assert!(
            !line.contains('…'),
            "{mode:?} 的模式标签不应被截断: {line:?}"
        );
    }
}

/// 模式标签的命中矩形要盖住完整的标签文字。
#[test]
fn the_mode_label_target_covers_its_text() {
    use typewords_tui::domain::types::WordPracticeMode;

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.boot.settings.word_practice_mode = WordPracticeMode::Custom;
    let area = Rect::new(0, 0, 120, 40);
    let targets = typewords_tui::ui::hit::targets(&app, area);
    let text = render_text(&app, 120, 40);
    let lines: Vec<&str> = text.lines().collect();

    let target = targets
        .iter()
        .find(|target| target.action == Action::OpenModePicker)
        .expect("模式标签应有命中目标");
    let covered = slice_by_columns(
        lines[target.rect.y as usize],
        target.rect.x,
        target.rect.width,
    );
    assert!(
        covered.contains('▾'),
        "命中矩形应盖住完整的「模式 X ▾」，实际 {covered:?}"
    );
}

/// 帮助页要列出文章与流程的入口键。
///
/// 早期帮助页只有 `d / s / t`，用户按不到 `a` 和 `Ctrl+F`，只能查文档。
#[test]
fn help_lists_the_articles_and_flows_shortcuts() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    let text = render_text(&app, 120, 40);
    assert!(text.contains("文章"), "帮助页应说明文章入口");
    assert!(text.contains("Ctrl+F"), "帮助页应说明练习流程入口");
}
