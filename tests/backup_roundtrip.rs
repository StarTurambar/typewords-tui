//! 网页版备份（BackupData v5）的导入导出互通。

mod common;

use std::path::Path;

use typewords_tui::backup;
use typewords_tui::domain::article::{ArticleCache, ArticleCacheEnvelope};
use typewords_tui::domain::types::{BaseState, PracticeCacheCompact, PracticeState, SettingState};
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
    let export = backup::export_web(&store).expect("导出");
    let text = export.text;
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

/// 带 BOM 的存档：`tw` 能启动，`tw export-web` 也要能读。
#[test]
fn export_web_reads_a_save_that_starts_with_a_bom() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    store.save_dict(&BaseState::default()).expect("写词典存档");
    let path = fixture.home().join("dict.json");
    let text = std::fs::read_to_string(&path).expect("读词典存档");
    std::fs::write(&path, format!("\u{feff}{text}")).expect("写回带 BOM 的词典存档");

    let exported = backup::export_web(&store).expect("带 BOM 的存档不该让导出失败");
    let value: serde_json::Value = serde_json::from_str(&exported.text).expect("JSON");
    assert_eq!(
        value["val"]["dict"]["version"], 4,
        "应导出真实存档，而不是当成解析失败退成空信封"
    );
}

/// `--out` 与标准输出必须是同一份内容，且写文件走原子写（同 `export-dict`）。
#[test]
fn export_web_out_writes_the_same_bytes_as_stdout() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let store = open_store(&fixture.home());
    store.save_dict(&BaseState::default()).expect("写 dict");
    store
        .save_settings(&SettingState::default())
        .expect("写 setting");

    let mut printed: Vec<u8> = Vec::new();
    typewords_tui::cli::export_web_to(&mut printed, &paths, None).expect("导出到标准输出");

    let out = fixture.root.path().join("web-backup.json");
    let mut notice: Vec<u8> = Vec::new();
    typewords_tui::cli::export_web_to(&mut notice, &paths, Some(&out)).expect("导出到文件");

    assert_eq!(
        std::fs::read(&out).expect("读导出文件"),
        printed,
        "--out 写出的内容应与标准输出完全一致"
    );
    assert!(
        String::from_utf8_lossy(&notice).contains("已导出"),
        "写文件时应打印导出结果: {}",
        String::from_utf8_lossy(&notice)
    );
    assert!(
        !std::fs::read_dir(fixture.root.path())
            .expect("读目录")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp")),
        "原子写不应留下临时文件"
    );
}

/// 存档坏了更需要导出：坏掉的 dict.json 退成空信封，并在 warnings 里点名。
///
/// 导出是抢救数据的通道，一份存档解析失败不该让整次导出失败。
#[test]
fn export_web_uses_an_empty_envelope_for_a_corrupt_dict_and_warns() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    std::fs::write(fixture.home().join("dict.json"), "{ 半截的 JSON").expect("写坏档");

    let export = backup::export_web(&store).expect("坏存档不该让导出整体失败");
    let value: serde_json::Value = serde_json::from_str(&export.text).expect("导出的仍是 JSON");
    assert_eq!(
        value["val"]["dict"]["version"],
        typewords_tui::domain::constants::DICT_STATE_VERSION,
        "空信封要带当前支持的版本号"
    );
    assert_eq!(
        value["val"]["dict"]["val"],
        serde_json::json!({}),
        "空信封的 val 应为空对象"
    );
    assert!(
        export
            .warnings
            .iter()
            .any(|warning| warning.contains("dict.json") && warning.contains("空信封")),
        "应警告 dict.json 被空信封占位: {:?}",
        export.warnings
    );
}

/// 能解析但不是存档信封的 JSON（数组 / 缺 version / 缺 val）也要空信封占位并警告。
///
/// 原样导出会产出一份连本程序都拒绝导入的备份，等于把坏档从「读不出来」
/// 变成了「看起来正常」。
#[test]
fn export_web_uses_an_empty_envelope_for_json_that_is_not_a_save_envelope() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());

    for text in ["[]", r#"{"val":{}}"#, r#"{"version":4}"#] {
        std::fs::write(fixture.home().join("dict.json"), text).expect("写形状不对的存档");
        let export = backup::export_web(&store).expect("导出");
        let value: serde_json::Value = serde_json::from_str(&export.text).expect("JSON");
        assert_eq!(
            value["val"]["dict"]["version"],
            typewords_tui::domain::constants::DICT_STATE_VERSION,
            "{text}"
        );
        assert_eq!(
            value["val"]["dict"]["val"],
            serde_json::json!({}),
            "非信封内容要退成空信封: {text}"
        );
        assert!(
            export
                .warnings
                .iter()
                .any(|warning| warning.contains("dict.json") && warning.contains("空信封")),
            "应警告 {text} 被空信封占位: {:?}",
            export.warnings
        );
    }
}

