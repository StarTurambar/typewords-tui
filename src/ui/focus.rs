//! 主页键盘焦点：导航区 / 内容区两栏，方向键在栏内逐项移动。
//!
//! 主页有两个「栏」：宽屏左侧是导航栏（单词 / 文章 / 词库 / 统计 / 帮助 /
//! 设置），右侧是页面内容（开始学习、每日目标、词库入口…）。窄屏没有侧栏，
//! 顶栏的标签承担导航栏的角色。
//!
//! `←` 从内容区跳到导航栏（光标落在当前页面对应的入口上），`→` 从导航栏
//! 回到内容区；`↑`/`↓` 只在当前栏内逐项移动、到头回绕。这样只靠四个方向键
//! 就能在页面之间切换，不必记 `1`~`6` 数字键。
//!
//! 栏内目标与鼠标命中同源（导航用 `shell::layout` 的 `nav_targets`，内容用
//! `home::targets`），所以键盘选中的一定是画出来、点得到的那一块。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::app::{Action, App, Screen};
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
use crate::ui::theme::Theme;

/// 焦点所在的栏。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    /// 左侧导航（宽屏侧栏 / 窄屏顶栏标签）。
    Nav,
    /// 右侧页面内容。
    Content,
}

/// 当前焦点所在的栏；不在主页时返回 `None`。
///
/// 默认在内容区：进主页时用户关心的是「开始学习」这类操作；`←` 随时可以
/// 去导航栏，`→` 再回来。
pub fn pane(app: &App) -> Option<Pane> {
    (app.screen == Screen::Home).then_some(match app.focus_pane {
        Some(Pane::Nav) => Pane::Nav,
        _ => Pane::Content,
    })
}

/// 导航栏的目标（宽屏侧栏 / 窄屏顶栏）。
///
/// 当前页面对应的入口排在最前，这样没有焦点时按 `↓` 就会落在它上面。
pub fn nav_targets(app: &App, area: Rect) -> Vec<Target> {
    let shell = shell::layout(area, Nav::Home);
    let mut targets: Vec<Target> = shell
        .nav_targets
        .iter()
        .map(|(screen, rect)| {
            let mut target = Target::new(*rect, Action::Navigate(*screen));
            target.label = Some(Nav::from_screen(*screen).label());
            target
        })
        .collect();
    let current = targets
        .iter()
        .position(|target| match target.action {
            Action::Navigate(screen) => Nav::from_screen(screen) == Nav::from_screen(app.screen),
            _ => false,
        })
        .unwrap_or(0);
    if current > 0 {
        let item = targets.remove(current);
        targets.insert(0, item);
    }
    targets
}

/// 导航栏里「当前页面对应入口」的下标（进导航栏时的落点）。
pub fn nav_current_index(app: &App, area: Rect) -> Option<usize> {
    nav_targets(app, area)
        .iter()
        .position(|target| match target.action {
            Action::Navigate(screen) => Nav::from_screen(screen) == Nav::from_screen(app.screen),
            _ => false,
        })
        .or(Some(0))
}

/// 内容区的目标（开始学习 / 自由练习 / 每日目标 / 词库入口 / 练习模式）。
pub fn content_targets(app: &App, area: Rect) -> Vec<Target> {
    if super::hit::modal_open(app) {
        return Vec::new();
    }
    let shell = shell::layout(area, Nav::Home);
    let mut targets = super::home::targets(app, shell.content);
    targets.sort_by_key(|target| tour_rank(&target.action));
    targets
}

/// 当前栏的全部目标。
pub fn targets(app: &App, area: Rect) -> Vec<Target> {
    match pane(app) {
        Some(Pane::Nav) => nav_targets(app, area),
        _ => content_targets(app, area),
    }
}

/// 内容区巡览顺序（越小越靠前）。
///
/// 「开始 / 继续学习」必须是第一项：首次按方向键选中它，再按 Enter 就是
/// 开始练习——即使误按 Enter 也不会改到任何设置。
fn tour_rank(action: &Action) -> u8 {
    match action {
        Action::StartPractice => 0,
        Action::StartFreePractice => 1,
        Action::Navigate(Screen::DictPicker) => 2,
        Action::AdjustDaily(delta) if *delta < 0 => 3,
        Action::AdjustDaily(_) => 4,
        Action::OpenModePicker => 5,
        // 以后主页新增可点元素时排在末尾，不打乱已有顺序。
        _ => 9,
    }
}

