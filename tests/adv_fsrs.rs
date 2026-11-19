//! 「高级 FSRS」子屏（Screen::AdvFsrs）的集成测试。
//!
//! 这是本仓库唯一能改 FSRS-6 参数的界面：目标留存率、最大间隔、两个调度开关、
//! 21 个权重与「恢复默认」。参数会直接影响调度器行为，所以这里既断言内存状态，
//! 也用 FileStore 重新装载 setting.json 校验落盘——只相信内存的话，写盘路径
//! 断掉也测不出来。

mod common;

use chrono::Utc;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typewords_tui::app::{self, Action, Screen};
use typewords_tui::domain::constants::SETTING_VERSION;
use typewords_tui::domain::fsrs::{Fsrs, default_w};
use typewords_tui::domain::types::{
    CARD_STATE_LEARNING, CARD_STATE_REVIEW, FsrsParameters, RATING_GOOD, SettingState,
};
use typewords_tui::store::FileStore;
use typewords_tui::ui::focus::{self, FocusId};
use typewords_tui::ui::hit;
use typewords_tui::ui::shell::{self, Nav};
use unicode_width::UnicodeWidthStr;

const WIDE: (u16, u16) = (120, 40);
const MINIMUM: (u16, u16) = (60, 20);

fn area_of(size: (u16, u16)) -> Rect {
    Rect::new(0, 0, size.0, size.1)
}

/// 建一个已经同步好视口的测试应用（Fixture 必须活得比 App 久，一起返回）。
fn test_app(size: (u16, u16)) -> (common::Fixture, app::App) {
    let fixture = common::Fixture::new();
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("build app");
    app.set_viewport(area_of(size));
    (fixture, app)
}

/// 先用原始 JSON 写一份 `setting.json` 再启动：模拟手改存档 / 导入网页版备份。
///
/// `raw` 是 `val` 里的内容（外层 camelCase），信封版本取程序支持的版本。
fn test_app_with_settings(raw: serde_json::Value) -> (common::Fixture, app::App) {
    let fixture = common::Fixture::new();
    let envelope = serde_json::json!({
        "val": raw,
        "version": SETTING_VERSION,
        "updated_at": "2026-01-01T00:00:00.000Z",
    });
    std::fs::write(
        fixture.home().join("setting.json"),
        serde_json::to_string(&envelope).expect("序列化 setting.json"),
    )
    .expect("写 setting.json");
    let mut app = app::build_for_test(fixture.paths(), fixture.cli()).expect("build app");
    app.set_viewport(area_of(WIDE));
    (fixture, app)
}

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn click(app: &mut app::App, area: Rect, column: u16, row: u16) {
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
}

fn type_str(app: &mut app::App, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

/// 按 key 定位设置行，避免依赖行号。
fn setting_row(key: &str) -> usize {
    app::SETTING_ROWS
        .iter()
        .position(|(row_key, _)| *row_key == key)
        .unwrap_or_else(|| panic!("设置页应有 {key} 行"))
}

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

/// 从主页进设置页，走到「高级 FSRS」入口行并按 Enter 进子屏。
fn open_adv_fsrs(app: &mut app::App) {
    press(app, KeyCode::Char('s'));
    assert_eq!(app.screen, Screen::Settings, "按 s 应进设置页");
    goto_setting(app, "advanced_fsrs");
    press(app, KeyCode::Enter);
    assert_eq!(
        app.screen,
        Screen::AdvFsrs,
        "「高级 FSRS」行按 Enter 应进入子屏"
    );
    assert_eq!(app.focus, Some(FocusId::Page), "进页后焦点应落在页面内容区");
}

/// 底栏「选中 X」显示的当前行名。
///
/// 用它定位行而不是写死行号：行顺序（留存率 → 开关 → 21 个权重 → 恢复默认）
/// 是产品约定，测试只关心「光标在哪一行」，不该被行号变更打断。
fn row_label(app: &app::App) -> String {
    focus::focused_label(app, app.viewport)
        .or_else(|| focus::page_label(app))
        .unwrap_or_else(|| panic!("高级 FSRS 页应显示当前选中的行名"))
}

/// 从首行按 ↓ 走到行名包含 needle 的那一行。
fn goto_row(app: &mut app::App, needle: &str) {
    press(app, KeyCode::Home);
    for _ in 0..40 {
        if row_label(app).contains(needle) {
            return;
        }
        press(app, KeyCode::Down);
    }
    panic!("没走到包含「{needle}」的行，当前选中：{}", row_label(app));
}

/// 清空输入框（数字行与权重行都只接受有限长度，循环退格是最稳的）。
fn clear_edit(app: &mut app::App) {
    while app
        .edit
        .as_ref()
        .is_some_and(|edit| !edit.buffer.is_empty())
    {
        press(app, KeyCode::Backspace);
    }
}

/// 按画出的单元格还原每一行文本；宽字符的续接单元跳过（与 ui_snapshots 同法）。
fn render_rows(app: &app::App, size: (u16, u16)) -> Vec<String> {
    let backend = TestBackend::new(size.0, size.1);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render(frame)).expect("draw");
    let buffer = terminal.backend().buffer();
    let mut rows = Vec::with_capacity(size.1 as usize);
    for y in 0..size.1 {
        let mut skip_cells = 0usize;
        let mut line = String::new();
        for x in 0..size.0 {
            if skip_cells > 0 {
                skip_cells -= 1;
                continue;
            }
            if let Some(cell) = buffer.cell((x, y)) {
                line.push_str(cell.symbol());
                let display_width = UnicodeWidthStr::width(cell.symbol());
                if display_width > 1 {
                    skip_cells = display_width - 1;
                }
            }
        }
        rows.push(line);
    }
    rows
}

