use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::fs::DirLock;

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
        let home = Self::resolve_home(home_arg)?;
        Self::resolve_in(home, assets_arg)
    }

    /// 只解析数据目录。
    ///
    /// 日志初始化必须在资源目录解析之前完成（缺少 assets/ 是最常见的启动
    /// 失败，错误要进日志），所以这条查找单独拆出来复用。
    pub fn resolve_home(home_arg: Option<PathBuf>) -> Result<PathBuf> {
        home_arg
            .or_else(|| env_path("TYPEWORDS_HOME"))
            .or_else(|| dirs::data_local_dir().map(|dir| dir.join("typewords-tui")))
            .ok_or_else(|| anyhow!("无法确定数据目录，请使用 --home 指定"))
    }

    /// 已知数据目录时只解析资源目录（失败信息与 [Paths::resolve] 完全一致）。
    pub fn resolve_in(home: PathBuf, assets_arg: Option<PathBuf>) -> Result<Self> {
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

    /// 取数据目录的独占锁（`<home>/.tw.lock`）。
    ///
    /// 另一个 `tw` 实例正在使用同一个数据目录时返回错误——存档是整份覆盖写，
    /// 两个实例同时运行必然丢进度。调用方必须让返回的守卫活到进程退出
    /// （`lib::run` 把守卫放在整个命令分发过程中），否则锁会提前释放。
    pub fn lock_home(&self) -> Result<DirLock> {
        self.ensure_home()?;
        crate::fs::try_lock_dir(&self.home)
    }

    /// 用户自定义主题目录：`<home>/themes/*.json`。
    pub fn themes_dir(&self) -> PathBuf {
        self.home.join("themes")
    }

    /// 运行日志与崩溃报告目录：`<home>/logs/`（首次写日志时创建）。
    pub fn logs_dir(&self) -> PathBuf {
        self.home.join("logs")
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
    fn lock_home_creates_missing_data_dir() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: root.path().join("home-not-created-yet"),
            assets: root.path().to_path_buf(),
        };
        let lock = paths.lock_home().expect("数据目录应被创建并取到锁");
        assert!(paths.home.is_dir());
        assert_eq!(lock.path(), paths.home.join(crate::fs::LOCK_FILE_NAME));
    }

    #[test]
    fn locate_assets_prefers_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        make_assets(dir.path());
        let picked = locate_assets(Some(dir.path())).expect("explicit assets should be found");
        assert_eq!(picked, dir.path());
    }
}
