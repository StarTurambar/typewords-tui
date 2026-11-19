//! 启动与断点续练：启动固定停在主页，续练由用户显式触发。
//!
//! - 常规练习：启动后在主页按 Enter 接着上次那一轮；
//! - 文章跟打：进入文章页后，缓存对应的文章行标出「上次第 N 句」，
//!   选中它按 Enter 才从断点句子继续；
//! - 两份缓存同时存在时不能叠成「单词会话 + 文章练习」，按 Esc 也不能
//!   留下一片只画了背景色的空白页；
//! - 结算只清本轮自己的缓存，不能误删另一轮的断点。

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::app;
use typewords_tui::backup;
use typewords_tui::domain::timer;
use typewords_tui::paths::Paths;
use typewords_tui::store::FileStore;
use unicode_width::UnicodeWidthStr;

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// Ctrl+C——练习页的「保存并返回主页」。
fn press_ctrl_c(app: &mut app::App) {
    app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
}

/// 输入一整句，让跟打推进到下一句。
fn type_sentence(app: &mut app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

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
                let width = UnicodeWidthStr::width(cell.symbol());
                if width > 1 {
                    skip_cells = width - 1;
                }
            }
        }
        out.push('\n');
    }
    out
}

/// 只制造文章跟打断点：完成第一句后按 Esc 回列表。
fn seed_article_cache(fixture: &common::Fixture) -> Paths {
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        app.open_articles();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, app::Screen::ArticlePractice);
        type_sentence(&mut app, "Hello there.");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen, app::Screen::Articles);
    }
    paths
}

/// 改写第一篇测试文章（id 11），模拟文章资源更新后的样子。
fn rewrite_first_article(fixture: &common::Fixture, text: &str, text_translate: &str) {
    let file = fixture
        .assets()
        .join("dicts")
        .join("en")
        .join("article")
        .join("toy.json");
    let raw = std::fs::read_to_string(&file).expect("读文章文件");
    let mut articles: serde_json::Value = serde_json::from_str(&raw).expect("解析文章文件");
    let first = articles.get_mut(0).expect("文章文件里应有一篇测试文章");
    first["text"] = serde_json::json!(text);
    first["textTranslate"] = serde_json::json!(text_translate);
    std::fs::write(&file, serde_json::to_string(&articles).unwrap()).expect("写文章文件");
}

/// 同时制造两份断点缓存：文章第一句 + 单词练习推进两步。
fn seed_both_caches(fixture: &common::Fixture) -> Paths {
    let paths = seed_article_cache(fixture);
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        press_ctrl_c(&mut app);
        assert_eq!(app.screen, app::Screen::Home);
    }
    paths
}

/// 常规练习：重启后停在主页，按 Enter 才接上词表与游标。
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

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home, "启动应停在主页");
    assert!(app.session.is_none(), "启动时不应自动建立会话");

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    let session = app.session.as_ref().expect("session restored");
    assert!(session.data.index >= 1);
}

/// 常规练习：恢复后统计快照也要接上。
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

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    assert_eq!(app.stat.spend, spend_before, "恢复后应保留断点里的用时");
}

/// 旧版（v1）单词缓存：启动要说明「版本不受支持」，文件原样保留，主页也
/// 不能一直挂着「上次任务」——否则按 Enter 静默新建，用户无从得知原因。
#[test]
fn an_unsupported_practice_cache_explains_itself_instead_of_silently_starting_fresh() {
    use typewords_tui::domain::flow::navigator::NoticeLevel;

    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let cache = paths.home.join("practice-word.json");
    let original = r#"{"version":1,"val":null}"#;
    std::fs::write(&cache, original).unwrap();

    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    assert!(
        app.toasts
            .iter()
            .any(|toast| toast.level == NoticeLevel::Warning && toast.message.contains("不受支持")),
        "启动时应说明缓存被忽略: {:?}",
        app.toasts
            .iter()
            .map(|toast| &toast.message)
            .collect::<Vec<_>>()
    );
    assert!(cache.is_file(), "不受支持的缓存不应被移动或删除");
    assert_eq!(
        std::fs::read_to_string(&cache).unwrap(),
        original,
        "不受支持的缓存应原样保留"
    );

    let text = render_text(&app, 120, 40);
    assert!(
        !text.contains("上次任务"),
        "不可恢复的缓存不该显示「上次任务」: {text}"
    );
    assert!(text.contains("今日任务"), "应显示「今日任务」: {text}");

    // 按 Enter 开新任务，而不是停在「看得见断点、续不上」的状态。
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice, "应正常开始新任务");
    assert!(app.session.is_some(), "应建立练习会话");
}

