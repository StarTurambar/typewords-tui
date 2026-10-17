//! 内置主题。颜色值来自 opencode 参考截图取样，并做可读性调整。

use super::{AnsiColor, BaseMode, Rgb, ThemeColor, ThemeColors, ThemeSpec};

pub const DEFAULT_THEME_ID: &str = "dark";

const fn rgb(value: u32) -> ThemeColor {
    ThemeColor::Rgb(Rgb::new(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ))
}

const fn ansi(value: AnsiColor) -> ThemeColor {
    ThemeColor::Ansi(value)
}

pub fn builtin_specs() -> Vec<ThemeSpec> {
    vec![
        ThemeSpec {
            id: "dark".to_string(),
            name: "深色".to_string(),
            base: BaseMode::Dark,
            colors: ThemeColors {
                bg: rgb(0x171720),
                panel: rgb(0x1e1e2b),
                element: rgb(0x262637),
                border: rgb(0x34344a),
                fg: rgb(0xd7d9e6),
                muted: rgb(0x8a8ca3),
                dim: rgb(0x6d7189),
                accent: rgb(0xc98ec7),
                accent_alt: rgb(0x7fd3d8),
                warn: rgb(0xe2b86b),
                good: rgb(0x9ece6a),
                bad: rgb(0xe06c75),
                selection: rgb(0x2a2b40),
            },
        },
        ThemeSpec {
            id: "light".to_string(),
            name: "浅色".to_string(),
            base: BaseMode::Light,
            colors: ThemeColors {
                bg: rgb(0xf7f7f9),
                panel: rgb(0xffffff),
                element: rgb(0xececf2),
                border: rgb(0xd9dae3),
                fg: rgb(0x21222b),
                muted: rgb(0x4f5262),
                dim: rgb(0x656a7a),
                accent: rgb(0x93418a),
                accent_alt: rgb(0x176f75),
                warn: rgb(0x8a6100),
                good: rgb(0x2f7d4f),
                bad: rgb(0xb23a48),
                selection: rgb(0xe9e7f1),
            },
        },
        ThemeSpec {
            id: "high-contrast".to_string(),
            name: "高对比".to_string(),
            base: BaseMode::HighContrast,
            colors: ThemeColors {
                bg: ansi(AnsiColor::Black),
                panel: ansi(AnsiColor::Black),
                // 高对比用纯黑块面，保证亮色文字在块面上仍有足够对比度；
                // 选中态靠左侧强调条 + 粗体标识。
                element: ansi(AnsiColor::Black),
                border: ansi(AnsiColor::White),
                fg: ansi(AnsiColor::White),
                muted: ansi(AnsiColor::Gray),
                dim: ansi(AnsiColor::Gray),
                accent: ansi(AnsiColor::LightMagenta),
                accent_alt: ansi(AnsiColor::LightCyan),
                warn: ansi(AnsiColor::LightYellow),
                good: ansi(AnsiColor::LightGreen),
                bad: ansi(AnsiColor::LightRed),
                selection: ansi(AnsiColor::Black),
            },
        },
    ]
}
