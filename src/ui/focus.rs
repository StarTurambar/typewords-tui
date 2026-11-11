//! 键盘焦点：焦点身份、两栏与方向移动规则。
//!
//! 焦点不存「列表下标」，而存 FocusId（导航栏 NavId / 主页 ContentId / 其它页面 Page）。
//! 下标会随终端尺寸、练习模式名、词库数量、区块显隐漂移；身份不会。
//! 每次按键、Enter、绘制底栏与高亮前，都按当前布局把身份解析成目标；
//! 解析不到就视为失效（清空焦点，不触发任何动作）。
//!
//! 所有套外壳的页面（主页、文章、词库、统计、设置、词条、流程、快捷键、帮助）
//! 共用同一套两栏模型：宽屏左侧导航栏（竖向）/ 右侧内容区；窄屏顶部导航栏
//! （横向）/ 下方内容区。方向键按栏的主轴工作：
//!
//! - 内容区：主页按视觉行分带（带内左右移动，带间上下移动），方向永不反转；
//!   其它页面的内容区就是页面自己的光标（列表行 / 日历日期 / 滚动位置），
//!   上下键移动它，左右键执行页面自己的横向操作（文章页换栏、统计页选日期）。
//! - 宽屏侧栏：上/下选导航项，右回内容区；窄屏顶栏：左/右选标签，下回内容区。
//! - 跨栏：宽屏内容区在左边缘再按 ←、窄屏内容区在最顶部再按 ↑；
//!   另外 Tab / Shift+Tab 在所有页面都能直接换栏。跨栏时内容区状态原样保留。
//! - 无焦点时 下/右 落内容区，上/左 进导航栏。到边不回头回绕。
//!
//! 底栏的「选中 X」文案在 navigation 栏取导航项名，在主页取按钮名，
//! 其它页面取页面当前选中项（词库名 / 设置项名 / 日期 / 词条……）。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::app::{Action, App, FocusDir, Screen};
use crate::domain::shortcut::ShortcutAction;
use crate::ui::home;
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
pub use crate::ui::target::{ContentId, FocusId, NavId};
use crate::ui::theme::Theme;

/// 焦点所在的栏。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    /// 导航栏（宽屏侧栏 / 窄屏顶栏）。
    Nav,
    /// 页面内容。
    Content,
}

/// 导航栏的主轴：宽屏侧栏竖向、窄屏顶栏横向。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavOrientation {
    Vertical,
    Horizontal,
}

/// 该页面是否套外壳、共用两栏焦点模型。
///
/// 练习页与文章跟打页是全屏页面，不画侧栏；单词测试页是专用答题页，
/// 键位由键位表单独处理。这些页面不参与焦点导航。
pub fn supports_focus(screen: Screen) -> bool {
    matches!(
        screen,
        Screen::Home
            | Screen::Articles
            | Screen::DictPicker
            | Screen::Stats
            | Screen::Settings
            | Screen::Words
            | Screen::Flows
            | Screen::Shortcuts
            | Screen::Help
    )
}

/// 当前是否启用键盘焦点导航。
///
/// 练习会话进行中（含练习中打开的帮助页）、有弹层、或词条搜索框正在输入时
/// 方向键属于别的东西，焦点导航整套让位。
pub fn enabled(app: &App) -> bool {
    app.session.is_none()
        && supports_focus(app.screen)
        && !app.words_search_focus
        && !super::hit::modal_open(app)
}

impl NavId {
    pub fn from_nav(nav: Nav) -> Self {
        match nav {
            Nav::Home => NavId::Word,
            Nav::Articles => NavId::Articles,
            Nav::Dicts => NavId::Dicts,
            Nav::Stats => NavId::Stats,
            Nav::Help => NavId::Help,
            Nav::Settings => NavId::Settings,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NavId::Word => Nav::Home.label(),
            NavId::Articles => Nav::Articles.label(),
            NavId::Dicts => Nav::Dicts.label(),
            NavId::Stats => Nav::Stats.label(),
            NavId::Help => Nav::Help.label(),
            NavId::Settings => Nav::Settings.label(),
            NavId::Quit => "退出",
        }
    }
}

