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
