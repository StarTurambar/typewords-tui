//! 文章背诵：书本列表、断句与句子跟打。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use typewords_tui::app;
use unicode_width::UnicodeWidthStr;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// 打开文章页并开始第一篇文章（fixture 里的文章 11：Hello there. / General Kenobi.）。
fn start_first_article(app: &mut app::App) {
    app.open_articles();
    press(app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice, "应进入文章跟打页");
    let practice = app.article_practice.as_ref().expect("文章练习状态");
    assert_eq!(practice.article_id, 11, "应开始第一篇测试文章");
    assert_eq!(practice.index, 0, "应从句首开始");
}

/// 当前句的跟打目标（字母、数字与词间空格按顺序排成的字符序列）。
fn current_target(app: &app::App) -> Vec<char> {
    let practice = app.article_practice.as_ref().expect("文章练习状态");
    practice.sentences[practice.index].typing_target()
}

/// 逐字符输入，经过真实的按键路由。
fn type_text(app: &mut app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

/// 把当前文章整篇打完（逐句输入整句文本，标点会被忽略），直到结算弹出。
fn finish_article(app: &mut app::App) {
    for _ in 0..2000 {
        let Some(practice) = app.article_practice.as_ref() else {
            return;
        };
        let Some(sentence) = practice.sentences.get(practice.index) else {
            return;
        };
        let text = sentence.text.clone();
        for character in text.chars() {
            press(app, KeyCode::Char(character));
        }
    }
    panic!("文章在 2000 轮内没有打完");
}

/// 渲染一帧，把测试终端的缓冲克隆出来，用于按坐标断言字符颜色。
fn render_buffer(app: &app::App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    terminal.backend().buffer().clone()
}

/// 把整屏缓冲拼成文本（跳过宽字符的续接单元）。
fn render_text(app: &app::App, width: u16, height: u16) -> String {
    let buffer = render_buffer(app, width, height);
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

/// 只取结算弹层边框内部的文本，避免断言被遮罩下面的主页内容干扰。
fn summary_inner_text(app: &app::App, width: u16, height: u16) -> String {
    let buffer = render_buffer(app, width, height);
    let mut corner = None;
    'outer: for y in 0..height {
        for x in 0..width {
            if buffer.cell((x, y)).is_some_and(|cell| cell.symbol() == "╭") {
                corner = Some((x, y));
                break 'outer;
            }
        }
    }
    let (left, top) = corner.expect("结算弹层应有左上圆角");
    let right = (left..width)
        .find(|x| {
            buffer
                .cell((*x, top))
                .is_some_and(|cell| cell.symbol() == "╮")
        })
        .expect("结算弹层应有右上圆角");
    let bottom = (top..height)
        .find(|y| {
            buffer
                .cell((left, *y))
                .is_some_and(|cell| cell.symbol() == "╰")
        })
        .expect("结算弹层应有左下圆角");
    let mut out = String::new();
    for y in (top + 1)..bottom {
        let mut skip = 0u16;
        for x in (left + 1)..right {
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

/// 在缓冲里找某个字符所在的单元坐标。
fn find_symbol(buffer: &Buffer, symbol: char) -> Option<(u16, u16)> {
    let mut text = [0u8; 4];
    let symbol = symbol.encode_utf8(&mut text);
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if let Some(cell) = buffer.cell((x, y))
                && cell.symbol() == symbol
            {
                return Some((x, y));
            }
        }
    }
    None
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

/// 文章页的选中行要有可见高亮：书本栏选中整块 accent，切到文章栏后
/// 高亮跟着移动到右侧。
#[test]
fn the_selected_row_is_highlighted_on_the_articles_page() {
    let fixture = common::Fixture::new();
    install_article_assets(&fixture.assets());
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    app.set_viewport(Rect::new(0, 0, 120, 40));
    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.screen, app::Screen::Articles);

    let theme = app.theme();
    let book_rect = typewords_tui::ui::focus::focused_rect(&app, app.viewport)
        .expect("书本栏选中行应有高亮矩形");
    let buffer = render_buffer(&app, 120, 40);
    let mut previous_width = 1usize;
    for x in book_rect.x..book_rect.right() {
        if previous_width == 2 {
            // CJK 宽字符的续接单元由前一个宽字形覆盖，测试后端保留默认样式。
            previous_width = 1;
            continue;
        }
        let cell = buffer.cell((x, book_rect.y)).expect("cell");
        previous_width = UnicodeWidthStr::width(cell.symbol());
        assert_eq!(
            cell.bg, theme.accent,
            "书本栏选中行应整块 accent 高亮（x={x}）"
        );
    }

    press(&mut app, KeyCode::Right);
    let article_rect = typewords_tui::ui::focus::focused_rect(&app, app.viewport)
        .expect("文章栏选中行应有高亮矩形");
    assert!(
        article_rect.x >= book_rect.right(),
        "切到文章栏后高亮应移到右侧：{article_rect:?} vs {book_rect:?}"
    );
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

/// 第一次进文章页就该看到当前书本的文章。
///
/// 早期实现只在移动书本光标时才载入文章，第一次进页面右边是空的。
#[test]
fn the_article_list_is_loaded_on_first_entry() {
    let fixture = common::Fixture::new();
    install_article_assets(&fixture.assets());
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.screen, app::Screen::Articles);
    assert!(!app.article_books.is_empty(), "应读到文章书本列表");
    assert!(
        !app.article_list.is_empty(),
        "第一次进文章页就该列出当前书本的文章"
    );
}

/// 侧栏 / 顶栏点进文章页，也必须列出当前书本的文章。
///
/// 早期只有主页按 `a` 走 `open_articles`（会载入书本与文章），侧栏与数字
/// 键走 `navigate_to`——那条路径只切了 `screen`，没载入任何资源，右边
/// 空空如也。同一页面的两个入口必须给出同样的结果。
#[test]
fn the_article_list_is_loaded_when_navigating_from_the_shell() {
    let fixture = common::Fixture::new();
    install_article_assets(&fixture.assets());
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    // 侧栏 / 顶栏的「文章」入口
    app.apply_action(app::Action::Navigate(app::Screen::Articles));
    assert_eq!(app.screen, app::Screen::Articles);
    assert!(
        !app.article_books.is_empty(),
        "从侧栏进文章页也应读到书本列表"
    );
    assert!(
        !app.article_list.is_empty(),
        "从侧栏进文章页也应列出当前书本的文章"
    );

    // 数字键 2 是同一个入口
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('2'));
    assert_eq!(app.screen, app::Screen::Articles);
    assert!(
        !app.article_list.is_empty(),
        "按数字键进文章页也应列出当前书本的文章"
    );
}

/// 鼠标点侧栏的「文章」入口，也要列出当前书本的文章。
#[test]
fn clicking_the_articles_nav_item_loads_the_article_list() {
    use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    use typewords_tui::app::Action;

    let fixture = common::Fixture::new();
    install_article_assets(&fixture.assets());
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    let area = Rect::new(0, 0, 120, 40);
    let target = typewords_tui::ui::hit::targets(&app, area)
        .into_iter()
        .find(|target| target.action == Action::Navigate(app::Screen::Articles))
        .expect("侧栏应有「文章」入口");
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.rect.x + target.rect.width / 2,
            row: target.rect.y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert_eq!(app.screen, app::Screen::Articles);
    assert!(
        !app.article_list.is_empty(),
        "点侧栏进文章页也应列出当前书本的文章"
    );
}

/// 文章跟打结束后要落在能操作的页面上。
///
/// 文章练习没有单词练习会话，结算后停在 `Screen::Practice` 会渲染成一个
/// 空白页——`render_practice` 遇到没有会话直接返回，用户既看不到结算，
/// 也退不出去。
#[test]
fn the_article_summary_lands_on_a_usable_screen() {
    let fixture = common::Fixture::new();
    install_article_assets(&fixture.assets());
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);

    // 打完所有句子（输完一句就自动进下一句，不用按回车）
    for _ in 0..500 {
        let Some(practice) = app.article_practice.as_ref() else {
            break;
        };
        let sentence = practice.sentences[practice.index].clone();
        for character in sentence.text.chars() {
            press(&mut app, KeyCode::Char(character));
        }
    }

    assert!(app.summary.is_some(), "应给出结算");
    // 文章跟打没有练习会话：结算弹出时底层不能停在练习页，
    // 否则渲染出来是一个没有会话的空白页。
    assert_eq!(
        app.screen,
        app::Screen::Home,
        "结算时底层应是主页，而不是没有会话的空白练习页"
    );
    assert!(app.session.is_none(), "文章跟打不应留下练习会话");

    app.apply_action(app::Action::SummaryHome);
    assert_eq!(app.screen, app::Screen::Home, "结算后应回到主页");
}

