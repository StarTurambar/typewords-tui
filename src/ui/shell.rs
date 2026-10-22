//! 应用外壳：侧栏 / 顶栏 / 底栏 + 响应式布局。
//!
//! 布局函数与渲染函数共用同一组 Rect，鼠标命中检测也复用 `layout`。

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Screen};
use crate::domain::constants::practice_mode_name;
use crate::ui::bigtext::{self, BigLetter};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text, footer_hints};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Home,
    Articles,
    Dicts,
    Stats,
    Settings,
    Help,
}

impl Nav {
    /// 窄屏顶栏的全部标签，顺序即视觉顺序。
    pub const ALL: [Nav; 6] = [
        Nav::Home,
        Nav::Articles,
        Nav::Dicts,
        Nav::Stats,
        Nav::Help,
        Nav::Settings,
    ];
    /// 宽屏侧栏主导航区（设置固定在底部，不在此列）。
    pub const MAIN: [Nav; 5] = [Nav::Home, Nav::Articles, Nav::Dicts, Nav::Stats, Nav::Help];

    pub fn from_screen(screen: Screen) -> Self {
        match screen {
            Screen::Home | Screen::Practice | Screen::WordTest => Nav::Home,
            Screen::Words => Nav::Dicts,
            Screen::Articles | Screen::ArticlePractice => Nav::Articles,
            // 流程页是单词练习的附属页面，归到「单词」下。
            Screen::Flows => Nav::Home,
            Screen::DictPicker => Nav::Dicts,
            Screen::Stats => Nav::Stats,
            Screen::Settings | Screen::Shortcuts => Nav::Settings,
            Screen::Help => Nav::Help,
        }
    }