#[test]
fn export_web_keeps_a_real_article_cache_in_a_web_safe_envelope() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    save_article_cache(&store);

    let export = backup::export_web(&store).expect("导出");
    let text = export.text;
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

    let export = backup::export_web(&store).expect("导出");
    let text = export.text;
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

    let export = backup::export_web(&store).expect("导出");
    let text = export.text;
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
            .any(|name| name == "practice-article.json"),
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
            .any(|item| item.contains("practice-article.json") && item.contains("文章 id")),
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
            .any(|item| item.contains("practice-article.json")),
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
            .any(|name| name == "practice-article.json"),
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
        serde_json::from_str(&backup::export_web(&source).expect("导出").text).expect("JSON");

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
            .any(|name| name == "practice-article.json"),
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

/// 读不到备份时，错误信息里的路径只该出现一次。
#[test]
fn import_web_does_not_repeat_the_path_in_a_missing_file_error() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let missing = fixture.home().join("nope.json");

    let error = backup::import_web(&store, &missing).expect_err("文件不存在应报错");
    let message = format!("{error:#}");
    let path = missing.display().to_string();
    assert_eq!(
        message.matches(&path).count(),
        1,
        "路径不该在「读取失败」的上下文里重复出现: {message}"
    );
}

/// 超限错误里路径只出现一次：调用方已经补了「读取 <path> 失败」。
///
/// 用 `set_len` 造稀疏文件，不真的写 512 MiB 数据。
#[test]
fn import_web_reports_an_oversized_backup_without_repeating_the_path() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let big = fixture.home().join("big-backup.json");
    let file = std::fs::File::create(&big).expect("建大文件");
    file.set_len(typewords_tui::fs::IMPORT_SIZE_LIMIT + 1)
        .expect("扩到上限以上");
    drop(file);

    let error = backup::import_web(&store, &big).expect_err("超限应报错");
    let message = format!("{error:#}");
    let path = big.display().to_string();
    assert!(message.contains("文件过大"), "{message}");
    assert_eq!(
        message.matches(&path).count(),
        1,
        "路径不该在「读取失败」的上下文里重复出现: {message}"
    );
    assert!(
        message.contains("536870913 字节（约 512.0 MiB）"),
        "越过上限 1 字节要保留精确字节数: {message}"
    );
    assert!(
        message.contains("上限 512 MiB"),
        "上限应按 MiB 显示: {message}"
    );
}

/// 「缺 version 字段」与「version 类型不对」是两种失败，文案不能混。
#[test]
fn import_web_reports_a_missing_backup_version() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("no-version.json");
    std::fs::write(&path, r#"{"val":{}}"#).expect("写文件");

    let error = backup::import_web(&store, &path).expect_err("缺 version 应报错");
    assert!(
        error.to_string().contains("备份缺少 version 字段"),
        "{error}"
    );
}

/// 字段在、只是不是非负整数：要说清实际值，不能报「缺少 version 字段」。
#[test]
fn import_web_reports_a_non_integer_backup_version() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    for (index, raw) in ["-1", "\"5\""].into_iter().enumerate() {
        let path = fixture.home().join(format!("bad-version-{index}.json"));
        std::fs::write(&path, format!(r#"{{"version":{raw},"val":{{}}}}"#)).expect("写文件");

        let error = backup::import_web(&store, &path).expect_err("类型不对应报错");
        let message = error.to_string();
        assert!(message.contains("不是非负整数"), "{message}");
        assert!(
            message.contains(&format!("实际：{raw}")),
            "应给出实际值: {message}"
        );
        assert!(
            !message.contains("缺少 version 字段"),
            "字段在，不该说缺少: {message}"
        );
    }
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
    assert_eq!(report.written, vec!["dict.json".to_string()]);
    assert!(
        report
            .skipped
            .iter()
            .any(|item| item.contains("setting.json")),
        "高版本的 setting 应被跳过"
    );
}

/// 文章信封在、`val` 却是 `null`：读不懂的内容要进「已丢弃」，不能静默忽略。
#[test]
fn import_web_discards_an_article_envelope_with_a_non_object_val() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("null-article.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveArticle":{"version":1,"val":null}}}"#,
    )
    .expect("写文件");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(report.written.is_empty(), "null 的 val 不该被写成本地断点");
    assert!(
        report
            .discarded
            .iter()
            .any(|item| item.contains("practice-article.json") && item.contains("文章断点")),
        "应记「已丢弃」并说明是文章断点: {:?}",
        report.discarded
    );
    assert!(
        report.discarded.iter().any(|item| item.contains("null")),
        "文案应说明实际类型: {:?}",
        report.discarded
    );
    assert!(!store.practice_article_exists());
}

