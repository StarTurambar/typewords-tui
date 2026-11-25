//! 绘制 / 命中覆盖矩阵。
//!
//! 守着「同一份布局同时服务绘制与命中」这条不变量（见 docs/architecture.md
//! 与 CLAUDE.md）：每个 Target::rect 都必须落在画面内，并且压在真正画出来的
//! 文字上。早先只有 ui_snapshots.rs 针对主页两个目标的断言，其余 12 个页面
//! 没有任何「命中区落在画出来的内容上」的校验；7 种弹层里有 3 种（跳组 / 收藏 /
//! 词条编辑）的 render 在测试里一次都没执行过（给它们加临时探针跑全量测试确认），
//! 跳组弹层缺标题行就是这样漏掉的。
//!
//! 断言分两层：
//! - 文本层用 common::render_text（按字符拼接，CJK 连续，可以 contains）；
//! - 几何层用 common::render_cells（按显示列索引，可以直接和 rect 对照）。

mod common;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Action, App, Screen};
use typewords_tui::cli::Cli;
use typewords_tui::domain::types::WordPracticeMode;
use typewords_tui::paths::{Paths, locate_assets};
use typewords_tui::ui::shell::{self, Nav};
use typewords_tui::ui::word_test;

use common::Fixture;

/// 最小终端、常见窄屏、侧栏 24 列、侧栏 26 列、宽屏。
const SIZES: [(u16, u16); 5] = [(60, 20), (80, 24), (100, 30), (120, 40), (160, 50)];

/// 13 个页面：状态构造 + 正文里必须出现的一句文案。
///
/// 文案都取自页面正文（不是导航栏标签），页面正文整个空白时就会红。
const SCREENS: [(Screen, &str, &str); 13] = [
    (Screen::Home, "主页", "今日任务"),
    (Screen::Practice, "练习", "释义"),
    (Screen::WordTest, "单词测试", "单词测试"),
    (Screen::Words, "词条管理", "改动写回词库文件"),
    (Screen::Articles, "文章", "书本"),
    (Screen::ArticlePractice, "文章跟打", "第一篇"),
    (Screen::Flows, "流程", "练习流程"),
    (Screen::DictPicker, "词库", "全部词库"),
    (Screen::Stats, "统计", "学习日历"),
    (Screen::Settings, "设置", "默认练习模式"),
    (Screen::Shortcuts, "快捷键", "编辑文章"),
    (Screen::AdvFsrs, "高级 FSRS", "高级 FSRS"),
    (Screen::Help, "帮助", "快捷键"),
];

/// 用公开 API 把 App 推到某个页面。
///
/// 优先走真实入口（open_words / start_practice / 按键），直接改 screen 会留下
/// 上一页的焦点身份，断言到的可能不是用户真正看到的那一帧。
fn build(screen: Screen) -> (Fixture, App) {
    let fixture = Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).unwrap();
    prepare_screen(&mut app, screen);
    (fixture, app)
}

/// 把一个已经装好的 App 推到某个页面。
fn prepare_screen(app: &mut App, screen: Screen) {
    match screen {
        Screen::Home => {}
        Screen::Practice => app.start_practice(),
        Screen::WordTest => app.start_word_test(WordPracticeMode::ReviewWordsTest),
        Screen::Words => app.open_words(),
        Screen::Articles => app.open_articles(),
        Screen::ArticlePractice => {
            // 跟打页要求文章已经载入：先进文章页（真实入口）再开始第一篇。
            app.open_articles();
            app.start_article_practice(0);
        }
        Screen::Flows => app.open_flows(),
        Screen::DictPicker => app.screen = Screen::DictPicker,
        Screen::Stats => app.screen = Screen::Stats,
        Screen::Settings => app.screen = Screen::Settings,
        Screen::Shortcuts => app.open_shortcuts(),
        Screen::AdvFsrs => app.screen = Screen::AdvFsrs,
        Screen::Help => app.screen = Screen::Help,
    }
}

