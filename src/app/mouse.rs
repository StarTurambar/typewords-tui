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

    /// 鼠标点击与键盘焦点共用同一个「当前对象」。
    ///
    /// 主页：点击可聚焦元素 → 焦点移到它（随后按 Enter 会重复该动作）；点空白 →
    /// 清空焦点。其它页面：点导航栏 → 导航栏焦点；点内容区 → 页面内容区焦点
    /// （页面自己的行高亮 / 光标就是选中效果）；点别处保持原样。
    fn sync_focus_from_pointer(&mut self, area: Rect, column: u16, row: u16) {
        if crate::ui::hit::modal_open(self) || !crate::ui::focus::enabled(self) {
            return;
        }
        if self.screen == Screen::Home {
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
            return;
        }
        let nav_hit = crate::ui::focus::nav_targets(self, area)
            .into_iter()
            .find(|target| target.contains(column, row));
        if let Some(id) = nav_hit.and_then(|target| target.focus) {
            self.focus = Some(id);
            return;
        }
        let shell = crate::ui::shell::layout(area, crate::ui::shell::Nav::from_screen(self.screen));
        let content = shell.content;
        let inside = column >= content.x
            && column < content.right()
            && row >= content.y
            && row < content.bottom();
        if inside {
            self.focus = Some(FocusId::Page);
        }
    }
}
