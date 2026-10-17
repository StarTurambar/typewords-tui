//! 主题配置层：与终端渲染无关的颜色/主题模型与用户主题加载。
//!
//! 这里只处理“主题是什么”（id、名称、base、13 个颜色 token），
//! 不依赖 ratatui；ratatui 的颜色映射在 `crate::ui::theme`。

mod builtin;
mod load;

pub use builtin::{DEFAULT_THEME_ID, builtin_specs};
pub use load::load_user_themes;

/// 8 位 RGB 颜色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// 标准 ANSI 颜色名（高对比主题与自定义主题可用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnsiColor {
    Reset,
    Black,
    DarkGray,
    Red,
    LightRed,
    Green,
    LightGreen,
    Yellow,
    LightYellow,
    Blue,
    LightBlue,
    Magenta,
    LightMagenta,
    Cyan,
    LightCyan,
    Gray,
    White,
}

/// 主题中的一个颜色值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeColor {
    Rgb(Rgb),
    Ansi(AnsiColor),
}

impl ThemeColor {
    /// 解析 `#rgb`、`#rrggbb` 或 ANSI 颜色名。
    pub fn parse(text: &str) -> Result<Self, String> {
        let value = text.trim();
        if value.is_empty() {
            return Err("颜色不能为空".to_string());
        }
        if let Some(hex) = value.strip_prefix('#') {
            return parse_hex(hex).map(Self::Rgb);
        }
        let ansi = match value.to_ascii_lowercase().replace('-', "_").as_str() {
            "reset" | "default" => AnsiColor::Reset,
            "black" => AnsiColor::Black,
            "darkgray" | "dark_gray" | "darkgrey" | "dark_grey" => AnsiColor::DarkGray,
            "red" => AnsiColor::Red,
            "lightred" | "light_red" | "brightred" | "bright_red" => AnsiColor::LightRed,
            "green" => AnsiColor::Green,
            "lightgreen" | "light_green" | "brightgreen" | "bright_green" => AnsiColor::LightGreen,
            "yellow" => AnsiColor::Yellow,
            "lightyellow" | "light_yellow" | "brightyellow" | "bright_yellow" => {
                AnsiColor::LightYellow
            }
            "blue" => AnsiColor::Blue,
            "lightblue" | "light_blue" | "brightblue" | "bright_blue" => AnsiColor::LightBlue,
            "magenta" | "purple" => AnsiColor::Magenta,
            "lightmagenta" | "light_magenta" | "brightmagenta" | "bright_magenta" => {
                AnsiColor::LightMagenta
            }
            "cyan" => AnsiColor::Cyan,
            "lightcyan" | "light_cyan" | "brightcyan" | "bright_cyan" => AnsiColor::LightCyan,
            "gray" | "grey" => AnsiColor::Gray,
            "white" => AnsiColor::White,
            other => {
                return Err(format!(
                    "未知颜色: {other}（可用 #rrggbb / #rgb 或 ANSI 颜色名）"
                ));
            }
        };
        Ok(Self::Ansi(ansi))
    }
}

fn parse_hex(hex: &str) -> Result<Rgb, String> {
    let chars: Vec<char> = hex.chars().collect();
    let parse_pair = |pair: &[char]| -> Result<u8, String> {
        let text: String = pair.iter().collect();
        u8::from_str_radix(&text, 16).map_err(|_| format!("无效的十六进制颜色: #{hex}"))
    };
    match chars.len() {
        3 => {
            let r = parse_pair(&[chars[0], chars[0]])?;
            let g = parse_pair(&[chars[1], chars[1]])?;
            let b = parse_pair(&[chars[2], chars[2]])?;
            Ok(Rgb::new(r, g, b))
        }
        6 => {
            let r = parse_pair(&chars[0..2])?;
            let g = parse_pair(&chars[2..4])?;
            let b = parse_pair(&chars[4..6])?;
            Ok(Rgb::new(r, g, b))
        }
        _ => Err(format!(
            "无效的十六进制颜色: #{hex}（需要 #rgb 或 #rrggbb）"
        )),
    }
}

/// 主题的基础模式（自定义主题用它继承一组默认颜色）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseMode {
    Dark,
    Light,
    HighContrast,
}

impl BaseMode {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "" | "dark" | "default" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            "high-contrast" | "high_contrast" | "contrast" => Ok(Self::HighContrast),
            other => Err(format!(
                "未知主题 base: {other}（可用 dark / light / high-contrast）"
            )),
        }
    }

    pub fn as_id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::HighContrast => "high-contrast",
        }
    }
}

/// 主题的 13 个颜色 token。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeColors {
    pub bg: ThemeColor,
    pub panel: ThemeColor,
    pub element: ThemeColor,
    pub border: ThemeColor,
    pub fg: ThemeColor,
    pub muted: ThemeColor,
    pub dim: ThemeColor,
    pub accent: ThemeColor,
    pub accent_alt: ThemeColor,
    pub warn: ThemeColor,
    pub good: ThemeColor,
    pub bad: ThemeColor,
    pub selection: ThemeColor,
}

/// 一个完整解析后的主题。
#[derive(Clone, Debug)]
pub struct ThemeSpec {
    pub id: String,
    pub name: String,
    pub base: BaseMode,
    pub colors: ThemeColors,
}