/// 文章跟打结束后的结算必须是文章口径。
///
/// 早期实现直接复用单词会话的 `SummaryData`：词典进度、新词、复习等
/// 字段只能填占位值，界面上同时出现「学习进度 0 / 2」与「已完成」、
/// 0% 的进度条与全空的本周记录，标题还是「今日任务完成」。
#[test]
fn the_article_summary_reports_article_progress_instead_of_dictionary_progress() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);
    finish_article(&mut app);

    let summary = app.summary.as_ref().expect("整篇打完应给出结算");
    assert!(
        matches!(
            &summary.kind,
            app::SummaryKind::Article { article_id: 11, title } if title.as_str() == "第一篇"
        ),
        "文章结算应带文章类型、id 与标题，实际 {:?}",
        summary.kind
    );
    assert_eq!(summary.last_learn_index, 2, "完成句数应是整篇句数");
    assert_eq!(summary.length, 2, "总句数应是整篇句数");
    assert!(summary.complete, "整篇打完应标记完成");
    assert_eq!(summary.wrong, 0, "正确输入不该有错句");
    assert!(
        summary.week_days.iter().any(Option::is_some),
        "文章统计已经落盘，本周记录里今天应有记录"
    );

    let text = summary_inner_text(&app, 100, 30);
    assert!(text.contains("文章跟打完成"), "文章结算标题不对:\n{text}");
    assert!(text.contains("第一篇"), "应显示文章标题:\n{text}");
    assert!(text.contains("完成 2 / 2 句"), "应显示句子进度:\n{text}");
    assert!(text.contains("错句 0"), "应显示错句数:\n{text}");
    assert!(
        !text.contains("今日任务完成"),
        "文章结算不该套用单词会话标题:\n{text}"
    );
    assert!(
        !text.contains("学习进度"),
        "文章结算不该显示词典学习进度:\n{text}"
    );
    assert!(!text.contains("新词"), "文章结算不该出现新词:\n{text}");
    assert!(!text.contains(" 0 / 2"), "不应再出现占位的 0 / N:\n{text}");
    assert!(text.contains("返回列表"), "主按钮应回文章列表:\n{text}");
    assert!(text.contains("重打本篇"), "重学按钮应重打本篇:\n{text}");
    assert!(text.contains("主页"), "应保留回主页按钮:\n{text}");
    assert!(
        text.contains("←/→ 选择 · Enter 确认"),
        "结算按钮应提示方向键选中:\n{text}"
    );
}