/// 文章信封连 `val` 字段都没有：同样记「已丢弃」，文案点明缺字段而不是类型不对。
#[test]
fn import_web_discards_an_article_envelope_without_a_val_field() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("no-val-article.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveArticle":{"version":1}}}"#,
    )
    .expect("写文件");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(report.written.is_empty(), "缺 val 不该被写成本地断点");
    assert!(
        report
            .discarded
            .iter()
            .any(|item| item.contains("practice-article.json") && item.contains("缺少 val 字段")),
        "应记「已丢弃」并说明缺字段: {:?}",
        report.discarded
    );
    assert!(!store.practice_article_exists());
}

/// 本地存档版本比程序支持的新时（运行时受只读保护），导入不能悄悄降级覆盖。
#[test]
fn import_web_keeps_a_local_save_whose_version_is_newer() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("dict.json");
    let future = r#"{"val":{"futureField":true},"version":99}"#;
    std::fs::write(&path, future).expect("写高版本本地存档");

    let backup_path = fixture.home().join("old-backup.json");
    std::fs::write(
        &backup_path,
        r#"{"version":5,"val":{"dict":{"version":4,"val":{"word":{"bookList":[],"studyIndex":-1}}}}}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &backup_path).expect("导入");
    assert!(
        report.written.is_empty(),
        "高版本本地存档不该被覆盖: {:?}",
        report.written
    );
    let skipped = report.skipped.join("；");
    assert!(skipped.contains("dict.json"), "应点名本地存档: {skipped}");
    assert!(skipped.contains("99"), "文案应含本地版本: {skipped}");
    assert!(skipped.contains("支持的 4"), "文案应含支持版本: {skipped}");
    assert!(
        skipped.contains("备份") && skipped.contains("升级"),
        "应提示先备份 / 升级: {skipped}"
    );
    assert_eq!(
        std::fs::read(&path).expect("读本地存档"),
        future.as_bytes(),
        "本地 dict.json 字节必须完全不变"
    );
    assert!(
        !std::fs::read_dir(fixture.home())
            .expect("读目录")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("dict.json.bak-")),
        "跳过的存档不该产生备份"
    );
}

/// 四份存档的本地版本保护都要生效：本地版本更高时跳过、字节不变、不留 `.bak`。
///
/// 表驱动：dict / setting / practice-word 走同一个循环，practice-article 是独立
/// 分支（最容易被漏掉）；本地文章断点 v2 正好落在「备份带断点」的场景里。
#[test]
fn import_web_keeps_every_local_save_whose_version_is_newer() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());

    let locals = [
        ("dict.json", r#"{"val":{"future":true},"version":99}"#),
        ("setting.json", r#"{"val":{},"version":99}"#),
        ("practice-word.json", r#"{"val":null,"version":99}"#),
        ("practice-article.json", r#"{"val":{},"version":2}"#),
    ];
    for (name, text) in locals {
        std::fs::write(fixture.home().join(name), text).expect("写高版本本地存档");
    }

    let backup_path = fixture.home().join("newer-local.json");
    std::fs::write(
        &backup_path,
        r#"{"version":5,"val":{
            "dict":{"version":4,"val":{"word":{"bookList":[],"studyIndex":-1}}},
            "setting":{"version":23,"val":{}},
            "PracticeSaveWord":{"version":2,"val":null},
            "PracticeSaveArticle":{"version":1,"val":{
                "tuiPracticeData":{"articleId":11,"sentenceIndex":1,"finished":1,"wrongSentences":0,"total":2,"spend":1000}
            }}
        }}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &backup_path).expect("导入");
    assert!(
        report.written.is_empty(),
        "高版本本地存档都不该被覆盖: {:?}",
        report.written
    );
    for (name, text) in locals {
        assert!(
            report.skipped.iter().any(|item| item.contains(name)),
            "应跳过 {name}: {:?}",
            report.skipped
        );
        assert_eq!(
            std::fs::read_to_string(fixture.home().join(name)).expect("读本地存档"),
            text,
            "{name} 字节必须完全不变"
        );
    }
    assert!(
        !std::fs::read_dir(fixture.home())
            .expect("读目录")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().contains(".bak-")),
        "跳过的存档不该产生备份"
    );
}