/// 一个页面在一个尺寸下的几何检查：每个命中区都在画面内、且压着画出来的内容。
fn assert_targets_land_on_drawn_content(app: &App, label: &str, width: u16, height: u16) {
    let cells = common::render_cells(app, width, height);
    let area = Rect::new(0, 0, width, height);
    let targets = typewords_tui::ui::hit::targets(app, area);
    assert!(
        !targets.is_empty(),
        "{label} {width}x{height}：页面应有可点目标"
    );
    for target in &targets {
        let rect = target.rect;
        let what = format!("{label} {width}x{height} {:?}", target.action);
        assert!(
            rect.width > 0 && rect.height > 0,
            "{what}：命中区不能是空矩形 {rect:?}"
        );
        assert!(
            rect.right() <= width && rect.bottom() <= height,
            "{what}：命中区越出画面 {rect:?}（终端 {width}x{height}）"
        );
        // 矩形里必须有画出来的内容。绝大多数命中区是一行，这时等价于
        // 「首行非空白」；主页的推荐卡片这类多行命中区只要求整块里有
        // 内容——卡片的顶行可能只是留白，强求首行反而会误报。
        assert!(
            !cells.rect_is_blank(rect),
            "{what}：命中区 {rect:?} 落在空白上（绘制与命中漂移），该行是 {:?}",
            cells.rect_text(rect)
        );
    }
}

#[test]
fn every_screen_draws_its_content_and_its_targets_land_on_it() {
    for (screen, label, needle) in SCREENS {
        for (width, height) in SIZES {
            let (_fixture, app) = build(screen);
            let text = common::render_text(&app, width, height);
            assert!(
                text.contains(needle),
                "{label} {width}x{height}：正文里应出现「{needle}」\n{text}"
            );
            assert_targets_land_on_drawn_content(&app, label, width, height);
        }
    }
}

/// CET-4 规模（2607 词 + 5 篇真实文章）下同一套几何不变量同样成立。
///
/// 12 词 fixture 的列表只有几行，真实词库 / 文章列表会滚动、行文本更长。
/// 这里只校验几何与「有可点目标」：正文文案那条和 fixture 绑定（真实资源
/// 的标题不是「第一篇」），正文覆盖由上面的 fixture 矩阵负责。
#[test]
fn real_assets_screens_keep_targets_on_drawn_content() {
    let Some(assets) = locate_assets(None) else {
        return;
    };
    for (screen, label, _needle) in SCREENS {
        let home = tempfile::tempdir().expect("tempdir");
        let paths = Paths {
            home: home.path().to_path_buf(),
            assets: assets.clone(),
        };
        let cli = Cli {
            assets: Some(paths.assets.clone()),
            home: Some(paths.home.clone()),
            dict: None,
            mode: None,
            theme: None,
            seed: Some(42),
            reset_practice: false,
            log: None,
            no_log: true,
            log_level: typewords_tui::logging::Level::Info,
            command: None,
        };
        let mut app = app::build_for_test(paths, cli).expect("app");
        prepare_screen(&mut app, screen);
        for (width, height) in [(60u16, 20u16), (120, 40)] {
            assert_targets_land_on_drawn_content(&app, label, width, height);
        }
    }
}

/// 7 种弹层。
#[derive(Clone, Copy, Debug)]
enum Overlay {
    Summary,
    Edit,
    Confirm,
    WordEdit,
    ModePicker,
    GroupPicker,
    CollectPicker,
}

const OVERLAYS: [Overlay; 7] = [
    Overlay::Summary,
    Overlay::Edit,
    Overlay::Confirm,
    Overlay::WordEdit,
    Overlay::ModePicker,
    Overlay::GroupPicker,
    Overlay::CollectPicker,
];