/// 文章结算的「错句」按句统计：一句打错只算一句，正确率按句算。
#[test]
fn the_article_summary_counts_wrong_sentences() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    // 第一句故意按错首字符（继续打完剩下的字符，本句会记一次错误）。
    let first = app.article_practice.as_ref().unwrap().sentences[0]
        .text
        .clone();
    let rest: String = first.chars().skip(1).collect();
    press(&mut app, KeyCode::Char('x'));
    type_text(&mut app, &rest);
    // 第二句打对；整篇结束后结算。
    finish_article(&mut app);

    let summary = app.summary.as_ref().expect("整篇打完应给出结算");
    assert_eq!(summary.wrong, 1, "只有第一句出现错误");
    assert_eq!(summary.accuracy, 50, "两句错一句，正确率应为 50%");

    let text = summary_inner_text(&app, 100, 30);
    assert!(text.contains("正确率 50%"), "正确率应按句统计:\n{text}");
    assert!(text.contains("错句 1"), "应显示错句数:\n{text}");
}

/// 文章结算的三个按钮各有去向：Enter 回文章列表、r 重打同一篇、q 回主页。
///
/// 早期文章跟打没有练习会话，三个动作都会落到 `leave_practice`——无论按
/// 哪个按钮都只是回主页。
#[test]
fn the_article_summary_actions_match_article_practice() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();

    // Enter：回文章列表。
    start_first_article(&mut app);
    finish_article(&mut app);
    assert!(app.summary.is_some(), "整篇打完应给出结算");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Articles, "Enter 应回文章列表");
    assert!(app.summary.is_none(), "回列表后结算应关闭");
    assert!(app.article_practice.is_none(), "回列表后不该留着跟打状态");
    assert!(!app.article_list.is_empty(), "回列表时应能看到文章");

    // r：重打同一篇，从头开始、重新计时。
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    finish_article(&mut app);
    assert!(app.summary.is_some(), "重打一轮后也应给出结算");
    press(&mut app, KeyCode::Char('r'));
    assert_eq!(app.screen, app::Screen::ArticlePractice, "r 应重打本篇");
    assert!(app.summary.is_none(), "重打时结算应关闭");
    let practice = app.article_practice.as_ref().expect("重打应重新进入跟打页");
    assert_eq!(practice.article_id, 11, "应重打同一篇文章");
    assert_eq!(practice.index, 0, "重打应从句首开始");
    assert_eq!(practice.finished, 0, "重打不该携带上次的进度");
    assert_eq!(practice.wrong_sentences, 0, "重打不该携带上次的错句数");
    assert_eq!(practice.spend, 0, "重打应重新计时");

    // q：回主页。
    finish_article(&mut app);
    press(&mut app, KeyCode::Char('q'));
    assert_eq!(app.screen, app::Screen::Home, "q 应回主页");
    assert!(app.summary.is_none(), "回主页后结算应关闭");
}

