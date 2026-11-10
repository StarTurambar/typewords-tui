//! 主页键盘焦点：导航区 / 内容区两栏，方向键在栏内逐项移动。
//!
//! 应用层只保存「当前栏 + 焦点下标」，栏内顺序与几何全部交给 `ui::focus`。
//! 导航栏目标来自 `shell::layout` 的 `nav_targets`（与鼠标命中同源），内容区
//! 来自 `home::targets`——所以键盘选中的一定是画出来、点得到的那一块。

use super::{Action, App, Screen};
use crate::ui::focus::{self, Pane};

/// 方向键方向：`↑`/`←` 上一项，`↓`/`→` 下一项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDir {
    Up,
    Down,
    Left,
    Right,
}

impl App {
    /// 方向键：栏内逐项移动焦点，`←`/`→` 在导航栏与内容区之间切换。
    ///
    /// 还没有焦点时：`↓`/`→` 选中内容区第一项（开始 / 继续学习），
    /// `↑`/`←` 进入导航栏并选中当前页面对应的入口——两个方向都有确定落点，
    /// 不会出现「按了没反应」。
    pub(crate) fn move_focus(&mut self, direction: FocusDir) {
        if self.screen != Screen::Home {
            return;
        }
        let in_nav = matches!(self.focus_pane, Some(Pane::Nav));
        let content = focus::content_targets(self, self.viewport);
        let nav = focus::nav_targets(self, self.viewport);

        // 跨栏：← 从内容区进导航栏（选中当前页对应的入口），
        // → 从导航栏回内容区（重新进入选择状态）。
        if !in_nav && matches!(direction, FocusDir::Left) {
            self.focus_pane = Some(Pane::Nav);
            self.focus = focus::nav_current_index(self, self.viewport);
            return;
        }
        if in_nav && matches!(direction, FocusDir::Right) {
            self.focus_pane = Some(Pane::Content);
            self.focus = None;
            return;
        }

        let targets = if in_nav { nav } else { content };
        if targets.is_empty() {
            self.focus = None;
            return;
        }
        let step = match direction {
            FocusDir::Down | FocusDir::Right => 1,
            FocusDir::Up | FocusDir::Left => -1,
        };
        self.focus = focus::step(&targets, self.focus, step);
        self.focus_pane = Some(if in_nav { Pane::Nav } else { Pane::Content });
    }

    /// Enter：触发焦点元素；没有焦点时保持原有语义（开始练习）。
    pub(crate) fn activate_focus(&mut self) {
        if self.screen != Screen::Home {
            return;
        }
        let Some(index) = self.focus else {
            self.apply_action(Action::StartPractice);
            return;
        };
        let targets = focus::targets(self, self.viewport);
        let Some(action) = targets.get(index).map(|target| target.action.clone()) else {
            // 布局变了（例如终端尺寸变化后目标变少）：清掉失效下标并回退。
            self.focus = None;
            self.apply_action(Action::StartPractice);
            return;
        };
        self.apply_action(action);
    }

    /// Esc：取消选中，回到「无焦点」状态。
    pub(crate) fn clear_focus(&mut self) {
        self.focus = None;
        self.focus_pane = None;
    }
}
