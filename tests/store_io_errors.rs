//! 读不到 ≠ 内容坏了：IO 读错误不隔离、不删除、不写回默认值。
//!
//! 跨平台的确定性 IO 错误：把存档路径做成目录——`read_to_string` 在 Unix
//! 得到 `EISDIR`、在 Windows 得到拒绝访问，两边都走 IO 分支。不用
//! `chmod 000`：Windows CI 无效，root 容器里也无效。

mod common;

use std::fs;
use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typewords_tui::{app, bootstrap};

fn press(app: &mut app::App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

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

/// `--reset-practice` 也不能删读不出来的缓存，更不能谎报「已清空」。
#[test]
fn reset_practice_keeps_an_unreadable_cache_and_says_it_did_not_clear() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("practice-word.json");
    fs::create_dir(&path).expect("把 practice-word.json 做成目录");
    let mut cli = fixture.cli();
    cli.reset_practice = true;

    let app = app::build_for_test(fixture.paths(), cli).expect("缓存读不出来不该阻断启动");
    assert!(path.is_dir(), "--reset-practice 不能删掉读不出来的缓存");
    assert!(!app.has_saved_practice(), "读不出来的缓存不算「上次任务」");

    let messages = toast_messages(&app);
    assert!(
        messages.iter().any(|message| message.contains("未清空")),
        "没删掉要提示「未清空」: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("已清空练习进度")),
        "没删掉不能报「已清空」: {messages:?}"
    );
}

/// 文章跟打的保存入口同样要跳过读不出来的缓存。
#[test]
fn saving_article_progress_skips_an_unreadable_cache() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("practice-article.json");
    fs::create_dir(&path).expect("把 practice-article.json 做成目录");

    let mut app =
        app::build_for_test(fixture.paths(), fixture.cli()).expect("缓存读不出来不该阻断启动");
    app.open_articles();
    app.start_article_practice(0);
    app.save_article_practice_now();

    assert!(path.is_dir(), "保存入口不能改写读不出来的缓存");
    let messages = toast_messages(&app);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("practice-article.json")
                && message.contains("本次运行不会写入")),
        "跳过保存要提示: {messages:?}"
    );
}

/// 流程存档的保存入口同样要跳过读不出来的文件。
#[test]
fn saving_flows_skips_an_unreadable_store() {
    let fixture = common::Fixture::new();
    let path = fixture.home().join("flows.json");
    fs::create_dir(&path).expect("把 flows.json 做成目录");

    let mut app =
        app::build_for_test(fixture.paths(), fixture.cli()).expect("流程读不出来不该阻断启动");
    app.open_flows();
    // 新建副本会立刻写回 flows.json；读不出来时必须跳过。
    press(&mut app, KeyCode::Char('n'));

    assert!(path.is_dir(), "保存入口不能改写读不出来的流程存档");
    let messages = toast_messages(&app);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("flows.json") && message.contains("本次运行不会写入")),
        "跳过保存要提示: {messages:?}"
    );
}

/// 「读不出来，本次不会写入」按文件各提示一次：两份都读不出来时不能只报第一份，
/// 同一份重复跳过也不能刷屏。
#[test]
fn each_unreadable_save_is_reported_once() {
    let fixture = common::Fixture::new();
    fs::create_dir(fixture.home().join("practice-word.json")).expect("把单词缓存做成目录");
    fs::create_dir(fixture.home().join("practice-article.json")).expect("把文章缓存做成目录");

    let mut app =
        app::build_for_test(fixture.paths(), fixture.cli()).expect("缓存读不出来不该阻断启动");
    app.start_practice();
    app.save_practice_now();
    app.save_article_practice_now();

    let messages = toast_messages(&app);
    let notices: Vec<&String> = messages
        .iter()
        .filter(|message| message.contains("本次运行不会写入"))
        .collect();
    assert_eq!(
        notices.len(),
        2,
        "两份缓存各提示一次，重复保存不再提示: {notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|message| message.contains("practice-word.json")),
        "缺少单词缓存的提示: {notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|message| message.contains("practice-article.json")),
        "缺少文章缓存的提示: {notices:?}"
    );
}

/// 词表变短触发进度校正时，读不出来的缓存不能被顺手删掉。
///
/// 用 `chmod 000` 制造「是普通文件但读不出来」：路径做成目录的写法在这里
/// 没用——`remove_practice` 本来就不会删目录，删不掉与「保护生效」分不开。
#[cfg(unix)]
#[test]
fn progress_repair_does_not_delete_an_unreadable_cache() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    // 先正常装载一次拿到带词表的 dict.json，再把进度改到词表之外。
    let mut boot = bootstrap::load(&paths, None).expect("首次装载");
    let dict = boot.base.study_dict_mut().expect("应选中一本学习词典");
    dict.last_learn_index = 99;
    boot.store.save_dict(&boot.base).expect("写词典存档");

    let cache = paths.home.join("practice-word.json");
    fs::write(&cache, "{\"val\":null,\"version\":2}").expect("写练习缓存");
    fs::set_permissions(&cache, fs::Permissions::from_mode(0o000)).expect("chmod 000");

    let _ = bootstrap::load(&paths, None).expect("装载不该失败");
    // root 无视权限位，这时用例没有可验证的东西（与 fs.rs 的同类用例同款前提）。
    if fs::read(&cache).is_ok() {
        return;
    }
    assert!(cache.is_file(), "校正进度时不能删掉读不出来的缓存");
}
