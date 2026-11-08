//! 主页键盘焦点：方向键在页面内的可点元素之间移动，Enter 触发。
//!
//! 应用层只保存「焦点下标」，顺序与几何全部交给 `ui::focus`——目标列表与
//! 鼠标命中同源（`ui::home::targets`），所以键盘选中的一定是画出来、
//! 点得到的那一块。

use super::{Action, App, Screen};

/// 方向键方向：`↑`/`←` 上一项、`↓`/`→` 下一项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDir {
    Up,
    Down,
    Left,
    Right,
}

/// 方向键与 Enter 的焦点语义。
///
/// 只作用于主页：其它页面已经用方向键移动自己的光标 / 滚动内容，
/// 再叠一层焦点会让同一个键有两种含义。
impl App {
    /// 方向键：按主页的巡览顺序逐项移动焦点（到头回绕）。
    ///
    /// 还没有焦点时：`↓`/`→` 选第一项（开始 / 继续学习），`↑`/`←` 选最后一项——
    /// 和菜单里「按 ↓ 选第一项、按 ↑ 选最后一项」的直觉一致，且第一项是主按钮，
    /// 误按 Enter 也只会开始练习，不会改到设置。
    pub(crate) fn move_focus(&mut self, direction: FocusDir) {
        if self.screen != Screen::Home {
            return;
        }
        let targets = crate::ui::focus::targets(self, self.viewport);
        if targets.is_empty() {
            self.focus = None;
            return;
        }
        // ↓ / → 下一项，↑ / ← 上一项；到头回绕，不会出现「按了没反应」。
        let step = match direction {
            FocusDir::Down | FocusDir::Right => 1,
            FocusDir::Up | FocusDir::Left => -1,
        };
        self.focus = crate::ui::focus::step(&targets, self.focus, step);
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
        let targets = crate::ui::focus::targets(self, self.viewport);
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
    }
}
