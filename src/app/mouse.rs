//! 鼠标输入适配：把点击/滚轮转换成 Action，不直接操作界面坐标。

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::{App, Screen};
use crate::ui::focus::FocusId;

impl App {
    pub fn on_mouse(&mut self, event: MouseEvent, area: Rect) {
        if area.width < crate::ui::MIN_WIDTH || area.height < crate::ui::MIN_HEIGHT {
            return;
        }
        self.last_activity = crate::domain::timer::now_ms();
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.sync_focus_from_pointer(area, event.column, event.row);
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

    /// 主页鼠标点击与键盘焦点共用同一个「当前对象」。
    ///
    /// 点击可聚焦元素 → 焦点移到它（随后按 Enter 会重复该动作）；点空白 →
    /// 清空焦点。否则会出现「高亮停在 A，用户刚点了 B，再按 Enter 又触发 A」。
    fn sync_focus_from_pointer(&mut self, area: Rect, column: u16, row: u16) {
        if self.screen != Screen::Home || crate::ui::hit::modal_open(self) {
            return;
        }
        let hit = crate::ui::focus::nav_targets(self, area)
            .into_iter()
            .chain(crate::ui::focus::content_targets(self, area))
            .find(|target| target.contains(column, row));
        match hit.and_then(|target| target.focus) {
            Some(FocusId::Content(id)) => {
                self.focus = Some(FocusId::Content(id));
                self.last_content_focus = Some(id);
            }
            Some(id) => self.focus = Some(id),
            None => self.clear_focus(),
        }
    }
}
