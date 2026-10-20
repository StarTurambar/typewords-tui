//! 网页版备份（BackupData v5）的导入导出互通。

mod common;

use std::path::Path;
use typewords_tui::backup;
use typewords_tui::store::FileStore;

fn open_store(home: &Path) -> FileStore {
    FileStore::new(home.to_path_buf()).expect("数据目录")
}

#[test]
fn export_web_produces_the_upstream_backup_shape() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let text = backup::export_web(&store).expect("导出");
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(value["version"], 5, "备份版本应为 5");
    let val = &value["val"];
    assert!(val["dict"].is_object(), "应包含 dict");
    assert!(val["setting"].is_object(), "应包含 setting");
    assert!(
        val["PracticeSaveWord"].is_object(),
        "应包含 PracticeSaveWord"
    );
    assert!(
        val["PracticeSaveArticle"].is_object(),
        "应包含 PracticeSaveArticle 占位"
    );
}

#[test]
fn import_web_writes_the_three_save_files() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());

    // 先造一份带数据的存档
    let mut dict = typewords_tui::domain::types::BaseState::default();
    dict.word
        .book_list
        .push(typewords_tui::domain::types::Dict {
            id: "test".to_string(),
            name: "测试词库".to_string(),
            length: 3,
            ..Default::default()
        });
    store.save_dict(&dict).expect("写 dict");
    store
        .save_settings(&typewords_tui::domain::types::SettingState::default())
        .expect("写 setting");

    let text = backup::export_web(&store).expect("导出");
    let backup_path = fixture.home().join("web-backup.json");
    std::fs::write(&backup_path, text).expect("写备份");

    // 导入到另一个数据目录
    let target = fixture.root.path().join("restored");
    let target_store = open_store(&target);
    let report = backup::import_web(&target_store, &backup_path).expect("导入");
    assert_eq!(report.written.len(), 3, "应写入 dict、setting 与练习缓存");
    assert!(
        report
            .discarded
            .iter()
            .any(|item| item.contains("PracticeSaveArticle")),
        "文章缓存应被报告为丢弃"
    );

    let loaded = target_store.load_dict();
    assert!(
        loaded
            .value
            .word
            .book_list
            .iter()
            .any(|dict| dict.name == "测试词库"),
        "导入的词典应被还原"
    );
}

#[test]
fn import_web_rejects_a_newer_backup_version() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("future.json");
    std::fs::write(&path, r#"{"version": 99, "val": {}}"#).expect("写文件");
    let error = backup::import_web(&store, &path).expect_err("应拒绝导入");
    assert!(
        error.to_string().contains("99"),
        "错误信息应说明版本号: {error}"
    );
}

#[test]
fn import_web_skips_entries_with_a_newer_version() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("mixed.json");
    std::fs::write(
        &path,
        r#"{"version": 5, "val": {
            "dict": {"version": 4, "val": {"word": {"bookList": [], "studyIndex": -1}, "simpleWords": [], "load": false, "article": {"bookList": [], "studyIndex": -1}, "dictListVersion": 1, "fsrsData": {}, "noteData": {}, "_ignoreWatch": false}},
            "setting": {"version": 99, "val": {}}
        }}"#,
    )
    .expect("写文件");
    let report = backup::import_web(&store, &path).expect("导入");
    assert_eq!(report.written, vec!["dict".to_string()]);
    assert!(
        report.skipped.iter().any(|item| item.contains("setting")),
        "高版本的 setting 应被跳过"
    );
}