/// 标点不用打；词与词之间的空格可以显式输入，也可以省略（另见下一个用例）。
#[test]
fn punctuation_is_ignored_and_word_spaces_are_optional() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    // 首句 Hello there. 的跟打目标：只去掉句末标点，保留词间空格。
    let target: String = current_target(&app).into_iter().collect();
    assert_eq!(target, "Hello there", "跟打目标应保留词间空格、去掉标点");
    assert!(target.contains(' '), "词间空格应包含在跟打目标里");

    // 标点按了被忽略：不前进、不计错。
    type_text(&mut app, "Hello");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.typing.cursor(), 5, "字母应被记录进跟打进度");
    press(&mut app, KeyCode::Char('.'));
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.typing.cursor(), 5, "句点不用打，按下不应推进光标");
    assert!(!practice.typing.wrong, "按标点不该算错");
    assert_eq!(practice.index, 0, "按标点不应结束当前句");

    // 空格可以显式输入：按下去算正确输入。
    press(&mut app, KeyCode::Char(' '));
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.typing.cursor(), 6, "词间空格应推进光标");
    assert!(!practice.typing.wrong, "正确按空格不该算错");

    type_text(&mut app, "there");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "打完字母和词间空格应完成首句");
    assert_eq!(practice.index, 1, "完成首句后应自动进入第二句");
    assert!(!practice.typing.wrong, "正确输入不应算错");
}

/// 不打词间空格、直接打下一个单词的字母：空格按正确输入补上，不计错也不错位。
#[test]
fn a_skipped_word_space_counts_no_mistake_and_keeps_the_letters_aligned() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);
    let target = current_target(&app);

    // 一口气打到 "Helloth"，中间不按空格。
    type_text(&mut app, "Helloth");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(
        practice.typing.cursor(),
        8,
        "省略空格应自动补上，后续按键继续对齐"
    );
    assert_eq!(
        practice.typing.input,
        vec!['H', 'e', 'l', 'l', 'o', ' ', 't', 'h'],
        "补上的空格应落在目标位置上"
    );
    assert!(!practice.typing.wrong, "省略词间空格不该算错");
    assert!(
        !practice.typing.is_mismatch(5, &target, true),
        "补上的空格本身应是正确输入"
    );
    assert!(
        (0..practice.typing.cursor())
            .all(|position| !practice.typing.is_mismatch(position, &target, true)),
        "补上空格后，后面的字母不该全部错位"
    );

    // 打完剩余字母（仍然不按空格），整句应完成且不计错误句。
    type_text(&mut app, "ere.");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "省略空格不该卡住整句完成");
    assert_eq!(practice.index, 1, "打完整句应自动进入第二句");
    assert_eq!(practice.wrong_sentences, 0, "省略空格不该记错误句");
}

