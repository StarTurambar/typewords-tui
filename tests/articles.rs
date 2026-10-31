//! 文章背诵：书本列表、断句与句子跟打。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

/// 标点不用打，但词与词之间的空格是跟打内容，必须打。
#[test]
fn punctuation_is_ignored_but_word_spaces_must_be_typed() {
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

    // 空格是跟打内容：必须按下去才继续。
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

/// 漏打词间空格只记一次错误，并且会自动补空格，后面的字母不会整体错位。
#[test]
fn a_missing_word_space_counts_one_mistake_without_shifting_later_letters() {
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
        "漏打空格应自动补上，后续按键继续对齐"
    );
    assert_eq!(
        practice.typing.input,
        vec!['H', 'e', 'l', 'l', 'o', ' ', 't', 'h'],
        "自动补上的空格应落在目标位置上"
    );
    assert!(practice.typing.wrong, "漏打词间空格应记一次错误");
    assert!(
        (0..practice.typing.cursor())
            .all(|position| !practice.typing.is_mismatch(position, &target, true)),
        "补上空格后，后面的字母不该全部错位"
    );

    // 打完剩余字母（仍然不按空格），整句应完成且只记一句错误。
    type_text(&mut app, "ere.");
    let practice = app.article_practice.as_ref().expect("练习状态");
    assert_eq!(practice.finished, 1, "漏打空格不该卡住整句完成");
    assert_eq!(practice.index, 1, "打完整句应自动进入第二句");
    assert_eq!(practice.wrong_sentences, 1, "漏打空格只计一次错误句");
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