impl Overlay {
    fn label(self) -> &'static str {
        match self {
            Self::Summary => "结算",
            Self::Edit => "输入",
            Self::Confirm => "确认",
            Self::WordEdit => "词条编辑",
            Self::ModePicker => "模式选择",
            Self::GroupPicker => "跳组",
            Self::CollectPicker => "收藏",
        }
    }
}

/// 打开弹层：走 App 的公开入口，不手工拼内部弹层结构体。
fn open_overlay(app: &mut App, overlay: Overlay) {
    match overlay {
        Overlay::Summary => {
            app.summary = Some(app::SummaryData {
                spend_ms: 65_000,
                accuracy: 90,
                new_words: 5,
                review: 2,
                wrong: 1,
                last_learn_index: 5,
                length: 12,
                wrong_top: vec![("word01".to_string(), 3)],
                ..Default::default()
            });
        }
        Overlay::Edit => {
            app.edit = Some(app::EditState {
                target: app::EditTarget::PerDay,
                buffer: "20".to_string(),
            });
        }
        Overlay::Confirm => {
            app.confirm = Some(app::ConfirmState::new(
                "退出测试？",
                app::ConfirmAction::Quit,
            ));
        }
        Overlay::WordEdit => app.open_word_edit(Some(0)),
        Overlay::ModePicker => app.open_mode_picker(),
        Overlay::GroupPicker => {
            // 12 个词、每组 2 个 → 6 组：标题里的总组数才不是 1。
            if let Some(dict) = app.boot.base.study_dict_mut() {
                dict.per_day_study_number = 2;
            }
            app.open_group_picker();
        }
        Overlay::CollectPicker => {
            app.start_practice();
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        }
    }
}

/// 弹层里必须出现的文案：标题、至少一行行文本、底部提示。
fn overlay_texts(app: &App, overlay: Overlay) -> Vec<String> {
    match overlay {
        Overlay::Summary => vec![
            "今日任务完成".to_string(),
            "学习报告".to_string(),
            "错词 Top10".to_string(),
            "继续".to_string(),
            "重学".to_string(),
            "主页".to_string(),
            "←/→ 选择 · Enter 确认".to_string(),
        ],
        Overlay::Edit => vec![
            "输入".to_string(),
            "修改 每日词数".to_string(),
            "> 20_".to_string(),
        ],
        Overlay::Confirm => vec![
            "确认".to_string(),
            "退出测试？".to_string(),
            "←/→ 选择 · Enter 确认 · Esc 取消".to_string(),
        ],
        Overlay::WordEdit => vec![
            "词条".to_string(),
            "编辑词条".to_string(),
            "音标".to_string(),
            "笔记".to_string(),
            "word00".to_string(),
        ],
        Overlay::ModePicker => vec![
            "选择练习模式".to_string(),
            "自由练习".to_string(),
            "自定义流程".to_string(),
            "↑↓ 选择 · Enter 确认 · Esc 取消".to_string(),
        ],
        // 标题里的组数是运行期算的，所以在这里现算。
        Overlay::GroupPicker => vec![
            format!("跳到第几组（共 {} 组）", app.total_groups()),
            "第 1 组".to_string(),
            "↑↓ 选择 · Enter 跳转 · Esc 取消".to_string(),
        ],
        Overlay::CollectPicker => vec![
            "收藏到哪个词典".to_string(),
            "▶ 收藏".to_string(),
            "↑↓ 选择 · Enter 收藏 · Esc 取消".to_string(),
        ],
    }
}

#[test]
fn every_overlay_draws_its_title_rows_and_hint() {
    for overlay in OVERLAYS {
        for (width, height) in [(60u16, 20u16), (100, 30), (120, 40)] {
            let (_fixture, mut app) = build(Screen::Home);
            open_overlay(&mut app, overlay);
            let text = common::render_text(&app, width, height);
            for needle in overlay_texts(&app, overlay) {
                assert!(
                    text.contains(&needle),
                    "{} {width}x{height}：弹层里应出现「{needle}」\n{text}",
                    overlay.label()
                );
            }

            // 弹层自己的命中区同样要压在画出来的行上。
            let cells = common::render_cells(&app, width, height);
            let area = Rect::new(0, 0, width, height);
            for target in typewords_tui::ui::hit::targets(&app, area) {
                assert!(
                    !cells.rect_is_blank(target.rect),
                    "{} {width}x{height}：命中区 {:?} 落在空白上（{:?}）",
                    overlay.label(),
                    target.rect,
                    target.action
                );
            }
        }
    }
}