/// 本地文章断点版本更高、但备份只带空占位时，不该报「已跳过」。
///
/// 空占位本来就不会写本地文件；提前做版本检查会让任何不带断点的备份都刷一条
/// 误导信息（等文章缓存升到 v2，日常导入 TUI 备份都会中招）。
#[test]
fn import_web_does_not_report_a_skipped_article_save_for_an_empty_envelope() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let local = r#"{"val":{},"version":2}"#;
    std::fs::write(fixture.home().join("practice-article.json"), local).expect("写本地 v2 断点");

    let path = fixture.home().join("empty-article.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveArticle":{"version":1,"val":{}}}}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(report.written.is_empty());
    assert!(
        report.skipped.is_empty(),
        "备份没带断点，不该报已跳过: {:?}",
        report.skipped
    );
    assert!(report.discarded.is_empty());
    assert_eq!(
        std::fs::read_to_string(fixture.home().join("practice-article.json")).expect("读本地断点"),
        local,
        "本地断点字节不变"
    );
}

/// 导入前必须按目标存档类型校验 `val`：结构不对的条目不写盘，原文件保持原样。
#[test]
fn import_rejects_unparsable_envelopes() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let mut dict = BaseState::default();
    dict.note_data
        .insert("word00".to_string(), "本地笔记".to_string());
    store.save_dict(&dict).expect("写 dict");
    let before = std::fs::read(fixture.home().join("dict.json")).expect("读 dict");

    let path = fixture.home().join("broken-dict.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"dict":{"version":4,"val":"not-an-object"}}}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &path).expect("导入应继续并报告拒绝");
    assert!(
        report.written.is_empty(),
        "损坏的存档不应被写入: {:?}",
        report.written
    );
    assert!(
        report
            .rejected
            .iter()
            .any(|item| item.contains("dict.json")),
        "应报告拒绝 dict.json: {:?}",
        report.rejected
    );
    assert!(
        report.backups.is_empty(),
        "没有写入就不该产生备份: {:?}",
        report.backups
    );
    assert_eq!(
        std::fs::read(fixture.home().join("dict.json")).expect("读 dict"),
        before,
        "原 dict.json 字节必须保持不变"
    );
    let loaded = store.load_dict();
    assert!(loaded.is_ok(), "原存档仍应可读: {:?}", loaded.status);
    assert_eq!(
        loaded.value.note_data.get("word00").map(String::as_str),
        Some("本地笔记")
    );
    assert!(
        !std::fs::read_dir(fixture.home())
            .expect("读目录")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("dict.json.bak-")),
        "被拒绝的条目不产生备份文件"
    );
}

/// 一项结构损坏只拒绝该项，其余合法存档照常导入。
#[test]
fn import_rejects_only_the_unparsable_save() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let mut local = BaseState::default();
    local
        .note_data
        .insert("word00".to_string(), "本地".to_string());
    store.save_dict(&local).expect("写 dict");
    store
        .save_settings(&SettingState::default())
        .expect("写 setting");
    let setting_before = std::fs::read(fixture.home().join("setting.json")).expect("读 setting");

    let path = fixture.home().join("mixed-broken.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{
            "dict":{"version":4,"val":{"word":{"bookList":[],"studyIndex":-1},"article":{"bookList":[],"studyIndex":-1},"noteData":{"word01":"导入"}}},
            "setting":{"version":23,"val":"not-an-object"}
        }}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &path).expect("导入");
    assert_eq!(report.written, vec!["dict.json".to_string()]);
    assert!(
        report
            .rejected
            .iter()
            .any(|item| item.contains("setting.json")),
        "应只拒绝 setting.json: {:?}",
        report.rejected
    );
    assert_eq!(
        store
            .load_dict()
            .value
            .note_data
            .get("word01")
            .map(String::as_str),
        Some("导入"),
        "合法的 dict 应照常导入"
    );
    assert_eq!(
        std::fs::read(fixture.home().join("setting.json")).expect("读 setting"),
        setting_before,
        "被拒绝的 setting.json 不应被改动"
    );
}