/// 可反序列化但没有可恢复内容的 v2 缓存（`val:{}`）也不能冒充「上次任务」。
///
/// `PracticeCacheCompact` 整体带 `serde(default)`，空对象会得到
/// `Some(默认值)`；`tw import-web` 也会把这种信封原样写进来。只看
/// `Option::is_some()` 会重现「主页挂着上次任务、按 Enter 却静默新建」。
#[test]
fn a_structurally_empty_practice_cache_is_not_advertised_as_resumable() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let cache = paths.home.join("practice-word.json");
    let original = r#"{"version":2,"val":{}}"#;
    std::fs::write(&cache, original).unwrap();

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    let text = render_text(&app, 120, 40);
    assert!(
        !text.contains("上次任务"),
        "没有任务词 / 快照的空壳缓存不该显示「上次任务」: {text}"
    );
    assert!(text.contains("今日任务"), "应显示「今日任务」: {text}");

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice, "应正常开始新任务");
    assert!(app.session.is_some(), "应建立练习会话");
}

/// 文章跟打：Esc 退出后重启，启动停在主页；进文章页并选中同一篇才续打。
#[test]
fn article_practice_progress_persists_and_resumes() {
    let fixture = common::Fixture::new();
    let paths = seed_article_cache(&fixture);

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home, "启动应停在主页");
    let resume = app.article_resume.as_ref().expect("应读入文章断点");
    assert_eq!(resume.article_id, 11, "应读到同一篇文章的断点");
    assert_eq!(resume.sentence_index, 1, "应记录到第二句");

    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.screen, app::Screen::Articles);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    let practice = app.article_practice.as_ref().expect("practice restored");
    assert_eq!(practice.index, 1, "应从第二句接着练");
    assert_eq!(practice.finished, 1, "已完成句数要接上");
    assert_eq!(practice.article_id, 11, "应恢复同一篇文章");
}

/// 文章断点要能经网页版备份往返：导出、清空、导入后仍标出「上次第 N 句」并续打。
#[test]
fn article_cache_survives_web_roundtrip() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    {
        let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
        app.open_articles();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, app::Screen::ArticlePractice);
        type_sentence(&mut app, "Hello there.");
        app.article_practice.as_mut().expect("文章练习状态").spend = 45_000;
        app.save_article_practice_now();
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen, app::Screen::Articles);
    }

    let store = FileStore::new(paths.home.clone()).expect("数据目录");
    let text = backup::export_web(&store).expect("导出");
    let backup_path = paths.home.join("web-backup.json");
    std::fs::write(&backup_path, text).expect("写备份");
    store.remove_practice_article().expect("清空断点");
    assert!(!paths.home.join("practice-article.json").is_file());

    let report = backup::import_web(&store, &backup_path).expect("导入");
    assert!(
        report.discarded.is_empty(),
        "TUI 导出的文章断点不该被丢弃: {:?}",
        report.discarded
    );
    assert!(
        report
            .written
            .iter()
            .any(|key| key == "PracticeSaveArticle"),
        "应导入 PracticeSaveArticle: {:?}",
        report.written
    );

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    let resume = app.article_resume.as_ref().expect("文章断点应恢复");
    assert_eq!(resume.article_id, 11, "应恢复到同一篇文章");
    assert_eq!(resume.sentence_index, 1, "应恢复到第二句");
    assert_eq!(resume.spend, 45_000, "用时也应恢复");

    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.screen, app::Screen::Articles);
    let text = render_text(&app, 100, 30);
    assert!(text.contains("上次第 2 句"), "列表应标出断点句: {text}");

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    let practice = app.article_practice.as_ref().expect("跟打状态");
    assert_eq!(practice.index, 1, "应续到第二句");
    assert_eq!(practice.finished, 1, "已完成句数要接上");
    assert_eq!(practice.spend, 45_000, "用时也要接上");
}