fn render_text(app: &app::App, size: (u16, u16)) -> String {
    render_rows(app, size).join("\n")
}

/// 子屏所在的外壳内容区（侧栏高亮「设置」，布局与设置页共用）。
fn content_of(size: (u16, u16)) -> Rect {
    shell::layout(area_of(size), Nav::Settings).content
}

/// 内容区里第一处包含 needle 的行号（行外的侧栏 / 底栏不算）。
fn line_in_content(app: &app::App, size: (u16, u16), needle: &str) -> u16 {
    let content = content_of(size);
    render_rows(app, size)
        .iter()
        .enumerate()
        .find(|(index, line)| {
            *index >= content.y as usize
                && *index < content.bottom() as usize
                && line.contains(needle)
        })
        .map(|(index, _)| index as u16)
        .unwrap_or_else(|| panic!("内容区里找不到「{needle}」"))
}

/// 某一行画在屏幕上的可点击点；顺带断言这一行确实注册了鼠标目标。
fn row_click_point(app: &app::App, size: (u16, u16), needle: &str) -> (u16, u16) {
    let content = content_of(size);
    let row = line_in_content(app, size, needle);
    let column = content.x.saturating_add(2);
    assert!(
        hit::action_at(app, area_of(size), column, row).is_some(),
        "「{needle}」行应注册鼠标命中目标"
    );
    (column, row)
}

fn assert_rows_fit(text: &str, size: (u16, u16)) {
    for (index, line) in text.lines().enumerate() {
        let width = UnicodeWidthStr::width(line);
        assert!(
            width <= size.0 as usize,
            "{size:?} 下第 {index} 行显示宽度 {width} 超出终端宽度"
        );
    }
}