/// 答题页：题干、四个选项、选中态与底栏提示。
///
/// 早期这条页面只有状态断言、画面零断言——题干其实从来没画过（用户只看得到
/// 四个中文释义，不知道在考哪个词）。
#[test]
fn word_test_screen_shows_prompt_options_and_footer() {
    for (width, height) in [(100u16, 30u16), (60, 20)] {
        let (_fixture, mut app) = build(Screen::WordTest);
        let question = app
            .word_test
            .as_ref()
            .and_then(|state| state.question.clone())
            .expect("应生成题目");
        let answer = app
            .word_test
            .as_ref()
            .and_then(|state| state.answer_word())
            .expect("应能取到答案词")
            .to_string();
        let area = Rect::new(0, 0, width, height);
        let rows = common::render_rows(&app, width, height);
        let row_text = |y: u16| {
            rows.get(y as usize)
                .map(|(text, _)| text.clone())
                .unwrap_or_default()
        };

        // 题干画在题干行上，而不是在别处顺带出现。
        let content = shell::layout(area, Nav::Home).content;
        let layout = word_test::layout(content, question.candidates.len());
        let prompt = row_text(layout.prompt.y);
        assert!(
            prompt.contains(&answer),
            "{width}x{height}：题干行应显示被测试的单词 {answer}，实际 {prompt:?}"
        );

        // 四个选项：命中区与绘制同源，行里画着自己的字母标记与释义。
        let targets = typewords_tui::ui::hit::targets(&app, area);
        let options: Vec<_> = targets
            .iter()
            .filter(|target| matches!(target.action, Action::WordTestChoose(_)))
            .collect();
        assert_eq!(
            options.len(),
            4,
            "{width}x{height}：一题应有 4 个选项命中区"
        );
        for target in &options {
            let Action::WordTestChoose(index) = target.action else {
                unreachable!("已按 WordTestChoose 过滤")
            };
            let candidate = question
                .candidates
                .get(index)
                .expect("选项下标应落在候选里");
            let marker = ["A", "B", "C", "D"][index];
            let row = row_text(target.rect.y);
            assert!(
                row.contains(&format!("{marker}   ")),
                "{width}x{height}：第 {index} 个选项行应以 {marker} 开头，实际 {row:?}"
            );
            assert!(
                row.contains(&candidate.word.trans[0].cn),
                "{width}x{height}：第 {index} 个选项行应画着它自己的释义，实际 {row:?}"
            );
            assert_eq!(
                target.rect.y, layout.option_rows[index].y,
                "{width}x{height}：第 {index} 个选项的命中行与布局行不一致"
            );
        }

        // 底栏提示：未作答时是「1 / 2 / 3 / 4 选择答案」。
        let footer = row_text(layout.footer.y);
        assert!(
            footer.contains("1 / 2 / 3 / 4 选择答案"),
            "{width}x{height}：底栏应提示用 1~4 选择答案，实际 {footer:?}"
        );

        // 选中态：答错时选中项用 bad、正确项用 good，并把单词补在正确项后面。
        let wrong_index = (question.correct_index + 1) % question.candidates.len();
        app.apply_action(Action::WordTestChoose(wrong_index));
        let rows = common::render_rows(&app, width, height);
        let row_text = |y: u16| {
            rows.get(y as usize)
                .map(|(text, _)| text.clone())
                .unwrap_or_default()
        };
        let row_colors = |y: u16| {
            rows.get(y as usize)
                .map(|(_, colors)| colors.clone())
                .unwrap_or_default()
        };
        let theme = app.theme();
        let chosen = options[wrong_index].rect.y;
        assert!(
            row_colors(chosen).contains(&theme.bad),
            "{width}x{height}：选错的选项应标成 bad 色，实际 {chosen} 行 {:?}",
            row_text(chosen)
        );
        let correct_row = options[question.correct_index].rect.y;
        assert!(
            row_colors(correct_row).contains(&theme.good),
            "{width}x{height}：正确选项应标成 good 色，实际 {:?}",
            row_text(correct_row)
        );
        assert!(
            row_text(correct_row).contains(&answer),
            "{width}x{height}：答完后正确选项应补上单词 {answer}"
        );
        let footer = row_text(layout.footer.y);
        assert!(
            footer.contains("空格 下一题"),
            "{width}x{height}：答完后底栏应提示「空格 下一题」，实际 {footer:?}"
        );
    }
}

