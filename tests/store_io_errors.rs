//! 读不到 ≠ 内容坏了：IO 读错误不隔离、不删除、不写回默认值。
//!
//! 跨平台的确定性 IO 错误：把存档路径做成目录——`read_to_string` 在 Unix
//! 得到 `EISDIR`、在 Windows 得到拒绝访问，两边都走 IO 分支。不用
//! `chmod 000`：Windows CI 无效，root 容器里也无效。

mod common;

use std::fs;
use std::path::Path;

use typewords_tui::{app, bootstrap};

fn corrupt_files(home: &Path) -> Vec<String> {
    fs::read_dir(home)
        .expect("读取数据目录")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".corrupt-"))
        .collect()
}

fn toast_messages(app: &app::App) -> Vec<String> {
    app.toasts
        .iter()
        .map(|toast| toast.message.clone())
        .collect()
}

#[test]
fn a_dict_directory_aborts_startup_without_quarantine() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("dict.json");
    fs::create_dir(&path).expect("把 dict.json 做成目录");

    let err = bootstrap::load(&fixture.paths(), None)
        .err()
        .expect("dict.json 读不出来应中止启动");
    let message = format!("{err:#}");
    assert!(message.contains("dict.json"), "文案要含路径: {message}");
    assert!(
        message.contains("关闭占用"),
        "文案要提示关闭占用后重试: {message}"
    );
    assert!(path.is_dir(), "原目录必须原样保留");
    assert!(
        corrupt_files(&fixture.home()).is_empty(),
        "读不到不能隔离: {:?}",
        corrupt_files(&fixture.home())
    );
}

#[test]
fn a_settings_directory_aborts_startup_without_quarantine() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("setting.json");
    fs::create_dir(&path).expect("把 setting.json 做成目录");

    let err = bootstrap::load(&fixture.paths(), None)
        .err()
        .expect("setting.json 读不出来应中止启动");
    let message = format!("{err:#}");
    assert!(message.contains("setting.json"), "文案要含路径: {message}");
    assert!(
        message.contains("关闭占用"),
        "文案要提示关闭占用后重试: {message}"
    );
    assert!(path.is_dir(), "原目录必须原样保留");
    assert!(
        corrupt_files(&fixture.home()).is_empty(),
        "读不到不能隔离: {:?}",
        corrupt_files(&fixture.home())
    );
}

#[test]
fn an_unreadable_practice_cache_is_read_only_and_not_quarantined() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("practice-word.json");
    fs::create_dir(&path).expect("把 practice-word.json 做成目录");

    let mut app =
        app::build_for_test(fixture.paths(), fixture.cli()).expect("缓存读不出来不该阻断启动");
    assert!(app.fatal.is_none(), "缓存读不出来不是致命错误");
    assert!(
        !app.has_saved_practice(),
        "读不出来的缓存不能算「上次任务」"
    );
    assert!(path.is_dir(), "读不到的缓存不能被删除或改写");
    assert!(
        corrupt_files(&fixture.home()).is_empty(),
        "读不到不能隔离: {:?}",
        corrupt_files(&fixture.home())
    );

    let messages = toast_messages(&app);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("practice-word.json") && message.contains("同步盘")),
        "启动提示要含路径与占用提示: {messages:?}"
    );

    // 开一轮练习会尝试保存断点：读不出来的文件必须跳过，并提示一次。
    app.start_practice();
    assert!(path.is_dir(), "保存入口不能改写读不出来的缓存");
    assert!(
        corrupt_files(&fixture.home()).is_empty(),
        "保存入口也不能隔离"
    );
    let messages = toast_messages(&app);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("本次运行不会写入")),
        "跳过保存要提示: {messages:?}"
    );
}

#[test]
fn an_unreadable_article_cache_is_not_reported_as_resumable() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("practice-article.json");
    fs::create_dir(&path).expect("把 practice-article.json 做成目录");

    let app =
        app::build_for_test(fixture.paths(), fixture.cli()).expect("缓存读不出来不该阻断启动");
    assert!(
        !app.has_saved_article_practice(),
        "读不出来的文章缓存不能谎称可恢复"
    );
    assert!(path.is_dir(), "读不到的缓存不能被删除或改写");
    assert!(
        corrupt_files(&fixture.home()).is_empty(),
        "读不到不能隔离: {:?}",
        corrupt_files(&fixture.home())
    );
    let messages = toast_messages(&app);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("practice-article.json") && message.contains("同步盘")),
        "启动提示要含路径与占用提示: {messages:?}"
    );
}

#[test]
fn an_unreadable_flow_store_is_not_quarantined() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("flows.json");
    fs::create_dir(&path).expect("把 flows.json 做成目录");

    let app =
        app::build_for_test(fixture.paths(), fixture.cli()).expect("流程读不出来不该阻断启动");
    assert!(app.flow_store.flows.is_empty(), "读不出来时按空集合运行");
    assert!(path.is_dir(), "读不到的流程存档不能被删除或改写");
    assert!(
        corrupt_files(&fixture.home()).is_empty(),
        "读不到不能隔离: {:?}",
        corrupt_files(&fixture.home())
    );
    let messages = toast_messages(&app);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("flows.json") && message.contains("同步盘")),
        "启动提示要含路径与占用提示: {messages:?}"
    );
}