/// 当前焦点所在的栏；不在支持焦点的页面或没有焦点时返回 None。
pub fn pane(app: &App) -> Option<Pane> {
    if !enabled(app) {
        return None;
    }
    Some(match app.focus? {
        FocusId::Nav(_) => Pane::Nav,
        FocusId::Content(_) | FocusId::Page => Pane::Content,
    })
}

/// 导航栏的主轴（布局在窄屏时是顶栏，否则是侧栏）。
pub fn nav_orientation(area: Rect) -> NavOrientation {
    if shell::layout(area, Nav::Home).narrow {
        NavOrientation::Horizontal
    } else {
        NavOrientation::Vertical
    }
}

/// 导航栏的可聚焦目标（宽屏侧栏 / 窄屏顶栏，宽屏额外包含底部「退出」）。
pub fn nav_targets(app: &App, area: Rect) -> Vec<Target> {
    if !enabled(app) {
        return Vec::new();
    }
    let shell = shell::layout(area, Nav::from_screen(app.screen));
    let mut targets: Vec<Target> = shell
        .nav_targets
        .iter()
        .map(|(screen, rect)| {
            let nav = Nav::from_screen(*screen);
            Target::focusable(
                *rect,
                Action::Navigate(*screen),
                nav.label(),
                FocusId::Nav(NavId::from_nav(nav)),
            )
        })
        .collect();
    if let Some(quit) = shell.quit_target {
        targets.push(Target::focusable(
            quit,
            Action::QuitRequest,
            "退出",
            FocusId::Nav(NavId::Quit),
        ));
    }
    targets
}

/// 主页内容区的可聚焦目标（开始 / 自由 / 词库入口 / 每日目标 / 模式）。
pub fn content_targets(app: &App, area: Rect) -> Vec<Target> {
    if app.screen != Screen::Home || !enabled(app) {
        return Vec::new();
    }
    let shell = shell::layout(area, Nav::Home);
    home::targets(app, shell.content)
}

/// 当前栏的全部目标（导航栏或主页内容区）。
pub fn targets(app: &App, area: Rect) -> Vec<Target> {
    match pane(app) {
        Some(Pane::Nav) => nav_targets(app, area),
        Some(Pane::Content) => content_targets(app, area),
        None => Vec::new(),
    }
}

/// 指定身份的内容区目标（按当前布局解析）。
pub fn content_target(app: &App, area: Rect, id: ContentId) -> Option<Target> {
    content_targets(app, area)
        .into_iter()
        .find(|target| target.focus == Some(FocusId::Content(id)))
}

/// 指定身份的导航栏目标（按当前布局解析）。
pub fn nav_target(app: &App, area: Rect, id: NavId) -> Option<Target> {
    nav_targets(app, area)
        .into_iter()
        .find(|target| target.focus == Some(FocusId::Nav(id)))
}

/// 当前焦点对应的目标；仅导航栏与主页内容区有几何目标。
///
/// 其它页面的内容区焦点是页面自己的光标（`FocusId::Page`），没有独立的
/// 矩形：选中效果由页面自己的行高亮提供，这里返回 None。
pub fn focused_target(app: &App, area: Rect) -> Option<Target> {
    match app.focus? {
        FocusId::Nav(id) => nav_target(app, area, id),
        FocusId::Content(id) => content_target(app, area, id),
        FocusId::Page => None,
    }
}

/// 当前焦点对应的矩形（用于整块高亮）。
///
/// 导航栏与主页内容区取目标自身的矩形；其它页面取「页面当前选中项」的
/// 矩形（词库行 / 设置行 / 文章 / 书本行 / 词条行 / 流程行 / 快捷键行 /
/// 日历日期），同样用整块 accent 高亮，与主页保持同一套选中语言。
pub fn focused_rect(app: &App, area: Rect) -> Option<Rect> {
    pane(app)?;
    match app.focus? {
        FocusId::Page => page_selection_rect(app, area),
        _ => focused_target(app, area).map(|target| target.rect),
    }
}

