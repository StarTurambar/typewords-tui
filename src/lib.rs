//! Type Words TUI：终端版单词练习客户端。
//!
//! 分层（由内到外）：
//! - domain：与终端无关的领域逻辑（类型、词库、调度、流程、键入、会话）
//! - store：本地存档读写
//! - app：消息驱动的应用状态机
//! - ui：ratatui 渲染
//! - audio：终端响铃、系统 TTS 与外部播放器
//! - cli / paths：进程入口与路径解析
//! - fs：底层的文件原语（原子写 + 数据目录锁，domain 与 store 都用它，
//!   保持分层单向）
//! - logging / crash：运行日志与崩溃报告（只依赖 std + chrono，不做业务判断）

pub mod app;
pub mod audio;
pub mod backup;
pub mod bootstrap;
pub mod cli;
pub mod crash;
pub mod domain;
pub mod fs;
pub mod logging;
pub mod paths;
pub mod store;
pub mod theme;
pub mod ui;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cli::{Cli, Command};
use crate::paths::Paths;

pub fn run(cli: Cli) -> Result<()> {
    // 日志与崩溃上下文都必须在资源目录解析之前就位：缺少 assets/ 是最常见的
    // 启动失败，这条错误要留在日志里，而不是只在 stderr 闪一下。
    let home = Paths::resolve_home(cli.home.clone())?;
    let location = logging::resolve_location(&home, cli.log.as_deref());
    let log_file = logging::init(
        &home,
        logging::Options {
            file: cli.log.as_deref(),
            level: cli.log_level,
            disabled: cli.no_log,
        },
    );
    crash::set_context(&home, None, log_file.as_deref(), &location.dir);
    logging::info(format!(
        "平台: {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    logging::info(format!("进程号: {}", std::process::id()));
    logging::info(format!("命令行: tw {}", args_line()));
    logging::info(format!("数据目录: {}", home.display()));
    match &log_file {
        Some(path) => logging::info(format!("日志文件: {}", path.display())),
        None if cli.no_log => logging::info("运行日志已关闭（--no-log）"),
        None => logging::info("运行日志不可用（目录不可写），本次只输出到 stderr"),
    }

    let result = run_resolved(home, cli, log_file.as_deref(), &location.dir);
    match &result {
        Ok(()) => logging::info("进程退出: 正常"),
        Err(err) => logging::error(format!("进程退出: 失败: {err:#}")),
    }
    result
}

/// 资源目录解析成功后的主体流程；资源目录缺失时错误已经由 [run] 记进日志。
fn run_resolved(home: PathBuf, cli: Cli, log_file: Option<&Path>, report_dir: &Path) -> Result<()> {
    let paths = Paths::resolve_in(home, cli.assets.clone())?;
    crash::set_context(&paths.home, Some(&paths.assets), log_file, report_dir);
    logging::info(format!("资源目录: {}", paths.assets.display()));
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
    matches!(
        command,
        Some(Command::Doctor | Command::ExportWeb { .. } | Command::ExportDict { .. })
    )
}

/// 按子命令分发；是否持锁由 [run_resolved] 决定。
fn dispatch(paths: Paths, cli: Cli) -> Result<()> {
    logging::info(format!("执行: {}", command_label(cli.command.as_ref())));
    match cli.command {
        Some(Command::Doctor) => cli::doctor(&paths),
        Some(Command::Task) => cli::task_preview(&paths, &cli),
        Some(Command::ImportDict { file }) => cli::import_dict(&paths, &file),
        Some(Command::ExportDict { id, out }) => cli::export_dict(
            &paths,
            id.as_deref().or(cli.dict.as_deref()),
            out.as_deref(),
        ),
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
    logging::info(format!("获取数据目录锁: {}", paths.home.display()));
    let _lock = paths.lock_home()?;
    logging::info("已获取数据目录锁");
    action()
}

/// 子命令名（写日志用）。
fn command_label(command: Option<&Command>) -> &'static str {
    match command {
        None => "tui",
        Some(Command::Doctor) => "doctor",
        Some(Command::Task) => "task",
        Some(Command::ImportDict { .. }) => "import-dict",
        Some(Command::ExportDict { .. }) => "export-dict",
        Some(Command::ImportWeb { .. }) => "import-web",
        Some(Command::ExportWeb { .. }) => "export-web",
    }
}

fn args_line() -> String {
    std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 打印版本（供 `tw --version` 之外的排障使用）。
pub fn version_line() -> String {
    format!(
        "typewords-tui {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_NAME")
    )
}