/// 当前焦点对应的矩形。
pub fn focused_rect(app: &App, area: Rect) -> Option<Rect> {
    let index = app.focus?;
    targets(app, area).get(index).map(|target| target.rect)
}

/// 当前焦点的提示文案（底栏显示「选中 X」）。
pub fn focused_label(app: &App, area: Rect) -> Option<&'static str> {
    let index = app.focus?;
    let targets = targets(app, area);
    let target = targets.get(index)?;
    Some(match target.action {
        // 有断点时按钮文案是「继续学习」，底栏跟着用同一个词。
        Action::StartPractice if app.has_saved_practice() => "继续学习",
        _ => target.label?,
    })
}

/// 栏内移动一步：`step` 为 `+1`（下一项）/ `-1`（上一项），到头回绕。
///
/// 还没有焦点时：向下的方向从第一项开始、向上的方向从最后一项开始——
/// 与菜单里「按 ↓ 选第一项、按 ↑ 选最后一项」的直觉一致。
pub fn step(targets: &[Target], from: Option<usize>, step: i32) -> Option<usize> {
    let len = targets.len();
    if len == 0 {
        return None;
    }
    Some(match from {
        Some(index) if index < len => (index as i32 + step).rem_euclid(len as i32) as usize,
        _ if step > 0 => 0,
        _ => len - 1,
    })
}

/// 焦点高亮：整块用 `accent` 铺满。
///
/// 主按钮常态就是 `accent` 底、自由练习按钮是 `element` 底；焦点时两者都变成
/// 整块 `accent` + `bg` 文字 + 加粗——「整块紫 = 选中」。不再画左侧竖条：
/// 那会盖住主按钮自己的左边距，看起来像按钮样式被改坏了。
pub fn paint(frame: &mut Frame, rect: Rect, theme: &Theme) {
    frame.buffer_mut().set_style(
        rect,
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;

    fn targets_for(actions: &[Action]) -> Vec<Target> {
        actions
            .iter()
            .enumerate()
            .map(|(index, action)| Target::new(Rect::new(0, index as u16, 4, 1), action.clone()))
            .collect()
    }

    #[test]
    fn step_moves_one_item_and_wraps_at_both_ends() {
        let targets = targets_for(&[
            Action::StartPractice,
            Action::StartFreePractice,
            Action::OpenModePicker,
        ]);
        assert_eq!(step(&targets, None, 1), Some(0));
        assert_eq!(step(&targets, None, -1), Some(2));
        assert_eq!(step(&targets, Some(0), 1), Some(1));
        assert_eq!(step(&targets, Some(2), 1), Some(0));
        assert_eq!(step(&targets, Some(0), -1), Some(2));
        assert_eq!(step(&targets, Some(0), -3), Some(0));
    }

    #[test]
    fn step_treats_a_stale_index_like_no_focus() {
        let targets = targets_for(&[Action::StartPractice, Action::StartFreePractice]);
        assert_eq!(step(&targets, Some(9), 1), Some(0));
        assert_eq!(step(&targets, Some(9), -1), Some(1));
        assert_eq!(step(&[], Some(0), 1), None);
    }

    #[test]
    fn tour_rank_starts_at_the_primary_action_and_ends_with_unknown_actions() {
        let start = tour_rank(&Action::StartPractice);
        let free = tour_rank(&Action::StartFreePractice);
        let dict = tour_rank(&Action::Navigate(Screen::DictPicker));
        let minus = tour_rank(&Action::AdjustDaily(-5));
        let plus = tour_rank(&Action::AdjustDaily(5));
        let mode = tour_rank(&Action::OpenModePicker);
        assert!(start < free && free < dict && dict < minus && minus < plus && plus < mode);
        assert!(mode < tour_rank(&Action::QuitRequest));
    }
}
