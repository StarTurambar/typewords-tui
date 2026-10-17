//! 主题到 ratatui 颜色的映射。配置层在 `crate::theme`。

use ratatui::style::Color;

use crate::theme::{AnsiColor, ThemeColor, ThemeSpec};

/// 渲染层使用的主题：所有颜色已转换成 ratatui Color。
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg: Color,
    pub panel: Color,
    pub element: Color,
    pub border: Color,
    pub fg: Color,
    pub muted: Color,
    pub dim: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub warn: Color,
    pub good: Color,
    pub bad: Color,
    pub selection: Color,
}

impl Theme {
    pub fn from_spec(spec: &ThemeSpec) -> Self {
        Self {
            bg: map(spec.colors.bg),
            panel: map(spec.colors.panel),
            element: map(spec.colors.element),
            border: map(spec.colors.border),
            fg: map(spec.colors.fg),
            muted: map(spec.colors.muted),
            dim: map(spec.colors.dim),
            accent: map(spec.colors.accent),
            accent_alt: map(spec.colors.accent_alt),
            warn: map(spec.colors.warn),
            good: map(spec.colors.good),
            bad: map(spec.colors.bad),
            selection: map(spec.colors.selection),
        }
    }
}

fn map(value: ThemeColor) -> Color {
    match value {
        ThemeColor::Rgb(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        ThemeColor::Ansi(color) => match color {
            AnsiColor::Reset => Color::Reset,
            AnsiColor::Black => Color::Black,
            AnsiColor::DarkGray => Color::DarkGray,
            AnsiColor::Red => Color::Red,
            AnsiColor::LightRed => Color::LightRed,
            AnsiColor::Green => Color::Green,
            AnsiColor::LightGreen => Color::LightGreen,
            AnsiColor::Yellow => Color::Yellow,
            AnsiColor::LightYellow => Color::LightYellow,
            AnsiColor::Blue => Color::Blue,
            AnsiColor::LightBlue => Color::LightBlue,
            AnsiColor::Magenta => Color::Magenta,
            AnsiColor::LightMagenta => Color::LightMagenta,
            AnsiColor::Cyan => Color::Cyan,
            AnsiColor::LightCyan => Color::LightCyan,
            AnsiColor::Gray => Color::Gray,
            AnsiColor::White => Color::White,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Rgb, default_dark};

    #[test]
    fn maps_rgb_values() {
        let theme = Theme::from_spec(&default_dark());
        assert_eq!(theme.bg, Color::Rgb(0x17, 0x17, 0x20));
        assert_eq!(theme.accent, Color::Rgb(0xc9, 0x8e, 0xc7));
    }

    #[test]
    fn maps_ansi_values() {
        assert_eq!(map(ThemeColor::Rgb(Rgb::new(1, 2, 3))), Color::Rgb(1, 2, 3));
        assert_eq!(
            map(ThemeColor::Ansi(AnsiColor::LightCyan)),
            Color::LightCyan
        );
    }
}
