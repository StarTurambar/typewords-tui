//! 可点击目标：布局产出的 Rect + 对应 Action。

use ratatui::layout::Rect;

use crate::app::Action;

#[derive(Clone, Debug)]
pub struct Target {
    pub rect: Rect,
    pub action: Action,
    /// 给键盘焦点的提示文案（底栏「选中 X」）；鼠标路径不需要。
    pub label: Option<&'static str>,
}

impl Target {
    pub fn new(rect: Rect, action: Action) -> Self {
        Self {
            rect,
            action,
            label: None,
        }
    }

    /// 带键盘焦点提示的 target。
    pub fn labeled(rect: Rect, action: Action, label: &'static str) -> Self {
        Self {
            rect,
            action,
            label: Some(label),
        }
    }

    pub fn contains(&self, column: u16, row: u16) -> bool {
        column >= self.rect.x
            && column < self.rect.right()
            && row >= self.rect.y
            && row < self.rect.bottom()
    }
}
