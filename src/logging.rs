//! 运行日志：追加写文件、按天分文件、启动时清理过期文件。
//!
//! 刻意不引入日志框架：需求只有「把启动、警告、错误与退出写进一个用户
//! 能交回来的文件」，用标准库的追加写就够，也避免给 TUI 增加依赖、后台
//! 线程或异步运行时。每行都单独 open + append + close，进程崩溃时最后一行
//! 不会留在用户态缓冲区里——崩溃现场最需要的恰恰是最后几行。

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};

/// 运行日志保留天数（按文件修改时间清理）。
pub const LOG_KEEP_DAYS: u64 = 14;
/// 崩溃报告保留天数。
pub const CRASH_KEEP_DAYS: u64 = 30;
/// 崩溃报告文件名前缀与后缀（crash-<时间戳>.txt，由 crash 模块使用）。
pub const CRASH_PREFIX: &str = "crash-";
pub const CRASH_SUFFIX: &str = ".txt";

const LOG_PREFIX: &str = "tw-";
const LOG_SUFFIX: &str = ".log";
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
/// 读「最近日志」时最多回看的字节数：崩溃报告不该把整份大日志读进内存。
const TAIL_WINDOW: u64 = 256 * 1024;

/// 日志级别；声明顺序即过滤顺序（Error 最低）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    /// 解析 --log-level 的取值；大小写不敏感，warning 视为 warn。
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

/// clap 的 value_parser：把字符串转成 Level，错误信息给用户看。
pub fn parse_level(value: &str) -> Result<Level, String> {
    Level::parse(value)
        .ok_or_else(|| format!("未知日志级别: {value}（可选 error / warn / info / debug）"))
}

/// 本次运行的日志落点。
///
/// 崩溃报告与日志放在同一个目录：用户只需要知道「--log 指定的那个目录」
/// 一个位置就能把两份材料一起交回来。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub file: PathBuf,
    pub dir: PathBuf,
}

/// 计算日志落点：显式 --log FILE 用该文件，否则 <home>/logs/tw-<日期>.log。
pub fn resolve_location(home: &Path, file: Option<&Path>) -> Location {
    match file {
        Some(path) => Location {
            file: path.to_path_buf(),
            dir: log_dir_for(path),
        },
        None => {
            let dir = home.join("logs");
            let stamp = chrono::Local::now().format("%Y-%m-%d");
            Location {
                file: dir.join(format!("tw-{stamp}.log")),
                dir,
            }
        }
    }
}

/// 日志文件所在目录；相对路径（--log tw.log）视为当前目录。
fn log_dir_for(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// init 的输入；字段与 CLI 一一对应。
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    /// 显式日志文件；None 用默认位置。
    pub file: Option<&'a Path>,
    /// 低于该级别的记录不落盘。
    pub level: Level,
    /// --no-log：本次运行不写日志（崩溃报告仍然生成）。
    pub disabled: bool,
}

struct Sink {
    file: PathBuf,
    level: Level,
}

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// 初始化运行日志，返回实际日志文件。
///
/// 失败只打印一条警告后继续运行：日志是排障辅助，不能成为启动的前提。
/// 返回 None 表示本次运行没有日志文件（--no-log 或目录不可写）。
pub fn init(home: &Path, options: Options<'_>) -> Option<PathBuf> {
    disable();
    if options.disabled {
        return None;
    }
    let location = resolve_location(home, options.file);
    if let Err(err) = prepare(&location) {
        eprintln!("警告: 无法写入日志 {}: {err:#}", location.file.display());
        return None;
    }
    if let Ok(mut guard) = SINK.lock() {
        *guard = Some(Sink {
            file: location.file.clone(),
            level: options.level,
        });
    }

    // 当前日志文件永远不参与清理：--log 可能指向一个很久没写的 tw-*.log，
    // 它正要被本次运行复用，删掉它会连历史内容一起丢。
    let pruned = prune_matching_except(
        &location.dir,
        LOG_PREFIX,
        LOG_SUFFIX,
        Duration::from_secs(LOG_KEEP_DAYS * SECONDS_PER_DAY),
        location.file.file_name(),
    );
    let pruned_crashes = prune_matching(
        &location.dir,
        CRASH_PREFIX,
        CRASH_SUFFIX,
        Duration::from_secs(CRASH_KEEP_DAYS * SECONDS_PER_DAY),
    );
    if pruned > 0 || pruned_crashes > 0 {
        debug(format!(
            "已清理过期文件：日志 {pruned} 个、崩溃报告 {pruned_crashes} 个"
        ));
    }
    info(format!("===== {} =====", crate::version_line()));
    Some(location.file)
}

