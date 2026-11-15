//! 崩溃报告与早退保护。
//!
//! - panic 时把时间戳、版本、路径与回溯写进 <日志目录>/crash-<时间戳>.txt，
//!   并打印报告路径。普通用户（尤其 Windows 双击启动）不需要自己加
//!   RUST_BACKTRACE 就能交回完整现场。
//! - Windows 双击启动（无命令行参数、交互式终端）遇到早期错误或崩溃时等待
//!   回车，避免控制台窗口在用户看清错误前关闭；自动化可用 TW_NO_PAUSE=1
//!   或 CI 环境变量跳过。

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Error;

use crate::logging;

/// 报告里附带最近多少行日志。
const LOG_TAIL_LINES: usize = 40;
/// 设置该环境变量后不再等待回车（自动化 / 脚本）。
const NO_PAUSE_ENV: &str = "TW_NO_PAUSE";
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
/// 同一秒内为崩溃报告申请文件名的重试上限。
const REPORT_NAME_ATTEMPTS: u32 = 64;

/// 崩溃报告需要的路径；在 main 解析出数据目录后尽早写入。
#[derive(Clone, Debug, Default)]
struct Context {
    home: Option<PathBuf>,
    assets: Option<PathBuf>,
    log_file: Option<PathBuf>,
    report_dir: Option<PathBuf>,
}

static CONTEXT: Mutex<Option<Context>> = Mutex::new(None);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
/// panic 钩子内部再 panic 会递归；只允许处理第一次。
static REPORTING: AtomicBool = AtomicBool::new(false);
/// 交互式回车等待的总开关（默认开）。测试进程会在安装钩子前关掉，
/// 否则在交互式终端里跑 cargo test 会被 panic 用例的「按回车」卡住。
static ENTER_WAIT: AtomicBool = AtomicBool::new(true);

/// 记录崩溃报告要用的路径。
///
/// 分两次调用：先记数据目录（资源目录还没解析出来），解析成功后再补资源
/// 目录与真实日志文件。这样连「找不到 assets/」的崩溃报告也能落到数据目录。
pub fn set_context(home: &Path, assets: Option<&Path>, log_file: Option<&Path>, report_dir: &Path) {
    let context = Context {
        home: Some(home.to_path_buf()),
        assets: assets.map(Path::to_path_buf),
        log_file: log_file.map(Path::to_path_buf),
        report_dir: Some(report_dir.to_path_buf()),
    };
    if let Ok(mut guard) = CONTEXT.lock() {
        *guard = Some(context);
    }
}

/// 安装 panic 钩子；必须早于任何可能 panic 的代码（main 的第一件事）。
///
/// 原有钩子会被保留并继续调用：App::run 在进入终端后安装的钩子负责先恢复
/// 终端与鼠标捕获，再调用这里的报告钩子，顺序不会因为新增报告功能而改变。
pub fn install_hook() {
    if HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if REPORTING.swap(true, Ordering::SeqCst) {
            previous(info);
            return;
        }
        // 报告本身的任何 panic 都不能盖掉原始 panic 信息或造成递归。
        let report = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_report(info)));
        let _ = &report;
        // 复位只覆盖「写报告期间」：将来若有代码用 catch_unwind 接住 panic
        // 继续运行，之后真正的崩溃仍然会生成报告。
        REPORTING.store(false, Ordering::SeqCst);
        previous(info);
        wait_for_enter_on_windows();
    }));
}

fn current_context() -> Context {
    CONTEXT
        .lock()
        .map(|guard| guard.clone().unwrap_or_default())
        .unwrap_or_default()
}

