//! 词库导入导出：`import-dict` 的目录登记与 `export-dict` 的往返。

mod common;

use std::fs;

use typewords_tui::cli;
use typewords_tui::domain::catalog::{Catalog, import_dict};
use typewords_tui::domain::types::{BaseState, Word};
use typewords_tui::store::FileStore;

#[test]
fn repeated_import_replaces_catalog_entry() {
    let fixture = common::Fixture::new();
    let source = fixture.root.path().join("my-words.json");
    let words = vec![Word::new("alpha"), Word::new("beta")];
    std::fs::write(&source, serde_json::to_string(&words).unwrap()).unwrap();

    let paths = fixture.paths();
    let first = import_dict(&paths, &source).expect("首次导入");
    let second = import_dict(&paths, &source).expect("重复导入");
    assert_eq!(first.id_string(), "my-words");
    assert_eq!(second.id_string(), "my-words");

    let catalog: Vec<serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(fixture.home().join("list").join("word.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(catalog.len(), 1, "重复导入不应追加重复目录项");

    // 词条文件也只保留一份，且可用 --dict 找到。
    let words_file = fixture
        .home()
        .join("dicts")
        .join("en")
        .join("word")
        .join("my-words.json");
    assert!(words_file.is_file());
    assert!(fixture.assets().join("list").join("word.json").is_file());
}

#[test]
fn import_requires_json_extension() {
    let fixture = common::Fixture::new();
    let source = fixture.root.path().join("words.txt");
    let words = vec![Word::new("alpha")];
    std::fs::write(&source, serde_json::to_string(&words).unwrap()).unwrap();
    let err = import_dict(&fixture.paths(), &source).expect_err("非 .json 应失败");
    assert!(format!("{err:#}").contains(".json"));
}

/// 导入的词库要标成自建词典。
///
/// `shake_common_dict` 会在落盘前清掉「非自建、非系统」词典的词表（词表按需
/// 从资源文件重新加载）。导入的词库文件在数据目录里、不是随包分发的内置
/// 资源，但它没带 `custom` 标记，标记丢了之后一存盘词表就被清空——
/// 用户改一个词或换个词库，重启后整本词典就没了。
#[test]
fn an_imported_dictionary_is_marked_as_custom() {
    let fixture = common::Fixture::new();
    let source = fixture.root.path().join("my-words.json");
    let words = vec![Word::new("alpha"), Word::new("beta")];
    std::fs::write(&source, serde_json::to_string(&words).unwrap()).unwrap();

    let paths = fixture.paths();
    let resource = import_dict(&paths, &source).expect("导入");
    assert!(
        resource.is_custom(),
        "导入的词库应标记为自建词典，否则落盘时词表会被清空"
    );

    // 目录文件里也要带上这个标记
    let catalog: Vec<serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(fixture.home().join("list").join("word.json")).unwrap(),
    )
    .unwrap();
    let entry = catalog.first().expect("目录里应有这条");
    assert_eq!(
        entry.get("custom").and_then(|value| value.as_bool()),
        Some(true),
        "目录项应带 custom = true，实际：{entry}"
    );

    // 切到这本词典并保存：词表不该被清空
    let mut base = typewords_tui::domain::types::BaseState::default();
    let dict = resource.to_dict(words);
    base.set_study_dict(dict);
    let before = base.study_dict().map(|dict| dict.words.len()).unwrap_or(0);
    assert_eq!(before, 2, "切换后词表应有 2 个词");
    base.shake_common_dict();
    let after = base.study_dict().map(|dict| dict.words.len()).unwrap_or(0);
    assert_eq!(after, 2, "落盘前的清理不该清掉导入词库的词表");
}

/// 在数据目录里登记一本用户词库（词条文件由调用方写）。
fn register_user_dict(fixture: &common::Fixture, id: &str, length: usize) {
    fs::create_dir_all(fixture.home().join("list")).unwrap();
    let entry = serde_json::json!([{
        "id": id,
        "enName": id,
        "name": id,
        "description": "测试用自建词库",
        "url": format!("{id}.json"),
        "length": length,
        "language": "en",
        "translateLanguage": "zh_CN",
        "type": "word",
        "category": "我的词库",
        "tags": ["测试"],
        "custom": true
    }]);
    fs::write(
        fixture.home().join("list").join("word.json"),
        serde_json::to_string(&entry).unwrap(),
    )
    .unwrap();
}

fn write_user_words(fixture: &common::Fixture, id: &str, text: &str) {
    let dir = fixture.home().join("dicts").join("en").join("word");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{id}.json")), text).unwrap();
}

/// 导出 → 导入：`export-dict` 的输出必须能被 `import-dict` 原样读回。
///
/// 两个命令是一对相反操作，导出格式（词条数组、空词条过滤、pretty JSON）
/// 一旦和导入对不上，备份就成了一张读不回来的快照。
#[test]
fn exported_dictionary_round_trips_through_import_dict() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let exported = fixture.root.path().join("toy-export.json");
    let mut sink: Vec<u8> = Vec::new();
    cli::export_dict_to(&mut sink, &paths, Some("toy"), Some(&exported)).expect("导出");

    let words: Vec<Word> = serde_json::from_str(&fs::read_to_string(&exported).unwrap())
        .expect("导出文件应是词条数组");
    assert_eq!(words.len(), 12);
    assert_eq!(words[0].word, "word00");
    assert_eq!(
        words[0].trans[0].cn, "释义0",
        "导出要保留释义，不能只剩单词"
    );
    assert!(
        String::from_utf8(sink).unwrap().contains("已导出词库"),
        "写文件时应打印导出结果"
    );

    // 换个文件名重新导入（id 取文件名）：词条与原文件一致。
    let imported = fixture.root.path().join("toy-copy.json");
    fs::copy(&exported, &imported).unwrap();
    let resource = import_dict(&paths, &imported).expect("导出的文件应能重新导入");
    assert_eq!(resource.id_string(), "toy-copy");
    assert_eq!(resource.length, 12);
}