/// 全部 Toast 文案拼接，用于断言用户确实收到了提示。
fn toasts_text(app: &app::App) -> String {
    app.toasts
        .iter()
        .map(|toast| toast.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// 用 FileStore 重新装载 setting.json：确认改动真的写盘了（不信内存）。
fn store_settings(fixture: &common::Fixture) -> SettingState {
    let store = FileStore::new(fixture.home()).expect("FileStore");
    let loaded = store.load_settings();
    assert!(loaded.is_ok(), "setting.json 应能正常装载");
    loaded.value
}

/// setting.json 的原始 JSON，用来锁存档字段名（camelCase 外壳 + snake_case 内层）。
fn raw_settings(fixture: &common::Fixture) -> serde_json::Value {
    let text =
        std::fs::read_to_string(fixture.home().join("setting.json")).expect("应写出 setting.json");
    serde_json::from_str(&text).expect("setting.json 应是合法 JSON")
}

// ── 入口与返回 ───────────────────────────────────────────────────────────

/// 设置页 FSRS 分区的「高级 FSRS」行是唯一入口；Esc / q 保存并回设置页。
///
/// 回设置页时光标必须回到第一行（与快捷键页一致）：停在原来那行的话，
/// 下一次 Enter 会直接触发「其他」分区里的破坏性动作。
#[test]
fn settings_entry_opens_advanced_fsrs_and_escape_returns() {
    let (_fixture, mut app) = test_app(WIDE);

    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.screen, Screen::Settings);
    // 入口行右侧要写清「按 Enter 打开」，不能只给一个名字。
    let entry_line = render_rows(&app, (100, 60))
        .into_iter()
        .find(|line| line.contains("高级 FSRS"))
        .expect("设置页应有「高级 FSRS」行");
    assert!(
        entry_line.contains("按 Enter 打开"),
        "入口行应提示按 Enter 打开：{entry_line:?}"
    );

    goto_setting(&mut app, "advanced_fsrs");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::AdvFsrs);
    assert_eq!(app.focus, Some(FocusId::Page));
    assert!(row_label(&app).contains("目标留存率"), "进页光标应在第一行");
    let text = render_text(&app, WIDE);
    assert!(
        text.contains("高级 FSRS"),
        "内容区首行应写着子屏名字（外壳顶栏只显示「设置」）"
    );

    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Settings, "Esc 应回到设置页");
    assert_eq!(app.settings_cursor, 0, "回到设置页时光标应重置到第一行");
    assert!(app.edit.is_none(), "返回不应留下输入框");
    assert!(app.confirm.is_none(), "返回不应留下确认框");

    // q 与 Esc 同义：快捷键页也是这么约定的。
    goto_setting(&mut app, "advanced_fsrs");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::AdvFsrs);
    press(&mut app, KeyCode::Char('q'));
    assert_eq!(app.screen, Screen::Settings, "q 也应回到设置页");
}

// ── 数值行：输入、夹取与落盘 ─────────────────────────────────────────────

/// 留存率与最大间隔：数字输入框只收数字，越界按区间夹取，改完立即写盘。
#[test]
fn retention_and_maximum_interval_are_clamped_and_persisted() {
    let (fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);

    // 最大间隔：默认 36500 天，改成 500 天。
    goto_row(&mut app, "最大间隔");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("36500"),
        "输入框应预填当前值"
    );

    // 只接受数字：字母被丢弃，已输入的内容不受影响。
    press(&mut app, KeyCode::Char('a'));
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("36500"),
        "字母不应进入数字输入框"
    );

    clear_edit(&mut app);
    type_str(&mut app, "500");
    press(&mut app, KeyCode::Enter);
    assert!(app.edit.is_none(), "保存后应关闭输入框");
    assert_eq!(app.boot.settings.fsrs_parameters.maximum_interval, 500.0);

    let reloaded = store_settings(&fixture);
    assert_eq!(
        reloaded.fsrs_parameters.maximum_interval, 500.0,
        "最大间隔应落盘"
    );
    let raw = raw_settings(&fixture);
    assert_eq!(
        raw["val"]["fsrsParameters"]["maximum_interval"].as_f64(),
        Some(500.0),
        "存档字段是 snake_case 的 maximum_interval"
    );
    assert_eq!(
        raw["version"].as_u64(),
        Some(u64::from(SETTING_VERSION)),
        "只是多了子屏，存档版本号不应变化"
    );

    // 目标留存率区间 50 ~ 99：输入 10 应夹到下限 50%。
    goto_row(&mut app, "目标留存率");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("90"),
        "输入框应预填 90"
    );
    clear_edit(&mut app);
    type_str(&mut app, "10");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.boot.settings.fsrs_parameters.request_retention, 0.5,
        "留存率应夹到 50%"
    );
    assert_eq!(
        store_settings(&fixture).fsrs_parameters.request_retention,
        0.5,
        "留存率应落盘"
    );
}

