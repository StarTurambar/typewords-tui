//! `tw task` 的输出：预览会走完整的启动装载，因此也可能有写副作用。
//!
//! 词表比存档进度短时，装载会按校正后的进度丢弃单词断点（`practice-word.json`）。
//! 用户只是来「看一眼今日任务」，这条删除必须在输出里披露。

mod common;

use std::fs;

use typewords_tui::bootstrap;
use typewords_tui::cli;

/// 造「词表比存档进度短 + 存在断点」：`Dict::normalize_progress` 的校正分支。
fn fixture_with_stale_progress() -> common::Fixture {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let mut boot = bootstrap::load(&paths, None).expect("首次装载");
    boot.base
        .study_dict_mut()
        .expect("应选中测试词库")
        .last_learn_index = 99;
    boot.store.save_dict(&boot.base).expect("写词典存档");
    fs::write(
        paths.home.join("practice-word.json"),
        r#"{"val":null,"version":2}"#,
    )
    .expect("写练习缓存");
    fixture
}

#[test]
fn task_preview_discloses_the_discarded_practice_checkpoint() {
    let fixture = fixture_with_stale_progress();
    let paths = fixture.paths();
    let cache = paths.home.join("practice-word.json");
    assert!(cache.is_file(), "前置条件：断点存在");

    let mut out: Vec<u8> = Vec::new();
    cli::task_preview_to(&mut out, &paths, &fixture.cli()).expect("预览应当成功");
    let text = String::from_utf8(out).unwrap();

    assert!(!cache.exists(), "校正进度后断点应被丢弃");
    assert!(
        text.contains("丢弃上次的单词断点") && text.contains("practice-word.json"),
        "预览删了断点就必须在输出里说明: {text}"
    );
}

#[test]
fn task_preview_says_nothing_about_discarding_when_progress_is_healthy() {
    let fixture = common::Fixture::new();
    let cache = fixture.home().join("practice-word.json");
    fs::write(&cache, r#"{"val":null,"version":2}"#).expect("写练习缓存");

    let mut out: Vec<u8> = Vec::new();
    cli::task_preview_to(&mut out, &fixture.paths(), &fixture.cli()).expect("预览应当成功");
    let text = String::from_utf8(out).unwrap();

    assert!(cache.is_file(), "进度正常时断点不该被删");
    assert!(!text.contains("丢弃"), "没有删除就不该出现删除提示: {text}");
}
