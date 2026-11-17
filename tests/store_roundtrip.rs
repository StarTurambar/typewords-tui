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
    store.remove_practice().unwrap();
    assert!(!store.practice_exists());
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