/// 打完后回退到词间空格、再按一个错误字母：空格仍是正确输入，错字落在
/// 下一个单词的首字母上（用户反馈的「回退回来再在空格处输入别的字母」场景）。
#[test]
fn backspacing_to_a_word_space_then_typing_a_wrong_letter_marks_the_next_letter() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);
    let target = current_target(&app);

    // 省略空格打到 "Hellot"（空格被自动补上），再退格两次回到空格位置。
    type_text(&mut app, "Hellot");
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.typing.cursor(), 5, "应退回到词间空格上");

    // 在空格位置按错误字母：空格补正确，错字记在下一个字母位置。
    press(&mut app, KeyCode::Char('z'));
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.typing.cursor(), 7);
    assert_eq!(practice.typing.typed_char(5), Some(' '), "空格应是正确输入");
    assert!(!practice.typing.is_mismatch(5, &target, true));
    assert_eq!(practice.typing.typed_char(6), Some('z'));
    assert!(
        practice.typing.is_mismatch(6, &target, true),
        "错字应落在下一个单词的首字母上，而不是空格上"
    );
    assert!(practice.typing.wrong);
}

/// 多余的空格不是目标内容：按了被忽略，不推进也不计错。
#[test]
fn an_extra_word_space_is_ignored() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    type_text(&mut app, "Hello");
    press(&mut app, KeyCode::Char(' '));
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.typing.cursor(), 6, "词间空格应被记录");

    // 第二个空格落在字母位置上：忽略。
    press(&mut app, KeyCode::Char(' '));
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.typing.cursor(), 6, "多余的空格不应推进光标");
    assert!(!practice.typing.wrong, "多余的空格不该算错");
    assert_eq!(practice.index, 0, "多余的空格不应结束当前句");

    type_text(&mut app, "there");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "有多余空格后仍应能完成首句");
    assert_eq!(practice.index, 1, "完成首句后应自动进入第二句");
    assert!(!practice.typing.wrong, "多余的空格不该留下错误标记");
}

/// 打错的字符标红在它自己的位置，而不是钉在进度最前面。
#[test]
fn a_wrong_character_is_marked_red_at_its_own_position() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    // 首句是 Hello there.，第二个字母按成 x：H 正确，x 是错的。
    type_text(&mut app, "Hx");
    let theme = app.theme();
    let buffer = render_buffer(&app, 120, 40);
    let (x_column, x_row) = find_symbol(&buffer, 'x').expect("当前句应显示实际打出的 'x'");
    assert_eq!(
        buffer.cell((x_column, x_row)).unwrap().fg,
        theme.bad,
        "打错的字符应使用主题 bad 色"
    );
    let x_cell = buffer.cell((x_column, x_row)).unwrap();
    assert!(
        !x_cell.modifier.contains(Modifier::UNDERLINED),
        "打错只标红，不加下划线"
    );

    // 继续正确输入后续字母：错误标记不跟着光标前移。
    type_text(&mut app, "llo");
    let buffer = render_buffer(&app, 120, 40);
    let (after_column, after_row) = find_symbol(&buffer, 'x').expect("错误标记不该消失");
    assert_eq!(
        (after_column, after_row),
        (x_column, x_row),
        "打错的字符应留在它自己的位置，而不是停在进度最前面"
    );
    assert_eq!(
        buffer.cell((after_column, after_row)).unwrap().fg,
        theme.bad,
        "继续输入后，之前打错的字符仍应是 bad 色"
    );
    let (h_column, h_row) = find_symbol(&buffer, 'H').expect("应显示已正确输入的 'H'");
    assert_eq!(
        buffer.cell((h_column, h_row)).unwrap().fg,
        theme.good,
        "正确输入的字符应使用主题 good 色"
    );
}

