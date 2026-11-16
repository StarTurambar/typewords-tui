//! 帮助页：快捷键 / 常见问题 / 关于与反馈三个分页。
//!
//! 分页条固定在内容区顶部：内容区焦点下 `←` / `→`（`h` / `l` 别名）切换，
//! 焦点在导航栏时方向键仍归焦点模型，鼠标点标签也可；
//! 「快捷键」宽屏分两栏（练习 / 导航），其余分页单栏，`↑` / `↓`、PgUp / PgDn
//! 与滚轮滚动。高度不足时优先显示前面的内容，正文右侧画滚动条（点滑块
//! 上下的轨道可上下滚动）。
//!
//! 正文是成段的说明文字，逻辑条目与屏幕行数不是一一对应：滚动按**屏幕行**
//! 计算（`build_rows` 先把条目按当前宽度折成行），否则段落比视口长时滚不到底。
//!
//! 上游把「FAQ」「关于」「反馈」拆成 help / about / feedback 三个页面（about
//! 与 feedback 渲染的是同一个 `About.vue`）；终端里合并为一个帮助页的三个
//! 分页，入口仍是数字 5 与 `?`。内容是终端形态下的重写，不是上游文案的复制。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthChar;

use crate::app::{App, HelpTab, Screen};
use crate::ui::shell::{self, Nav};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

/// 快捷键分页的内容宽度达到此值时左练习、右导航分两栏展示。
const TWO_COLUMN_MIN_WIDTH: u16 = 78;
/// 两栏布局还要求正文至少有这么高，否则单栏滚动更省空间。
const TWO_COLUMN_MIN_HEIGHT: u16 = 20;

/// 分页条高度。
const TAB_BAR_HEIGHT: u16 = 1;
/// 分页条与正文之间的留白行。
const TAB_BAR_GAP: u16 = 1;

/// 拆成短行，避免窄屏按字符折行时把 Shift+拖拽 拆开。
const MOUSE_NOTES: &[&str] = &[
    "鼠标：",
    "侧栏/按钮/列表/分页标签可点击，滚轮滚动；",
    "Shift+拖拽选择文本",
];