/// 覆盖前必须留下字节级备份，且原样落盘不能丢未建模字段。
#[test]
fn import_backs_up_replaced_saves_and_keeps_unmodelled_fields() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let mut local = BaseState::default();
    local
        .note_data
        .insert("word00".to_string(), "本地".to_string());
    store.save_dict(&local).expect("写 dict");
    let before = std::fs::read(fixture.home().join("dict.json")).expect("读 dict");

    let path = fixture.home().join("future-field.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"dict":{"version":4,"val":{
            "word":{"bookList":[],"studyIndex":-1},
            "article":{"bookList":[],"studyIndex":-1},
            "futureField":{"x":1}
        }}}}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &path).expect("导入");
    assert_eq!(report.written, vec!["dict.json".to_string()]);
    let backup_name = report
        .backups
        .iter()
        .find(|name| name.starts_with("dict.json.bak-"))
        .unwrap_or_else(|| panic!("应留下 dict 备份: {:?}", report.backups))
        .clone();
    assert_eq!(
        std::fs::read(fixture.home().join(&backup_name)).expect("读备份"),
        before,
        "备份必须是覆盖前的原始字节"
    );

    let imported: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture.home().join("dict.json")).expect("读导入结果"),
    )
    .expect("JSON");
    assert_eq!(
        imported["val"]["futureField"]["x"], 1,
        "未建模字段必须原样保留: {imported}"
    );
}

/// 多文件写入中途失败时，已写入的文件要回滚到导入前。
#[test]
fn import_rolls_back_when_a_later_save_cannot_be_written() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let mut local = BaseState::default();
    local
        .note_data
        .insert("word00".to_string(), "本地".to_string());
    store.save_dict(&local).expect("写 dict");
    let before = std::fs::read(fixture.home().join("dict.json")).expect("读 dict");

    // setting.json 是目录：写入前就会拒绝，用来稳定触发回滚。
    std::fs::create_dir(fixture.home().join("setting.json")).expect("建目录");

    let path = fixture.home().join("rollback.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{
            "dict":{"version":4,"val":{"word":{"bookList":[],"studyIndex":-1},"article":{"bookList":[],"studyIndex":-1},"noteData":{"word01":"导入"}}},
            "setting":{"version":23,"val":{}}
        }}"#,
    )
    .expect("写备份");

    let error = backup::import_web(&store, &path).expect_err("写入失败应中止导入");
    let detail = format!("{error:#}");
    assert!(
        detail.contains("setting.json"),
        "错误应指出失败的文件: {detail}"
    );
    assert!(detail.contains("已回滚"), "错误应说明回滚结果: {detail}");
    let backup_name = std::fs::read_dir(fixture.home())
        .expect("读目录")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .find(|name| name.starts_with("dict.json.bak-"))
        .unwrap_or_else(|| panic!("覆盖前应留下 dict 备份"));
    assert!(
        detail.contains(&backup_name),
        "错误信息应给出备份路径 {backup_name}: {detail}"
    );
    assert_eq!(
        std::fs::read(fixture.home().join("dict.json")).expect("读 dict"),
        before,
        "失败后 dict.json 应回滚到导入前"
    );
}

/// 一份都没写成时才可以说「没有存档被改动」；写过了就必须交代回滚结果。
#[test]
fn import_that_fails_before_any_write_says_nothing_was_changed() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    // dict.json 是目录：第一份存档在写入前就被拒绝，一份都没改动。
    std::fs::create_dir(fixture.home().join("dict.json")).expect("建目录");

    let path = fixture.home().join("no-write.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"dict":{"version":4,"val":{}}}}"#,
    )
    .expect("写备份");

    let error = backup::import_web(&store, &path).expect_err("写入失败应中止导入");
    let detail = format!("{error:#}");
    assert!(
        detail.contains("没有存档被改动"),
        "一份都没写时才能这么说: {detail}"
    );
    assert!(!detail.contains("已回滚"), "没有写入就无从回滚: {detail}");
    assert!(
        !std::fs::read_dir(fixture.home())
            .expect("读目录")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry.file_name().to_string_lossy().contains(".bak-")),
        "没有写入就不该产生备份"
    );
}