/// 创建目录并验证文件可追加写（打开后立刻丢弃句柄）。
fn prepare(location: &Location) -> Result<()> {
    fs::create_dir_all(&location.dir)
        .with_context(|| format!("创建日志目录失败: {}", location.dir.display()))?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&location.file)
        .with_context(|| format!("打开日志文件失败: {}", location.file.display()))?;
    Ok(())
}

/// 关闭本次运行的日志（后续写日志调用变成空操作）。
pub fn disable() {
    if let Ok(mut guard) = SINK.lock() {
        *guard = None;
    }
}

/// 本次运行是否正在写日志。
pub fn enabled() -> bool {
    SINK.lock().map(|guard| guard.is_some()).unwrap_or(false)
}

/// 本次运行的日志文件路径（未启用时为 None）。
pub fn log_file() -> Option<PathBuf> {
    SINK.lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|sink| sink.file.clone()))
}

pub fn error(message: impl AsRef<str>) {
    write_line(Level::Error, message.as_ref());
}

pub fn warn(message: impl AsRef<str>) {
    write_line(Level::Warn, message.as_ref());
}

pub fn info(message: impl AsRef<str>) {
    write_line(Level::Info, message.as_ref());
}

pub fn debug(message: impl AsRef<str>) {
    write_line(Level::Debug, message.as_ref());
}

fn write_line(level: Level, message: &str) {
    let Ok(guard) = SINK.lock() else {
        return;
    };
    let Some(sink) = guard.as_ref() else {
        return;
    };
    if !allows(sink.level, level) {
        return;
    }
    let timestamp = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string();
    let line = format_line(&timestamp, level, message);
    // 写失败只能忽略：日志本身不该让程序崩掉，也不该递归记录错误。
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&sink.file)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// 一行日志的格式（纯函数，便于单测固定时间戳）。
fn format_line(timestamp: &str, level: Level, message: &str) -> String {
    let mut line = String::with_capacity(message.len() + timestamp.len() + 16);
    let _ = writeln!(
        line,
        "{timestamp} {:<5} {}",
        level.label(),
        message.trim_end()
    );
    line
}

/// 当前级别是否要记录 message：级别值越大越「吵」，大于门槛就丢弃。
fn allows(sink: Level, message: Level) -> bool {
    message <= sink
}

/// 读取日志最后 limit 行（给崩溃报告附现场用）。
pub fn recent_lines(limit: usize) -> Vec<String> {
    match log_file() {
        Some(path) => read_last_lines(&path, limit),
        None => Vec::new(),
    }
}

/// 读取文件最后 limit 行；只回看末尾 TAIL_WINDOW，不整份读入内存。
pub(crate) fn read_last_lines(path: &Path, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = file.metadata().map(|meta| meta.len()) else {
        return Vec::new();
    };
    let start = len.saturating_sub(TAIL_WINDOW);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<&str> = text.lines().collect();
    // 从文件中间开始读时，第一行大概率被截断，丢掉它。
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(limit);
    lines.into_iter().skip(skip).map(str::to_string).collect()
}

/// 删除目录中文件名匹配 prefix...suffix 且修改时间早于 keep 的文件。
///
/// 日志与崩溃报告共用这一套保留策略；返回删除数量。任何 IO 失败都静默跳过
/// ——清理过期文件失败不应该影响本次运行。
pub fn prune_matching(dir: &Path, prefix: &str, suffix: &str, keep: Duration) -> usize {
    prune_matching_except(dir, prefix, suffix, keep, None)
}

