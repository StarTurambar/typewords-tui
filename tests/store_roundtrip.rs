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
