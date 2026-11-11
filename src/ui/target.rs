//! 可点击目标：布局产出的 Rect + 对应 Action。

use ratatui::layout::Rect;

use crate::app::Action;

/// 主页内容区的稳定焦点身份。
///
/// 键盘焦点不能只存「巡览列表下标」：同一块终端在不同尺寸、不同练习模式名、
/// 不同词库数量下会多出/少掉可点元素，下标会指到另一个按钮。身份与布局解耦后，
/// 每次按键 / Enter / 绘制前按当前布局重新解析，失效就干净地清空。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentId {
    Start,
    Free,
    DictPage,
    ManageDict,
    MoreDicts,
    DailyMinus,
    DailyPlus,
    Mode,
}

/// 主页导航栏的稳定焦点身份（宽屏侧栏 / 窄屏顶栏共用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavId {
    Word,
    Articles,
    Dicts,
    Stats,
    Help,
    Settings,
    /// 宽屏侧栏底部的「退出」；窄屏顶栏没有这一项。
    Quit,
}

/// 焦点身份：导航栏或内容区的某个元素。
///
/// - `Nav`：所有套外壳页面共用的导航栏条目（侧栏 / 顶栏 / 退出）。
/// - `Content`：主页内容区的 8 个固定元素。
/// - `Page`：其它页面的内容区选中项。页面自己的光标（词库行、设置行、
///   日历日期、滚动位置……）就是它的身份，由页面状态保存，不需要再存下标；
///   方向键移动的是这个光标，跨栏回来时页面状态原样保留。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusId {
    Nav(NavId),
    Content(ContentId),
    Page,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub rect: Rect,
    pub action: Action,
    /// 给键盘焦点的提示文案（底栏「选中 X」）；鼠标路径不需要。
    pub label: Option<&'static str>,
    /// 键盘焦点身份；只有主页的可聚焦元素才带。
    pub focus: Option<FocusId>,
}

impl Target {
    pub fn new(rect: Rect, action: Action) -> Self {
        Self {
            rect,
            action,
            label: None,
            focus: None,
        }
    }

    /// 带键盘焦点提示的 target（鼠标 / 非主页页面使用）。
    pub fn labeled(rect: Rect, action: Action, label: &'static str) -> Self {
        Self {
            rect,
            action,
            label: Some(label),
            focus: None,
        }
    }

    /// 主页可聚焦 target：带中文提示与稳定身份。
    pub fn focusable(rect: Rect, action: Action, label: &'static str, focus: FocusId) -> Self {
        Self {
            rect,
            action,
            label: Some(label),
            focus: Some(focus),
        }
    }

    pub fn contains(&self, column: u16, row: u16) -> bool {
        column >= self.rect.x
            && column < self.rect.right()
            && row >= self.rect.y
            && row < self.rect.bottom()
    }
}