/// 同 [prune_matching]，但跳过文件名等于 keep_name 的条目。
///
/// 用于启动清理时保护「本次运行正在写的日志文件」：--log 还没开始写时
/// 文件可能已经存在且很旧，按保留策略会被误删。
pub fn prune_matching_except(
    dir: &Path,
    prefix: &str,
    suffix: &str,
    keep: Duration,
    keep_name: Option<&OsStr>,
) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name() else {
            continue;
        };
        if keep_name.is_some_and(|keep_name| name == keep_name) {
            continue;
        }
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if is_expired(now.duration_since(modified).ok(), keep) && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// 修改时间早于保留窗口才删除；时间戳在未来（时钟回拨）时保留。
fn is_expired(age: Option<Duration>, keep: Duration) -> bool {
    age.is_some_and(|age| age > keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_location_is_dated_tw_log_under_home_logs() {
        let home = Path::new("/tmp/tw-home");
        let location = resolve_location(home, None);
        assert_eq!(location.dir, Path::new("/tmp/tw-home/logs"));
        let name = location.file.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("tw-"), "{name}");
        assert!(name.ends_with(".log"), "{name}");
    }

    #[test]
    fn explicit_log_file_uses_its_parent_directory_for_reports() {
        let location = resolve_location(
            Path::new("/tmp/tw-home"),
            Some(Path::new("/tmp/custom/tw.log")),
        );
        assert_eq!(location.file, Path::new("/tmp/custom/tw.log"));
        assert_eq!(location.dir, Path::new("/tmp/custom"));

        let relative = resolve_location(Path::new("/tmp/tw-home"), Some(Path::new("tw.log")));
        assert_eq!(relative.dir, Path::new("."));
    }

    #[test]
    fn log_line_has_timestamp_level_and_message() {
        let line = format_line("2026-01-02 03:04:05.678", Level::Warn, "存档损坏");
        assert_eq!(line, "2026-01-02 03:04:05.678 WARN  存档损坏\n");
    }

    #[test]
    fn level_filter_drops_more_verbose_records() {
        assert!(allows(Level::Info, Level::Error));
        assert!(allows(Level::Info, Level::Info));
        assert!(!allows(Level::Info, Level::Debug));
        assert!(!allows(Level::Error, Level::Warn));
    }

    #[test]
    fn expiry_requires_a_known_age_beyond_the_window() {
        let keep = Duration::from_secs(7 * SECONDS_PER_DAY);
        assert!(is_expired(Some(keep + Duration::from_secs(1)), keep));
        assert!(!is_expired(Some(keep), keep));
        // 时钟回拨 / 读取不到修改时间时保留文件，宁可多留不可误删。
        assert!(!is_expired(None, keep));
    }

    #[test]
    fn read_last_lines_returns_only_the_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tw.log");
        let text: String = (0..100).map(|index| format!("line-{index}\n")).collect();
        fs::write(&path, text).unwrap();

        let tail = read_last_lines(&path, 3);
        assert_eq!(tail, vec!["line-97", "line-98", "line-99"]);
        assert!(read_last_lines(&path, 0).is_empty());
        assert!(read_last_lines(&dir.path().join("missing.log"), 3).is_empty());
    }

    #[test]
    fn pruning_removes_expired_matching_files_but_keeps_the_active_log() {
        let dir = tempfile::tempdir().unwrap();
        let active = dir.path().join("tw-active.log");
        let expired = dir.path().join("tw-expired.log");
        let foreign = dir.path().join("notes.txt");
        for path in [&active, &expired, &foreign] {
            fs::write(path, "x").unwrap();
        }

        // keep=0 让刚写的文件也算过期，从而覆盖真正的删除分支。
        let removed = prune_matching_except(
            dir.path(),
            LOG_PREFIX,
            LOG_SUFFIX,
            Duration::ZERO,
            active.file_name(),
        );
        assert_eq!(removed, 1, "只应删掉过期的 tw-*.log");
        assert!(active.is_file(), "本次运行正在写的日志不能被清理");
        assert!(!expired.exists(), "过期的 tw-*.log 应被删除");
        assert!(foreign.is_file(), "不匹配命名的文件不能碰");
    }

    #[test]
    fn log_level_parsing_accepts_aliases_case_and_spaces() {
        assert_eq!(Level::parse(" INFO "), Some(Level::Info));
        assert_eq!(Level::parse("Warning"), Some(Level::Warn));
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("ERROR"), Some(Level::Error));
        assert_eq!(Level::parse("trace"), None);
    }

    #[test]
    fn truncating_inexact_tail_is_safe() {
        // 构造超过 TAIL_WINDOW 的文件，验证只回看末尾、第一行被丢弃。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let filler = "x".repeat(TAIL_WINDOW as usize + 100);
        fs::write(&path, format!("{filler}\nlast-line\n")).unwrap();
        let tail = read_last_lines(&path, 10);
        assert_eq!(tail.last().map(String::as_str), Some("last-line"));
        assert!(tail.len() <= 10);
    }
}
