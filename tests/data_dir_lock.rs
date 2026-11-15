//! 数据目录互斥：第二个实例必须被拒绝，只读入口不受影响。
//!
//! TUI 主循环要真终端，没法在测试里跑；但取锁发生在命令分发之前
//! （`lib::run`），所以用同样会写数据目录的一次性子命令来验证接线。

mod common;

use typewords_tui::cli::{Cli, Command};
use typewords_tui::fs::LOCK_FILE_NAME;

fn cli(fixture: &common::Fixture, command: Command) -> Cli {
    let mut cli = fixture.cli();
    cli.command = Some(command);
    cli
}

fn toy_dict(fixture: &common::Fixture) -> std::path::PathBuf {
    fixture
        .assets()
        .join("dicts")
        .join("en")
        .join("word")
        .join("toy.json")
}

#[test]
fn second_instance_is_refused_while_data_dir_is_locked() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let lock = paths.lock_home().expect("第一次取锁应成功");

    for command in [
        Command::Task,
        Command::ImportDict {
            file: toy_dict(&fixture),
        },
    ] {
        let kind = format!("{command:?}");
        let err =
            typewords_tui::run(cli(&fixture, command)).expect_err("持锁期间第二个实例必须被拒绝");
        assert!(
            err.to_string().contains("另一个实例正在使用该数据目录"),
            "{kind} 的报错应说明数据目录被占用: {err}"
        );
    }

    // 释放后同样的命令可以正常执行。
    drop(lock);
    typewords_tui::run(cli(&fixture, Command::Task)).expect("释放锁后应能正常执行");
}

#[test]
fn another_process_is_refused_while_data_dir_is_locked() {
    // 真正的跨进程验证：同一进程里两次 try_lock 只能证明系统调用没被复用，
    // 这里让一个真实子进程去抢锁，覆盖「两个 tw 同时启动」的原始场景。
    let fixture = common::Fixture::new();
    let _lock = fixture.paths().lock_home().expect("父进程先持锁");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tw"))
        .arg("task")
        .arg("--home")
        .arg(fixture.home())
        .arg("--assets")
        .arg(fixture.assets())
        .output()
        .expect("应能启动 tw 子进程");

    assert!(
        !output.status.success(),
        "持锁期间子进程必须失败退出，实际却成功了"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("另一个实例正在使用该数据目录"),
        "子进程 stderr 应说明数据目录被占用，实际: {stderr}"
    );
}

#[test]
fn read_only_commands_work_while_data_dir_is_locked() {
    let fixture = common::Fixture::new();
    let lock = fixture.paths().lock_home().unwrap();

    // doctor 只读：TUI 开着也能用它排障。
    typewords_tui::run(cli(&fixture, Command::Doctor)).expect("doctor 不应被数据目录锁拒绝");

    // export-web 只读数据目录，只写 --out 指定的文件。
    let out = fixture.root.path().join("backup.json");
    typewords_tui::run(cli(
        &fixture,
        Command::ExportWeb {
            out: Some(out.clone()),
        },
    ))
    .expect("export-web 不应被数据目录锁拒绝");
    assert!(out.is_file());

    // export-dict 同理：TUI 正在跑的时候照样能把词库倒出来。
    let words = fixture.root.path().join("toy-export.json");
    typewords_tui::run(cli(
        &fixture,
        Command::ExportDict {
            id: Some("toy".to_string()),
            out: Some(words.clone()),
        },
    ))
    .expect("export-dict 不应被数据目录锁拒绝");
    assert!(words.is_file());

    drop(lock);
}

#[test]
fn startup_takes_lock_file_and_releases_it_on_exit() {
    let fixture = common::Fixture::new();
    let lock_file = fixture.paths().home.join(LOCK_FILE_NAME);
    assert!(!lock_file.exists(), "启动前不应有锁文件");

    typewords_tui::run(cli(&fixture, Command::Task)).expect("首次启动应成功");
    assert!(lock_file.is_file(), "启动时应创建 <home>/.tw.lock");

    // 上一次已退出（守卫随 run 返回释放），再启动一次不会被自己的锁拦住。
    typewords_tui::run(cli(&fixture, Command::Task)).expect("上次退出后锁必须已释放");
}
