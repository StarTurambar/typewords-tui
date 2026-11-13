//! 键入练习页底部的动作按钮条：布局、绘制与鼠标命中共用同一份矩形。
//!
//! 两个键入页（背单词 / 文章跟打）共用这一套按钮条：两行按钮 + 一行提示，
//! 列数按按钮数量均分（见 app::buttons::columns），按钮之间留一列空隙。
//! layout 只吃 Rect 与按钮数量，绘制与 ui::hit 都读它，命中区域不会
//! 和画出来的按钮漂移。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::app::buttons::{TypingButton, columns};
use crate::ui::theme::Theme;
use crate::ui::widgets::{self, display_width, fit_text};

/// 按钮条占的行数：两行按钮 + 中间一行留白 + 一行提示。
///
/// 两行按钮之间必须留一行空白：按钮底色铺满整行，紧贴在一起会看成
/// 一个两行高的大按钮。
pub const HEIGHT: u16 = 4;

/// 按钮条布局：每个按钮一个矩形，最底下一行是提示。
pub struct ButtonBar {
    /// 每个按钮的矩形（行优先，与按钮表一一对应）
    pub buttons: Vec<Rect>,
    /// 按钮下方的提示行
    pub hint: Rect,
}

/// 单个按钮的最大宽度：宽终端上按钮拉得过长会显得空，整组居中更好看。
const MAX_CELL: u16 = 26;

/// 计算按钮条布局；area 需要 HEIGHT 行。
pub fn layout(area: Rect, count: usize) -> ButtonBar {
    let columns = columns(count);
    let gap = 1u16;
    let gaps = gap.saturating_mul(columns.saturating_sub(1) as u16);
    // 按钮等宽、之间留一列空隙；宽终端上整组居中，不让按钮铺满整行。
    let cell = (area.width.saturating_sub(gaps) / columns as u16).min(MAX_CELL);
    let total = cell.saturating_mul(columns as u16).saturating_add(gaps);
    let start = area.x + area.width.saturating_sub(total) / 2;
    let mut buttons = Vec::with_capacity(count);
    for index in 0..count {
        let row = (index / columns) as u16;
        let column = (index % columns) as u16;
        let x = start + column * (cell + gap);
        buttons.push(Rect::new(x, area.y + row * 2, cell, 1));
    }
    ButtonBar {
        buttons,
        hint: Rect::new(area.x, area.y + HEIGHT - 1, area.width, 1),
    }
}

/// 绘制按钮条与底部提示；tail 是页面自己的补充键位（如「空格 继续」）。
pub fn render(
    frame: &mut Frame,
    app: &App,
    theme: &Theme,
    bar: &ButtonBar,
    buttons: &[TypingButton],
    focused: Option<usize>,
    tail: &str,
) {
    for (index, (button, rect)) in buttons.iter().zip(&bar.buttons).enumerate() {
        let label = label(app, *button, rect.width as usize);
        if focused == Some(index) {
            // 选中：与页面焦点、弹层按钮同一套高亮（accent 底 + bg 字 + 加粗）。
            widgets::draw_button(frame, *rect, &label, theme.accent, theme.bg, true);
        } else {
            widgets::draw_button(frame, *rect, &label, theme.element, theme.fg, false);
        }
    }
    // 最小终端 60 列（边框内 58 列）也要放得下：提示只保留最关键的三个动作，
    // 页面自己的补充键位接在后面。
    let hint = format!(" 方向键选中 · Enter 执行 · 鼠标点击 · {tail}");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            fit_text(&hint, bar.hint.width as usize),
            Style::default().fg(theme.dim),
        ))),
        bar.hint,
    );
}

/// 按钮文案：放得下就把键名一起写上（改绑后跟着变），放不下退回动作名。
pub fn label(app: &App, button: TypingButton, width: usize) -> String {
    let name = button_name(app, button);
    let key = match button.shortcut() {
        Some(action) => app.shortcuts.key_for(action).map(|key| key.to_display()),
        None => button.fixed_key().map(str::to_string),
    };
    let Some(key) = key.filter(|key| !key.is_empty()) else {
        return fit_text(&name, width);
    };
    let full = format!("{key} {name}");
    if display_width(&full) <= width {
        full
    } else {
        fit_text(&name, width)
    }
}

/// 动作名；「收藏」在已收藏时带实心星标。
///
/// 与单词上方的「★ 已收藏」徽标同源（都读 `PracticeSession::is_collect`）：
/// 收藏按钮本身就是开关，状态直接标在按钮上，点一下就能看到它变。
fn button_name(app: &App, button: TypingButton) -> String {
    let collected = button == TypingButton::Collect
        && app
            .session
            .as_ref()
            .is_some_and(|session| session.is_collect(&app.boot.base));
    if collected {
        format!("★ {}", button.label())
    } else {
        button.label().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_stay_inside_the_bar_and_do_not_overlap() {
        for count in [6usize, 10] {
            for width in [60u16, 80, 120] {
                let area = Rect::new(2, 5, width, HEIGHT);
                let bar = layout(area, count);
                assert_eq!(bar.buttons.len(), count);
                for (index, rect) in bar.buttons.iter().enumerate() {
                    assert!(rect.right() <= area.right(), "第 {index} 个按钮越界");
                    assert!(rect.y < area.y + HEIGHT - 1, "按钮不该压到提示行");
                    if index % columns(count) > 0 {
                        let previous = bar.buttons[index - 1];
                        assert_eq!(previous.y, rect.y, "同一行才对");
                        assert!(previous.right() < rect.x, "相邻按钮之间要留空隙");
                    }
                }
                assert_eq!(bar.hint.y, area.y + HEIGHT - 1);
            }
        }
    }

    /// 最小终端 60 列下，提示行不能被裁掉关键字样。
    #[test]
    fn hint_fits_the_minimum_terminal() {
        let hint = "方向键选中 · Enter 执行 · 鼠标点击 · 空格 继续 · ? 帮助";
        let bar = layout(Rect::new(0, 0, 58, HEIGHT), 10);
        assert!(display_width(hint) <= bar.hint.width as usize);
        assert!(bar.hint.width >= 58);
    }
}
