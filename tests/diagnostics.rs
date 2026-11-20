//! 运行日志端到端：默认落点、--log / --no-log / --log-level、启动错误入日志。
//!
//! 日志是进程级全局状态，这个文件里的用例必须串行执行（同一个测试进程内
//! 并行跑会互相改掉当前日志文件）。

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::MutexGuard;

use typewords_tui::cli::Command;
use typewords_tui::logging::Level;

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn log_files(home: &Path) -> Vec<PathBuf> {
    let dir = home.join("logs");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("tw-") && name.ends_with(".log"))
        })
        .collect();
    files.sort();
    files
}

fn only_log_text(home: &Path) -> String {
    let files = log_files(home);
    assert_eq!(files.len(), 1, "应恰好有一份日志文件: {files:?}");
    fs::read_to_string(&files[0]).expect("应能读取日志")
}

#[test]
fn successful_command_writes_log_with_startup_context() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    let mut cli = fixture.cli();
    cli.command = Some(Command::Task);
    typewords_tui::run(cli).expect("task 应执行成功");

    let text = only_log_text(&fixture.home());
    assert!(text.contains("INFO"), "{text}");
    assert!(text.contains(&typewords_tui::version_line()), "{text}");
    assert!(
        text.contains(&format!("数据目录: {}", fixture.home().display())),
        "{text}"
    );
    assert!(text.contains("执行: task"), "{text}");
    assert!(text.contains("已获取数据目录锁"), "{text}");
    assert!(text.contains("进程退出: 正常"), "{text}");
}

#[test]
fn explicit_log_file_replaces_the_default_location() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    let custom = fixture.root.path().join("custom-logs").join("my-tw.log");
    let mut cli = fixture.cli();
    cli.command = Some(Command::Task);
    cli.log = Some(custom.clone());
    typewords_tui::run(cli).expect("task 应执行成功");

    let text = fs::read_to_string(&custom).expect("--log 指定的文件应存在");
    assert!(text.contains("执行: task"), "{text}");
    assert!(
        !fixture.home().join("logs").exists(),
        "指定 --log 后不应再创建默认日志目录"
    );
}

#[test]
fn no_log_skips_logging_entirely() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    let mut cli = fixture.cli();
    cli.command = Some(Command::Task);
    cli.no_log = true;
    typewords_tui::run(cli).expect("task 应执行成功");

    assert!(
        log_files(&fixture.home()).is_empty(),
        "--no-log 不应创建日志"
    );
    assert!(!fixture.home().join("logs").exists());
}

#[test]
fn log_level_error_filters_info_records() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    let mut cli = fixture.cli();
    cli.command = Some(Command::Task);
    cli.log_level = Level::Error;
    typewords_tui::run(cli).expect("task 应执行成功");

    let text = only_log_text(&fixture.home());
    assert!(
        text.trim().is_empty(),
        "级别为 error 时成功的 task 不该写任何记录: {text:?}"
    );
}

#[test]
fn startup_error_is_logged_even_when_the_command_fails() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    // 父进程先占住数据目录，子命令会在 dispatch 之前被拒绝；这条错误必须
    // 留在日志里（用户双击启动第二个实例时最常遇到）。
    let lock = fixture.paths().lock_home().expect("父进程应先拿到锁");
    let mut cli = fixture.cli();
    cli.command = Some(Command::Task);
    let err = typewords_tui::run(cli).expect_err("持锁期间必须报错");
    assert!(
        err.to_string().contains("另一个实例正在使用该数据目录"),
        "错误信息应指向数据目录锁: {err}"
    );
    drop(lock);

    let text = only_log_text(&fixture.home());
    assert!(text.contains("ERROR"), "启动错误必须写进日志: {text}");
    assert!(
        text.contains("另一个实例正在使用该数据目录"),
        "日志应包含错误原文: {text}"
    );
    assert!(text.contains("进程退出: 失败"), "{text}");
}

#[test]
fn doctor_reports_the_log_directory() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    let mut out: Vec<u8> = Vec::new();
    typewords_tui::cli::doctor_to(&mut out, &fixture.paths()).expect("doctor 应成功");
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains(&format!(
            "日志目录: {}",
            fixture.home().join("logs").display()
        )),
        "doctor 应打印日志目录: {text}"
    );
}
/// 实际使用的随机种子必须进日志：帮助页让报障时「附上随机种子」，但没传
/// `--seed` 时种子是启动时随机生成的——不记下来，那次洗牌 / 抽题顺序就永远
/// 无法复现，这条建议也就无法执行。
#[test]
fn app_bootstrap_logs_the_random_seed_it_actually_used() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    let log_file = typewords_tui::logging::init(
        &fixture.home(),
        typewords_tui::logging::Options {
            file: None,
            level: Level::Info,
            disabled: false,
        },
    )
    .expect("临时数据目录应能写日志");
    let mut cli = fixture.cli();
    // 缺省随机：正是需要落日志的场景（固定种子的运行看参数就够了）。
    cli.seed = None;
    let app = typewords_tui::app::build_for_test(fixture.paths(), cli).expect("启动应成功");

    let text = fs::read_to_string(&log_file).expect("应能读取日志");
    assert!(text.contains(&format!("随机种子: {}", app.seed)), "{text}");
    assert!(
        text.contains(&format!("--seed {}", app.seed)),
        "日志要给出可直接复制的参数: {text}"
    );
}

/// 传了 `--seed` 时记同一个值，并标明来源（省得用户以为这是随机得到的）。
#[test]
fn app_bootstrap_logs_an_explicit_seed_as_such() {
    let _guard = serial();
    let fixture = common::Fixture::new();
    let log_file = typewords_tui::logging::init(
        &fixture.home(),
        typewords_tui::logging::Options {
            file: None,
            level: Level::Info,
            disabled: false,
        },
    )
    .expect("临时数据目录应能写日志");
    let mut cli = fixture.cli();
    cli.seed = Some(1234);
    typewords_tui::app::build_for_test(fixture.paths(), cli).expect("启动应成功");

    let text = fs::read_to_string(&log_file).expect("应能读取日志");
    assert!(text.contains("随机种子: 1234（来自 --seed"), "{text}");
}