#[derive(Clone, Copy, Debug)]
enum HelpEntry {
    /// 分区标题（带横线）。
    Section(&'static str),
    /// 键位 → 说明。
    Key(&'static str, &'static str),
    /// FAQ 的问题（加粗，`▸` 前缀）。
    Question(&'static str),
    /// 正文 / 答案行（暗色，自动折行）。
    Note(&'static str),
    /// 动态行：当前数据目录（构造常量时拿不到，渲染时从 `App::paths` 读）。
    DataDir,
}

const PRACTICE: &[HelpEntry] = &[
    HelpEntry::Section("练习"),
    HelpEntry::Key("字母 / 数字", "输入单词"),
    HelpEntry::Key("空格", "确认 / 继续"),
    HelpEntry::Key("Esc", "显示完整单词"),
    HelpEntry::Key("Tab", "跳过当前词"),
    HelpEntry::Key("\u{60}", "标记 / 取消已掌握"),
    HelpEntry::Key("Enter", "收藏 / 取消收藏"),
    HelpEntry::Key("1 / 2 / 3", "自测：认识 / 不认识 / 已掌握"),
    HelpEntry::Key("Alt+1~4", "自测选择题 A~D"),
    HelpEntry::Key("Ctrl+I", "切换默写遮罩"),
    HelpEntry::Key("Ctrl+Z", "切换翻译"),
    HelpEntry::Key("Ctrl+,", "暂停 / 恢复计时"),
    HelpEntry::Key("Ctrl+P", "播放单词发音"),
    HelpEntry::Key("Ctrl+R", "随机默写当前词表"),
    HelpEntry::Key("Ctrl+B / Ctrl+L", "收起状态栏 / 词表"),
    HelpEntry::Key("Ctrl+E", "显示 / 隐藏例句与词源"),
    HelpEntry::Key("Shift+→ / Ctrl+J", "跳到下一阶段"),
    HelpEntry::Key("Ctrl+Enter", "重学本组"),
    HelpEntry::Key("Alt+← / Alt+→", "上一组 / 下一组"),
    HelpEntry::Key("Alt+Enter", "默写本组"),
    HelpEntry::Key("Shift+Enter", "收藏到指定词典"),
    HelpEntry::Key("Ctrl+G", "切换简洁模式"),
    HelpEntry::Key("Ctrl+Q", "切换主题"),
    HelpEntry::Key("Ctrl+C", "保存并返回主页"),
    HelpEntry::Key("方向键 + Enter", "选中 / 执行底部动作按钮（鼠标可点）"),
];

/// 文章跟打页的键位。
///
/// 朗读 / 译文 / 主题三个键与练习页**同一个来源**（键位表），这里只列默认
/// 绑定；`Esc` 是页面级返回，`Ctrl+C` 与 `Ctrl+,` 与练习页共用同一个键。
const ARTICLE: &[HelpEntry] = &[
    HelpEntry::Section("文章跟打"),
    HelpEntry::Key("字母 / 数字 / 空格", "跟打当前句（标点不用打）"),
    HelpEntry::Key("Esc / Ctrl+C", "返回 / 保存并返回文章列表"),
    HelpEntry::Key("Ctrl+P", "朗读当前句"),
    HelpEntry::Key("Ctrl+Z", "切换当前句译文"),
    HelpEntry::Key("Ctrl+,", "暂停 / 恢复计时"),
    HelpEntry::Key("方向键 + Enter", "选中 / 执行底部动作按钮（鼠标可点）"),
];

const NAVIGATION: &[HelpEntry] = &[
    HelpEntry::Section("导航/查看"),
    HelpEntry::Key("1 ~ 6", "切换页面"),
    HelpEntry::Key("?", "帮助"),
    HelpEntry::Key("q", "退出"),
    HelpEntry::Key("d / s / t", "词库 / 设置 / 统计"),
    HelpEntry::Key("a", "文章背诵"),
    HelpEntry::Key("Ctrl+F", "练习流程"),
    HelpEntry::Key("m", "切换练习模式"),
    HelpEntry::Key(
        "↑ ↓ ← →",
        "页面焦点导航：移动选中 / 到边跨栏（Esc 取消或返回）",
    ),
    HelpEntry::Key("Tab", "导航栏 ↔ 内容区换栏"),
    HelpEntry::Key("Enter", "触发选中项（主页 = 开始学习）"),
    HelpEntry::Key("f", "自由练习"),
    HelpEntry::Key("+ / -", "每日词数 ±5（设置页可精确输入）"),
    HelpEntry::Key("Ctrl+←", "上一个词"),
    HelpEntry::Key("Ctrl+→", "下一个词"),
    HelpEntry::Key(", / .", "统计页翻月"),
    HelpEntry::Key("↑ / ↓", "列表滚动 / 词库与设置选择"),
    HelpEntry::Key("← / →", "文章页换栏 / 统计页选日期 / 帮助页切分页"),
    HelpEntry::Key("h / l", "帮助页上一分页 / 下一分页"),
    HelpEntry::Key("鼠标点击", "侧栏 / 按钮 / 列表 / 帮助分页"),
    HelpEntry::Key("滚轮", "滚动列表"),
    HelpEntry::Key("Shift+拖拽", "选择文本"),
];

/// 常见问题分页。
///
/// 问题从上游 `pages/help.vue` 的 FAQ 与本站排障表里挑出终端形态下有意义的
/// 部分，答案是按本项目实际行为重写的，不照搬 Web 版（浏览器插件拦截、
/// 手机 / 平板、Supabase 同步等条目在终端里不适用，已替换为终端相关条目）。
const FAQ: &[HelpEntry] = &[
    HelpEntry::Section("常见问题"),
    HelpEntry::Question("数据保存在哪里？怎么备份 / 换电脑？"),
    HelpEntry::Note(
        "全部在本地数据目录（Windows 默认 %LOCALAPPDATA%\\typewords-tui），可用 --home 或 TYPEWORDS_HOME 指定。退出所有实例后复制整个目录即可备份 / 迁移；网页版存档可用 tw export-web / tw import-web 互转。",
    ),
    HelpEntry::Question("会联网吗？有账号吗？"),
    HelpEntry::Note(
        "默认完全离线：没有账号、云同步与遥测，数据全部是本地 JSON。唯一的联网能力是把设置「音频 → 发音来源」切到「有道发音」后下载 mp3，切回系统 TTS 即恢复离线。",
    ),
    HelpEntry::Question("和上游 TypeWords 是什么关系？"),
    HelpEntry::Note(
        "独立的 Rust 终端客户端：共享词库资源格式与存档字段命名，不共享代码，也不读写浏览器 IndexedDB；功能差异见发布包内 docs/upstream-parity.md。",
    ),
    HelpEntry::Question("支持哪些平台？"),
    HelpEntry::Note(
        "Windows / Linux / macOS 都有发布包（Linux 另提供 musl 静态包），也可从源码构建；单个可执行文件 + assets/ 目录，无 WebView 与 Node 运行时。终端窗口最小 60 × 20。",
    ),
    HelpEntry::Question("快捷键能改吗？"),
    HelpEntry::Note(
        "能。设置页「快捷键」进入编辑页，42 个动作都可改绑（Enter 改绑、Delete 清除、r / R 恢复默认），保存在 setting.json 的 shortcutKeyMap；本页「快捷键」分页列的是默认绑定。",
    ),
    HelpEntry::Question("没有发音 / 没有声音？"),
    HelpEntry::Note(
        "先在设置「音频」确认「单词发音」「文章朗读」已开启。系统 TTS 走系统命令：Windows 用 PowerShell 调 System.Speech，安全软件可能拦截脚本引擎——放行一次或在安全软件里排除 tw.exe 即可；练习中可按 Ctrl+P 手动重播。",
    ),
    HelpEntry::Question("每日词数、复习数量怎么算？"),
    HelpEntry::Note(
        "每日词数用主页或词库页的 + / -（每次 ±5），也可在设置页「复习 → 每日词数」精确输入。复习上限 = 每日词数 × 复习比例（默认 3 倍）；无到期复习时按设置「无到期复习时随机填充」补足。",
    ),
    HelpEntry::Question("存档损坏 / 设置乱了怎么办？"),
    HelpEntry::Note(
        "解析失败的存档会重命名为 *.corrupt-<时间戳>.json 并回退默认值；版本高于程序的 dict.json / setting.json 只读不覆盖。设置页「其他 → 清除所有数据」可恢复默认，删掉 setting.json 只重置设置。",
    ),
    HelpEntry::Question("终端显示错位 / 颜色不对？"),
    HelpEntry::Note(
        "窗口至少 60 × 20；字体要等宽且覆盖中文与制表符，推荐 Windows Terminal 与真彩色终端。颜色发灰可换「高对比」主题或自定义 ANSI 颜色主题，详见 docs/themes.md。",
    ),
    HelpEntry::Question("手机 / 平板能用吗？"),
    HelpEntry::Note(
        "不能。这是桌面终端程序：需要实体键盘与至少 60 列的终端窗口，练习靠逐字符键入，触屏没有对应的输入方式。",
    ),
];

/// 关于与反馈分页。
///
/// 对应上游 `pages/about.vue` 与 `pages/feedback.vue`（两者渲染同一个
/// `About.vue`）。联系方式按终端形态改写：上游的微信群 / 工单 / 微信小程序
/// 在 TUI 里没有等价入口，这里只给仓库 issue 地址与需要附带的诊断信息。
const ABOUT: &[HelpEntry] = &[
    HelpEntry::Section("关于"),
    HelpEntry::Note(concat!(
        "Type Words TUI v",
        env!("CARGO_PKG_VERSION"),
        "：独立的 Rust 终端客户端，单二进制、默认离线，数据全部是本地 JSON。"
    )),
    HelpEntry::Note(
        "上游项目 TypeWords（https://github.com/zyronon/TypeWords）是 Nuxt/Vue Web + Tauri 桌面端；本客户端共享它的词库资源格式与存档字段命名，不共享代码。",
    ),
    HelpEntry::Note(concat!(
        "本项目仓库：",
        env!("CARGO_PKG_REPOSITORY"),
        "（GPL-3.0）。"
    )),
    HelpEntry::DataDir,
    HelpEntry::Note(
        "词库与文章数据来自上游仓库，遵循同一 GPL-3.0 许可证。完整文档在发布包的 docs/ 目录，仓库里同路径。",
    ),
    HelpEntry::Section("反馈"),
    HelpEntry::Note(concat!(
        "提 issue：",
        env!("CARGO_PKG_REPOSITORY"),
        "/issues"
    )),
    HelpEntry::Note(
        "请附上 tw --version 输出、tw doctor 输出、操作系统与终端、复现步骤、是否用了 --home / --assets / --seed，以及出现的 Toast 文案；随机行为请附 --seed 值。",
    ),
    HelpEntry::Note(
        "崩溃或异常：附 <home>/logs 里最近的 tw-<日期>.log 与 crash-<时间戳>.txt（用过 --log 就附该文件所在目录）。",
    ),
    HelpEntry::Note(
        "词库、文章内容与上游功能建议请提到上游仓库：https://github.com/zyronon/TypeWords/issues。",
    ),
    HelpEntry::Note("本页文字只读；复制内容请按住 Shift 拖拽选择（终端开启了鼠标捕获）。"),
];

/// 一行已经按当前宽度折好的显示内容。
///
/// 逻辑条目 → 屏幕行的展开只为帮助页服务：FAQ 答案、关于信息是成段的，
/// 一条可能占 1~3 行，滚动与命中都必须按屏幕行算。
enum Row {
    Section(String),
    Key(String, String),
    /// 问题首行（带 `▸` 前缀）。
    Question(String),
    /// 问题续行（前缀位置留白）。
    QuestionCont(String),
    Note(String),
}

/// 帮助页的布局：分页条标签矩形 + 正文区域。
///
/// 绘制与鼠标命中检测共用这一份布局，分页标签画在哪就能点在哪。
struct HelpLayout {
    tabs: [Rect; HelpTab::COUNT],
    body: Rect,
}

fn layout(content: Rect) -> HelpLayout {
    let mut tabs = [Rect::new(0, 0, 0, 0); HelpTab::COUNT];
    let mut x = content.x;
    for (index, tab) in HelpTab::ALL.iter().enumerate() {
        let width = (display_width(tab.label()) + 2) as u16;
        let width = width.min(content.right().saturating_sub(x));
        tabs[index] = Rect::new(x, content.y, width, TAB_BAR_HEIGHT);
        x = x.saturating_add(width).saturating_add(1);
    }
    let body_y = content
        .y
        .saturating_add(TAB_BAR_HEIGHT + TAB_BAR_GAP)
        .min(content.bottom());
    let body = Rect::new(
        content.x,
        body_y,
        content.width,
        content.bottom().saturating_sub(body_y),
    );
    HelpLayout { tabs, body }
}

/// 练习 / 跟打中打开的整屏帮助内容区（绘制与鼠标命中共用）。
pub fn fullscreen_content(area: Rect) -> Rect {
    Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(1),
    )
}

/// 按当前屏幕算帮助页的内容区（分页条 + 正文所在区域）。
fn content_rect(app: &App) -> Rect {
    if app.in_typing_session() {
        fullscreen_content(app.viewport)
    } else {
        shell::layout(app.viewport, Nav::Help).content
    }
}

pub fn render(app: &App, frame: &mut Frame, area: Rect, theme: &Theme) {
    // 练习 / 跟打中进入帮助：整屏显示，不画侧栏。
    //
    // 练习中侧栏的导航是禁用的，画出来却点不动，用户会以为鼠标坏了。
    if app.in_typing_session() {
        let title = if app.session.is_some() {
            "帮助（Esc 返回练习）"
        } else {
            "帮助（Esc 返回跟打）"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                title,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(area.x + 2, area.y, area.width.saturating_sub(4), 1),
        );
        render_body(app, frame, fullscreen_content(area), theme);
        return;
    }
    shell::render(app, frame, area, theme, Nav::Help, |frame, content| {
        render_body(app, frame, content, theme);
    });
}

fn render_body(app: &App, frame: &mut Frame, content: Rect, theme: &Theme) {
    if content.width == 0 || content.height == 0 {
        return;
    }
    let layout = layout(content);
    // 整屏帮助没有底栏，按键提示只能画在分页条右侧；套外壳时底栏已经有一份
    // 同样的提示（`hints_for(Screen::Help)`），不重复占位。
    render_tabs(frame, &layout, app.help_tab, theme, app.in_typing_session());
    let body = layout.body;
    if body.height == 0 {
        return;
    }
    let rows = body_rows(app, body);
    let state =
        widgets::ScrollbarState::new(app.help_scroll as usize, rows.total(), body.height as usize);
    let offset = state.offset;
    match &rows {
        Rows::Two(left, right) => {
            let left_max = left.len().saturating_sub(state.visible);
            let right_max = right.len().saturating_sub(state.visible);
            render_columns(
                frame,
                body,
                left,
                right,
                theme,
                offset.min(left_max),
                offset.min(right_max),
            );
        }
        Rows::Single(rows) => {
            let list = Rect::new(
                body.x,
                body.y,
                body.width.saturating_sub(1).max(1),
                body.height,
            );
            render_rows(frame, list, rows, theme, offset);
        }
    }
    widgets::scrollbar(frame, scrollbar_rect(body), state, theme);
}

/// 滚动条所占的列：正文最右侧 1 列。
fn scrollbar_rect(body: Rect) -> Rect {
    Rect::new(body.right().saturating_sub(1), body.y, 1, body.height)
}

/// 分页条：当前分页高亮（selection 底色）；整屏帮助时右侧给出按键提示。
fn render_tabs(
    frame: &mut Frame,
    layout: &HelpLayout,
    active: HelpTab,
    theme: &Theme,
    show_hint: bool,
) {
    for (tab, rect) in HelpTab::ALL.iter().zip(layout.tabs.iter()) {
        if rect.width == 0 {
            continue;
        }
        let selected = *tab == active;
        if selected {
            widgets::paint_rect(frame, *rect, theme.selection);
        }
        let text = fit_text(&format!(" {} ", tab.label()), rect.width as usize);
        let mut style = Style::default().fg(if selected { theme.fg } else { theme.muted });
        if selected {
            style = style.add_modifier(Modifier::BOLD);
        }
        frame.render_widget(Paragraph::new(Span::styled(text, style)), *rect);
    }
    if !show_hint {
        return;
    }
    // 提示与底栏共用同一份键位定义，避免两处文案（包括大小写）漂移。
    let hint = shell::hints_for(Screen::Help)
        .iter()
        .map(|(key, label)| format!("{key} {label}"))
        .collect::<Vec<_>>()
        .join(" · ");
    let hint_width = display_width(&hint) as u16;
    if hint_width == 0 || layout.body.width < hint_width {
        return;
    }
    let last_tab_right = layout
        .tabs
        .iter()
        .map(|rect| rect.right())
        .max()
        .unwrap_or(0);
    let x = layout.body.right().saturating_sub(hint_width);
    // 60~64 列时分页条右侧放不下：落到分页条与正文之间的留白行，保证
    // 最小支持宽度也有「←→ 分页」提示（整屏帮助没有底栏可退）。
    let y = if x > last_tab_right {
        layout.tabs[0].y
    } else if layout.tabs[0].y.saturating_add(TAB_BAR_GAP) < layout.body.y {
        layout.tabs[0].y.saturating_add(TAB_BAR_GAP)
    } else {
        return;
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(theme.dim))),
        Rect::new(x, y, hint_width, 1),
    );
}