    pub fn screen(self) -> Screen {
        match self {
            Nav::Home => Screen::Home,
            Nav::Articles => Screen::Articles,
            Nav::Dicts => Screen::DictPicker,
            Nav::Stats => Screen::Stats,
            Nav::Settings => Screen::Settings,
            Nav::Help => Screen::Help,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Nav::Home => "单词",
            Nav::Articles => "文章",
            Nav::Dicts => "词库",
            Nav::Stats => "统计",
            Nav::Settings => "设置",
            Nav::Help => "帮助",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Nav::Home => "1",
            Nav::Articles => "2",
            Nav::Dicts => "3",
            Nav::Stats => "4",
            Nav::Help => "5",
            Nav::Settings => "6",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SidebarLayout {
    pub area: Rect,
    pub wordmark: Rect,
    pub wordmark_rows: u16,
    pub subtitle: Option<Rect>,
    pub top_nav: Vec<(Screen, Rect)>,
    pub bottom_rule: Rect,
    pub version: Rect,
    pub settings: Rect,
    pub quit: Rect,
}

#[derive(Clone, Debug)]
pub struct ShellLayout {
    pub sidebar: Option<SidebarLayout>,
    pub main: Rect,
    pub topbar: Rect,
    pub rule: Rect,
    pub body: Rect,
    pub footer: Rect,
    pub content: Rect,
    pub nav_targets: Vec<(Screen, Rect)>,
    pub quit_target: Option<Rect>,
    pub narrow: bool,
}

pub fn layout(area: Rect, _active: Nav) -> ShellLayout {
    let narrow = area.width < 100;
    let (sidebar, main) = if narrow {
        (None, area)
    } else {
        let width = if area.width >= 120 { 26 } else { 24 };
        let sidebar = Rect::new(area.x, area.y, width, area.height);
        let main = Rect::new(
            area.x + width,
            area.y,
            area.width.saturating_sub(width),
            area.height,
        );
        (Some(sidebar_layout(sidebar, area.height)), main)
    };

    let topbar = Rect::new(main.x, main.y, main.width, 1);
    let rule = Rect::new(main.x, main.y.saturating_add(1), main.width, 1);
    let footer = Rect::new(
        main.x,
        main.y + main.height.saturating_sub(1),
        main.width,
        1,
    );
    let body_y = main.y.saturating_add(3);
    let body_height = footer.y.saturating_sub(body_y);
    let body = Rect::new(main.x, body_y, main.width, body_height);
    let content = Rect::new(
        body.x.saturating_add(2),
        body.y,
        body.width.saturating_sub(4),
        body.height,
    );

    let mut nav_targets = Vec::new();
    let mut quit_target = None;
    if let Some(sidebar) = &sidebar {
        nav_targets.extend(sidebar.top_nav.iter().cloned());
        nav_targets.push((Screen::Settings, sidebar.settings));
        quit_target = Some(sidebar.quit);
    } else {
        nav_targets.extend(narrow_nav_targets(topbar, area.width));
    }

    ShellLayout {
        sidebar,
        main,
        topbar,
        rule,
        body,
        footer,
        content,
        nav_targets,
        quit_target,
        narrow,
    }
}

fn sidebar_layout(area: Rect, height: u16) -> SidebarLayout {
    let inner_width = area.width.saturating_sub(4).max(1);
    let can_2x = inner_width >= 22 && height >= 28;
    let max_height = if can_2x { 7 } else { 4 };
    let letters = [
        BigLetter::new(ratatui::style::Color::Reset, false),
        BigLetter::new(ratatui::style::Color::Reset, false),
    ];
    let rows = bigtext::render_word("TW", &letters, inner_width as usize, max_height as usize)
        .map(|lines| lines.len() as u16)
        .unwrap_or(4)
        .min(max_height);
    let wordmark = Rect::new(area.x + 2, area.y + 1, inner_width, rows);
    let subtitle_y = area.y + 1 + rows;
    let subtitle =
        (subtitle_y + 1 < height).then(|| Rect::new(area.x + 2, subtitle_y, inner_width, 1));
    let nav_start = subtitle_y + 2;
    let mut top_nav = Vec::new();
    for (index, nav) in Nav::MAIN.iter().enumerate() {
        let y = nav_start + (index as u16) * 2;
        if y + 1 >= height.saturating_sub(5) {
            break;
        }
        top_nav.push((
            nav.screen(),
            Rect::new(area.x, y, area.width.saturating_sub(1), 2),
        ));
    }
    let bottom_rule_y = height.saturating_sub(5);
    let version_y = height.saturating_sub(4);
    let settings_y = height.saturating_sub(3);
    let quit_y = height.saturating_sub(2);
    SidebarLayout {
        area,
        wordmark,
        wordmark_rows: rows,
        subtitle,
        top_nav,
        bottom_rule: Rect::new(
            area.x,
            area.y + bottom_rule_y,
            area.width.saturating_sub(1),
            1,
        ),
        version: Rect::new(area.x + 2, area.y + version_y, inner_width, 1),
        settings: Rect::new(area.x, area.y + settings_y, area.width.saturating_sub(1), 1),
        quit: Rect::new(area.x, area.y + quit_y, area.width.saturating_sub(1), 1),
    }
}

fn narrow_nav_targets(topbar: Rect, _width: u16) -> Vec<(Screen, Rect)> {
    let x_start = topbar.x.saturating_add(2);
    let mut x = x_start + display_width("Type Words") as u16 + 2;
    let mut targets = Vec::new();
    for (index, nav) in Nav::ALL.iter().enumerate() {
        let label = nav.label();
        let width = (display_width(label) + 2) as u16;
        if x + width > topbar.x + topbar.width.saturating_sub(1) {
            break;
        }
        targets.push((nav.screen(), Rect::new(x, topbar.y, width, 1)));
        x += width + if index == Nav::ALL.len() - 1 { 0 } else { 1 };
    }
    targets
}

pub fn render(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    active: Nav,
    body: impl FnOnce(&mut Frame, Rect),
) {
    let shell = layout(area, active);
    if let Some(sidebar) = &shell.sidebar {
        render_sidebar(app, frame, sidebar, theme, active);
    }
    render_topbar(app, frame, &shell, theme, active);
    frame.render_widget(
        Paragraph::new(widgets::rule(shell.content.width as usize, theme)),
        shell.rule,
    );
    body(frame, shell.content);
    render_footer(app, frame, &shell, theme, active);
}

fn render_sidebar(
    app: &App,
    frame: &mut Frame,
    sidebar: &SidebarLayout,
    theme: &Theme,
    active: Nav,
) {
    frame.render_widget(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(theme.border)),
        sidebar.area,
    );

    let letters = [
        BigLetter::new(theme.accent, false),
        BigLetter::new(theme.accent_alt, false),
    ];
    if let Some(lines) = bigtext::render_word(
        "TW",
        &letters,
        sidebar.wordmark.width as usize,
        sidebar.wordmark_rows as usize,
    ) {
        frame.render_widget(Paragraph::new(lines), sidebar.wordmark);
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "TW",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            sidebar.wordmark,
        );
    }

    if let Some(subtitle) = sidebar.subtitle {
        frame.render_widget(
            Paragraph::new(Span::styled("Type Words", Style::default().fg(theme.muted))),
            subtitle,
        );
    }

    for (screen, rect) in &sidebar.top_nav {
        let nav = Nav::from_screen(*screen);
        render_nav_row(frame, *rect, sidebar.area, theme, nav, nav == active);
    }
    frame.render_widget(
        Paragraph::new(widgets::rule(sidebar.bottom_rule.width as usize, theme)),
        sidebar.bottom_rule,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(theme.dim),
        )),
        sidebar.version,
    );
    render_nav_row(
        frame,
        sidebar.settings,
        sidebar.area,
        theme,
        Nav::Settings,
        active == Nav::Settings,
    );
    render_quit_row(frame, sidebar.quit, sidebar.area, theme);
    let _ = app;
}

