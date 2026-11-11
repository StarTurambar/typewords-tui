//! 键盘焦点：状态迁移、跨栏规则与 Enter 触发。
//!
//! 焦点身份（FocusId）由 ui::focus 定义，几何与带模型也在那儿；应用层保存
//! 「当前焦点 + 主页内容区上次选择」，并在每次操作前按当前 viewport 解析身份。
//! 身份失效时绝不回退到 StartPractice——那会让「看不见的高亮」按 Enter 就开练。
//!
//! 所有套外壳的页面共用同一套两栏规则：导航栏 ↔ 内容区；内容区在宽屏左边缘
//! 按 ← / 窄屏最顶部按 ↑ 跨栏，Tab 随时换栏。非主页的内容区焦点是页面
//! 自己的光标（FocusId::Page），移动与激活都交给页面自己的逻辑。

use super::{Action, App, Screen};
use crate::domain::flow::navigator::NoticeLevel;
use crate::ui::focus::{self, ContentId, FocusId, NavId, NavOrientation};
use crate::ui::shell::Nav;

/// 方向键方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDir {
    Up,
    Down,
    Left,
    Right,
}

impl App {
    /// 切屏：清空旧焦点并按目标页面设置默认选中。
    ///
    /// 所有直接改 `screen` 的路径都要走它，否则会留下上一个页面的焦点身份
    /// （例如从文章页回主页后焦点还停在 FocusId::Page，方向键无处可去）。
    pub(crate) fn set_screen(&mut self, screen: Screen) {
        if self.screen == screen {
            return;
        }
        self.screen = screen;
        self.clear_focus();
        self.focus_default();
    }

    /// 当前页面的默认选中。
    ///
    /// 主页选中主按钮（开始 / 继续学习）；其它套外壳页面选中页面自己的
    /// 内容区（词库光标、设置行、日历日期……），这样进来就能直接用方向键
    /// 操作页面内容，再按 ← / Tab 才进导航栏。练习页 / 文章跟打页 / 单词
    /// 测试页没有可导航的内容区，保持无焦点。
    pub(crate) fn focus_default(&mut self) {
        self.focus = None;
        self.last_content_focus = None;
        if self.session.is_some() {
            return;
        }
        if self.screen == Screen::Home {
            self.focus_home_default();
            return;
        }
        if focus::supports_focus(self.screen) {
            self.focus = Some(FocusId::Page);
        }
    }

    /// 方向键：按栏的主轴移动；到边时跨栏（宽屏左右跨、窄屏上下跨）。
    pub(crate) fn move_focus(&mut self, direction: FocusDir) {
        if !focus::enabled(self) {
            return;
        }
        let area = self.viewport;
        // 布局变化让当前身份失效时，按「没有焦点」重新起步，而不是继续用旧身份。
        // Page 由页面自己的光标表示，不依赖几何，永远有效。
        if matches!(
            self.focus,
            Some(FocusId::Nav(_)) | Some(FocusId::Content(_))
        ) && focus::focused_target(self, area).is_none()
        {
            self.focus = None;
        }
        match self.focus {
            None => self.focus_from_idle(direction, area),
            Some(FocusId::Nav(id)) => self.move_nav_focus(id, direction, area),
            Some(FocusId::Content(id)) => {
                if self.screen == Screen::Home {
                    self.move_content_focus(id, direction, area);
                } else {
                    self.focus = Some(FocusId::Page);
                    self.move_page_focus(direction, area);
                }
            }
            Some(FocusId::Page) => self.move_page_focus(direction, area),
        }
    }

    /// 无焦点时：上/左进导航栏，下/右落内容区。
    fn focus_from_idle(&mut self, direction: FocusDir, area: ratatui::layout::Rect) {
        match direction {
            FocusDir::Up | FocusDir::Left => self.enter_nav(area),
            FocusDir::Down | FocusDir::Right => self.enter_content(area),
        }
    }

    fn move_content_focus(
        &mut self,
        id: ContentId,
        direction: FocusDir,
        area: ratatui::layout::Rect,
    ) {
        if let Some(next) = focus::content_move(self, area, id, direction) {
            self.set_content_focus(next);
            return;
        }
        // 内容区到边才跨栏：宽屏用左，窄屏用上。右/下在内容区永远不跨栏。
        let narrow = focus::nav_orientation(area) == NavOrientation::Horizontal;
        let crosses = match direction {
            FocusDir::Left => !narrow,
            FocusDir::Up => narrow,
            _ => false,
        };
        if crosses {
            self.enter_nav(area);
        }
    }

    /// 非主页内容区一次方向移动：先让页面自己的光标动；动不了再按几何跨栏。
    fn move_page_focus(&mut self, direction: FocusDir, area: ratatui::layout::Rect) {
        if self.move_selection_page(direction) {
            return;
        }
        let narrow = focus::nav_orientation(area) == NavOrientation::Horizontal;
        let crosses = match direction {
            FocusDir::Left => !narrow,
            FocusDir::Up => narrow,
            _ => false,
        };
        if crosses {
            self.enter_nav(area);
        }
    }