fn two_columns(tab: HelpTab, body: Rect) -> bool {
    tab == HelpTab::Shortcuts
        && body.width >= TWO_COLUMN_MIN_WIDTH
        && body.height >= TWO_COLUMN_MIN_HEIGHT
}

/// 两栏的列宽（左栏 / 右栏）：中间 1 列分隔线 + 1 列留白，最右 1 列留给
/// 滚动条。折行与绘制必须用同一组宽度。
fn column_widths(body: Rect) -> (u16, u16) {
    let left = body.width.saturating_sub(3) / 2;
    (left, body.width.saturating_sub(left + 3))
}

/// 快捷键分页的两栏行；其它分页或宽度不够时返回 `None`（走单栏）。
fn two_column_rows(app: &App, body: Rect) -> Option<(Vec<Row>, Vec<Row>)> {
    if !two_columns(app.help_tab, body) {
        return None;
    }
    let (left_width, right_width) = column_widths(body);
    if left_width < 26 {
        return None;
    }
    let data_dir = data_dir_line(app);
    Some((
        build_rows(&typing_entries(), left_width as usize, data_dir.as_deref()),
        build_rows(&nav_entries(), right_width as usize, data_dir.as_deref()),
    ))
}

/// 「关于」分页的动态行：当前数据目录（`--home` / `TYPEWORDS_HOME` 可覆盖）。
fn data_dir_line(app: &App) -> Option<String> {
    app.paths.as_ref().map(|paths| {
        format!(
            "数据目录：{}（可用 --home / TYPEWORDS_HOME 覆盖）",
            paths.home.display()
        )
    })
}

