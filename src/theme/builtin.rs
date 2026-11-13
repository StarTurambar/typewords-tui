//! 内置主题。深色取自 opencode 默认主题（中性近黑面 + 暖橙主色），
//! 浅色取自归藏「靛蓝瓷」，预设另有归藏「午夜墨」「纸墨」与瑞士风「克莱因蓝」；
//! 高对比保留纯 ANSI 方案，供不支持真彩色的终端使用。
//!
//! 色板与来源见 docs/themes.md；顺序即设置页的循环顺序：dark、light、
//! high-contrast 之后才是可选预设，因此 `base` 继承仍然取 dark / light。

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
                // 曜石 Obsidian：opencode 的中性近黑阶梯 + 暖橙主色。
                bg: rgb(0x0a0a0a),
                panel: rgb(0x141414),
                element: rgb(0x202020),
                border: rgb(0x444444),
                fg: rgb(0xeeeeee),
                muted: rgb(0x8a8a8a),
                dim: rgb(0x757575),
                accent: rgb(0xfab283),
                accent_alt: rgb(0x5c9cf5),
                warn: rgb(0xe5c07b),
                good: rgb(0x7fd88f),
                bad: rgb(0xe06c75),
                selection: rgb(0x2a2a2a),
            },
        },
        ThemeSpec {
            id: "light".to_string(),
            name: "浅色".to_string(),
            base: BaseMode::Light,
            colors: ThemeColors {
                // 靛蓝瓷 Indigo Porcelain：归藏瓷白纸 + 单一靛蓝强调。
                bg: rgb(0xf1f3f5),
                panel: rgb(0xfbfcfd),
                element: rgb(0xdde3e9),
                border: rgb(0xb8c0ca),
                fg: rgb(0x0a1f3d),
                muted: rgb(0x5f6d78),
                dim: rgb(0x6f7b88),
                accent: rgb(0x315d93),
                accent_alt: rgb(0x43546d),
                warn: rgb(0x8a6100),
                good: rgb(0x2e6b4f),
                bad: rgb(0xa63a43),
                selection: rgb(0xd2deea),
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
        ThemeSpec {
            id: "midnight".to_string(),
            name: "午夜墨".to_string(),
            base: BaseMode::Dark,
            colors: ThemeColors {
                // 归藏 midnight-ink：暖黑纸 + 暖白墨 + 古金强调。
                bg: rgb(0x0e0d0c),
                panel: rgb(0x1a1714),
                element: rgb(0x2a241b),
                border: rgb(0x3b352d),
                fg: rgb(0xece2cf),
                muted: rgb(0xa89b84),
                dim: rgb(0x7d7260),
                accent: rgb(0xd4a04a),
                accent_alt: rgb(0x9fb6c6),
                warn: rgb(0xe8874f),
                good: rgb(0x88c6a6),
                bad: rgb(0xd97a72),
                selection: rgb(0x2d271e),
            },
        },
        ThemeSpec {
            id: "paper".to_string(),
            name: "纸墨".to_string(),
            base: BaseMode::Light,
            colors: ThemeColors {
                // 归藏 ink-classic：宣纸 + 墨色强调。注意 accent 与 fg 同色系，
                // 练习页「下一个待输入字母」的区分度弱，默认浅色仍用靛蓝瓷。
                bg: rgb(0xf1efea),
                panel: rgb(0xfbf9f5),
                element: rgb(0xe1dacb),
                border: rgb(0xc9c3b5),
                fg: rgb(0x0a0a0b),
                muted: rgb(0x4a463f),
                dim: rgb(0x6d675d),
                accent: rgb(0x111111),
                accent_alt: rgb(0x315d93),
                warn: rgb(0x9b5a2e),
                good: rgb(0x2e6b4f),
                bad: rgb(0xa63a43),
                selection: rgb(0xdcd5c5),
            },
        },
        ThemeSpec {
            id: "ikb".to_string(),
            name: "克莱因蓝".to_string(),
            base: BaseMode::Light,
            colors: ThemeColors {
                // 归藏瑞士风 IKB：纸白 + 克莱因蓝 + 安全橙次强调。
                bg: rgb(0xfafaf8),
                panel: rgb(0xffffff),
                element: rgb(0xe8e8e2),
                border: rgb(0xd4d4d2),
                fg: rgb(0x0a0a0a),
                muted: rgb(0x525252),
                dim: rgb(0x737373),
                accent: rgb(0x002fa7),
                accent_alt: rgb(0xc2410c),
                warn: rgb(0xa16207),
                good: rgb(0x0f7a4a),
                bad: rgb(0xb91c1c),
                selection: rgb(0xe2e7f2),
            },
        },
    ]
}