/// 非主页内容区的选中项矩形；页面没有可高亮的选中项（帮助页 / 按词典
/// 视图）时返回 None。
///
/// 绘制用的行矩形统一由各页面的 layout 计算，和 targets / 渲染同源，
/// 因此高亮不会随滚动或分栏变化而漂移。
fn page_selection_rect(app: &App, area: Rect) -> Option<Rect> {
    let shell = shell::layout(area, Nav::from_screen(app.screen));
    let content = shell.content;
    match app.screen {
        Screen::DictPicker => {
            let layout = crate::ui::dict_picker::layout(app, content);
            let (_, rect) = layout
                .rows
                .iter()
                .find(|(index, _)| *index == app.dict_cursor)?;
            Some(row_rect(*rect, 1))
        }
        Screen::Settings => {
            let layout = crate::ui::settings::layout(app, content);
            let (_, rect) = layout
                .rows
                .iter()
                .find(|(entry, _)| *entry == Some(app.settings_cursor))?;
            Some(row_rect(*rect, 1))
        }
        Screen::Shortcuts => {
            let layout = crate::ui::shortcuts::layout(app, content);
            let (_, rect) = layout
                .rows
                .iter()
                .find(|(index, _)| *index == app.shortcut_cursor)?;
            Some(*rect)
        }
        Screen::Words => {
            let layout = crate::ui::words::layout(app, content);
            let total = app.filtered_word_indices().len();
            if total == 0 || app.words_cursor >= total || layout.list.height == 0 {
                return None;
            }
            let row = app.words_cursor.saturating_sub(layout.offset) as u16;
            let y = layout.list.y.saturating_add(row);
            (y < layout.list.bottom()).then(|| Rect::new(layout.list.x, y, layout.list.width, 1))
        }
        Screen::Flows => {
            let layout = crate::ui::flows::layout(app, content);
            if app.flow_cursor >= app.flow_entries().len() || layout.list.height == 0 {
                return None;
            }
            let y = layout.list.y.saturating_add(app.flow_cursor as u16);
            (y < layout.list.bottom()).then(|| Rect::new(layout.list.x, y, layout.list.width, 1))
        }
        Screen::Articles => {
            let layout = crate::ui::articles::layout(content);
            let (pane, cursor, count) = if app.article_pane_left {
                (
                    layout.books,
                    app.article_book_cursor,
                    app.article_books.len(),
                )
            } else {
                (layout.articles, app.article_cursor, app.article_list.len())
            };
            if count == 0 || cursor >= count || pane.width < 2 || pane.height < 3 {
                return None;
            }
            // 列表有边框：行从边框内侧开始，最后一行同样让出下边框。
            let y = pane.y.saturating_add(1).saturating_add(cursor as u16);
            if y >= pane.bottom().saturating_sub(1) {
                return None;
            }
            Some(Rect::new(
                pane.x.saturating_add(1),
                y,
                pane.width.saturating_sub(2),
                1,
            ))
        }
        Screen::Stats => {
            // 按词典视图没有日历选中项；日历视图高亮选中那一天（含记录点）。
            if app.stats_by_dict {
                return None;
            }
            crate::ui::stats::targets(app, content)
                .into_iter()
                .find(|target| match &target.action {
                    Action::StatsSelectDay(date) => *date == app.stats_day,
                    _ => false,
                })
                .map(|target| target.rect)
        }
        _ => None,
    }
}

/// 缩到行内、并让出右侧的滚动指示列（词库页 / 设置页用）。
fn row_rect(rect: Rect, reserved_right: u16) -> Rect {
    Rect::new(rect.x, rect.y, rect.width.saturating_sub(reserved_right), 1)
}

/// 当前焦点的提示文案（底栏「选中 X」）。
pub fn focused_label(app: &App, area: Rect) -> Option<String> {
    pane(app)?;
    match app.focus? {
        FocusId::Nav(id) => Some(id.label().to_string()),
        FocusId::Content(id) => content_target(app, area, id)
            .and_then(|target| target.label)
            .map(str::to_string),
        FocusId::Page => page_label(app),
    }
}