/// 单栏分页的行（正文右侧留 1 列给滚动条）。
fn single_rows(app: &App, body: Rect) -> Vec<Row> {
    let width = body.width.saturating_sub(1).max(1) as usize;
    build_rows(
        &flat_entries(app.help_tab),
        width,
        data_dir_line(app).as_deref(),
    )
}

/// 当前分页的滚动状态：实际生效的偏移量、内容总行数与视口行数。
///
/// 偏移量以**屏幕行**为单位；两栏时以更长的一栏为准，否则右栏最后几行
/// 永远滚不出来。
fn scroll_state(app: &App, body: Rect) -> widgets::ScrollbarState {
    if body.height == 0 {
        return widgets::ScrollbarState::new(0, 0, 0);
    }
    let total = body_rows(app, body).total();
    widgets::ScrollbarState::new(app.help_scroll as usize, total, body.height as usize)
}

/// 当前分页的屏幕行：两栏时左 / 右各一份，否则单栏一份。
///
/// 绘制、滚动上限与点击轨道都要用同一份折行结果；分开算一次会多画一遍折行，
/// 也容易出现「上限按这一份算、绘制按那一份算」的漂移。
enum Rows {
    Two(Vec<Row>, Vec<Row>),
    Single(Vec<Row>),
}

impl Rows {
    /// 内容总行数：两栏取更长的一栏。
    fn total(&self) -> usize {
        match self {
            Rows::Two(left, right) => left.len().max(right.len()),
            Rows::Single(rows) => rows.len(),
        }
    }
}

