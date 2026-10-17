//! 可点击目标：布局产出的 Rect + 对应 Action。

use ratatui::layout::Rect;

use crate::app::Action;

#[derive(Clone, Debug)]
pub struct Target {
    pub rect: Rect,
    pub action: Action,
}

impl Target {
    pub fn new(rect: Rect, action: Action) -> Self {
        Self { rect, action }
    }

    pub fn contains(&self, column: u16, row: u16) -> bool {
        column >= self.rect.x
            && column < self.rect.right()
            && row >= self.rect.y
            && row < self.rect.bottom()
    }
}
