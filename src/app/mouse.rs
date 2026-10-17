//! 鼠标输入适配：把点击/滚轮转换成 Action，不直接操作界面坐标。

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;

impl App {
    pub fn on_mouse(&mut self, event: MouseEvent, area: Rect) {
        self.last_activity = crate::domain::timer::now_ms();
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = crate::ui::hit::action_at(self, area, event.column, event.row)
                {
                    self.apply_action(action);
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(action) =
                    crate::ui::hit::wheel_action(self, area, event.column, event.row, -1)
                {
                    self.apply_action(action);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(action) =
                    crate::ui::hit::wheel_action(self, area, event.column, event.row, 1)
                {
                    self.apply_action(action);
                }
            }
            _ => {}
        }
    }
}