/// 手改存档把留存率 / 最大间隔写成区间外的值时，设置页主列表与高级 FSRS 页
/// 显示的是**调度器实际使用的生效值**，两处口径一致。
///
/// 此前主列表用原始值 `× 100`（`requestRetention: 95` 会显示「9500%」），
/// 子页用回退后的生效值（90%）；编辑预填也读主列表那个函数，于是预填的
/// 同样不是调度真正在用的值。
#[test]
fn out_of_range_fsrs_numbers_display_the_effective_values() {
    // 外层 camelCase、`fsrsParameters` 内部 snake_case（与 ts-fsrs 一致）。
    let (_fixture, mut app) = test_app_with_settings(serde_json::json!({
        "fsrsParameters": { "request_retention": 95.0, "maximum_interval": 0.0 }
    }));
    // 前置：存档里确实是区间外的原始值（否则这条测试测的是别的东西）。
    assert_eq!(
        app.boot.settings.fsrs_parameters.request_retention, 95.0,
        "原始留存率应原样读入"
    );
    assert_eq!(app.boot.settings.fsrs_parameters.maximum_interval, 0.0);

    // 设置页主列表：显示 90%（不是 9500%），编辑预填也是 90。
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.screen, Screen::Settings);
    goto_setting(&mut app, "request_retention");
    let rows = render_rows(&app, (100, 60));
    let row = rows
        .iter()
        .find(|line| line.contains("目标留存率"))
        .expect("设置页应有「目标留存率」行");
    assert!(row.contains("90%"), "主列表应显示生效值 90%：{row:?}");
    assert!(!row.contains("9500"), "主列表不该显示原始值 9500%：{row:?}");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("90"),
        "编辑预填应等于调度实际使用的值"
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.edit.is_none());

    // 高级 FSRS 页：留存率同样是 90%，最大间隔是 36500 天。
    goto_setting(&mut app, "advanced_fsrs");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::AdvFsrs);
    let rows = render_rows(&app, WIDE);
    let retention = rows
        .iter()
        .find(|line| line.contains("目标留存率"))
        .expect("高级 FSRS 页应有「目标留存率」行");
    assert!(retention.contains("90%"), "子页应显示 90%：{retention:?}");
    let interval = rows
        .iter()
        .find(|line| line.contains("最大间隔"))
        .expect("高级 FSRS 页应有「最大间隔」行");
    assert!(
        interval.contains("36500"),
        "子页应显示生效的最大间隔 36500 天：{interval:?}"
    );

    goto_row(&mut app, "最大间隔");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("36500"),
        "最大间隔的编辑预填也应是生效值"
    );
}

/// 数字行清空后直接回车：拒绝保存并提示，保持原值。
///
/// 此前整数路径用 `parse::<i64>().unwrap_or_default()`，空输入静默变成 0
/// 再被夹到区间下限——留存率被改成 50%、最大间隔被改成 1 天（下次结算所有
/// 卡片都排到次日），而且没有任何提示。权重行的同一种输入一直会提示
/// 「请输入数字」，现在两条路径同一口径。
#[test]
fn empty_number_input_is_rejected_instead_of_saved_as_zero() {
    let (fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);

    // 目标留存率：清空 → 回车。
    goto_row(&mut app, "目标留存率");
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    press(&mut app, KeyCode::Enter);
    assert!(app.edit.is_none(), "拒绝保存也要关闭输入框");
    assert_eq!(
        app.boot.settings.fsrs_parameters.request_retention, 0.9,
        "空输入不应把留存率改成区间下限 50%"
    );
    assert!(
        toasts_text(&app).contains("请输入数字"),
        "应提示请输入数字：{}",
        toasts_text(&app)
    );
    assert_eq!(
        store_settings(&fixture).fsrs_parameters.request_retention,
        0.9,
        "空输入不应写盘"
    );

    // 最大间隔同理：此前会静默写成 1 天。
    goto_row(&mut app, "最大间隔");
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.boot.settings.fsrs_parameters.maximum_interval, 36500.0,
        "空输入不应把最大间隔改成 1 天"
    );
    assert_eq!(
        store_settings(&fixture).fsrs_parameters.maximum_interval,
        36500.0,
        "空输入不应写盘"
    );
}

// ── 权重行：与调度器同源的区间夹取 ───────────────────────────────────────

