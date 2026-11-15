//! `tw doctor` 的输出：词数必须等于程序实际加载的词数。

mod common;

use std::fs;

use typewords_tui::cli;
use typewords_tui::domain::catalog::Catalog;

/// 在数据目录里登记一本词典（词条文件由调用方写）。
fn register_user_dict(fixture: &common::Fixture, id: &str, name: &str, length: usize) {
    fs::create_dir_all(fixture.home().join("list")).unwrap();
    let entry = serde_json::json!([{
        "id": id,
        "enName": id,
        "name": name,
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

fn doctor_output(fixture: &common::Fixture) -> String {
    let mut out: Vec<u8> = Vec::new();
    cli::doctor_to(&mut out, &fixture.paths()).expect("doctor 应当成功");
    String::from_utf8(out).unwrap()
}

/// 含空词条的词库：doctor 报的词数必须和 `Catalog::load_words` 实际加载的一致。
///
/// 直接解析词条 JSON 再 `len()` 会把空词条也算进去，而真正的加载路径
/// （`Catalog::load_words`）用 `retain(Word::is_usable)` 过滤了它们。两个数字
/// 打架时排障最容易误判——用户看到「10 词」却只能练到 8 个词。
#[test]
fn doctor_word_count_matches_words_the_program_loads() {
    let fixture = common::Fixture::new();
    register_user_dict(&fixture, "demo", "演示词库", 4);
    // 4 条词条，其中 2 条是空词（一个是空串，一个是纯空格）。
    write_user_words(
        &fixture,
        "demo",
        r#"[{"word":"alpha"},{"word":""},{"word":"   "},{"word":"beta"}]"#,
    );

    let paths = fixture.paths();
    let catalog = Catalog::load(&paths).unwrap();
    let resource = catalog.find("demo").expect("应能找到自建词库");
    assert_eq!(
        catalog.load_words(resource).unwrap().len(),
        2,
        "程序实际只加载 2 个可用词（空词条被 Word::is_usable 过滤）"
    );

    let text = doctor_output(&fixture);
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("- demo"))
        .unwrap_or_else(|| panic!("doctor 输出里应有 demo 词库: {text}"));
    assert!(
        line.contains("2 词"),
        "doctor 的词数应与实际加载的 2 个词一致，实际输出: {line:?}"
    );
    assert!(
        !line.contains("4 词"),
        "doctor 不该把空词条算进词数，实际输出: {line:?}"
    );
}

/// 词库文件缺失或解析失败时，doctor 要标注原因并继续打印后面的内容。
///
/// 它以前对这两种情况只会打一个 `None`，看不出是路径不对还是文件坏了。
#[test]
fn doctor_marks_broken_dicts_and_keeps_going() {
    let fixture = common::Fixture::new();
    fs::create_dir_all(fixture.home().join("list")).unwrap();
    let entries = serde_json::json!([
        {
            "id": "broken", "enName": "broken", "name": "坏词库",
            "url": "broken.json", "length": 3, "type": "word"
        },
        {
            "id": "gone", "enName": "gone", "name": "缺文件词库",
            "url": "gone.json", "length": 3, "type": "word"
        }
    ]);
    fs::write(
        fixture.home().join("list").join("word.json"),
        serde_json::to_string(&entries).unwrap(),
    )
    .unwrap();
    write_user_words(&fixture, "broken", "{ 不是词条数组 }");

    let text = doctor_output(&fixture);
    let broken = text
        .lines()
        .find(|line| line.trim_start().starts_with("- broken"))
        .expect("应打印坏词库");
    assert!(
        broken.contains("解析词库文件失败"),
        "坏词库应标注解析失败的原因，实际输出: {broken:?}"
    );
    let gone = text
        .lines()
        .find(|line| line.trim_start().starts_with("- gone"))
        .expect("应打印缺文件的词库");
    assert!(
        gone.contains("<缺失>"),
        "词条文件不存在时应标注缺失，实际输出: {gone:?}"
    );
    assert!(
        text.contains("存档："),
        "单个词库坏掉不该中断 doctor 后面的输出"
    );
}
