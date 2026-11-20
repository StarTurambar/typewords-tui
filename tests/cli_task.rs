//! `tw task` 预览的输出：装载副作用与装载告警都必须说出来。

mod common;

use std::fs;

use typewords_tui::{bootstrap, cli};

fn preview(fixture: &common::Fixture) -> String {
    let mut out: Vec<u8> = Vec::new();
    cli::task_preview_to(&mut out, &fixture.paths(), &fixture.cli()).expect("预览应成功");
    String::from_utf8(out).unwrap()
}

/// 把当前学习词典的进度推到词表之外（等价于「词表被外部改短」）。
fn push_progress_past_the_word_list(fixture: &common::Fixture) {
    let paths = fixture.paths();
    let mut boot = bootstrap::load(&paths, None).expect("首次装载");
    let dict = boot.base.study_dict_mut().expect("应选中一本学习词典");
    dict.last_learn_index = 99;
    boot.store.save_dict(&boot.base).expect("写词典存档");
}

fn write_practice_cache(fixture: &common::Fixture) -> std::path::PathBuf {
    let path = fixture.home().join("practice-word.json");
    fs::write(&path, "{\"val\":null,\"version\":2}").expect("写练习缓存");
    path
}

/// 预览命令会按词表校准进度并删掉失效的断点缓存——删了就必须在输出里说明。
///
/// 这是预览唯一的写副作用（与 TUI 启动同一条装载路径）。不披露时用户只会看到
/// 「`practice-word.json` 不见了」，而命令看起来什么都没改。
#[test]
fn task_preview_reports_when_it_drops_a_stale_practice_cache() {
    let fixture = common::Fixture::new();
    push_progress_past_the_word_list(&fixture);
    let cache = write_practice_cache(&fixture);

    let text = preview(&fixture);

    assert!(!cache.exists(), "失效的断点缓存应被删除: {text}");
    assert!(
        text.contains("学习进度已按词表校准，失效的断点缓存已清除"),
        "预览必须说明缓存被删了: {text}"
    );
    assert!(text.contains("今日任务"), "预览正文应照常输出: {text}");
}

/// 进度正常时预览不删缓存，也不该打那句提示。
#[test]
fn task_preview_keeps_a_valid_practice_cache_without_a_hint() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let boot = bootstrap::load(&paths, None).expect("首次装载");
    boot.store.save_dict(&boot.base).expect("写词典存档");
    drop(boot);
    let cache = write_practice_cache(&fixture);

    let text = preview(&fixture);

    assert!(cache.exists(), "进度正常时不该删缓存: {text}");
    assert!(
        !text.contains("失效的断点缓存已清除"),
        "没删缓存就不该有这句提示: {text}"
    );
}

/// 装载期的告警（损坏存档被隔离、主题解析失败、词库目录项被跳过等）此前在预览里
/// 完全看不到：TUI 有 Toast，`tw task` 只有 stdout，吞掉就等于让用户去猜数字为什么变了。
#[test]
fn task_preview_prints_bootstrap_notices() {
    let fixture = common::Fixture::new();
    let themes = fixture.home().join("themes");
    fs::create_dir_all(&themes).expect("建主题目录");
    fs::write(themes.join("broken.json"), "{ not json").expect("写坏主题");

    let text = preview(&fixture);

    assert!(text.contains("提示:"), "装载告警应打印出来: {text}");
    assert!(text.contains("broken.json"), "告警要点名具体文件: {text}");
}