/// 权重编辑：区间内原样保存，越界按调度器实际夹取的范围钳住并落盘。
///
/// w[16]（Easy 加成）合法 1 ~ 6、w[12]（遗忘难度幂）合法 0.001 ~ 0.25；
/// 界面显示的区间与 Fsrs::new 的夹取同源，测试直接拿默认权重验证这两条。
#[test]
fn weight_edits_clamp_to_the_scheduler_range_and_persist() {
    let (fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);

    goto_row(&mut app, "w[16]");
    assert_eq!(
        app.adv_fsrs_weight(16),
        1.8729,
        "默认权重来自 FSRS-6 默认表"
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("1.8729"),
        "权重输入框应预填当前值（去尾 0）"
    );
    clear_edit(&mut app);
    type_str(&mut app, "99");
    press(&mut app, KeyCode::Enter);
    assert!(app.edit.is_none());
    assert_eq!(
        app.boot.settings.fsrs_parameters.w[16], 6.0,
        "越界权重应夹到区间上限 6"
    );
    assert!(
        toasts_text(&app).contains("w[16]") && toasts_text(&app).contains("夹取"),
        "夹取要告诉用户改成了多少：{}",
        toasts_text(&app)
    );
    assert_eq!(
        store_settings(&fixture).fsrs_parameters.w[16],
        6.0,
        "夹取后的权重应落盘"
    );

    // 区间内的值原样保存（0.1 落在 0.001 ~ 0.25）。
    goto_row(&mut app, "w[12]");
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    type_str(&mut app, "0.1");
    press(&mut app, KeyCode::Enter);
    assert!(app.edit.is_none());
    assert!((app.boot.settings.fsrs_parameters.w[12] - 0.1).abs() < 1e-12);
    assert!((store_settings(&fixture).fsrs_parameters.w[12] - 0.1).abs() < 1e-12);

    let raw = raw_settings(&fixture);
    assert_eq!(
        raw["val"]["fsrsParameters"]["w"][16].as_f64(),
        Some(6.0),
        "权重数组按原下标落盘"
    );
    assert_eq!(
        raw["val"]["fsrsParameters"]["w"][12].as_f64(),
        Some(0.1),
        "权重数组按原下标落盘"
    );
}

/// 非法权重输入：空串与只有小数点都不写内存、不写盘，并弹出可读提示。
///
/// 权重是浮点，直接 unwrap_or_default() 会把空输入静默写成 0（低于所有下限，
/// 等于把参数改坏），因此必须拒绝并保留原值。
#[test]
fn invalid_weight_input_keeps_the_previous_value() {
    let (fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);
    goto_row(&mut app, "w[12]");
    let original = app.adv_fsrs_weight(12);

    // 清空后直接保存：空串不是数字。
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    press(&mut app, KeyCode::Enter);
    assert!(app.edit.is_none(), "非法输入保存后仍应关闭输入框");
    assert_eq!(app.adv_fsrs_weight(12), original, "空输入不应改权重");
    assert!(
        toasts_text(&app).contains("请输入数字"),
        "应提示请输入数字：{}",
        toasts_text(&app)
    );

    // 只有小数点同样是非法输入。
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    press(&mut app, KeyCode::Char('.'));
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("."),
        "权重输入框允许一个小数点"
    );
    press(&mut app, KeyCode::Enter);
    assert!(app.edit.is_none());
    assert_eq!(app.adv_fsrs_weight(12), original, "「.」不应改权重");
    assert_eq!(
        store_settings(&fixture).fsrs_parameters.w[12],
        original,
        "非法输入不应写盘"
    );
}

/// 权重往返一致：手改存档写 7 位小数 → 界面显示 6 位 → 直接回车保存 →
/// 落盘就是显示的那个值；再次进入编辑，预填与落盘一致；输入框也拦住第 7 位。
///
/// 此前输入框允许任意位数（≤12 字符），显示与预填按 6 位舍入：打开权重行
/// 再直接回车，就把存储值悄悄改成了舍入后的值。
#[test]
fn weight_precision_survives_a_round_trip_through_the_edit_box() {
    let mut w = default_w().to_vec();
    w[0] = 0.1234567;
    let (fixture, mut app) = test_app_with_settings(serde_json::json!({
        "fsrsParameters": { "w": w }
    }));
    assert!(
        (app.adv_fsrs_weight(0) - 0.1234567).abs() < 1e-12,
        "前置：原始权重应原样读入"
    );

    open_adv_fsrs(&mut app);
    goto_row(&mut app, "w[0]");
    let text = render_text(&app, WIDE);
    assert!(text.contains("0.123457"), "行内显示应舍入到 6 位：{text}");

    // 打开输入框：预填就是显示值。
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("0.123457"),
        "预填应与显示一致"
    );
    // 已经有 6 位小数，第 7 位进不来。
    press(&mut app, KeyCode::Char('7'));
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("0.123457"),
        "小数点后超过 6 位的字符应被拒绝"
    );
    // 直接回车：落盘值 == 显示值。
    press(&mut app, KeyCode::Enter);
    let saved = store_settings(&fixture).fsrs_parameters.w[0];
    assert!(
        (saved - 0.123457).abs() < 1e-12,
        "直接回车保存后应归一为 6 位，实际 {saved}"
    );
    let raw = raw_settings(&fixture);
    assert_eq!(
        raw["val"]["fsrsParameters"]["w"][0].as_f64(),
        Some(0.123457),
        "落盘的权重就是显示值"
    );

    // 再次进入编辑：预填与落盘一致，再保存一次也不会变。
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("0.123457"),
        "重开编辑的预填应与落盘一致"
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        store_settings(&fixture).fsrs_parameters.w[0],
        saved,
        "再保存一次不应改变值"
    );

    // 新输入也只保留 6 位：敲 `0.1234567` 只进到 `0.123456`。
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    type_str(&mut app, "0.1234567");
    assert_eq!(
        app.edit.as_ref().map(|edit| edit.buffer.as_str()),
        Some("0.123456"),
        "第 7 位小数应被拒绝"
    );
    press(&mut app, KeyCode::Enter);
    assert!(
        (store_settings(&fixture).fsrs_parameters.w[0] - 0.123456).abs() < 1e-12,
        "保存的应是输入框里那 6 位小数"
    );
}