fn body_rows(app: &App, body: Rect) -> Rows {
    match two_column_rows(app, body) {
        Some((left, right)) => Rows::Two(left, right),
        None => Rows::Single(single_rows(app, body)),
    }
}

fn render_columns(
    frame: &mut Frame,
    body: Rect,
    left_rows: &[Row],
    right_rows: &[Row],
    theme: &Theme,
    left_offset: usize,
    right_offset: usize,
) {
    let (left_width, right_width) = column_widths(body);
    let left = Rect::new(body.x, body.y, left_width, body.height);
    let divider_x = body.x + left_width;
    let right = Rect::new(
        divider_x.saturating_add(2),
        body.y,
        right_width,
        body.height,
    );
    render_rows(frame, left, left_rows, theme, left_offset);
    render_rows(frame, right, right_rows, theme, right_offset);
    for y in body.y..body.bottom() {
        frame.render_widget(
            Paragraph::new(Span::styled("│", Style::default().fg(theme.border))),
            Rect::new(divider_x, y, 1, 1),
        );
    }
}

/// 帮助页的鼠标目标：分页标签 + 正文右侧滚动条的可点轨道。
///
/// 正文是只读文本，点哪里都不产生动作——早期实现把整块内容区映射成
/// `HelpBack`，任何一次点击（聚焦窗口、点空白、选文字）都会退出帮助页。
/// 返回由 `Esc` / 键盘「任意键返回」与侧栏导航负责。
pub fn targets(app: &App, content: Rect) -> Vec<crate::ui::target::Target> {
    use crate::ui::target::Target;
    let mut result = Vec::new();
    if content.width == 0 || content.height == 0 {
        return result;
    }
    let layout = layout(content);
    for (tab, rect) in HelpTab::ALL.iter().zip(layout.tabs.iter()) {
        if rect.width > 0 {
            result.push(Target::labeled(
                *rect,
                crate::app::Action::HelpSelectTab(*tab),
                tab.label(),
            ));
        }
    }
    if layout.body.width == 0 || layout.body.height == 0 {
        return result;
    }
    // 滚动条上滑块上下各有一段轨道：点一次滚一行。滑块本身不产生动作。
    let state = scroll_state(app, layout.body);
    for (rect, delta) in widgets::scrollbar_click_rows(state, scrollbar_rect(layout.body)) {
        result.push(Target::new(rect, crate::app::Action::HelpScroll(delta)));
    }
    result
}

