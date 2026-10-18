use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

/// 解析后的运行路径。
///
/// - `home`：可写的数据目录（存档、用户词库、日志）
/// - `assets`：只读的词库资源目录（含 `list/word.json` 与 `dicts/`）
#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub assets: PathBuf,
}

impl Paths {
    pub fn resolve(assets_arg: Option<PathBuf>, home_arg: Option<PathBuf>) -> Result<Self> {
        let home = home_arg
            .or_else(|| env_path("TYPEWORDS_HOME"))
            .or_else(|| dirs::data_local_dir().map(|dir| dir.join("typewords-tui")))
            .ok_or_else(|| anyhow!("无法确定数据目录，请使用 --home 指定"))?;

        let assets = locate_assets(assets_arg.as_deref()).ok_or_else(|| {
            anyhow!(
                "找不到词库资源目录（需要包含 list/word.json）。可用 --assets 或环境变量 TYPEWORDS_ASSETS 指定；发布包可把 assets/ 放在可执行文件同级"
            )
        })?;
        Ok(Self { home, assets })
    }

    /// 确保数据目录存在（首次启动）。
    pub fn ensure_home(&self) -> Result<()> {
        std::fs::create_dir_all(&self.home)
            .map_err(|err| anyhow!("创建数据目录失败 {}: {err}", self.home.display()))?;
        Ok(())
    }

    pub fn log_file(&self) -> PathBuf {
        self.home.join("logs").join("tui.log")
    }

    /// 用户自定义主题目录：`<home>/themes/*.json`。
    pub fn themes_dir(&self) -> PathBuf {
        self.home.join("themes")
    }
}

/// 按优先级查找词库资源目录：显式参数 > `TYPEWORDS_ASSETS` > 可执行文件同级 `assets/` >
/// 当前目录及最多 3 级祖先的 `assets/`（开发期）> 同层 `public/`（兼容从上游仓库内运行）。
pub fn locate_assets(explicit: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = explicit {
        candidates.push(dir.to_path_buf());
    }
    if let Some(dir) = env_path("TYPEWORDS_ASSETS") {
        candidates.push(dir);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("assets"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        // 支持在仓库根、crate 根或其子目录下运行。
        for ancestor in cwd.ancestors().take(4) {
            candidates.push(ancestor.join("assets"));
            candidates.push(ancestor.join("public"));
        }
    }
    first_assets_root(candidates)
}

fn first_assets_root(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|dir| is_assets_root(dir))
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

pub fn is_assets_root(dir: &Path) -> bool {
    dir.join("list").join("word.json").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_assets(root: &Path) {
        fs::create_dir_all(root.join("list")).unwrap();
        fs::write(root.join("list").join("word.json"), "[]").unwrap();
    }

    #[test]
    fn is_assets_root_requires_catalog_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_assets_root(dir.path()));
        make_assets(dir.path());
        assert!(is_assets_root(dir.path()));
    }

    #[test]
    fn first_assets_root_prefers_order() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        make_assets(first.path());
        make_assets(second.path());

        let picked = first_assets_root(vec![
            second.path().to_path_buf(),
            first.path().to_path_buf(),
        ]);
        assert_eq!(picked.as_deref(), Some(second.path()));

        let missing = first.path().join("missing");
        let picked = first_assets_root(vec![missing, first.path().to_path_buf()]);
        assert_eq!(picked.as_deref(), Some(first.path()));

        assert!(first_assets_root(Vec::new()).is_none());
    }

    #[test]
    fn locate_assets_prefers_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        make_assets(dir.path());
        let picked = locate_assets(Some(dir.path())).expect("explicit assets should be found");
        assert_eq!(picked, dir.path());
    }
}
