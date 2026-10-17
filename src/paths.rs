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

        let assets = resolve_assets(assets_arg)?;
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

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn resolve_assets(assets_arg: Option<PathBuf>) -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = assets_arg {
        candidates.push(dir);
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
        // 支持在仓库根目录或 tui/ 子目录下运行
        for ancestor in cwd.ancestors().take(4) {
            candidates.push(ancestor.join("public"));
        }
    }

    candidates
        .into_iter()
        .find(|dir| is_assets_root(dir))
        .ok_or_else(|| {
            anyhow!(
                "找不到词库资源目录（需要包含 list/word.json）。可用 --assets 或环境变量 TYPEWORDS_ASSETS 指定"
            )
        })
}

pub fn is_assets_root(dir: &Path) -> bool {
    dir.join("list").join("word.json").is_file()
}