/// 非主页内容区的选中项名称（底栏「选中 X」）。
///
/// 页面自己的光标是唯一来源；拿不到名称时返回 None（底栏退回常规提示）。
pub fn page_label(app: &App) -> Option<String> {
    match app.screen {
        Screen::Articles => {
            if app.article_pane_left {
                app.article_books
                    .get(app.article_book_cursor)
                    .map(|book| book.name.clone())
            } else {
                app.article_list
                    .get(app.article_cursor)
                    .map(|article| article.title.clone())
            }
        }
        Screen::DictPicker => app
            .boot
            .catalog
            .resources()
            .get(app.dict_cursor)
            .map(|resource| resource.name.clone()),
        Screen::Settings => crate::app::SETTING_ROWS
            .get(app.settings_cursor)
            .map(|(key, _)| crate::domain::i18n::tr(key)),
        Screen::Stats => Some(format!("统计 {}", app.stats_day.format("%m-%d"))),
        Screen::Words => {
            let indices = app.filtered_word_indices();
            let index = *indices.get(app.words_cursor)?;
            app.boot
                .base
                .study_dict()?
                .words
                .get(index)
                .map(|word| word.word.clone())
        }
        Screen::Flows => app
            .flow_entries()
            .get(app.flow_cursor)
            .map(|entry| entry.name.clone()),
        Screen::Shortcuts => ShortcutAction::ALL
            .get(app.shortcut_cursor)
            .map(|action| action.label().to_string()),
        Screen::Help => Some("帮助".to_string()),
        _ => None,
    }
}

/// 无焦点时下/右的落点：主页优先「开始 / 继续学习」，否则第一项。
pub fn content_default(app: &App, area: Rect) -> Option<ContentId> {
    let targets = content_targets(app, area);
    targets
        .iter()
        .find(|target| target.focus == Some(FocusId::Content(ContentId::Start)))
        .or_else(|| targets.first())
        .and_then(content_id)
}

/// 进入导航栏时的落点：当前页面对应的入口，找不到则第一项。
pub fn nav_entry(app: &App, area: Rect) -> Option<NavId> {
    let targets = nav_targets(app, area);
    let wanted = NavId::from_nav(Nav::from_screen(app.screen));
    targets
        .iter()
        .find(|target| target.focus == Some(FocusId::Nav(wanted)))
        .or_else(|| targets.first())
        .and_then(nav_id)
}

/// 主页内容区一次方向移动；None 表示该方向没有可移动目标（到边）。
///
/// 上 / 下在视觉行带之间移动（目标带内水平中心最近者），永不反向；
/// 左 / 右在同一带内按 x 移动。带内到达两端时返回 None，由调用方决定
/// 是否跨栏。
pub fn content_move(
    app: &App,
    area: Rect,
    from: ContentId,
    direction: FocusDir,
) -> Option<ContentId> {
    let targets = content_targets(app, area);
    let current = targets
        .iter()
        .position(|target| target.focus == Some(FocusId::Content(from)))?;
    let bands = bands(&targets);
    let (band_index, position) = bands.iter().enumerate().find_map(|(band_index, band)| {
        band.iter()
            .position(|&index| index == current)
            .map(|position| (band_index, position))
    })?;

    let next = match direction {
        FocusDir::Up => nearest_in_band(&targets, &bands, band_index.checked_sub(1)?, current),
        FocusDir::Down => nearest_in_band(&targets, &bands, band_index + 1, current),
        FocusDir::Left => {
            let previous = position.checked_sub(1)?;
            bands[band_index].get(previous).copied()
        }
        FocusDir::Right => bands[band_index].get(position + 1).copied(),
    }?;
    content_id(&targets[next])
}

