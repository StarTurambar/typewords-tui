//! 网页版备份（BackupData v5）的导入导出互通。

mod common;

use std::path::Path;

use typewords_tui::backup;
use typewords_tui::domain::article::{ArticleCache, ArticleCacheEnvelope};
use typewords_tui::store::FileStore;

fn open_store(home: &Path) -> FileStore {
    FileStore::new(home.to_path_buf()).expect("数据目录")
}

fn article_cache() -> ArticleCache {
    ArticleCache {
        article_id: 11,
        sentence_index: 3,
        finished: 3,
        wrong_sentences: 1,
        total: 72,
        spend: 45_000,
    }
}

fn save_article_cache(store: &FileStore) {
    store
        .save_practice_article(Some(&ArticleCacheEnvelope {
            practice_data: article_cache(),
            stat_store_data: None,
        }))
        .expect("写文章缓存");
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
    assert_eq!(
        val["PracticeSaveArticle"]["version"], 1,
        "没有文章断点时应写 v1 空信封"
    );
    assert_eq!(
        val["PracticeSaveArticle"]["val"],
        serde_json::json!({}),
        "空信封的 val 应为空对象"
    );
}

#[test]
fn export_web_keeps_a_real_article_cache_in_a_web_safe_envelope() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    save_article_cache(&store);

    let text = backup::export_web(&store).expect("导出");
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let article = &value["val"]["PracticeSaveArticle"];
    assert_eq!(article["version"], 1, "文章信封仍为 v1");

    let web = &article["val"]["practiceData"];
    assert_eq!(web["sectionIndex"], 0, "Web 段落游标应归零");
    assert_eq!(web["sentenceIndex"], 0, "不能把扁平句下标塞给 Web");
    assert_eq!(web["wordIndex"], 0, "Web 词游标应归零");
    assert!(
        web.get("articleId").is_none(),
        "articleId 是 TUI 私有字段，不应混进 Web 游标"
    );
    assert!(
        article["val"]["statStoreData"].is_object(),
        "Web 离页时会写 statStoreData.spend，不能是 null"
    );
    assert_eq!(article["val"]["statStoreData"]["spend"], 45_000);

    let tui = &article["val"]["tuiPracticeData"];
    assert_eq!(tui["articleId"], 11);
    assert_eq!(tui["sentenceIndex"], 3);
    assert_eq!(tui["finished"], 3);
    assert_eq!(tui["wrongSentences"], 1);
    assert_eq!(tui["total"], 72);
    assert_eq!(tui["spend"], 45_000);
}

#[test]
fn export_web_falls_back_to_an_empty_article_envelope_for_unsupported_versions() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    std::fs::write(
        fixture.home().join("practice-article.json"),
        r#"{"val":{"practiceData":{"articleId":11,"sentenceIndex":3,"finished":3,"wrongSentences":0,"total":72,"spend":45000}},"version":2}"#,
    )
    .expect("写文件");

    let text = backup::export_web(&store).expect("导出");
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(
        value["val"]["PracticeSaveArticle"]["val"],
        serde_json::json!({}),
        "不支持的本地版本只写空占位"
    );
}

#[test]
fn import_web_writes_the_three_save_files_and_ignores_an_empty_article_envelope() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());

    // 先造一份带数据的存档（没有文章断点）
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
        !report
            .written
            .iter()
            .any(|key| key == "PracticeSaveArticle"),
        "空占位不该算已导入"
    );
    assert!(
        report.discarded.is_empty(),
        "空占位不是丢弃: {:?}",
        report.discarded
    );
    assert!(
        !target_store.practice_article_exists(),
        "备份没有文章断点时不创建 practice-article.json"
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
fn import_web_discards_a_web_article_cursor_without_an_article_id() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("web-article.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveArticle":{"version":1,"val":{
            "practiceData":{"sectionIndex":2,"sentenceIndex":5,"wordIndex":1},
            "statStoreData":{"spend":1000}
        }}}}"#,
    )
    .expect("写文件");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(report.written.is_empty(), "Web 游标不该被写成本地断点");
    assert!(
        report
            .discarded
            .iter()
            .any(|item| item.contains("PracticeSaveArticle") && item.contains("文章 id")),
        "应说明缺少文章 id: {:?}",
        report.discarded
    );
    assert!(!store.practice_article_exists());
}

