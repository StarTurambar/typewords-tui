//! 用户主题 JSON 的发现、校验与 base 继承。

use std::path::Path;

use serde::Deserialize;

use super::builtin::builtin_specs;
use super::{BaseMode, ThemeColor, ThemeColors, ThemeSpec};

const RESERVED_IDS: &[&str] = &["auto", "dark", "light", "high-contrast"];

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct ThemeFile {
    name: Option<String>,
    base: Option<String>,
    colors: PartialColors,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct PartialColors {
    bg: Option<String>,
    panel: Option<String>,
    element: Option<String>,
    border: Option<String>,
    fg: Option<String>,
    muted: Option<String>,
    dim: Option<String>,
    accent: Option<String>,
    accent_alt: Option<String>,
    warn: Option<String>,
    good: Option<String>,
    bad: Option<String>,
    selection: Option<String>,
}

/// 读取目录下的 `*.json` 主题。返回 (主题列表, 告警列表)。
pub fn load_user_themes(dir: &Path) -> (Vec<ThemeSpec>, Vec<String>) {
    let mut specs = Vec::new();
    let mut warnings = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return (specs, warnings),
        Err(err) => {
            warnings.push(format!("读取主题目录失败 {}: {err}", dir.display()));
            return (specs, warnings);
        }
    };

    let mut files: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();

    for path in files {
        match parse_theme_file(&path) {
            Ok(spec) => specs.push(spec),
            Err(err) => warnings.push(format!("{}: {err}", path.display())),
        }
    }
    (specs, warnings)
}

fn parse_theme_file(path: &Path) -> Result<ThemeSpec, String> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "文件名不是有效 UTF-8".to_string())?;
    validate_id(stem)?;
    // 主题 id 统一小写；显示名仍保留文件名/name 的原始大小写。
    let id = stem.to_ascii_lowercase();
    if RESERVED_IDS.contains(&id.as_str()) {
        return Err(format!("主题 id `{stem}` 为内置保留名，请换一个文件名"));
    }
    let text = std::fs::read_to_string(path).map_err(|err| format!("读取失败: {err}"))?;
    let file: ThemeFile =
        serde_json::from_str(&text).map_err(|err| format!("JSON 解析失败: {err}"))?;

    let base_mode = match file.base.as_deref() {
        Some(value) => BaseMode::parse(value)?,
        None => BaseMode::Dark,
    };
    let base = builtin_specs()
        .into_iter()
        .find(|spec| spec.base == base_mode)
        .ok_or_else(|| format!("找不到 base 主题: {}", base_mode.as_id()))?;
    let colors = merge_colors(base.colors, &file.colors)?;
    let name = file
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(stem)
        .to_string();

    Ok(ThemeSpec {
        id,
        name,
        base: base_mode,
        colors,
    })
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 32 {
        return Err("主题文件名需为 1~32 个字符".to_string());
    }
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err("主题 id 只能包含字母、数字、下划线和连字符".to_string());
    }
    Ok(())
}

fn merge_colors(base: ThemeColors, partial: &PartialColors) -> Result<ThemeColors, String> {
    Ok(ThemeColors {
        bg: pick(base.bg, &partial.bg, "bg")?,
        panel: pick(base.panel, &partial.panel, "panel")?,
        element: pick(base.element, &partial.element, "element")?,
        border: pick(base.border, &partial.border, "border")?,
        fg: pick(base.fg, &partial.fg, "fg")?,
        muted: pick(base.muted, &partial.muted, "muted")?,
        dim: pick(base.dim, &partial.dim, "dim")?,
        accent: pick(base.accent, &partial.accent, "accent")?,
        accent_alt: pick(base.accent_alt, &partial.accent_alt, "accent_alt")?,
        warn: pick(base.warn, &partial.warn, "warn")?,
        good: pick(base.good, &partial.good, "good")?,
        bad: pick(base.bad, &partial.bad, "bad")?,
        selection: pick(base.selection, &partial.selection, "selection")?,
    })
}

fn pick(current: ThemeColor, value: &Option<String>, token: &str) -> Result<ThemeColor, String> {
    match value {
        Some(text) => ThemeColor::parse(text).map_err(|err| format!("colors.{token}: {err}")),
        None => Ok(current),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn loads_partial_custom_theme_inheriting_dark() {
        let dir = temp_dir();
        fs::write(
            dir.path().join("ocean.json"),
            r##"{
                "name": "Ocean",
                "base": "dark",
                "colors": { "bg": "#0b1020", "accent": "#6fd3c7" }
            }"##,
        )
        .unwrap();
        let (specs, warnings) = load_user_themes(dir.path());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.id, "ocean");
        assert_eq!(spec.name, "Ocean");
        assert_eq!(
            spec.colors.bg,
            ThemeColor::Rgb(super::super::Rgb::new(0x0b, 0x10, 0x20))
        );
        // 未覆盖的 token 继承 dark 的 panel
        assert_eq!(
            spec.colors.panel,
            ThemeColor::Rgb(super::super::Rgb::new(0x1e, 0x1e, 0x2b))
        );
    }

    #[test]
    fn rejects_invalid_files_and_reserved_ids() {
        let dir = temp_dir();
        fs::write(
            dir.path().join("bad.json"),
            r##"{ "colors": { "bg": "#zzzzzz" } }"##,
        )
        .unwrap();
        fs::write(
            dir.path().join("dark.json"),
            r##"{ "colors": { "bg": "#000000" } }"##,
        )
        .unwrap();
        fs::write(
            dir.path().join("bad name.json"),
            r##"{ "colors": { "bg": "#000000" } }"##,
        )
        .unwrap();
        fs::write(dir.path().join("broken.json"), "{").unwrap();
        let (specs, warnings) = load_user_themes(dir.path());
        assert!(specs.is_empty(), "{specs:?}");
        assert_eq!(warnings.len(), 4, "{warnings:?}");
    }

    #[test]
    fn rejects_unknown_base_and_unknown_token() {
        let dir = temp_dir();
        fs::write(dir.path().join("a.json"), r#"{ "base": "purple" }"#).unwrap();
        fs::write(
            dir.path().join("b.json"),
            r##"{ "colors": { "bgx": "#000000" } }"##,
        )
        .unwrap();
        let (specs, warnings) = load_user_themes(dir.path());
        assert!(specs.is_empty(), "{specs:?}");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }
    #[test]
    fn uppercase_filename_is_normalized_to_lowercase_id() {
        let dir = temp_dir();
        fs::write(
            dir.path().join("Ocean.json"),
            r##"{ "name": "OceanBlue", "colors": { "bg": "#010203" } }"##,
        )
        .unwrap();
        let (specs, warnings) = load_user_themes(dir.path());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].id, "ocean");
        assert_eq!(specs[0].name, "OceanBlue");
    }
}