/// 光标停在词间空格上时按了别的字母：空格按正确输入补上，错字落在下一个
/// 单词的首字母上（红色、不加下划线），后面的字母仍与目标对齐。
#[test]
fn a_wrong_letter_at_a_skipped_space_is_marked_on_the_next_letter() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);
    let target = current_target(&app);

    // 打到 "Hello"，光标停在词间空格上；'z' 不等于下一个目标字符 't'。
    type_text(&mut app, "Hello");
    assert_ne!(target[6], 'z', "测试前提：下一个目标字符不是 z");
    press(&mut app, KeyCode::Char('z'));

    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(
        practice.typing.cursor(),
        7,
        "补上空格后，错字落在下一个字母的位置上"
    );
    assert_eq!(
        practice.typing.typed_char(5),
        Some(' '),
        "空格应被补成正确输入"
    );
    assert!(
        !practice.typing.is_mismatch(5, &target, true),
        "空格本身不再算错"
    );
    assert_eq!(
        practice.typing.typed_char(6),
        Some('z'),
        "错字应记在下一个单词的首字母位置"
    );
    assert!(
        practice.typing.is_mismatch(6, &target, true),
        "下一个字母位置上的 'z' 应算打错"
    );
    assert!(practice.typing.wrong, "打错应留下错误标记");

    // 渲染：'z' 落在下一个单词首字母那一列（'H' 所在列 + 6），不是空格列。
    let theme = app.theme();
    let buffer = render_buffer(&app, 120, 40);
    let (h_column, h_row) = find_symbol(&buffer, 'H').expect("应显示已正确输入的 'H'");
    let (z_column, z_row) = find_symbol(&buffer, 'z').expect("应显示下一个字母位置上的错字 'z'");
    assert_eq!(
        (z_column, z_row),
        (h_column + 6, h_row),
        "错字应显示在下一个单词的首字母处，而不是空格处"
    );
    let cell = buffer.cell((z_column, z_row)).expect("错字所在单元");
    assert_eq!(cell.fg, theme.bad, "错字应使用主题 bad 色");
    assert!(
        !cell.modifier.contains(Modifier::UNDERLINED),
        "字母位置的错字不加下划线"
    );

    // 后面照常按目标输入：从错字之后的位置继续，整句完成且只记一次错误句。
    type_text(&mut app, "here");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "打错不该卡住整句完成");
    assert_eq!(practice.index, 1, "打完整句应自动进入下一句");
    assert_eq!(practice.wrong_sentences, 1, "本句出现过错误，错误句数应 +1");
}

/// 折行恰好落在词间空格上时，空格被省略；空格后第一个字母上的错字
/// 仍然显示在下一行行首（红色、不加下划线）。
///
/// `article_lines` 在折行处会丢掉空格，避免下一行以空格开头。60 列的正文
/// 宽度是 56（终端宽 - 边框 - 行首 gutter），因此用 56 个 'a' + 一个词间
/// 空格 + 'b' 制造折行边界：省略空格后 'b' 的位置正好折到下一行行首。
#[test]
fn a_wrong_letter_after_a_wrapped_space_stays_visible_on_the_next_line() {
    use typewords_tui::domain::article::{Sentence, SentenceTyping, parse_sentence};

    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    // 只留一句：56 个 'a' + 词间空格 + "bcd"，空格正好落在第一行行尾的折行边界。
    let text = format!("{} bcd", "a".repeat(56));
    let practice = app.article_practice.as_mut().expect("练习状态");
    practice.sentences = vec![Sentence {
        text: text.clone(),
        translate: String::new(),
        words: parse_sentence(&text),
    }];
    practice.section_starts = vec![0];
    practice.index = 0;
    practice.typing = SentenceTyping::default();

    type_text(&mut app, &"a".repeat(56));
    // 空格省略：这次按键被记到下一个字母 'b' 的位置上。
    press(&mut app, KeyCode::Char('z'));
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(
        practice.typing.typed_char(56),
        Some(' '),
        "折行处的空格应被补成正确输入"
    );
    assert_eq!(
        practice.typing.typed_char(57),
        Some('z'),
        "错字应记在空格后的第一个字母位置上"
    );

    let theme = app.theme();
    let buffer = render_buffer(&app, 60, 20);
    let (a_column, a_row) = find_symbol(&buffer, 'a').expect("应显示第一行的 'a'");
    let (z_column, z_row) = find_symbol(&buffer, 'z').expect("折行后的错字仍应可见");
    assert!(
        z_row > a_row,
        "折行边界后的错字应显示在下一行，而不是被折行吞掉"
    );
    assert_eq!(
        z_column, a_column,
        "错字应落在下一行行首（与上一行正文同列）"
    );
    let cell = buffer.cell((z_column, z_row)).expect("错字所在单元");
    assert_eq!(cell.fg, theme.bad, "折行处的错字仍应是主题 bad 色");
    assert!(
        !cell.modifier.contains(Modifier::UNDERLINED),
        "字母位置的错字不加下划线"
    );
}