/// 空态文案。
///
/// 词库页的「没有可用词库」不在其中：Catalog::load 合并后为空会直接报
/// 「词库目录为空」让程序拒绝启动，所以那条分支在真实运行里到不了，这里不
/// 假装覆盖它。
#[test]
fn empty_states_render_their_hint() {
    // 统计页：一条记录都没有。
    let (_fixture, app) = build(Screen::Stats);
    let text = common::render_text(&app, 100, 30);
    assert!(
        text.contains("暂无学习记录"),
        "空统计应提示「暂无学习记录」\n{text}"
    );
    assert!(
        text.contains("这一天没有学习记录"),
        "选中日期没有记录时应如实说明\n{text}"
    );

    // 文章跟打页：没有进行中的练习。
    let (_fixture, mut app) = build(Screen::Home);
    app.screen = Screen::ArticlePractice;
    let text = common::render_text(&app, 100, 30);
    assert!(
        text.contains("没有进行中的文章练习"),
        "空跟打页应说明没有进行中的练习\n{text}"
    );
    assert!(
        text.contains("Esc 返回文章列表"),
        "空跟打页应给出返回方式\n{text}"
    );

    // 词条管理：搜索没有匹配。
    let (_fixture, mut app) = build(Screen::Words);
    app.words_filter = "zzzz".to_string();
    let text = common::render_text(&app, 100, 30);
    assert!(
        text.contains("没有匹配的词条"),
        "搜索无匹配时应提示\n{text}"
    );

    // 主页：没有可推荐的词库。
    let (_fixture, app) = build(Screen::Home);
    let text = common::render_text(&app, 100, 30);
    assert!(
        text.contains("暂无推荐 · d 浏览词库"),
        "没有推荐词库时应给出入口\n{text}"
    );
}

/// 宽屏（160 列）分支：侧栏 26 列、正文更宽，页面正文照旧完整。
#[test]
fn wide_screens_render_their_wide_layout() {
    for (screen, needles) in [
        (Screen::Home, vec!["今日任务", "我的词典", "推荐"]),
        (Screen::Stats, vec!["学习日历", "最近 14 天"]),
        (Screen::Help, vec!["快捷键", "常见问题", "关于与反馈"]),
    ] {
        let (_fixture, app) = build(screen);
        let text = common::render_text(&app, 160, 50);
        for needle in needles {
            assert!(
                text.contains(needle),
                "{screen:?} 160x50：正文里应出现「{needle}」\n{text}"
            );
        }
        assert!(
            text.contains("Type Words"),
            "{screen:?} 160x50：宽屏应有侧栏副标题"
        );
        assert!(
            text.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))),
            "{screen:?} 160x50：底栏 / 侧栏应显示版本号"
        );
        for line in text.lines() {
            let width = unicode_width::UnicodeWidthStr::width(line);
            assert!(
                width <= 160,
                "{screen:?} 160x50：某一行宽 {width} 超过终端宽度"
            );
        }
    }
}
