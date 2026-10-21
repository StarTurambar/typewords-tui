//! import-dict 的目录登记行为。

mod common;

use typewords_tui::domain::catalog::import_dict;
use typewords_tui::domain::types::Word;

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
