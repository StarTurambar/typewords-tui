//! 与网页版备份的互通：读取 / 写入上游的 `BackupData` v5 格式。
//!
//! 上游「设置 → 数据管理 → 导出」产出的 JSON 结构是：
//!
//! ```text
//! {
//!   "version": 5,
//!   "val": {
//!     "setting":           { "version": 23, "val": { … } },
//!     "dict":              { "version": 4,  "val": { … } },
//!     "PracticeSaveWord":  { "version": 2,  "val": { … } },
//!     "PracticeSaveArticle": { "version": 1, "val": { … } }
//!   }
//! }
//! ```
//!
//! 导入时**逐字段原样落盘**（不经过强类型反序列化再序列化），因此上游存档里
//! 本程序没有建模的字段也不会丢。版本高于本程序支持的会**拒绝导入**，避免把
//! 读不懂的数据写进数据目录。

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use crate::domain::constants::{DICT_STATE_VERSION, PRACTICE_CACHE_VERSION, SETTING_VERSION};
use crate::store::FileStore;

/// 上游备份格式的版本号。
pub const WEB_BACKUP_VERSION: u32 = 5;

/// 上游存档的键名。
const KEY_DICT: &str = "dict";
const KEY_SETTING: &str = "setting";
const KEY_PRACTICE_WORD: &str = "PracticeSaveWord";
const KEY_PRACTICE_ARTICLE: &str = "PracticeSaveArticle";

/// 导入结果摘要。
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    /// 实际写入的存档
    pub written: Vec<String>,
    /// 因版本过高而跳过的存档
    pub skipped: Vec<String>,
    /// 无法映射而丢弃的内容
    pub discarded: Vec<String>,
}

/// 读取网页版备份并写入数据目录。
pub fn import_web(store: &FileStore, file: &Path) -> Result<ImportReport> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("读取 {} 失败", file.display()))?;
    let backup: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} 不是合法的 JSON", file.display()))?;
    import_web_value(store, &backup)
}

/// 从已解析的 JSON 导入。
pub fn import_web_value(store: &FileStore, backup: &Value) -> Result<ImportReport> {
    let version = backup
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("备份缺少 version 字段"))? as u32;
    if version > WEB_BACKUP_VERSION {
        bail!("备份版本 {version} 高于本程序支持的 {WEB_BACKUP_VERSION}，请先升级");
    }
    let val = backup
        .get("val")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("备份缺少 val 对象"))?;

    let mut report = ImportReport::default();
    // (存档键, 目标文件名, 支持的版本)
    let targets: [(&str, &str, u32); 3] = [
        (KEY_DICT, "dict.json", DICT_STATE_VERSION),
        (KEY_SETTING, "setting.json", SETTING_VERSION),
        (
            KEY_PRACTICE_WORD,
            "practice-word.json",
            PRACTICE_CACHE_VERSION,
        ),
    ];
    for (key, file_name, expected) in targets {
        let Some(envelope) = val.get(key) else {
            continue;
        };
        let found = envelope.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
        if found > expected {
            report
                .skipped
                .push(format!("{key}（版本 {found} > {expected}）"));
            continue;
        }
        store.write_envelope_raw(file_name, &serde_json::to_string(envelope)?)?;
        report.written.push(key.to_string());
    }

    if val.get(KEY_PRACTICE_ARTICLE).is_some() {
        report
            .discarded
            .push("PracticeSaveArticle（文章练习缓存，本程序尚未实现文章背诵）".to_string());
    }
    Ok(report)
}

/// 把当前数据目录导出成网页版能导入的备份 JSON。
pub fn export_web(store: &FileStore) -> Result<String> {
    let dict = read_envelope_raw(store, "dict.json")?
        .unwrap_or_else(|| empty_envelope(DICT_STATE_VERSION));
    let setting = read_envelope_raw(store, "setting.json")?
        .unwrap_or_else(|| empty_envelope(SETTING_VERSION));
    let practice = read_envelope_raw(store, "practice-word.json")?;

    let mut val = serde_json::Map::new();
    val.insert(KEY_SETTING.to_string(), setting);
    val.insert(KEY_DICT.to_string(), dict);
    val.insert(
        KEY_PRACTICE_WORD.to_string(),
        practice.unwrap_or_else(|| empty_envelope(PRACTICE_CACHE_VERSION)),
    );
    // 文章练习缓存本程序没有，导出空信封占位，保持与上游结构一致
    val.insert(KEY_PRACTICE_ARTICLE.to_string(), empty_envelope(1));

    let backup = serde_json::json!({
        "version": WEB_BACKUP_VERSION,
        "val": Value::Object(val),
    });
    Ok(serde_json::to_string_pretty(&backup)?)
}

/// 读取一份存档的原始 JSON 文本；文件不存在时返回 `None`。
fn read_envelope_raw(store: &FileStore, name: &str) -> Result<Option<Value>> {
    let path = store.file_path(name);
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("读取 {} 失败", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} 不是合法的 JSON", path.display()))?;
    Ok(Some(value))
}

/// 空的存档信封。
fn empty_envelope(version: u32) -> Value {
    serde_json::json!({ "val": {}, "version": version })
}
