//! 存档读写、版本校验与损坏隔离。

mod common;

use typewords_tui::domain::types::{BaseState, SettingState};
use typewords_tui::store::{FileStore, LoadStatus};

#[test]
fn dict_roundtrip() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    let mut base = BaseState::default();
    base.simple_words.push("hello".to_string());
    base.word.study_index = 0;
    store.save_dict(&base).unwrap();

    let loaded = store.load_dict();
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert!(loaded.value.simple_words.contains(&"hello".to_string()));
    assert_eq!(loaded.value.word.study_index, 0);
}

/// 存档带只读属性（手工防误删、保留属性的备份还原、`robocopy /MIR`）时，
/// 保存仍要真正落到磁盘：Windows 的 rename 拒绝覆盖只读目标，兜底在
/// `fs::write_atomic` 里，这一层从 `FileStore` 入口验证接线。
#[cfg(windows)]
#[test]
fn saving_a_readonly_dict_still_reaches_disk() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    let mut base = BaseState::default();
    store.save_dict(&base).unwrap();

    let path = store.file_path("dict.json");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();

    base.word.study_index = 3;
    store.save_dict(&base).expect("只读存档不应让保存失败");

    let loaded = store.load_dict();
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert_eq!(loaded.value.word.study_index, 3);
    assert!(
        !std::fs::metadata(&path).unwrap().permissions().readonly(),
        "保存后的存档不应仍带只读属性"
    );
}

#[test]
fn settings_roundtrip_preserves_unknown_fields() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    std::fs::write(
        store.file_path("setting.json"),
        r#"{"version":23,"updated_at":"2026-01-01T00:00:00Z","val":{"wordReviewRatio":5,"someFutureField":true}}"#,
    )
    .unwrap();

    let loaded = store.load_settings();
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert_eq!(loaded.value.word_review_ratio, 5);
    assert!(loaded.value.extra.contains_key("someFutureField"));

    store.save_settings(&loaded.value).unwrap();
    let raw = std::fs::read_to_string(store.file_path("setting.json")).unwrap();
    assert!(raw.contains("someFutureField"));
}

#[test]
fn corrupt_dict_is_quarantined() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    std::fs::write(store.file_path("dict.json"), "{ not json").unwrap();

    let loaded = store.load_dict();
    assert!(matches!(loaded.status, LoadStatus::Corrupt(_)));
    assert_eq!(loaded.value.word.book_list.len(), 3);

    let quarantined: Vec<_> = std::fs::read_dir(fixture.home())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
        .collect();
    assert_eq!(quarantined.len(), 1, "损坏文件应被隔离保留");
}

#[test]
fn newer_dict_version_is_not_overwritten() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    std::fs::write(store.file_path("dict.json"), r#"{"version":99,"val":{}}"#).unwrap();

    let loaded = store.load_dict();
    assert!(matches!(
        loaded.status,
        LoadStatus::VersionTooNew {
            found: 99,
            expected: 4
        }
    ));
    // 文件仍在，内容未被改写
    let raw = std::fs::read_to_string(store.file_path("dict.json")).unwrap();
    assert!(raw.contains("99"));
}

/// 高版本存档的 `val` 结构可能已经变了。版本判定必须先于载荷解析：
/// 否则这里会走「损坏」分支隔离原文件，退出时再用默认值覆盖活跃路径。
#[test]
fn newer_dict_version_with_incompatible_payload_is_not_quarantined() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    let raw = r#"{"version":99,"val":{"word":"not-an-object"}}"#;
    std::fs::write(store.file_path("dict.json"), raw).unwrap();

    let loaded = store.load_dict();
    assert!(matches!(
        loaded.status,
        LoadStatus::VersionTooNew {
            found: 99,
            expected: 4
        }
    ));
    assert_eq!(
        std::fs::read_to_string(store.file_path("dict.json")).unwrap(),
        raw,
        "版本过高时原文件必须原地保留"
    );
    let quarantined: Vec<_> = std::fs::read_dir(fixture.home())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
        .collect();
    assert!(quarantined.is_empty(), "版本过高不应触发隔离");
}

/// `updated_at` 也是会演进的头字段：类型变了不能把高版本存档判成损坏。
#[test]
fn newer_dict_version_with_numeric_timestamp_is_not_quarantined() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    let raw = r#"{"version":99,"updated_at":1760000000000,"val":{"word":"not-an-object"}}"#;
    std::fs::write(store.file_path("dict.json"), raw).unwrap();

    let loaded = store.load_dict();
    assert!(
        matches!(
            loaded.status,
            LoadStatus::VersionTooNew {
                found: 99,
                expected: 4
            }
        ),
        "数字 updated_at 不应影响版本判定，实际: {:?}",
        loaded.status
    );
    assert_eq!(
        std::fs::read_to_string(store.file_path("dict.json")).unwrap(),
        raw,
        "版本过高时原文件必须原地保留"
    );
    let quarantined: Vec<_> = std::fs::read_dir(fixture.home())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
        .collect();
    assert!(quarantined.is_empty(), "版本过高不应触发隔离");
}