/// 帮助页当前分页、当前终端尺寸下的最大滚动量（单位：屏幕行）。
///
/// 条目会随版本增减，写死一个上限迟早会截断或滚过头；三个分页长度不同、
/// 正文还会按宽度折行，所以上限必须按分页 + 当前终端尺寸现算。
pub fn max_scroll(app: &App) -> u16 {
    let content = content_rect(app);
    if content.width == 0 || content.height == 0 {
        return 0;
    }
    let layout = layout(content);
    scroll_state(app, layout.body).max_offset() as u16
}

/// 两个练习页的键位分区（练习 + 文章跟打），快捷键分页的左栏。
fn typing_entries() -> Vec<HelpEntry> {
    let mut entries: Vec<HelpEntry> = Vec::new();
    entries.extend_from_slice(PRACTICE);
    entries.extend_from_slice(ARTICLE);
    entries
}

/// 导航键位 + 鼠标说明，快捷键分页的右栏。
fn nav_entries() -> Vec<HelpEntry> {
    let mut entries: Vec<HelpEntry> = NAVIGATION.to_vec();
    entries.push(HelpEntry::Note(""));
    for line in MOUSE_NOTES {
        entries.push(HelpEntry::Note(line));
    }
    entries
}

/// 某个分页按单栏顺序展开的条目（单栏折行与滚动上限共用）。
fn flat_entries(tab: HelpTab) -> Vec<HelpEntry> {
    match tab {
        HelpTab::Shortcuts => {
            let mut entries = typing_entries();
            entries.extend(nav_entries());
            entries
        }
        HelpTab::Faq => FAQ.to_vec(),
        HelpTab::About => ABOUT.to_vec(),
    }
}