/// 导航栏一次方向移动（按当前主轴）；None 表示到边或方向不属于主轴。
pub fn nav_move(app: &App, area: Rect, from: NavId, direction: FocusDir) -> Option<NavId> {
    let targets = nav_targets(app, area);
    let current = targets
        .iter()
        .position(|target| target.focus == Some(FocusId::Nav(from)))?;
    let step = match (nav_orientation(area), direction) {
        (NavOrientation::Vertical, FocusDir::Up) => -1i32,
        (NavOrientation::Vertical, FocusDir::Down) => 1,
        (NavOrientation::Horizontal, FocusDir::Left) => -1,
        (NavOrientation::Horizontal, FocusDir::Right) => 1,
        _ => return None,
    };
    let next = current as i32 + step;
    if next < 0 {
        return None;
    }
    nav_id(targets.get(next as usize)?)
}

/// 焦点高亮：整块用 accent 铺满。
pub fn paint(frame: &mut Frame, rect: Rect, theme: &Theme) {
    frame.buffer_mut().set_style(
        rect,
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    );
}

fn content_id(target: &Target) -> Option<ContentId> {
    match target.focus? {
        FocusId::Content(id) => Some(id),
        _ => None,
    }
}

fn nav_id(target: &Target) -> Option<NavId> {
    match target.focus? {
        FocusId::Nav(id) => Some(id),
        _ => None,
    }
}

/// 把目标按视觉行分带：按 y 排序，纵向有重叠的算同一带；带内按 x 排序。
fn bands(targets: &[Target]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..targets.len()).collect();
    order.sort_by_key(|&index| (targets[index].rect.y, targets[index].rect.x));
    let mut bands: Vec<Vec<usize>> = Vec::new();
    let mut current_bottom = 0u16;
    for index in order {
        let rect = targets[index].rect;
        if bands.is_empty() || rect.y >= current_bottom {
            bands.push(vec![index]);
            current_bottom = rect.bottom();
        } else {
            bands
                .last_mut()
                .expect("bands 非空时才有 last_mut")
                .push(index);
            current_bottom = current_bottom.max(rect.bottom());
        }
    }
    for band in &mut bands {
        band.sort_by_key(|&index| targets[index].rect.x);
    }
    bands
}

/// 目标带内水平中心最近的元素；没有该带时返回 None。
fn nearest_in_band(
    targets: &[Target],
    bands: &[Vec<usize>],
    band_index: usize,
    current: usize,
) -> Option<usize> {
    let band = bands.get(band_index)?;
    let center = center_x(targets[current].rect);
    band.iter()
        .copied()
        .min_by_key(|&index| center_x(targets[index].rect).abs_diff(center))
}

fn center_x(rect: Rect) -> u16 {
    rect.x.saturating_add(rect.width / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(y: u16, x: u16, height: u16, width: u16) -> Target {
        Target::new(Rect::new(x, y, width, height), Action::Back)
    }

    #[test]
    fn bands_group_visual_rows_and_sort_by_x() {
        let targets = vec![
            target(5, 90, 1, 3),
            target(5, 100, 1, 3),
            target(4, 105, 1, 11),
            target(12, 30, 1, 22),
            target(12, 53, 1, 13),
        ];
        let bands = bands(&targets);
        assert_eq!(bands, vec![vec![2], vec![0, 1], vec![3, 4]]);
    }

    #[test]
    fn bands_keep_a_tall_card_in_its_own_band() {
        let targets = vec![
            target(23, 112, 1, 6),
            target(24, 100, 5, 17),
            target(31, 112, 1, 6),
        ];
        let bands = bands(&targets);
        assert_eq!(bands, vec![vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn nearest_in_band_prefers_horizontal_center() {
        let targets = vec![
            target(5, 90, 1, 3),
            target(5, 100, 1, 3),
            target(12, 30, 1, 22),
            target(12, 53, 1, 13),
        ];
        let bands = bands(&targets);
        assert_eq!(nearest_in_band(&targets, &bands, 1, 1), Some(3));
        assert_eq!(nearest_in_band(&targets, &bands, 0, 2), Some(0));
    }
}