/// 手改出非法步进：启动时整组回退默认步进，并用 Toast 点名是哪个串。
///
/// 回退发生在 `Fsrs::new`（domain 纯函数），提示由 `bootstrap::load` 推进
/// notices——没有这层提示，用户只会看到「步进没生效」，无从知道是拼错了。
#[test]
fn invalid_learning_steps_are_reported_at_startup() {
    let (_fixture, app) = test_app_with_settings(serde_json::json!({
        "fsrsParameters": { "learning_steps": ["1m", "abc"] }
    }));
    let toasts = toasts_text(&app);
    assert!(
        toasts.contains("步进无法解析"),
        "启动时应提示步进被回退：{toasts}"
    );
    assert!(toasts.contains("abc"), "提示里应点名哪个步进：{toasts}");

    // 默认步进仍生效：新卡评 Good 停在 10 分钟那一步，而不是直接毕业。
    let scheduler = Fsrs::new(&app.boot.settings.fsrs_parameters);
    let card = scheduler.next_card(None, RATING_GOOD, Utc::now());
    assert_eq!(card.state, CARD_STATE_LEARNING);
    assert!(card.scheduled_days < 1.0, "非法步进不应把新卡排到 ≥1 天");
}

// ── 调度开关 ─────────────────────────────────────────────────────────────

/// 两个开关行直接切换并立即落盘；关掉短期调度后调度器行为也要跟着变。
///
/// 开关的语义只有落到 Fsrs 上才算生效：enable_short_term = false 时新卡走长期
/// 调度器，按 Good 直接进 Review；默认（开启）时同一张新卡还留在学习步进里。
#[test]
fn schedule_switches_toggle_and_persist() {
    let (fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);
    assert!(
        !app.boot.settings.fsrs_parameters.enable_fuzz,
        "默认不开启间隔抖动"
    );

    goto_row(&mut app, "fuzz");
    press(&mut app, KeyCode::Enter);
    assert!(app.boot.settings.fsrs_parameters.enable_fuzz);
    assert!(app.edit.is_none(), "开关行不应打开输入框");
    assert!(
        store_settings(&fixture).fsrs_parameters.enable_fuzz,
        "fuzz 开关应落盘"
    );

    goto_row(&mut app, "短期调度");
    press(&mut app, KeyCode::Enter);
    assert!(!app.boot.settings.fsrs_parameters.enable_short_term);
    assert!(
        !store_settings(&fixture).fsrs_parameters.enable_short_term,
        "短期调度开关应落盘"
    );
    let raw = raw_settings(&fixture);
    assert_eq!(
        raw["val"]["fsrsParameters"]["enable_fuzz"].as_bool(),
        Some(true),
        "开关字段是 snake_case 的 enable_fuzz"
    );
    assert_eq!(
        raw["val"]["fsrsParameters"]["enable_short_term"].as_bool(),
        Some(false),
        "开关字段是 snake_case 的 enable_short_term"
    );

    let now = Utc::now();
    let short_term_off = Fsrs::new(&app.boot.settings.fsrs_parameters);
    assert_eq!(
        short_term_off.next_card(None, RATING_GOOD, now).state,
        CARD_STATE_REVIEW,
        "关掉短期调度后新卡按 Good 应直接进 Review"
    );
    // 对照：默认开启短期调度时，同一张新卡还在学习步进里（10 分钟后）。
    let short_term_on = Fsrs::new(&FsrsParameters::default());
    assert_eq!(
        short_term_on.next_card(None, RATING_GOOD, now).state,
        CARD_STATE_LEARNING,
        "开启短期调度时新卡按 Good 仍应留在学习步进"
    );
}