/// 文章跟打：整篇练完后断点缓存要删掉，再次进入从头开始。
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
        assert!(
            app.summary.is_some(),
            "整篇练完应弹结算，实际 summary={:?}",
            app.summary.is_some()
        );
    }

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home, "练完后重开应回主页");
    assert!(app.article_resume.is_none(), "练完后不该再有断点标记");

    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    let practice = app.article_practice.as_ref().expect("practice");
    assert_eq!(practice.index, 0, "应从头开始");
    assert_eq!(practice.spend, 0, "用时也应从零开始");
}

/// 文章跟打：缓存文件损坏时不该阻断启动，只提示并忽略。
#[test]
fn a_corrupt_article_cache_does_not_block_startup() {
    let fixture = common::Fixture::new();
    let paths = seed_article_cache(&fixture);
    // 写坏文件。
    std::fs::write(paths.home.join("practice-article.json"), b"{ not json").unwrap();

    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home, "损坏缓存不该阻断启动");
    assert!(app.fatal.is_none(), "缓存损坏不该是致命错误");
    assert!(app.article_resume.is_none(), "损坏的缓存不该被读入");
    assert!(
        app.toasts.iter().any(|toast| {
            toast.message.contains("文章练习缓存损坏，已丢弃") && toast.message.contains("已移动到")
        }),
        "启动时应提示缓存损坏、丢弃原因与隔离路径: {:?}",
        app.toasts
            .iter()
            .map(|toast| &toast.message)
            .collect::<Vec<_>>()
    );
}

/// 清空练习进度会清掉两个断点缓存与内存里的断点标记。
#[test]
fn clearing_practice_progress_clears_both_caches() {
    let fixture = common::Fixture::new();
    let paths = seed_both_caches(&fixture);
    assert!(paths.home.join("practice-word.json").is_file());
    assert!(paths.home.join("practice-article.json").is_file());

    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home);
    assert!(app.article_resume.is_some());
    app.clear_practice_cache();
    assert!(
        !paths.home.join("practice-word.json").is_file(),
        "应删掉单词练习的断点缓存"
    );
    assert!(
        !paths.home.join("practice-article.json").is_file(),
        "应删掉文章跟打的断点缓存"
    );
    assert!(app.article_resume.is_none(), "内存里的断点标记也要清掉");
}

/// 同一次运行内：退出练习页再按 Enter，要接着上次练，而不是重开一轮。
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

/// 两份缓存同时存在时，启动也必须停在主页，且不能同时存在两个活动状态。
#[test]
fn startup_with_both_caches_stays_on_home() {
    let fixture = common::Fixture::new();
    let paths = seed_both_caches(&fixture);

    let app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home, "启动应固定在主页");
    assert!(app.session.is_none(), "启动时不该有单词练习会话");
    assert!(app.article_practice.is_none(), "启动时不该有文章跟打状态");
    assert!(app.article_resume.is_some(), "文章断点应被读入用于展示");

    let text = render_text(&app, 120, 40);
    assert!(
        text.contains("上次任务"),
        "有断点时应显示「上次任务」: {text}"
    );
    assert!(
        text.contains("继续学习"),
        "有断点时应显示「继续学习」: {text}"
    );
}

/// 跟打页按 Esc 必须回到可操作的文章列表，即使同时存在单词断点。
#[test]
fn escaping_article_practice_returns_to_a_non_blank_list() {
    let fixture = common::Fixture::new();
    let paths = seed_both_caches(&fixture);

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    assert_eq!(app.screen, app::Screen::Home);
    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);

    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, app::Screen::Articles, "Esc 应回到文章列表");
    assert!(app.article_practice.is_none(), "跟打状态应被清空");
    assert!(app.article_resume.is_some(), "断点缓存应保留");

    // 回归点：这里曾渲染成一整屏只有背景色的空白页。
    let text = render_text(&app, 100, 30);
    assert!(text.contains("书本"), "Esc 后应能画出文章列表: {text}");
    assert!(text.contains("测试文章集"), "应能画出书本条目: {text}");

    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, app::Screen::Home, "再按 Esc 应回主页");
}

/// 文章列表要标出断点所在：「上次第 N 句」。
#[test]
fn the_article_list_marks_the_saved_article() {
    let fixture = common::Fixture::new();
    let paths = seed_article_cache(&fixture);

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('a'));
    let text = render_text(&app, 100, 30);
    assert!(text.contains("上次第 2 句"), "列表应标出断点句: {text}");
}