/// 把逻辑条目按 `width` 折成屏幕行；问题与答案折行，键位行保持一行。
///
/// `data_dir` 是「关于」分页的数据目录行：构造常量时拿不到，由调用方从
/// `App::paths` 取出后传进来（单测因此不必构造整个 App）。
fn build_rows(entries: &[HelpEntry], width: usize, data_dir: Option<&str>) -> Vec<Row> {
    let mut rows = Vec::new();
    for entry in entries {
        match entry {
            HelpEntry::Section(title) => rows.push(Row::Section((*title).to_string())),
            HelpEntry::Key(key, label) => {
                rows.push(Row::Key((*key).to_string(), (*label).to_string()));
            }
            HelpEntry::Question(question) => {
                for (index, line) in wrap_prose(question, width.saturating_sub(2))
                    .into_iter()
                    .enumerate()
                {
                    rows.push(if index == 0 {
                        Row::Question(line)
                    } else {
                        Row::QuestionCont(line)
                    });
                }
            }
            HelpEntry::Note(text) => push_note_rows(&mut rows, text, width),
            HelpEntry::DataDir => match data_dir {
                Some(text) => push_note_rows(&mut rows, text, width),
                None => push_note_rows(&mut rows, "数据目录：未解析（可用 --home 指定）", width),
            },
        }
    }
    rows
}

fn push_note_rows(rows: &mut Vec<Row>, text: &str, width: usize) {
    for line in wrap_prose(text, width.saturating_sub(2)) {
        rows.push(Row::Note(line));
    }
}

fn render_rows(frame: &mut Frame, area: Rect, rows: &[Row], theme: &Theme, offset: usize) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    for (y, row) in (area.y..area.bottom()).zip(rows.get(offset..).unwrap_or(&[])) {
        let rect = Rect::new(area.x, y, area.width, 1);
        match row {
            Row::Section(title) => render_section(frame, rect, title, theme),
            Row::Key(key, label) => render_key(frame, rect, key, label, theme),
            Row::Question(text) => render_question(frame, rect, text, theme, true),
            Row::QuestionCont(text) => render_question(frame, rect, text, theme, false),
            Row::Note(text) => frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {text}"),
                    Style::default().fg(theme.dim),
                )),
                rect,
            ),
        }
    }
}

fn render_question(frame: &mut Frame, rect: Rect, text: &str, theme: &Theme, first: bool) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let prefix = if first { "▸ " } else { "  " };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prefix.to_string(), Style::default().fg(theme.accent_alt)),
            Span::styled(
                text.to_string(),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
        ])),
        rect,
    );
}

fn render_section(frame: &mut Frame, rect: Rect, title: &str, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let prefix = fit_text(&format!("▎ {title} "), rect.width as usize);
    let prefix_width = display_width(&prefix);
    let rule_width = (rect.width as usize).saturating_sub(prefix_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(rule_width), Style::default().fg(theme.border)),
        ])),
        rect,
    );
}

fn render_key(frame: &mut Frame, rect: Rect, key: &str, label: &str, theme: &Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let width = rect.width as usize;
    let indent = usize::from(width > 2);
    let available = width.saturating_sub(indent);
    let key = fit_text(key, available);
    let key_width = display_width(&key);
    let label = fit_text(label, available.saturating_sub(key_width + 1));
    frame.render_widget(
        Paragraph::new(Line::from(widgets::key_hint(&key, &label, theme))),
        Rect::new(
            rect.x.saturating_add(indent as u16),
            rect.y,
            available as u16,
            1,
        ),
    );
}