/// 写崩溃报告；返回报告路径（失败时为 None）。
fn write_report(info: &std::panic::PanicHookInfo<'_>) -> Option<PathBuf> {
    let context = current_context();
    let dir = context
        .report_dir
        .clone()
        .unwrap_or_else(fallback_report_dir);
    let message = panic_message(info);
    let location = info
        .location()
        .map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        })
        .unwrap_or_else(|| "<未知>".to_string());
    logging::error(format!("崩溃: {message}（位置 {location}）"));

    let now = chrono::Local::now();
    let fallback = fallback_report_dir();
    let (path, mut file) = open_report(&dir, &fallback, &now)?;

    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let log_tail = logging::recent_lines(LOG_TAIL_LINES);
    let args = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let text = build_report(
        &context, &message, &location, &backtrace, &log_tail, &args, &now,
    );

    if let Err(err) = file.write_all(text.as_bytes()) {
        eprintln!(
            "\n程序崩溃，但写入崩溃报告失败（{}）: {err}",
            path.display()
        );
        return None;
    }
    // 崩溃进程马上就要退出，尽量把报告同步到磁盘再走。
    let _ = file.sync_all();
    logging::error(format!("崩溃报告: {}", path.display()));
    eprintln!("\n程序崩溃，崩溃报告已写入: {}", path.display());
    Some(path)
}

/// 崩溃报告落到哪里：优先数据目录下的 logs/（与日志同目录），否则临时目录。
fn fallback_report_dir() -> PathBuf {
    std::env::temp_dir().join("typewords-tui-crash")
}

/// 优先写到日志目录；该目录不可创建时降级到系统临时目录。
///
/// 崩溃往往正是配置文件或路径坏了的时候，不能因为 <home> 不可写就丢掉现场。
fn open_report(
    preferred: &Path,
    fallback: &Path,
    now: &chrono::DateTime<chrono::Local>,
) -> Option<(PathBuf, File)> {
    match create_report_file(preferred, now) {
        Ok(created) => Some(created),
        Err(primary_err) => {
            if fallback == preferred {
                eprintln!(
                    "\n程序崩溃，且无法创建崩溃报告目录 {}: {primary_err}",
                    preferred.display()
                );
                return None;
            }
            match create_report_file(fallback, now) {
                Ok(created) => {
                    eprintln!(
                        "\n崩溃报告目录不可用（{}: {primary_err}），已改写到 {}",
                        preferred.display(),
                        fallback.display()
                    );
                    Some(created)
                }
                Err(fallback_err) => {
                    eprintln!(
                        "\n程序崩溃，且无法创建崩溃报告（{}: {primary_err}；{}: {fallback_err}）",
                        preferred.display(),
                        fallback.display()
                    );
                    None
                }
            }
        }
    }
}

/// 建目录、清理过期报告，并用 create_new 原子占一个不重名的文件名。
///
/// 用 create_new 而不是「先 exists 再写入」：多个进程同一秒崩溃时，exists
/// 检查与截断写之间有竞态，会互相覆盖报告。同一秒内的报告追加 -2、-3……，
/// 绝不覆盖旧报告。
fn create_report_file(
    dir: &Path,
    now: &chrono::DateTime<chrono::Local>,
) -> std::io::Result<(PathBuf, File)> {
    fs::create_dir_all(dir)?;
    logging::prune_matching(
        dir,
        logging::CRASH_PREFIX,
        logging::CRASH_SUFFIX,
        Duration::from_secs(logging::CRASH_KEEP_DAYS * SECONDS_PER_DAY),
    );
    let stem = format!("{}{}", logging::CRASH_PREFIX, now.format("%Y%m%d-%H%M%S"));
    for index in 1..=REPORT_NAME_ATTEMPTS {
        let name = if index == 1 {
            format!("{stem}{}", logging::CRASH_SUFFIX)
        } else {
            format!("{stem}-{index}{}", logging::CRASH_SUFFIX)
        };
        let path = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "同一秒内崩溃报告过多",
    ))
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<非字符串 panic 载荷>".to_string()
    }
}

