//! Type Words TUI：终端版单词练习客户端。
//!
//! 分层（由内到外）：
//! - `domain`：与终端无关的领域逻辑（类型、词库、调度、流程、键入、会话）
//! - `store`：本地存档读写
//! - `app`：消息驱动的应用状态机
//! - `ui`：ratatui 渲染
//! - `audio`：终端响铃、系统 TTS 与外部播放器
//! - `cli` / `paths`：进程入口与路径解析
//! - `fs`：底层的原子写原语（`domain` 与 `store` 都用它，保持分层单向）

pub mod app;
pub mod audio;
pub mod backup;
pub mod bootstrap;
pub mod cli;
pub mod domain;
pub mod fs;
pub mod paths;
pub mod store;
pub mod theme;
pub mod ui;

use anyhow::Result;

use crate::cli::{Cli, Command};
use crate::paths::Paths;

pub fn run(cli: Cli) -> Result<()> {
    let paths = Paths::resolve(cli.assets.clone(), cli.home.clone())?;

    match cli.command {
        Some(Command::Doctor) => cli::doctor(&paths),
        Some(Command::Task) => cli::task_preview(&paths, &cli),
        Some(Command::ImportDict { file }) => cli::import_dict(&paths, &file),
        Some(Command::ImportWeb { file }) => cli::import_web(&paths, &file),
        Some(Command::ExportWeb { out }) => cli::export_web(&paths, out.as_deref()),
        None => app::run(paths, cli),
    }
}

/// 打印版本与解析到的路径（供 `tw --version` 之外的排障使用）。
pub fn version_line() -> String {
    format!(
        "typewords-tui {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_NAME")
    )
}