/// 练习缓存只接受版本 2；低版本信封写进去只会覆盖本地断点。
#[test]
fn import_skips_an_unsupported_practice_cache_version() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let cache = PracticeCacheCompact {
        stat_store_data: Some(PracticeState {
            spend: 1234,
            ..PracticeState::default()
        }),
        ..PracticeCacheCompact::default()
    };
    store.save_practice(Some(&cache)).expect("写练习缓存");

    let path = fixture.home().join("old-practice.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveWord":{"version":1,"val":{}}}}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &path).expect("导入");
    assert!(report.written.is_empty(), "不应写入: {:?}", report.written);
    assert!(
        report
            .skipped
            .iter()
            .any(|item| item.contains("practice-word.json")),
        "旧版本练习缓存应跳过: {:?}",
        report.skipped
    );
    let loaded = store.load_practice();
    assert_eq!(
        loaded
            .value
            .and_then(|cache| cache.stat_store_data)
            .map(|stat| stat.spend),
        Some(1234),
        "本地断点不应被覆盖"
    );
}

/// 校验类型必须和加载器一致：`val: null` 的练习缓存是合法表示。
#[test]
fn import_accepts_a_null_practice_cache() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    let path = fixture.home().join("null-practice.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"PracticeSaveWord":{"version":2,"val":null}}}"#,
    )
    .expect("写备份");

    let report = backup::import_web(&store, &path).expect("导入");
    assert_eq!(report.written, vec!["practice-word.json".to_string()]);
    assert!(
        report.rejected.is_empty(),
        "val=null 是加载器接受的练习缓存表示: {:?}",
        report.rejected
    );
    let loaded = store.load_practice();
    assert!(loaded.is_ok(), "应视为已加载: {:?}", loaded.status);
    assert!(loaded.value.is_none());
}

/// 带 BOM 的网页版备份要能导入：Windows 记事本等工具另存为就会带 BOM。
/// 上游 `JSON.parse` 拒收 BOM，这里有意宽容，写回时不带 BOM。
#[test]
fn import_web_accepts_a_backup_with_a_utf8_bom() {
    let fixture = common::Fixture::new();
    let store = open_store(&fixture.home());
    store.save_dict(&BaseState::default()).expect("写 dict");
    store
        .save_settings(&SettingState::default())
        .expect("写 setting");
    let export = backup::export_web(&store).expect("导出");
    let text = export.text;
    let path = fixture.home().join("bom-backup.json");
    std::fs::write(&path, format!("\u{feff}{text}")).expect("写备份");

    let target = fixture.root.path().join("bom-restored");
    let target_store = open_store(&target);
    let report = backup::import_web(&target_store, &path).expect("带 BOM 的备份应能导入");
    assert_eq!(report.written.len(), 3, "应写入 dict、setting 与练习缓存");
    assert!(target_store.practice_exists());
}

/// 全部跳过时只打印「已跳过」，不再先打一句「没有可导入的存档」。
#[test]
fn import_web_output_does_not_contradict_itself_when_everything_is_skipped() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    std::fs::write(
        fixture.home().join("dict.json"),
        r#"{"val":{},"version":99}"#,
    )
    .expect("写高版本本地存档");
    let path = fixture.home().join("skip-all.json");
    std::fs::write(
        &path,
        r#"{"version":5,"val":{"dict":{"version":4,"val":{}}}}"#,
    )
    .expect("写备份");

    let mut output: Vec<u8> = Vec::new();
    typewords_tui::cli::import_web_to(&mut output, &paths, &path).expect("导入");
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("已跳过"), "应逐条说明为什么没导入: {text}");
    assert!(
        !text.contains("没有可导入的存档"),
        "全部跳过时不该说「没有可导入的存档」: {text}"
    );

    // 备份里确实一条都没有时，这句才成立。
    let empty = fixture.home().join("empty.json");
    std::fs::write(&empty, r#"{"version":5,"val":{}}"#).expect("写空备份");
    let mut output: Vec<u8> = Vec::new();
    typewords_tui::cli::import_web_to(&mut output, &paths, &empty).expect("导入空备份");
    assert!(
        String::from_utf8_lossy(&output).contains("没有可导入的存档。"),
        "空备份应明确说没有可导入的存档"
    );
}