/// 生成报告正文（纯函数：时间、命令行、回溯都由调用方传入，便于单测）。
fn build_report(
    context: &Context,
    message: &str,
    location: &str,
    backtrace: &str,
    log_tail: &[String],
    args: &str,
    now: &chrono::DateTime<chrono::Local>,
) -> String {
    use std::fmt::Write as _;

    let mut text = String::new();
    let _ = writeln!(text, "typewords-tui 崩溃报告");
    let _ = writeln!(text, "==============================");
    let _ = writeln!(text, "生成时间: {}", now.format("%Y-%m-%d %H:%M:%S%.3f %z"));
    let _ = writeln!(text, "版本: {}", crate::version_line());
    let _ = writeln!(text, "进程号: {}", std::process::id());
    let _ = writeln!(
        text,
        "平台: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(text, "命令行: tw {args}");
    let _ = writeln!(text, "当前目录: {}", current_dir_line());
    let _ = writeln!(text, "可执行文件: {}", exe_line());
    let _ = writeln!(text, "数据目录: {}", path_line(&context.home, "（未解析）"));
    let _ = writeln!(
        text,
        "资源目录: {}",
        path_line(&context.assets, "（未解析）")
    );
    let _ = writeln!(
        text,
        "日志文件: {}",
        path_line(&context.log_file, "（未启用）")
    );
    let _ = writeln!(text);
    let _ = writeln!(text, "panic 消息: {message}");
    let _ = writeln!(text, "panic 位置: {location}");
    let _ = writeln!(text);
    let _ = writeln!(text, "回溯:");
    if backtrace.trim().is_empty() {
        let _ = writeln!(text, "（未捕获到回溯）");
    } else {
        let _ = writeln!(text, "{}", backtrace.trim_end());
    }
    let _ = writeln!(text);
    let _ = writeln!(text, "最近日志（最后 {} 行）:", log_tail.len());
    if log_tail.is_empty() {
        let _ = writeln!(text, "（没有可用的日志）");
    } else {
        for line in log_tail {
            let _ = writeln!(text, "{line}");
        }
    }
    let _ = writeln!(text);
    let _ = writeln!(
        text,
        "提示: 反馈问题时请附上本文件与同目录日志，并注明复现步骤。"
    );
    text
}

fn path_line(path: &Option<PathBuf>, missing: &str) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => missing.to_string(),
    }
}

fn current_dir_line() -> String {
    std::env::current_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_else(|_| "<无法读取>".to_string())
}

fn exe_line() -> String {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "<无法读取>".to_string())
}

/// 把早期致命错误（anyhow 链）写进日志并打印给用户；回车等待由 main 决定。
pub fn report_fatal_error(err: &Error) {
    let message = format!("{err:#}");
    logging::error(format!("致命错误: {message}"));
    eprintln!("Error: {message}");
    match logging::log_file() {
        Some(path) => {
            eprintln!("日志文件: {}", path.display());
            eprintln!("反馈问题时请附上该日志与 tw doctor 的输出。");
        }
        None => eprintln!("运行日志未启用；可用 --log <FILE> 指定日志文件。"),
    }
}

/// Windows 双击启动时在早期错误 / 崩溃后等待回车，防止控制台窗口一闪而过。
///
/// 只在「Windows + 没有命令行参数 + stdin/stderr 都是终端」时等待：
/// 从 cmd / PowerShell 运行（通常带参数，或有 CI 变量）不会停住。
/// TW_NO_PAUSE=1 可显式跳过。
pub fn wait_for_enter_on_windows() {
    if !should_wait_for_enter() {
        return;
    }
    eprintln!();
    eprintln!("按回车键退出...");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}

/// 显式开关交互式回车等待；自动化与测试进程应在安装钩子前关掉。
pub fn set_enter_wait_enabled(enabled: bool) {
    ENTER_WAIT.store(enabled, Ordering::SeqCst);
}

fn should_wait_for_enter() -> bool {
    if !ENTER_WAIT.load(Ordering::SeqCst) {
        return false;
    }
    if std::env::var_os(NO_PAUSE_ENV).is_some() || std::env::var_os("CI").is_some() {
        return false;
    }
    should_pause(
        cfg!(windows),
        std::env::args_os().count(),
        std::io::stdin().is_terminal(),
        std::io::stderr().is_terminal(),
    )
}