/// 缺省输出是标准输出：导出结果可直接重定向进文件或接进管道。
#[test]
fn export_dict_prints_the_word_array_by_default() {
    let fixture = common::Fixture::new();
    let mut sink: Vec<u8> = Vec::new();
    cli::export_dict_to(&mut sink, &fixture.paths(), Some("toy"), None).expect("导出");
    let text = String::from_utf8(sink).unwrap();
    let words: Vec<Word> = serde_json::from_str(&text).expect("标准输出应是词条数组");
    assert_eq!(words.len(), 12);
    assert!(
        !text.contains("已导出词库"),
        "标准输出只放 JSON，脚本才能直接重定向: {text}"
    );
}

/// 不指定 id 时导出当前学习词库。
///
/// 普通词库的词表在 `dict.json` 落盘时被 `shake_common_dict` 清空（只留
/// 元数据），导出必须按 id 回词条文件补齐，否则导出的是一份空词库。
#[test]
fn export_dict_defaults_to_the_current_study_dictionary() {
    let fixture = common::Fixture::new();
    let paths = fixture.paths();
    let catalog = Catalog::load(&paths).unwrap();
    let resource = catalog.find("toy").expect("toy 词库");
    let mut base = BaseState::default();
    base.set_study_dict(resource.to_dict(fixture.words()));
    FileStore::new(fixture.home())
        .unwrap()
        .save_dict(&base)
        .expect("写存档");

    let mut sink: Vec<u8> = Vec::new();
    cli::export_dict_to(&mut sink, &paths, None, None).expect("导出当前学习词库");
    let words: Vec<Word> = serde_json::from_str(&String::from_utf8(sink).unwrap()).unwrap();
    assert_eq!(words.len(), 12, "词表被存档清空后要能回词条文件补齐");
}

/// 内置词典（收藏 / 错词 / 已掌握）没有词条文件，词条只在 `dict.json` 里。
#[test]
fn export_dict_exports_a_builtin_wordbook_from_the_save() {
    let fixture = common::Fixture::new();
    let mut base = BaseState::default();
    base.wrong_mut().expect("错词典").words = vec![Word::new("alpha"), Word::new("beta")];
    FileStore::new(fixture.home())
        .unwrap()
        .save_dict(&base)
        .expect("写存档");

    let mut sink: Vec<u8> = Vec::new();
    cli::export_dict_to(&mut sink, &fixture.paths(), Some("wordWrong"), None).expect("导出错词");
    let words: Vec<Word> = serde_json::from_str(&String::from_utf8(sink).unwrap()).unwrap();
    assert_eq!(words.len(), 2);
    assert_eq!(words[0].word, "alpha");
}