/// 最小终端 60×20 下，跟打页底部的规则行与按钮条必须完整显示（末尾不能被裁掉）。
///
/// 底部现在是「规则一行 + 两行动作按钮 + 一行操作提示」：按钮上带键名，
/// 退出键（Ctrl+C）与页面级返回（Esc）分别落在按钮与提示行里。
#[test]
fn the_article_hint_fits_the_minimum_terminal_width() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);
    let text = render_text(&app, 60, 20);
    assert!(
        text.contains("空格可省略"),
        "规则行应完整显示「空格可省略」:\n{text}"
    );
    assert!(
        text.contains("Backspace 退格"),
        "规则行末尾不该被裁掉:\n{text}"
    );
    // 按钮上的键名与练习页同一套：朗读 / 译文 / 主题 / 帮助 / 暂停 / 返回。
    for label in [
        "? 帮助",
        "Ctrl+P 朗读",
        "Ctrl+Z 翻译",
        "Ctrl+Q 主题",
        "Ctrl+, 暂停",
    ] {
        assert!(text.contains(label), "按钮「{label}」应完整显示:\n{text}");
    }
    assert!(
        text.contains("Ctrl+C 返回"),
        "退出键位提示不该被裁掉:\n{text}"
    );
    assert!(
        text.contains("Esc 返回列表"),
        "返回键位提示不该被裁掉:\n{text}"
    );
    assert!(
        text.contains("方向键选中 · Enter 执行 · 鼠标点击"),
        "按钮条的操作提示应完整显示:\n{text}"
    );
}

/// 整篇文章显示在同一页，仍按句子逐句跟打。
#[test]
fn the_whole_article_is_rendered_on_one_page() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("Hello there."),
        "首句应出现在同一屏里: {text}"
    );
    assert!(
        text.contains("General Kenobi."),
        "第二句也应出现在同一屏里，而不是一句一页: {text}"
    );
}

/// 打错后整句仍能完成，但本句要计入错误句数。
#[test]
fn a_sentence_with_a_typo_still_finishes_and_counts_one_wrong_sentence() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    let target = current_target(&app);
    assert_ne!(target[1], 'x', "测试前提：第二个目标字符不是 x");
    // 第二个字母按错，其余照打。
    let mut typed = target.clone();
    typed[1] = 'x';
    for character in typed {
        press(&mut app, KeyCode::Char(character));
    }

    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "打错不该卡住整句完成");
    assert_eq!(practice.index, 1, "打完整句应自动进入下一句");
    assert_eq!(practice.wrong_sentences, 1, "本句出现过错误，错误句数应 +1");
    assert!(!practice.typing.wrong, "进入下一句后本句跟打状态应重置");
}

/// 正确输入全句不应计入错误句数。
#[test]
fn typing_a_sentence_correctly_does_not_count_a_wrong_sentence() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    for character in current_target(&app) {
        press(&mut app, KeyCode::Char(character));
    }

    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "正确输入应完成首句");
    assert_eq!(practice.wrong_sentences, 0, "正确输入不该计入错误句数");
    assert!(!practice.typing.wrong, "正确输入不该留下错误标记");
}

/// Backspace 退掉最后输入的字符，但本句的出错标记要保留。
#[test]
fn backspace_removes_the_typed_character_but_keeps_the_wrong_mark() {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    start_first_article(&mut app);

    type_text(&mut app, "Hx");
    press(&mut app, KeyCode::Backspace);
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(
        practice.typing.input,
        vec!['H'],
        "退格应删掉最后输入的那个字符"
    );
    assert_eq!(practice.typing.cursor(), 1, "退格后光标应回退一格");
    assert!(practice.typing.wrong, "退格不该清掉本句的出错标记");
    assert_eq!(practice.index, 0, "退格不应让句子推进");

    // 显示层回到原文的 'e'，打错的 'x' 不再出现。
    let buffer = render_buffer(&app, 120, 40);
    assert!(
        find_symbol(&buffer, 'x').is_none(),
        "退格后不该再显示打错的 'x'"
    );

    // 补完本句：错误标记仍在，完成时记一次错误句。
    type_text(&mut app, "ello");
    type_text(&mut app, "there");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "补完本句应完成首句");
    assert_eq!(
        practice.wrong_sentences, 1,
        "本句出现过错误，补完时仍应计入错误句数"
    );
}