#[test]
fn import_web_skips_a_newer_article_cache() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("future-article.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveArticle":{"version":2,"val":{
            "tuiPracticeData":{"articleId":11,"sentenceIndex":1,"finished":1,"wrongSentences":0,"total":2,"spend":1000}
        }}}}"#,
    )
    .expect("写文件");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(report.written.is_empty());
    assert!(
        report
            .skipped
            .iter()
            .any(|item| item.contains("PracticeSaveArticle")),
        "高版本文章缓存应跳过: {:?}",
        report.skipped
    );
    assert!(!store.practice_article_exists());
}

#[test]
fn import_web_does_not_clear_the_local_article_cache_for_an_empty_envelope() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    save_article_cache(&store);

    let path = fixture.home().join("empty-article.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveArticle":{"version":1,"val":{}}}}"#,
    )
    .expect("写文件");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(report.written.is_empty());
    assert!(report.discarded.is_empty());

    let loaded = store.load_practice_article();
    let envelope = loaded.value.expect("本地断点应保留");
    assert_eq!(envelope.practice_data.article_id, 11);
    assert_eq!(envelope.practice_data.sentence_index, 3);
    assert_eq!(envelope.practice_data.spend, 45_000);
}

#[test]
fn import_web_accepts_a_tui_native_article_envelope() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("raw-article.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveArticle":{"version":1,"val":{
            "practiceData":{"articleId":11,"sentenceIndex":1,"finished":1,"wrongSentences":0,"total":2,"spend":45000},
            "statStoreData":null
        }}}}"#,
    )
    .expect("写文件");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(
        report
            .written
            .iter()
            .any(|key| key == "PracticeSaveArticle"),
        "TUI 原生信封应被导入: {:?}",
        report.written
    );
    let loaded = store.load_practice_article();
    let envelope = loaded.value.expect("文章断点应还原");
    assert_eq!(envelope.practice_data.article_id, 11);
    assert_eq!(envelope.practice_data.sentence_index, 1);
    assert_eq!(envelope.practice_data.spend, 45_000);
}

#[test]
fn article_cache_survives_a_web_val_roundtrip() {
    let fixture = common::Fixture::new();
    let source = open_store(&fixture.home());
    save_article_cache(&source);

    let exported: serde_json::Value =
        serde_json::from_str(&backup::export_web(&source).expect("导出")).expect("JSON");

    // 模拟网页版：导入时只保存 PracticeSaveArticle.val，导出时用 v1 重新包信封。
    let mut web_val = exported["val"].clone();
    web_val["PracticeSaveArticle"] = serde_json::json!({
        "version": 1,
        "val": exported["val"]["PracticeSaveArticle"]["val"].clone(),
    });
    let web_backup = serde_json::json!({ "version": 5, "val": web_val });
    let path = fixture.home().join("web-roundtrip.json");
    std::fs::write(&path, serde_json::to_string(&web_backup).expect("序列化")).expect("写备份");

    let target = fixture.root.path().join("restored");
    let target_store = open_store(&target);
    let report = backup::import_web(&target_store, &path).expect("导入");
    assert!(
        report
            .written
            .iter()
            .any(|key| key == "PracticeSaveArticle"),
        "文章断点应随 Web 的 val 往返: {:?}",
        report.written
    );

    let loaded = target_store.load_practice_article();
    let envelope = loaded.value.expect("文章断点应还原");
    assert_eq!(envelope.practice_data.article_id, 11);
    assert_eq!(envelope.practice_data.sentence_index, 3);
    assert_eq!(envelope.practice_data.spend, 45_000);
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
