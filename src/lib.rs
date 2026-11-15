//! Type Words TUI：终端版单词练习客户端。
//!
//! 分层（由内到外）：
//! - `domain`：与终端无关的领域逻辑（类型、词库、调度、流程、键入、会话）
//! - `store`：本地存档读写
//! - `app`：消息驱动的应用状态机
//! - `ui`：ratatui 渲染
//! - `audio`：终端响铃、系统 TTS 与外部播放器
//! - `cli` / `paths`：进程入口与路径解析
//! - `fs`：底层的文件原语（原子写 + 数据目录锁，`domain` 与 `store` 都用它，
//!   保持分层单向）

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
    if is_read_only(cli.command.as_ref()) {
        return dispatch(paths, cli);
    }
    // 其余入口（含 TUI）一律在数据目录锁内执行：存档是整份覆盖写，第二个
    // 实例必然会在退出时把前一个实例的进度整份盖掉。
    with_home_lock(&paths, || dispatch(paths.clone(), cli.clone()))
}

/// 只读子命令：TUI 正在运行时仍然要能用它们排障、导出。
///
/// 这里刻意写成**白名单**：新增子命令默认会被加锁（宁可多锁一次，也不要
/// 漏掉互斥），只有确认只读的命令才登记进来。
fn is_read_only(command: Option<&Command>) -> bool {
    matches!(command, Some(Command::Doctor | Command::ExportWeb { .. }))
}

/// 按子命令分发；是否持锁由 `run` 决定。
fn dispatch(paths: Paths, cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::Doctor) => cli::doctor(&paths),
        Some(Command::Task) => cli::task_preview(&paths, &cli),
        Some(Command::ImportDict { file }) => cli::import_dict(&paths, &file),
        Some(Command::ImportWeb { file }) => cli::import_web(&paths, &file),
        Some(Command::ExportWeb { out }) => cli::export_web(&paths, out.as_deref()),
        None => app::run(paths, cli),
    }
}

/// 取数据目录独占锁后执行 `action`；抢不到锁立刻报错退出（不阻塞等待）。
///
/// 锁的粒度是「整个进程生命周期」而不是某一次写：运行期间任何时刻都可能
/// 落盘，逐次加锁没有意义，也无法阻止退出时的整份覆盖。
fn with_home_lock<T>(paths: &Paths, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let _lock = paths.lock_home()?;
    action()
}

/// 打印版本与解析到的路径（供 `tw --version` 之外的排障使用）。
pub fn version_line() -> String {
    format!(
        "typewords-tui {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_NAME")
    )
}
