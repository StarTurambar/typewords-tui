//! 词库目录项的 `url` / `language` 必须是单段路径组件。
//!
//! 词条路径是 `dicts/<language>/word/<url>` 直接 join 出来的，而目录项
//! 是手工可编辑的：`url = "../escape.json"` 或 `language = ".."` 会让
//! 词条的读写落到 `<home>` 之外。

mod common;

use std::fs;

use typewords_tui::domain::catalog::Catalog;

fn write_user_catalog(fixture: &common::Fixture, entries: serde_json::Value) {
    fs::create_dir_all(fixture.home().join("list")).unwrap();
    fs::write(
        fixture.home().join("list").join("word.json"),
        serde_json::to_string(&entries).unwrap(),
    )
    .unwrap();
}

#[test]
fn catalog_entries_with_path_traversal_are_skipped_with_a_warning() {
    let fixture = common::Fixture::new();
    write_user_catalog(
        &fixture,
        serde_json::json!([
            {
                "id": "escape", "enName": "escape", "name": "越界 url",
                "url": "../escape.json", "length": 1, "language": "en", "type": "word"
            },
            {
                "id": "escape-language", "enName": "escape-language", "name": "越界 language",
                "url": "ok.json", "length": 1, "language": "..", "type": "word"
            },
            {
                "id": "ok", "enName": "ok", "name": "正常",
                "url": "ok.json", "length": 1, "language": "en", "type": "word"
            }
        ]),
    );

    let catalog = Catalog::load(&fixture.paths()).expect("目录应能装载");
    assert!(catalog.find("escape").is_none(), "url 越界的条目必须被跳过");
    assert!(
        catalog.find("escape-language").is_none(),
        "language 越界的条目必须被跳过"
    );
    let ok = catalog.find("ok").expect("正常条目仍在");
    assert_eq!(
        catalog.warnings().len(),
        2,
        "两个被跳过的条目各一条告警: {:?}",
        catalog.warnings()
    );
    for warning in catalog.warnings() {
        assert!(warning.contains("已跳过"), "{warning}");
    }

    // 正常条目的读写路径都留在数据目录里。
    let write = catalog.dict_write_file(ok).expect("正常条目的写入路径");
    assert!(
        write.starts_with(fixture.home()),
        "写入路径不能逃出数据目录: {}",
        write.display()
    );
}

/// 调用方自己构造的资源对象也要挡住：`Catalog::load` 的校验不是唯一入口。
#[test]
fn a_hand_built_resource_with_an_unsafe_url_gets_no_path() {
    let fixture = common::Fixture::new();
    let catalog = Catalog::load(&fixture.paths()).expect("目录应能装载");
    let mut resource = catalog.resources()[0].clone();
    resource.url = "../../escape.json".to_string();

    assert!(catalog.dict_file(&resource).is_none(), "读取路径必须被拒绝");
    assert!(
        catalog.dict_write_file(&resource).is_none(),
        "写入路径必须被拒绝"
    );
    let err = catalog.load_words(&resource).expect_err("读词条也应报错");
    assert!(
        err.to_string().contains("不是安全的单段路径组件"),
        "错误应指向目录项而不是「文件不存在」: {err}"
    );
}

/// 上游目录项允许省略 `language`（反序列化成空串）：它不构成越界，仍按
/// `en` 兜底，不能把旧目录项一起判死。
#[test]
fn an_entry_without_language_still_defaults_to_en() {
    let fixture = common::Fixture::new();
    write_user_catalog(
        &fixture,
        serde_json::json!([{
            "id": "no-lang", "enName": "no-lang", "name": "缺 language",
            "url": "no-lang.json", "length": 1, "type": "word"
        }]),
    );
    let dir = fixture.home().join("dicts").join("en").join("word");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("no-lang.json"), "[]").unwrap();

    let catalog = Catalog::load(&fixture.paths()).expect("目录应能装载");
    let resource = catalog.find("no-lang").expect("缺 language 的条目仍可用");
    assert_eq!(
        catalog.dict_file(resource),
        Some(dir.join("no-lang.json")),
        "language 缺省为 en"
    );
    assert!(catalog.warnings().is_empty(), "缺 language 不该告警");
}