fn render_nav_row(
    frame: &mut Frame,
    row: Rect,
    sidebar: Rect,
    theme: &Theme,
    nav: Nav,
    active: bool,
) {
    let inner_width = sidebar.width.saturating_sub(4) as usize;
    let text_y = row.y;
    let label_width = display_width(nav.label());
    let key = nav.key();
    let key_width = display_width(key);
    let available = inner_width.saturating_sub(key_width + 1);
    let label = fit_text(nav.label(), available);
    let gap = available.saturating_sub(display_width(&label));

    if active {
        widgets::paint_rect(
            frame,
            Rect::new(row.x, text_y, row.width, 1),
            theme.selection,
        );
        widgets::paint_rect(frame, Rect::new(row.x, text_y, 1, 1), theme.accent);
    }
    let mut spans = vec![Span::styled(
        label,
        Style::default().fg(if active { theme.fg } else { theme.muted }),
    )];
    spans.push(Span::raw(" ".repeat(gap)));
    spans.push(Span::styled(
        key.to_string(),
        Style::default().fg(if active { theme.accent_alt } else { theme.dim }),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(row.x + 2, text_y, inner_width as u16, 1),
    );
    let _ = label_width;
}

fn render_quit_row(frame: &mut Frame, row: Rect, sidebar: Rect, theme: &Theme) {
    let inner_width = sidebar.width.saturating_sub(4) as usize;
    let available = inner_width.saturating_sub(2);
    let label = fit_text("退出", available);
    let gap = available.saturating_sub(display_width(&label));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(label, Style::default().fg(theme.muted)),
            Span::raw(" ".repeat(gap)),
            Span::styled("q", Style::default().fg(theme.dim)),
        ])),
        Rect::new(row.x + 2, row.y, inner_width as u16, 1),
    );
}

fn render_topbar(app: &App, frame: &mut Frame, shell: &ShellLayout, theme: &Theme, active: Nav) {
    let topbar = Rect::new(shell.content.x, shell.topbar.y, shell.content.width, 1);
    if shell.narrow {
        let title = "Type Words".to_string();
        let title_width = display_width(&title) as u16;
        frame.render_widget(
            Paragraph::new(Span::styled(
                title,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Rect::new(topbar.x, topbar.y, title_width, 1),
        );
        for (screen, rect) in &shell.nav_targets {
            let nav = Nav::from_screen(*screen);
            let is_active = nav == active;
            if is_active {
                widgets::paint_rect(frame, *rect, theme.element);
            }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {} ", nav.label()),
                    Style::default().fg(if is_active { theme.fg } else { theme.muted }),
                )))
                .alignment(Alignment::Center),
                *rect,
            );
        }
    } else {
        let title = active.label();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("▎ ", Style::default().fg(theme.accent)),
                Span::styled(
                    title.to_string(),
                    Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                ),
            ])),
            topbar,
        );
        let meta = format!(
            "模式 {}  ·  {}",
            practice_mode_name(app.boot.settings.word_practice_mode),
            app.theme_label()
        );
        let meta = fit_text(&meta, topbar.width as usize);
        if display_width(&meta) + display_width(title) + 4 < topbar.width as usize {
            frame.render_widget(
                Paragraph::new(Span::styled(meta, Style::default().fg(theme.muted)))
                    .alignment(Alignment::Right),
                topbar,
            );
        }
    }
}

