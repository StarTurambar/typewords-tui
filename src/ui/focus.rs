//! 主页键盘焦点：可聚焦目标、逐项移动与高亮绘制。
//!
//! 主页布局很稀疏——两个按钮、几个小链接散落在 hero 卡与分区标题上。
//! 按「几何最近邻」移动会让焦点在大跨度元素之间乱跳（按 ↓ 从开始按钮
//! 直接蹦到右下角的管理卡），按到底还会因为该方向没有目标而卡住。这里
//! 改成固定的页面顺序：主按钮 → 自由练习 → 词库入口 → 每日目标 →
//! 练习模式，方向键逐项移动、到头回绕，每次按键都有反馈。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::app::{Action, App, Screen};
use crate::ui::shell::{self, Nav};
use crate::ui::target::Target;
use crate::ui::theme::Theme;

/// 主页可聚焦的目标（页面内元素，不含外壳侧栏 / 顶栏）。
///
/// 有弹层或不在主页时返回空列表：方向键此时属于弹层 / 其它页面。
/// 返回顺序就是方向键的巡览顺序。
pub fn targets(app: &App, area: Rect) -> Vec<Target> {
    if app.screen != Screen::Home || super::hit::modal_open(app) {
        return Vec::new();
    }
    let shell = shell::layout(area, Nav::Home);
    let mut targets = super::home::targets(app, shell.content);
    // 排序是稳定的：同一优先级（三处词库入口）保持主页 target 的注册顺序。
    targets.sort_by_key(|target| tour_rank(&target.action));
    targets
}

/// 巡览顺序（越小越靠前）。
///
/// 「开始 / 继续学习」必须是第一项：首次按方向键选中它，再按 Enter 就是
/// 开始练习——即使误按 Enter 也不会改到任何设置。之后按页面区块向下：
/// 自由练习 → 词库入口 → 每日目标 → 练习模式。
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

/// 当前焦点对应的矩形；没有焦点、页面不对或下标失效时返回 `None`。
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

/// 按巡览顺序移动一步：`step` 为 `+1`（下一项）/ `-1`（上一项），到头回绕。
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

/// 焦点高亮：选中底色 + 加粗正文 + 左侧强调条。
///
/// 与列表的选中行用同一套语言（`selection` 底 + `accent` 强调条），而不是
/// 整块反显——反显会把按钮自己的强调色换成反色，看起来像按钮被按下去。
/// 强调条优先画在控件自身第一列（按钮 / 卡片的左边距通常是空白），有字时
/// 改画在矩形外面一格；两处都有字就不画，绝不覆盖控件文字。高对比主题的
/// `selection` 与 `bg` 同为黑色，就靠这条强调条识别焦点。
pub fn paint(frame: &mut Frame, rect: Rect, theme: &Theme) {
    let buffer = frame.buffer_mut();
    buffer.set_style(
        rect,
        Style::default()
            .fg(theme.fg)
            .bg(theme.selection)
            .add_modifier(Modifier::BOLD),
    );
    // 左侧强调条优先画在控件自己的第一列（按钮 / 卡片的左边距通常是空白），
    // 那一列有字时改画在矩形外面一格；两处都压着字就不画，绝不覆盖内容。
    for column in [rect.x, rect.x.saturating_sub(1)] {
        if rect.x == 0 && column == 0 {
            continue;
        }
        let bar = Rect::new(column, rect.y, 1, rect.height);
        if bar.right() > buffer.area.right() {
            continue;
        }
        let blank = (bar.y..bar.bottom()).all(|y| {
            buffer
                .cell((bar.x, y))
                .map(|cell| cell.symbol() == " ")
                .unwrap_or(false)
        });
        if blank {
            buffer.set_style(bar, Style::default().bg(theme.accent));
            return;
        }
    }
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
        // 超过一整圈的步长也能回绕。
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