// ── 恢复默认 ─────────────────────────────────────────────────────────────

/// 「恢复默认」先弹确认框：取消不动，确认后 21 个权重与其它参数一起回到默认并落盘。
#[test]
fn reset_restores_defaults_only_after_confirmation() {
    let (fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);

    // 先改坏三处（开关 + 最大间隔 + 权重），让「恢复默认」有可见效果。
    goto_row(&mut app, "fuzz");
    press(&mut app, KeyCode::Enter);
    goto_row(&mut app, "最大间隔");
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    type_str(&mut app, "500");
    press(&mut app, KeyCode::Enter);
    goto_row(&mut app, "w[16]");
    press(&mut app, KeyCode::Enter);
    clear_edit(&mut app);
    type_str(&mut app, "99");
    press(&mut app, KeyCode::Enter);
    let modified = app.boot.settings.fsrs_parameters.clone();
    assert_ne!(modified, FsrsParameters::default(), "前置：参数应已被改坏");

    goto_row(&mut app, "恢复默认");
    press(&mut app, KeyCode::Enter);
    let message = app
        .confirm
        .as_ref()
        .expect("恢复默认应先弹确认框")
        .message
        .clone();
    assert!(
        message.contains("恢复默认 FSRS 参数"),
        "确认框要写清后果：{message}"
    );

    // 取消：参数原样，不写盘。
    press(&mut app, KeyCode::Esc);
    assert!(app.confirm.is_none(), "Esc 应关闭确认框");
    assert_eq!(
        app.boot.settings.fsrs_parameters, modified,
        "取消后参数不应改变"
    );

    // 确认：默认焦点就是「确认」，Enter 即执行。
    press(&mut app, KeyCode::Enter);
    assert!(app.confirm.is_some());
    press(&mut app, KeyCode::Enter);
    assert!(app.confirm.is_none());
    assert_eq!(
        app.boot.settings.fsrs_parameters,
        FsrsParameters::default(),
        "恢复默认后参数应等于 FsrsParameters::default()"
    );
    assert_eq!(
        store_settings(&fixture).fsrs_parameters,
        FsrsParameters::default(),
        "默认参数应落盘"
    );
    assert!(
        toasts_text(&app).contains("已恢复默认"),
        "恢复默认应给出成功提示：{}",
        toasts_text(&app)
    );
}

// ── 鼠标 ─────────────────────────────────────────────────────────────────

/// 鼠标：第一次点未选中的行只移动光标，再点已选中的行才激活。
///
/// 这条规则和设置页 / 词库页一致——点一下就改参数的话，用户在浏览列表时
/// 很容易误改权重。
#[test]
fn mouse_rows_select_first_then_activate() {
    let (_fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);
    let area = area_of(WIDE);

    // 120×40 下一屏放得下全部 26 行，每行都要注册命中目标（分区标题行不注册）。
    let content = content_of(WIDE);
    let row_targets = hit::targets(&app, area)
        .into_iter()
        .filter(|target| target.rect.x >= content.x && target.rect.right() <= content.right())
        .count();
    assert_eq!(row_targets, 26, "26 行都应注册鼠标命中目标");

    // w[0]：未选中 → 只选中；再点 → 打开输入框。
    let (x, y) = row_click_point(&app, WIDE, "w[0]");
    click(&mut app, area, x, y);
    assert_eq!(app.focus, Some(FocusId::Page), "点内容区应把焦点交给页面");
    assert!(row_label(&app).contains("w[0]"));
    assert!(app.edit.is_none(), "点未选中行只应选中，不该直接开输入框");
    let (x, y) = row_click_point(&app, WIDE, "w[0]");
    click(&mut app, area, x, y);
    assert!(app.edit.is_some(), "再点已选中行应打开输入框");
    press(&mut app, KeyCode::Esc);
    assert!(app.edit.is_none());

    // 开关行：第二次点击直接切换，不弹输入框。
    let (x, y) = row_click_point(&app, WIDE, "fuzz");
    click(&mut app, area, x, y);
    assert!(row_label(&app).contains("fuzz"));
    assert!(!app.boot.settings.fsrs_parameters.enable_fuzz);
    let (x, y) = row_click_point(&app, WIDE, "fuzz");
    click(&mut app, area, x, y);
    assert!(
        app.boot.settings.fsrs_parameters.enable_fuzz,
        "点已选中的开关行应切换开关"
    );
    assert!(app.edit.is_none(), "开关行不应打开输入框");
}

