//! 底层的文件写入原语。
//!
//! 单独成模块是为了让分层保持单向：`domain` 不依赖 `store`，而 `store`
//! 依赖 `domain`。原子写是两边都要用的东西（存档层写 JSON，领域层写自建
//! 词库与用户词库目录），放在这里就不会形成 `domain ↔ store` 的环。

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

/// 原子写：先写同目录的临时文件，`fsync` 后 `rename` 覆盖目标。
///
/// 同目录保证 `rename` 在同一文件系统内（跨文件系统的 `rename` 可能失败）；
/// `fsync` 保证内容真的落盘，断电时不会留下半个文件。
pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "state.json".to_string());
    let tmp = path.with_file_name(format!(".{file_name}.tmp"));
    {
        let mut file = fs::File::create(&tmp)
            .with_context(|| format!("写入临时文件失败: {}", tmp.display()))?;
        file.write_all(text.as_bytes())
            .with_context(|| format!("写入临时文件失败: {}", tmp.display()))?;
        file.sync_all().ok();
    }
    fs::rename(&tmp, path).with_context(|| format!("替换存档失败: {}", path.display()))?;
    Ok(())
}
