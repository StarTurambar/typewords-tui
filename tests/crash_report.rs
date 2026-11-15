//! 崩溃报告端到端：安装钩子 -> 真 panic -> crash-<时间戳>.txt 落盘。
//!
//! 单独一个测试二进制：panic 钩子是进程级全局状态，和别的用例并行会互相干扰。

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};

use typewords_tui::logging::{self, Level, Options};

#[test]
fn panic_writes_crash_report_with_time_version_paths_and_backtrace() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let assets = root.path().join("assets");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&assets).unwrap();

    let log_file = logging::init(
        &home,
        Options {
            file: None,
            level: Level::Info,
            disabled: false,
        },
    )
    .expect("临时数据目录应能写日志");
    logging::info("崩溃报告测试：准备触发 panic");
    let report_dir = home.join("logs");
    typewords_tui::crash::set_context(&home, Some(&assets), Some(&log_file), &report_dir);

    // 测试进程不要因为「按回车」等待而挂住；真实程序默认开启。
    typewords_tui::crash::set_enter_wait_enabled(false);
    let previous_hook = std::panic::take_hook();
    typewords_tui::crash::install_hook();
    let outcome = catch_unwind(AssertUnwindSafe(|| panic!("崩溃报告测试 panic")));
    std::panic::set_hook(previous_hook);
    assert!(outcome.is_err(), "catch_unwind 应接住 panic");

    let reports: Vec<_> = fs::read_dir(&report_dir)
        .expect("崩溃报告目录应存在")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("crash-") && name.ends_with(".txt"))
        })
        .collect();
    assert_eq!(reports.len(), 1, "应恰好写一份崩溃报告: {reports:?}");

    let text = fs::read_to_string(&reports[0]).expect("应能读取崩溃报告");
    assert!(text.contains("typewords-tui 崩溃报告"), "{text}");
    assert!(text.contains(&typewords_tui::version_line()), "{text}");
    assert!(text.contains("生成时间: "), "{text}");
    assert!(
        text.contains(&format!("数据目录: {}", home.display())),
        "{text}"
    );
    assert!(
        text.contains(&format!("资源目录: {}", assets.display())),
        "{text}"
    );
    assert!(!text.contains("数据目录: （未解析）"), "{text}");
    assert!(text.contains("panic 消息: 崩溃报告测试 panic"), "{text}");
    assert!(text.contains("panic 位置: "), "{text}");
    assert!(text.contains("回溯:"), "{text}");
    assert!(
        !text.contains("（未捕获到回溯）"),
        "force_capture 应总能拿到回溯帧: {text}"
    );
    assert!(text.contains("最近日志"), "{text}");
    assert!(
        text.contains("崩溃报告测试：准备触发 panic"),
        "报告应附带最近日志: {text}"
    );

    let log_text = fs::read_to_string(&log_file).expect("日志应存在");
    assert!(
        log_text.contains("ERROR 崩溃: 崩溃报告测试 panic"),
        "日志必须记录崩溃: {log_text}"
    );
    assert!(
        log_text.contains(&format!("崩溃报告: {}", reports[0].display())),
        "日志必须记录报告路径: {log_text}"
    );
}