impl ThemeSpec {
    pub fn label(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.id
        } else {
            &self.name
        }
    }
}

/// 已加载的主题集合：内置主题在前，自定义主题按文件名排序。
#[derive(Clone, Debug)]
pub struct ThemeRegistry {
    specs: std::collections::BTreeMap<String, ThemeSpec>,
    order: Vec<String>,
    warnings: Vec<String>,
}

impl ThemeRegistry {
    pub fn builtin() -> Self {
        let mut registry = Self {
            specs: std::collections::BTreeMap::new(),
            order: Vec::new(),
            warnings: Vec::new(),
        };
        for spec in builtin_specs() {
            registry.order.push(spec.id.clone());
            registry.specs.insert(spec.id.clone(), spec);
        }
        registry
    }

    /// 从 `<home>/themes` 加载用户主题；失败只产生告警。
    pub fn load(dir: &std::path::Path) -> Self {
        let mut registry = Self::builtin();
        let (specs, warnings) = load_user_themes(dir);
        for spec in specs {
            if registry.specs.contains_key(&spec.id) {
                registry
                    .warnings
                    .push(format!("主题 {}.json 与内置主题重名，已跳过", spec.id));
                continue;
            }
            registry.order.push(spec.id.clone());
            registry.specs.insert(spec.id.clone(), spec);
        }
        registry.warnings.extend(warnings);
        registry
    }

    /// 解析主题；`auto` 映射到深色。返回 (spec, 是否发生回退)。
    pub fn resolve(&self, id: &str) -> (&ThemeSpec, bool) {
        let normalized = id.trim().to_ascii_lowercase();
        let key = match normalized.as_str() {
            "" | "auto" => DEFAULT_THEME_ID,
            other => other,
        };
        if let Some(spec) = self.specs.get(key) {
            return (spec, false);
        }
        let fallback = self
            .specs
            .get(DEFAULT_THEME_ID)
            .expect("内置默认主题必须存在");
        (
            fallback,
            !normalized.is_empty() && normalized != DEFAULT_THEME_ID,
        )
    }

    pub fn get(&self, id: &str) -> Option<&ThemeSpec> {
        self.specs.get(&id.trim().to_ascii_lowercase())
    }

    pub fn contains(&self, id: &str) -> bool {
        let normalized = id.trim().to_ascii_lowercase();
        normalized == "auto" || self.specs.contains_key(&normalized)
    }

    /// 设置页循环使用的顺序：auto + 内置 + 自定义。
    pub fn cycle_ids(&self) -> Vec<String> {
        let mut ids = vec!["auto".to_string()];
        ids.extend(self.order.iter().cloned());
        ids
    }

    pub fn label(&self, id: &str) -> String {
        let normalized = id.trim().to_ascii_lowercase();
        if normalized == "auto" {
            return "自动（深色）".to_string();
        }
        match self.specs.get(&normalized) {
            Some(spec) => spec.label().to_string(),
            None => format!("{id}（缺失，回退深色）"),
        }
    }

    pub fn custom_count(&self) -> usize {
        self.order.iter().filter(|id| self.is_custom(id)).count()
    }

    pub fn is_custom(&self, id: &str) -> bool {
        !builtin::builtin_specs().iter().any(|spec| spec.id == id)
    }

    pub fn ids(&self) -> &[String] {
        &self.order
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// 供 `ui` 使用的便捷入口。
pub fn default_dark() -> ThemeSpec {
    builtin_specs()
        .into_iter()
        .find(|spec| spec.id == DEFAULT_THEME_ID)
        .expect("内置默认主题必须存在")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_ansi_colors() {
        assert_eq!(
            ThemeColor::parse("#abc").unwrap(),
            ThemeColor::Rgb(Rgb::new(0xaa, 0xbb, 0xcc))
        );
        assert_eq!(
            ThemeColor::parse("#171720").unwrap(),
            ThemeColor::Rgb(Rgb::new(0x17, 0x17, 0x20))
        );
        assert_eq!(
            ThemeColor::parse("LightCyan").unwrap(),
            ThemeColor::Ansi(AnsiColor::LightCyan)
        );
        assert!(ThemeColor::parse("#12").is_err());
        assert!(ThemeColor::parse("#zzzzzz").is_err());
        assert!(ThemeColor::parse("not-a-color").is_err());
    }

    #[test]
    fn resolves_auto_and_missing_theme() {
        let registry = ThemeRegistry::builtin();
        let (spec, fallback) = registry.resolve("auto");
        assert!(!fallback);
        assert_eq!(spec.id, "dark");
        let (spec, fallback) = registry.resolve("nope");
        assert!(fallback);
        assert_eq!(spec.id, "dark");
    }
    #[test]
    fn custom_theme_lookups_are_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Ocean.json"),
            r##"{ "name": "OceanBlue", "colors": { "bg": "#010203" } }"##,
        )
        .unwrap();
        let registry = ThemeRegistry::load(dir.path());
        assert!(registry.contains("Ocean"));
        assert_eq!(registry.label("Ocean"), "OceanBlue");
        let (spec, fallback) = registry.resolve("Ocean");
        assert!(!fallback);
        assert_eq!(spec.id, "ocean");
    }
}