#[test]
fn settings_default_when_missing() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    let loaded = store.load_settings();
    assert_eq!(loaded.status, LoadStatus::Missing);
    assert_eq!(loaded.value, SettingState::default());
}

#[test]
fn practice_cache_remove_and_exists() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    assert!(!store.practice_exists());
    store.save_practice(Some(&Default::default())).unwrap();
    assert!(store.practice_exists());
    assert!(store.remove_practice().unwrap(), "存在时应真的删掉");
    assert!(!store.practice_exists());
    assert!(!store.remove_practice().unwrap(), "文件不存在时返回 false");
}

#[test]
fn practice_cache_unsupported_version_is_ignored_without_moving_file() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    std::fs::write(
        store.file_path("practice-word.json"),
        r#"{"version":1,"val":null}"#,
    )
    .unwrap();

    let loaded = store.load_practice();
    assert!(matches!(
        loaded.status,
        LoadStatus::UnsupportedVersion {
            found: 1,
            expected: 2
        }
    ));
    assert!(loaded.value.is_none());
    let notice = loaded.notice().expect("应有提示");
    assert!(notice.contains("不受支持"));
    assert!(store.practice_exists(), "不受支持的缓存不应被移动或删除");
}

/// tw doctor 的存档清单来自 FileStore::describe_files：数据目录里写过哪份
/// 存档，清单里就要有哪份，否则排障时看不到对应断点（例如文章跟打缓存）。
#[test]
fn doctor_lists_every_save_file_including_article_cache() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    store.save_dict(&BaseState::default()).unwrap();
    store.save_settings(&SettingState::default()).unwrap();
    store.save_practice(Some(&Default::default())).unwrap();
    store
        .save_practice_article(Some(&Default::default()))
        .unwrap();
    store.save_flows(&Default::default()).unwrap();

    let described: Vec<String> = store
        .describe_files()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        described.iter().any(|name| name == "practice-article.json"),
        "tw doctor 必须列出文章断点缓存，实际清单: {described:?}"
    );
    let missing: Vec<String> = store
        .describe_files()
        .into_iter()
        .filter(|(_, size)| size.is_none())
        .map(|(name, _)| name)
        .collect();
    assert!(
        missing.is_empty(),
        "已写出的存档不应显示成「不存在」: {missing:?}"
    );

    let mut written: Vec<String> = std::fs::read_dir(fixture.home())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".json"))
        .collect();
    written.sort();
    let mut described_sorted = described;
    described_sorted.sort();
    assert_eq!(
        described_sorted, written,
        "tw doctor 的存档清单与数据目录里实际写出的存档不一致"
    );
}

fn corrupt_files(home: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(home)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".corrupt-"))
        .collect()
}

/// Windows 记事本等工具会以「带 BOM 的 UTF-8」保存 JSON：读取要容忍开头的
/// 一个 U+FEFF，不能把好档判成损坏隔离。
#[test]
fn a_dict_saved_with_a_utf8_bom_is_loaded_without_quarantine() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    let raw = format!(
        "\u{feff}{}",
        r#"{"version":4,"val":{"simpleWords":["bom"]}}"#
    );
    std::fs::write(store.file_path("dict.json"), raw).unwrap();

    let loaded = store.load_dict();
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert!(loaded.value.simple_words.contains(&"bom".to_string()));
    assert!(
        corrupt_files(&fixture.home()).is_empty(),
        "带 BOM 的好档不能被隔离: {:?}",
        corrupt_files(&fixture.home())
    );
}

/// `flows.json` 有自己的读路径（裸结构、无版本头），BOM 同样要容忍。
#[test]
fn a_flow_store_saved_with_a_utf8_bom_is_loaded() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    let raw = format!(
        "\u{feff}{}",
        r#"{"activeId":"","flows":{"demo":{"name":"演示"}}}"#
    );
    std::fs::write(store.file_path("flows.json"), raw).unwrap();

    let loaded = store.load_flows();
    assert_eq!(loaded.status, LoadStatus::Loaded);
    assert!(loaded.value.flows.contains_key("demo"));
    assert!(corrupt_files(&fixture.home()).is_empty());
}

/// 隔离提示要带「哪份缓存、为什么丢、丢到哪」——只给一句固定文案等于把
/// 排障线索全丢了。
#[test]
fn a_corrupt_practice_cache_reports_the_reason_and_the_quarantine_path() {
    let fixture = common::Fixture::new();
    let store = FileStore::new(fixture.home()).unwrap();
    std::fs::write(
        store.file_path("practice-word.json"),
        r#"{"version":2,"val":{"practiceData":"not-an-object"}}"#,
    )
    .unwrap();

    let loaded = store.load_practice();
    let LoadStatus::Corrupt(reason) = &loaded.status else {
        panic!("坏缓存应被隔离，实际: {:?}", loaded.status);
    };
    assert!(reason.contains("练习缓存损坏，已丢弃"), "{reason}");
    assert!(reason.contains("已移动到"), "提示要含隔离结果: {reason}");
    assert!(reason.contains(".corrupt-"), "提示要含隔离路径: {reason}");
    assert!(loaded.value.is_none());
    let notice = loaded.notice().expect("应有提示");
    assert!(notice.contains("已移动到"), "{notice}");
}