/// 帮助正文折行：CJK 逐字折，拉丁单词尽量整词成行。
///
/// `widgets::wrap_text` 按显示宽度逐字符折行，长英文单词（TYPEWORDS_HOME、
/// PowerShell、IndexedDB）会被劈成两半；帮助页是大段说明文字，因此在空白 /
/// CJK 边界上换行，只有超长单词才退回逐字符。
fn wrap_prose(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut line_width = 0usize;
        for unit in prose_units(paragraph) {
            match unit {
                ProseUnit::Space => {
                    if line.is_empty() {
                        continue;
                    }
                    line.push(' ');
                    line_width += 1;
                }
                ProseUnit::Atom(text) => {
                    let unit_width = display_width(&text);
                    if line_width + unit_width > width && !line.is_empty() {
                        push_line(&mut lines, &mut line);
                        line_width = 0;
                    }
                    if unit_width > width {
                        // 超长单位（单词 / 字符）：逐字符折，保证不会画出界。
                        for character in text.chars() {
                            push_char(&mut lines, &mut line, &mut line_width, character, width);
                        }
                    } else {
                        line.push_str(&text);
                        line_width += unit_width;
                    }
                }
            }
        }
        push_line(&mut lines, &mut line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

enum ProseUnit {
    Space,
    /// 不可再拆的显示单位：连续 ASCII 非空白字符（单词、路径、URL、组合键），
    /// 或单个 CJK 字符 / 标点。行尾标点会粘到前一个单位上。
    Atom(String),
}

/// 把段落切成「尽量不拆的 ASCII 单词 + 可逐字折的字符」。
///
/// 行尾标点（。，、；：！？」…）粘到前一个单位上，保证它永远不落在行首：
/// 单独成单位的话，折行点正好卡在标点前时会只剩下标点一行。
fn prose_units(paragraph: &str) -> Vec<ProseUnit> {
    let mut units = Vec::new();
    let mut word = String::new();
    for character in paragraph.chars() {
        if character == ' ' {
            if !word.is_empty() {
                units.push(ProseUnit::Atom(std::mem::take(&mut word)));
            }
            units.push(ProseUnit::Space);
        } else if character.is_ascii() {
            word.push(character);
        } else {
            if !word.is_empty() {
                units.push(ProseUnit::Atom(std::mem::take(&mut word)));
            }
            if is_trailing_punctuation(character)
                && let Some(last) = units.last_mut()
            {
                match last {
                    ProseUnit::Atom(text) => {
                        text.push(character);
                        continue;
                    }
                    ProseUnit::Space => {}
                }
            }
            units.push(ProseUnit::Atom(character.to_string()));
        }
    }
    if !word.is_empty() {
        units.push(ProseUnit::Atom(word));
    }
    units
}

/// 单个字符超长（比整行还宽）时的兜底：逐字符折行。
fn push_char(
    lines: &mut Vec<String>,
    line: &mut String,
    line_width: &mut usize,
    character: char,
    width: usize,
) {
    let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
    if *line_width + character_width > width && !line.is_empty() {
        push_line(lines, line);
        *line_width = 0;
    }
    line.push(character);
    *line_width += character_width;
}

/// 不应出现在行首的中文 / 全角标点。
fn is_trailing_punctuation(character: char) -> bool {
    matches!(
        character,
        '。' | '，'
            | '、'
            | '；'
            | '：'
            | '！'
            | '？'
            | '）'
            | '」'
            | '』'
            | '》'
            | '〉'
            | '…'
            | '·'
    )
}

fn push_line(lines: &mut Vec<String>, line: &mut String) {
    let text = line.trim_end().to_string();
    line.clear();
    lines.push(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 折行不能把拉丁单词劈成两半；超长单词才允许逐字符折。
    #[test]
    fn prose_wrapping_keeps_latin_words_whole() {
        let lines = wrap_prose("可用 --home 或 TYPEWORDS_HOME 指定数据目录", 20);
        assert!(lines.iter().any(|line| line.contains("TYPEWORDS_HOME")));
        assert!(lines.iter().all(|line| display_width(line) <= 20));
    }

    /// 帮助页按屏幕行计算滚动上限：FAQ 段落比视口长时必须能滚到底。
    /// 中文标点不落在行首，也不会把前面的单词劈开。
    #[test]
    fn prose_wrapping_keeps_closing_punctuation_at_line_end() {
        let lines = wrap_prose("数据全部是本地 JSON。上游项目是 Nuxt", 20);
        assert!(
            lines.iter().all(|line| !line.starts_with('。')),
            "标点不应出现在行首: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("JSON。")),
            "标点应跟在前一个词后面，而不是拆开单词: {lines:?}"
        );
    }

    #[test]
    fn faq_rows_exceed_logical_entries() {
        let rows = build_rows(FAQ, 60, None).len();
        assert!(rows > FAQ.len(), "折行后屏幕行应多于逻辑条目");
    }
}