/// 资源更新后文章变短：列表上的「上次第 N 句」不能指向已经不存在的句子。
///
/// 断点缓存里的 `sentenceIndex` 是保存时那篇文章的下标。文章文件更新后从两句
/// 变成一句时，旧断点仍记着第二句；列表标记必须按当前句数 clamp（与按 Enter
/// 续打时把下标 clamp 到当前句数内一致），不能把存档里的旧数字原样显示出来
/// ——那和书本行写「72 篇」却只列出 5 篇是同一类缺陷。
#[test]
fn an_outdated_article_breakpoint_is_clamped_to_the_current_sentence_count() {
    let fixture = common::Fixture::new();
    let paths = seed_article_cache(&fixture);
    // 资源更新：第一篇从两句变成一句。
    rewrite_first_article(&fixture, "Hello there.", "你好。");

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    let resume = app.article_resume.as_ref().expect("旧断点仍应读入");
    assert_eq!(resume.sentence_index, 1, "存档里仍是保存时的第二句");
    assert_eq!(resume.total, 2, "存档里仍是保存时的总句数");

    press(&mut app, KeyCode::Char('a'));
    let text = render_text(&app, 120, 40);
    assert!(
        !text.contains("上次第 2 句"),
        "文章只剩 1 句，不该标出第 2 句: {text}"
    );
    assert!(
        text.contains("· 上次第 1 句"),
        "断点应 clamp 到当前最后一句: {text}"
    );

    // 标记上写的就是 Enter 真正落到的句子。
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    let practice = app.article_practice.as_ref().expect("跟打状态");
    assert_eq!(practice.index, 0, "续打应从当前最后一句开始");
}

/// 文章已经一句都不剩时，列表不显示断点标记：没有任何可续打的句子。
#[test]
fn an_article_without_sentences_shows_no_breakpoint_marker() {
    let fixture = common::Fixture::new();
    let paths = seed_article_cache(&fixture);
    rewrite_first_article(&fixture, "", "");

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('a'));
    let text = render_text(&app, 120, 40);
    assert!(text.contains("第一篇  0 句"), "文章应如实显示 0 句: {text}");
    assert!(
        !text.contains("上次第"),
        "没有句子的文章不该标出断点: {text}"
    );
}

/// 断点属于文章 11；选中另一篇（文章 12）时不能硬套进度。
///
/// 文章页进页在左栏（书本），`→` 切到右栏（文章），`↓` 选第二篇。
#[test]
fn selecting_another_article_starts_from_the_beginning() {
    let fixture = common::Fixture::new();
    let paths = seed_article_cache(&fixture);

    let mut app = app::build_for_test(paths, fixture.cli()).unwrap();
    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::ArticlePractice);
    let practice = app.article_practice.as_ref().expect("practice");
    assert_eq!(practice.article_id, 12, "应选中第二篇文章");
    assert_eq!(practice.index, 0, "另一篇文章应从头开始");
    assert_eq!(practice.spend, 0, "用时也应从零开始");
}

/// 单词练习结算只清单词断点，不能顺手删掉文章跟打的断点。
#[test]
fn finishing_a_word_round_keeps_the_article_breakpoint() {
    let fixture = common::Fixture::new();
    let paths = seed_both_caches(&fixture);

    let mut app = app::build_for_test(paths.clone(), fixture.cli()).unwrap();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, app::Screen::Practice);
    for _ in 0..500 {
        if app.summary.is_some() || app.session.is_none() {
            break;
        }
        press(&mut app, KeyCode::Tab);
    }
    assert!(app.summary.is_some(), "跳过整轮后应弹出结算");
    assert!(
        !paths.home.join("practice-word.json").is_file(),
        "结算后单词断点应删除"
    );
    // 结算后又排了一次防抖保存的话，这里会把缓存重新写回来。
    app.on_tick(timer::now_ms() + 1000);
    assert!(
        !paths.home.join("practice-word.json").is_file(),
        "结算后不该再写回单词断点"
    );
    assert!(
        paths.home.join("practice-article.json").is_file(),
        "单词结算不能删掉文章跟打的断点"
    );

    app.apply_action(app::Action::SummaryHome);
    assert_eq!(app.screen, app::Screen::Home);
    assert!(
        paths.home.join("practice-article.json").is_file(),
        "回主页也不能删掉文章跟打的断点"
    );
    assert!(app.article_resume.is_some(), "文章断点标记应保留");
}