fn render_footer(app: &App, frame: &mut Frame, shell: &ShellLayout, theme: &Theme, _active: Nav) {
    let footer = Rect::new(shell.content.x, shell.footer.y, shell.content.width, 1);
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let version_width = display_width(&version);
    let hint_width = (footer.width as usize).saturating_sub(version_width + 2);
    let now = crate::domain::timer::now_ms();
    let left = match app.current_toast(now) {
        Some(toast) => Line::from(Span::styled(
            fit_text(&toast.message, hint_width),
            Style::default().fg(match toast.level {
                crate::domain::flow::navigator::NoticeLevel::Success => theme.good,
                crate::domain::flow::navigator::NoticeLevel::Warning => theme.warn,
                crate::domain::flow::navigator::NoticeLevel::Error => theme.bad,
                crate::domain::flow::navigator::NoticeLevel::Info => theme.accent_alt,
            }),
        )),
        None => {
            let hints = if app.screen == Screen::Home && hint_width < 52 {
                HOME_COMPACT_HINTS
            } else {
                hints_for(app.screen)
            };
            Line::from(footer_hints(hints, hint_width, theme))
        }
    };
    frame.render_widget(Paragraph::new(left), footer);
    if footer.width as usize > version_width + 2 {
        frame.render_widget(
            Paragraph::new(Span::styled(version, Style::default().fg(theme.dim)))
                .alignment(Alignment::Right),
            footer,
        );
    }
}

/// 60 列左右时优先保留开始 / 自由 / 词库 / 帮助 / 退出。
const HOME_COMPACT_HINTS: &[(&str, &str)] = &[
    ("enter", "开始"),
    ("f", "自由"),
    ("d", "词库"),
    ("?", "帮助"),
    ("q", "退出"),
];

pub fn hints_for(screen: Screen) -> &'static [(&'static str, &'static str)] {
    match screen {
        Screen::Home => &[
            ("enter", "开始"),
            ("f", "自由练习"),
            ("d", "词库"),
            ("a", "文章"),
            ("s", "设置"),
            ("t", "统计"),
            ("?", "帮助"),
            ("q", "退出"),
        ],
        Screen::DictPicker => &[("enter", "选择"), ("+/-", "每日词数"), ("esc", "返回")],
        Screen::Stats => &[("↑↓", "滚动"), ("esc", "返回")],
        Screen::Settings => &[("↑↓", "选择"), ("enter", "修改"), ("esc", "保存返回")],
        Screen::WordTest => &[("1~4", "选择"), ("空格", "下一题"), ("esc", "退出")],
        Screen::Articles => &[
            ("↑↓", "选书本"),
            ("←→", "选文章"),
            ("enter", "开始"),
            ("esc", "返回"),
        ],
        Screen::ArticlePractice => &[("输入", "跟打"), ("ctrl+p", "朗读"), ("esc", "返回")],
        Screen::Flows => &[
            ("↑↓", "选择"),
            ("enter", "启用"),
            ("e", "编辑"),
            ("esc", "返回"),
        ],
        Screen::Words => &[
            ("↑↓", "选择"),
            ("输入", "搜索"),
            ("enter", "编辑"),
            ("space", "勾选"),
            ("esc", "返回"),
        ],
        Screen::Shortcuts => &[
            ("↑↓", "选择"),
            ("enter", "改绑"),
            ("delete", "清除"),
            ("r", "恢复默认"),
            ("esc", "返回"),
        ],
        Screen::Help => &[("↑↓", "滚动"), ("其它键", "返回")],
        Screen::Practice => &[],
    }
}

pub fn toast_line(app: &App, theme: &Theme) -> Line<'static> {
    let now = crate::domain::timer::now_ms();
    match app.current_toast(now) {
        Some(toast) => Line::from(Span::styled(
            toast.message.clone(),
            Style::default().fg(match toast.level {
                crate::domain::flow::navigator::NoticeLevel::Success => theme.good,
                crate::domain::flow::navigator::NoticeLevel::Warning => theme.warn,
                crate::domain::flow::navigator::NoticeLevel::Error => theme.bad,
                crate::domain::flow::navigator::NoticeLevel::Info => theme.accent_alt,
            }),
        )),
        None if app.settings().show_usage_tips => Line::from(Span::styled(
            "提示：Ctrl+C 退出练习并保存；? 查看快捷键",
            Style::default().fg(theme.dim),
        )),
        None => Line::from(""),
    }
}

pub fn render_too_small(frame: &mut Frame, area: Rect, theme: &Theme) {
    let text = vec![
        Line::from(Span::styled(
            "终端太小，请放大窗口",
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "最小尺寸 60 × 20",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .block(widgets::modal_block(" Type Words ", theme)),
        area,
    );
}