/// 滚轮：与本页方向键同一条 Action 入口，滚一行选一行。
#[test]
fn wheel_scrolls_the_cursor_through_rows() {
    let (_fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);
    let area = area_of(WIDE);
    assert!(row_label(&app).contains("目标留存率"));
    assert_eq!(
        hit::wheel_action(&app, area, 50, 20, 1),
        Some(Action::AdvFsrsMove(1)),
        "本页滚轮应转成 AdvFsrsMove"
    );
    assert_eq!(
        hit::wheel_action(&app, area, 50, 20, -1),
        Some(Action::AdvFsrsMove(-1))
    );

    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 20,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(
        row_label(&app).contains("最大间隔"),
        "下滚应把光标移到下一行"
    );
    app.on_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 50,
            row: 20,
            modifiers: KeyModifiers::NONE,
        },
        area,
    );
    assert!(
        row_label(&app).contains("目标留存率"),
        "上滚应把光标移回上一行"
    );
}

// ── 键盘焦点与底栏 ───────────────────────────────────────────────────────

/// 键盘：进页即选中内容区，↑↓ / Home / End / PgDn 都作用在行光标上，底栏跟着走。
#[test]
fn keyboard_focus_and_footer_track_the_selected_row() {
    let (_fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);
    assert_eq!(app.focus, Some(FocusId::Page), "进页应选中页面内容区");
    let hints = shell::hints_for(Screen::AdvFsrs);
    assert!(!hints.is_empty(), "本页底栏要有按键提示");
    assert!(
        hints.iter().any(|(key, _)| *key == "enter"),
        "底栏应提示 Enter 修改"
    );

    // ↓ 走统一的焦点导航 → 页面光标（与设置页同一套）。
    press(&mut app, KeyCode::Down);
    assert!(row_label(&app).contains("最大间隔"));
    assert_eq!(
        focus::focused_label(&app, app.viewport),
        focus::page_label(&app),
        "底栏文案与页面行名同源"
    );

    // End / Home / PgDn 直接改光标（keymap 发 AdvFsrsMove）。
    press(&mut app, KeyCode::End);
    assert!(row_label(&app).contains("恢复默认"));
    press(&mut app, KeyCode::Home);
    assert!(row_label(&app).contains("目标留存率"));
    press(&mut app, KeyCode::PageDown);
    assert!(
        row_label(&app).contains("w[6]"),
        "PageDown 一次走 10 行：{}",
        row_label(&app)
    );

    // 底栏「选中 X」要显示当前行名（含权重含义）。
    goto_row(&mut app, "w[0]");
    let label = focus::focused_label(&app, app.viewport).expect("底栏应有焦点文案");
    assert!(
        label.contains("w[0]") && label.contains("初始稳定性"),
        "底栏应显示行名与含义：{label}"
    );
    let text = render_text(&app, WIDE);
    let footer = text.lines().last().unwrap_or_default();
    assert!(
        footer.contains("选中") && footer.contains("w[0]"),
        "底栏「选中 X」应跟上光标：{footer:?}"
    );
}

// ── 渲染 ─────────────────────────────────────────────────────────────────

/// 渲染：120×40 与最低尺寸 60×20 都要看到子屏名字、留存率、权重与恢复默认行，
/// 且每一行的显示宽度都不超过终端宽度。
#[test]
fn page_renders_within_both_terminal_sizes() {
    let (_fixture, mut app) = test_app(WIDE);
    open_adv_fsrs(&mut app);

    for size in [WIDE, MINIMUM] {
        let text = render_text(&app, size);
        for needle in ["高级 FSRS", "目标留存率", "w[0]"] {
            assert!(text.contains(needle), "{size:?} 下应看到「{needle}」");
        }
        assert_rows_fit(&text, size);
    }

    // 最小终端放不下 26 行：滚到最后一行，「恢复默认」必须完整可见。
    app.set_viewport(area_of(MINIMUM));
    press(&mut app, KeyCode::End);
    let text = render_text(&app, MINIMUM);
    assert!(
        text.contains("恢复默认 FSRS 参数"),
        "60×20 滚到底应看到「恢复默认」行：{text}"
    );
    assert_rows_fit(&text, MINIMUM);
}