/// 空词库导出成 `[]` 之后 `import-dict` 会拒收（要求至少一个可用词），
/// 与其产出一个导不回去的文件，不如当场报错。
#[test]
fn export_dict_rejects_a_dictionary_without_usable_words() {
    let fixture = common::Fixture::new();
    register_user_dict(&fixture, "empty", 0);
    write_user_words(&fixture, "empty", r#"[{"word":""},{"word":"  "}]"#);

    let mut sink: Vec<u8> = Vec::new();
    let err = cli::export_dict_to(&mut sink, &fixture.paths(), Some("empty"), None)
        .expect_err("没有可用单词应报错");
    let message = format!("{err:#}");
    assert!(message.contains("词库中没有可用单词: empty"), "{message}");
}

/// 找不到词库时要说清是哪个 id，并提示怎么查可用的 id。
#[test]
fn export_dict_reports_unknown_dictionary_ids() {
    let fixture = common::Fixture::new();
    let mut sink: Vec<u8> = Vec::new();
    let err = cli::export_dict_to(&mut sink, &fixture.paths(), Some("nope"), None)
        .expect_err("不存在的 id 应报错");
    let message = format!("{err:#}");
    assert!(message.contains("未找到词库: nope"), "{message}");
    assert!(
        message.contains("tw doctor"),
        "应提示怎么查可用 id: {message}"
    );
}

/// 带 BOM 的词库文件要能导入：Windows 记事本等工具默认就带 BOM，
/// 上游 JSON.parse 拒收它，这里有意宽容；写出的词条文件不带 BOM。
#[test]
fn import_dict_accepts_a_word_file_with_a_utf8_bom() {
    let fixture = common::Fixture::new();
    let source = fixture.root.path().join("bom-words.json");
    let words = vec![Word::new("alpha"), Word::new("beta")];
    let text = format!("\u{feff}{}", serde_json::to_string(&words).unwrap());
    fs::write(&source, text).unwrap();

    let resource = import_dict(&fixture.paths(), &source).expect("带 BOM 的词库应能导入");
    assert_eq!(resource.length, 2);

    let written = fs::read_to_string(
        fixture
            .home()
            .join("dicts")
            .join("en")
            .join("word")
            .join("bom-words.json"),
    )
    .expect("导入应写出词条文件");
    assert!(!written.starts_with('\u{feff}'), "写出的词条文件不应带 BOM");
}

/// 读不出来的 `dict.json` 要说清是「读不到」，而不是「还没有当前学习词库」：
/// 后者会把人引向错误的方向（以为数据没了）。
#[test]
fn export_dict_reports_an_unreadable_save() {
    let fixture = common::Fixture::new();
    fs::create_dir(fixture.home().join("dict.json")).expect("把 dict.json 做成目录");

    let mut sink: Vec<u8> = Vec::new();
    let err = cli::export_dict_to(&mut sink, &fixture.paths(), None, None)
        .expect_err("读不出来的存档应报错");
    let message = format!("{err:#}");
    assert!(message.contains("读取词典存档失败"), "{message}");
    assert!(message.contains("关闭占用"), "{message}");
}

/// 还没有当前学习词库（空数据目录）时，缺省导出要给出可执行的提示，
/// 而不是一句「未找到词库: 」。
#[test]
fn export_dict_explains_a_missing_current_dictionary() {
    let fixture = common::Fixture::new();
    let mut sink: Vec<u8> = Vec::new();
    let err = cli::export_dict_to(&mut sink, &fixture.paths(), None, None)
        .expect_err("没有当前学习词库应报错");
    let message = format!("{err:#}");
    assert!(message.contains("还没有当前学习词库"), "{message}");
    assert!(message.contains("DICT_ID"), "{message}");
}