/// 纯函数形式的触发条件（便于在任意平台单测 Windows 分支）。
fn should_pause(
    windows: bool,
    arg_count: usize,
    stdin_is_terminal: bool,
    stderr_is_terminal: bool,
) -> bool {
    windows && arg_count <= 1 && stdin_is_terminal && stderr_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> Context {
        Context {
            home: Some(PathBuf::from("/home/tw")),
            assets: Some(PathBuf::from("/opt/tw/assets")),
            log_file: Some(PathBuf::from("/home/tw/logs/tw-2026-01-02.log")),
            report_dir: Some(PathBuf::from("/home/tw/logs")),
        }
    }

    #[test]
    fn report_contains_time_version_paths_and_backtrace() {
        let now = chrono::Local::now();
        let text = build_report(
            &sample_context(),
            "boom",
            "src/main.rs:10:5",
            "   0: typewords_tui::run\n   1: std::panic",
            &["2026-01-02 03:04:05.678 INFO  启动".to_string()],
            "task --home /tmp/tw",
            &now,
        );
        assert!(text.contains("typewords-tui 崩溃报告"), "{text}");
        assert!(text.contains("版本: "), "{text}");
        assert!(text.contains("数据目录: /home/tw"), "{text}");
        assert!(text.contains("资源目录: /opt/tw/assets"), "{text}");
        assert!(
            text.contains("日志文件: /home/tw/logs/tw-2026-01-02.log"),
            "{text}"
        );
        assert!(text.contains("panic 消息: boom"), "{text}");
        assert!(text.contains("panic 位置: src/main.rs:10:5"), "{text}");
        assert!(text.contains("回溯:"), "{text}");
        assert!(text.contains("typewords_tui::run"), "{text}");
        assert!(text.contains("最近日志"), "{text}");
        assert!(text.contains("命令行: tw task --home /tmp/tw"), "{text}");
    }

    #[test]
    fn report_marks_disabled_logging_and_missing_paths() {
        let now = chrono::Local::now();
        let text = build_report(&Context::default(), "boom", "<未知>", "", &[], "", &now);
        assert!(text.contains("数据目录: （未解析）"), "{text}");
        assert!(text.contains("日志文件: （未启用）"), "{text}");
        assert!(text.contains("（未捕获到回溯）"), "{text}");
        assert!(text.contains("（没有可用的日志）"), "{text}");
    }

    #[test]
    fn reports_get_unique_names_without_overwriting() {
        use std::io::Write as _;

        let dir = tempfile::tempdir().unwrap();
        let now = chrono::Local::now();
        let (first, mut first_file) = create_report_file(dir.path(), &now).expect("应能创建报告");
        first_file.write_all(b"first").unwrap();
        drop(first_file);
        let (second, mut second_file) =
            create_report_file(dir.path(), &now).expect("应能创建第二份报告");
        second_file.write_all(b"second").unwrap();
        drop(second_file);

        assert_ne!(first, second);
        let name = first.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("crash-"), "{name}");
        assert!(name.ends_with(".txt"), "{name}");
        assert!(
            second
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-2.txt")
        );
        // create_new 占位，后续创建不会截断已经写好的报告。
        assert_eq!(fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(fs::read_to_string(&second).unwrap(), "second");
    }

    #[test]
    fn report_creation_falls_back_to_the_secondary_directory() {
        let root = tempfile::tempdir().unwrap();
        let blocked = root.path().join("logs");
        fs::write(&blocked, "not a directory").unwrap();
        let fallback = root.path().join("fallback");
        let now = chrono::Local::now();

        let (path, _) = open_report(&blocked, &fallback, &now).expect("应降级到备用目录");
        assert!(path.starts_with(&fallback), "{path:?}");
        assert!(path.is_file());

        // 两边都不可用才放弃。
        let blocked_fallback = root.path().join("fallback-blocked");
        fs::write(&blocked_fallback, "x").unwrap();
        assert!(open_report(&blocked, &blocked_fallback, &now).is_none());
    }

    #[test]
    fn pause_can_be_disabled_for_tests_and_automation() {
        set_enter_wait_enabled(false);
        assert!(!should_wait_for_enter());
        set_enter_wait_enabled(true);
    }

    #[test]
    fn pause_only_applies_to_interactive_windows_double_click() {
        // Windows + 无参数 + 交互终端：双击启动，应该停住。
        assert!(should_pause(true, 1, true, true));
        // 带参数：从命令行或脚本启动，不停。
        assert!(!should_pause(true, 2, true, true));
        // 非 Windows / 无终端（管道、CI）：不停。
        assert!(!should_pause(false, 1, true, true));
        assert!(!should_pause(true, 1, false, true));
        assert!(!should_pause(true, 1, true, false));
    }
}