    fn move_nav_focus(&mut self, id: NavId, direction: FocusDir, area: ratatui::layout::Rect) {
        if let Some(next) = focus::nav_move(self, area, id, direction) {
            self.focus = Some(FocusId::Nav(next));
            return;
        }
        // 导航栏到边才回内容区：宽屏用右，窄屏用下。
        let narrow = focus::nav_orientation(area) == NavOrientation::Horizontal;
        let crosses = match direction {
            FocusDir::Right => !narrow,
            FocusDir::Down => narrow,
            _ => false,
        };
        if crosses {
            self.enter_content(area);
        }
    }

    fn enter_nav(&mut self, area: ratatui::layout::Rect) {
        self.focus = focus::nav_entry(self, area).map(FocusId::Nav);
    }

    fn enter_content(&mut self, area: ratatui::layout::Rect) {
        if self.screen != Screen::Home {
            if focus::supports_focus(self.screen) {
                self.focus = Some(FocusId::Page);
            }
            return;
        }
        let remembered = self
            .last_content_focus
            .filter(|id| focus::content_target(self, area, *id).is_some());
        match remembered.or_else(|| focus::content_default(self, area)) {
            Some(id) => self.set_content_focus(id),
            None => self.focus = None,
        }
    }

    /// 进入/移动主页内容区焦点：同时记住它，供跨栏回来时恢复。
    fn set_content_focus(&mut self, id: ContentId) {
        self.focus = Some(FocusId::Content(id));
        self.last_content_focus = Some(id);
    }

    /// Tab / Shift+Tab：在导航栏与内容区之间换栏（所有套外壳页面）。
    pub(crate) fn switch_focus_pane(&mut self) {
        if !focus::enabled(self) {
            return;
        }
        let area = self.viewport;
        match self.focus {
            Some(FocusId::Nav(_)) => self.enter_content(area),
            _ => self.enter_nav(area),
        }
    }

    /// Enter：触发当前焦点；没有焦点时保持原有语义（主页开始 / 继续学习）。
    ///
    /// 焦点身份失效（如窗口缩放后该按钮消失）时只清焦点并提示，绝不回退成
    /// 开始练习——否则用户会在「看不到任何选中」的状态下被直接丢进练习页。
    pub(crate) fn activate_focus(&mut self) {
        if !focus::enabled(self) {
            return;
        }
        match self.focus {
            Some(FocusId::Nav(_)) => self.activate_nav_focus(),
            Some(FocusId::Content(_)) if self.screen == Screen::Home => {
                self.activate_home_content()
            }
            Some(FocusId::Content(_)) => {
                // 身份残留（正常流程不会出现）：退化成页面内容区。
                self.focus = Some(FocusId::Page);
                self.activate_page_selection();
            }
            Some(FocusId::Page) => self.activate_page_selection(),
            None => {
                if self.screen == Screen::Home {
                    self.apply_action(Action::StartPractice);
                    return;
                }
                self.enter_content(self.viewport);
                if matches!(self.focus, Some(FocusId::Page)) {
                    self.activate_page_selection();
                }
            }
        }
    }

    /// 导航栏 Enter：触发选中项；落在当前页入口上只给提示，避免「按了没反应」。
    fn activate_nav_focus(&mut self) {
        let area = self.viewport;
        let Some(target) = focus::focused_target(self, area) else {
            self.clear_focus();
            self.toast(NoticeLevel::Info, "选中项已随布局变化失效，请重新选择");
            return;
        };
        if let Action::Navigate(screen) = &target.action
            && *screen == self.screen
        {
            self.toast(
                NoticeLevel::Info,
                format!("已在「{}」页", Nav::from_screen(*screen).label()),
            );
            return;
        }
        self.apply_action(target.action);
    }

    /// 主页内容区 Enter：执行焦点元素对应的鼠标动作。
    fn activate_home_content(&mut self) {
        let area = self.viewport;
        let Some(target) = focus::focused_target(self, area) else {
            self.clear_focus();
            self.toast(NoticeLevel::Info, "选中项已随布局变化失效，请重新选择");
            return;
        };
        self.apply_action(target.action);
    }

    /// 进入主页时的默认选中项：内容区主按钮（开始 / 继续学习）。
    ///
    /// 不带方向地「打开界面」不能没有选中项：没有高亮时用户既看不出方向键
    /// 会作用在哪，也不知道上一次按到哪里。启动、从其它页面回到主页、结算
    /// 返回主页都先把主按钮选上，底栏随之显示「选中 X」，方向键从它开始
    /// 移动。`Esc` 与点击空白仍会清空选中，回到「无焦点」状态。
    ///
    /// 只写身份、不读布局：主按钮（`ContentId::Start`）在主页始终由
    /// `ui::home::targets` 无条件注册，因此启动首帧之前（`viewport` 还是
    /// 默认 80 × 24）也能安全写入；真正的矩形每次绘制 / 按键时按当前
    /// viewport 重新解析。
    pub(crate) fn focus_home_default(&mut self) {
        if self.screen != Screen::Home {
            return;
        }
        self.set_content_focus(ContentId::Start);
    }

    /// Esc：取消选中，回到「无焦点」状态（仅主页有这一层语义）。
    pub(crate) fn clear_focus(&mut self) {
        self.focus = None;
        self.last_content_focus = None;
    }
}
