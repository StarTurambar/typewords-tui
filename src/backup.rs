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
//!     "PracticeSaveArticle": { "version": 1,  "val": { … } }
//!   }
//! }
//! ```
//!
//! 导入时先按目标类型做一次宽容反序列化**校验**（缺字段由 `serde(default)`
//! 补默认值、未知字段忽略），校验通过后原样落盘（不经过反序列化再序列化），
//! 因此上游存档里本程序没有建模的字段也不会丢。校验失败或版本不受支持的
//! 条目不会写入；覆盖已有文件前先备份为 `<文件名>.bak-<时间戳>.json`，
//! 多文件写入中途失败时按备份回滚。文章断点需要按来源分流：TUI 自己导出的
//! 备份带 `tuiPracticeData` 扩展，能还原成 `practice-article.json`；网页版
//! 自己产生的缓存只有「段落 / 句 / 词」三级游标、没有文章 id，无法对应到
//! TUI 的文章，仍按「丢弃」报告。
//!
//! 导出时文章断点写成网页版能安全解析的信封：Web 字段（三级游标）置 0、
//! `statStoreData` 用非空对象，真正的 TUI 断点放在 `tuiPracticeData` 里。
//! 浏览器端只做原样存取并忽略未知字段，因此只要网页版不在文章页推进进度，
//! 这份扩展就能随 Web 备份往返。
//!
//! 版本高于本程序支持的会**拒绝导入**，避免把读不懂的数据写进数据目录。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::domain::article::{ArticleCache, ArticleCacheEnvelope};
use crate::domain::constants::{
    DICT_STATE_VERSION, PRACTICE_ARTICLE_CACHE_VERSION, PRACTICE_CACHE_VERSION, SETTING_VERSION,
};
use crate::domain::types::{BaseState, PracticeCacheCompact, PracticeState, SettingState};
use crate::store::{FileStore, SaveEnvelope};

/// 上游备份格式的版本号。
pub const WEB_BACKUP_VERSION: u32 = 5;

/// 上游存档的键名。
const KEY_DICT: &str = "dict";
const KEY_SETTING: &str = "setting";
const KEY_PRACTICE_WORD: &str = "PracticeSaveWord";
const KEY_PRACTICE_ARTICLE: &str = "PracticeSaveArticle";

/// TUI 私有的文章断点扩展字段。
///
/// 网页版把 `PracticeSaveArticle.val` 原样存进 IndexedDB，导出时也原样带回，
/// 但不读这个字段；TUI 导入时优先用它还原扁平句子游标与文章 id。
const KEY_TUI_PRACTICE_DATA: &str = "tuiPracticeData";

/// 本地文章断点文件名。
const FILE_PRACTICE_ARTICLE: &str = "practice-article.json";

/// 导入结果摘要。
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    /// 实际写入的存档
    pub written: Vec<String>,
    /// 因版本不受支持而跳过的存档
    pub skipped: Vec<String>,
    /// 结构与目标存档不匹配、被拒绝写入的存档（原文件未改动）
    pub rejected: Vec<String>,
    /// 无法映射而丢弃的内容
    pub discarded: Vec<String>,
    /// 覆盖前留下的备份文件名（相对数据目录）
    pub backups: Vec<String>,
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
        .map(|raw| u32::try_from(raw).unwrap_or(u32::MAX))
        .ok_or_else(|| anyhow!("备份缺少 version 字段"))?;
    if version > WEB_BACKUP_VERSION {
        bail!("备份版本 {version} 高于本程序支持的 {WEB_BACKUP_VERSION}，请先升级");
    }
    let val = backup
        .get("val")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("备份缺少 val 对象"))?;

    let mut report = ImportReport::default();
    let mut writer = ImportWriter::new(store);
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
        let found = envelope
            .get("version")
            .and_then(Value::as_u64)
            .map(|raw| u32::try_from(raw).unwrap_or(u32::MAX));
        if let Some(found) = found {
            if found > expected {
                report
                    .skipped
                    .push(format!("{key}（版本 {found} > {expected}）"));
                continue;
            }
            // 练习缓存只认当前版本：低版本 TUI 也读不懂，写进去只会覆盖本地断点。
            if key == KEY_PRACTICE_WORD && found != expected {
                report
                    .skipped
                    .push(format!("{key}（版本 {found} != {expected}，不受支持）"));
                continue;
            }
        }

        // 校验的就是即将落盘的那份文本，保证「导入通过」与「下次启动能读」一致。
        let text = serde_json::to_string(envelope)?;
        let checked = match key {
            KEY_DICT => validate_envelope::<BaseState>(&text),
            KEY_SETTING => validate_envelope::<SettingState>(&text),
            KEY_PRACTICE_WORD => validate_envelope::<Option<PracticeCacheCompact>>(&text),
            _ => unreachable!("targets 表与校验分支必须一一对应"),
        };
        if let Err(reason) = checked {
            report.rejected.push(format!("{key}（{reason}）"));
            continue;
        }

        if let Err(err) = writer.write_raw(file_name, &text) {
            return Err(abort_with_rollback(&writer, err));
        }
        report.written.push(key.to_string());
    }

    // 文章断点不放进上面的原样写入：两边 payload 不同构（TUI 是文章 id +
    // 扁平句子下标，Web 是段落 / 句 / 词三级游标），必须先校验来源再转换。
    if let Some(envelope) = val.get(KEY_PRACTICE_ARTICLE)
        && let Err(err) = import_article_envelope(&mut writer, envelope, &mut report)
    {
        return Err(abort_with_rollback(&writer, err));
    }
    report.backups = writer.backup_names();
    Ok(report)
}

/// 按目标存档类型做一次宽容反序列化校验。
///
/// 只校验即将落盘的文本，不做「反序列化再序列化」：未知字段必须原样保留。
/// `T` 必须与 `FileStore::load_*` 使用的类型一致，否则会出现「导入通过、
/// 下次启动却被判损坏」的情况。
fn validate_envelope<T: DeserializeOwned>(text: &str) -> std::result::Result<(), String> {
    serde_json::from_str::<SaveEnvelope<T>>(text)
        .map(|_| ())
        .map_err(|err| format!("结构不匹配: {err}"))
}

/// 导入期间的文件替换事务：覆盖前备份，失败时回滚。
struct ImportWriter<'a> {
    store: &'a FileStore,
    /// 一次导入共用一个时间戳，便于把同一批备份归到一起。
    stamp: String,
    /// 已成功替换的文件，按写入顺序记录，回滚时逆序恢复。
    done: Vec<Restore>,
}

/// 一份已被替换的存档及其备份。
struct Restore {
    name: String,
    path: PathBuf,
    /// `None` 表示导入前文件不存在，回滚时删除。
    backup: Option<PathBuf>,
}

impl<'a> ImportWriter<'a> {
    fn new(store: &'a FileStore) -> Self {
        Self {
            store,
            stamp: chrono::Local::now().format("%Y%m%d-%H%M%S").to_string(),
            done: Vec::new(),
        }
    }

    /// 原样写入一项存档（调用前必须已完成 `validate_envelope`）。
    fn write_raw(&mut self, name: &str, text: &str) -> Result<()> {
        self.replace(name, |store| store.write_envelope_raw(name, text))
    }

    /// 写入文章断点（类型化写入，内容由 `ArticleCacheEnvelope` 重新序列化）。
    fn write_article(&mut self, name: &str, cache: Option<&ArticleCacheEnvelope>) -> Result<()> {
        self.replace(name, |store| store.save_practice_article(cache))
    }

    /// 覆盖一份存档：先备份旧内容，再调用 `write` 写入。
    fn replace(&mut self, name: &str, write: impl FnOnce(&FileStore) -> Result<()>) -> Result<()> {
        let path = self.store.file_path(name);
        if path.exists() && !path.is_file() {
            bail!("{} 不是普通文件，无法写入", path.display());
        }
        let backup = if path.is_file() {
            let backup = self.next_backup_path(name);
            fs::copy(&path, &backup).with_context(|| format!("备份 {} 失败", path.display()))?;
            sync_file(&backup);
            Some(backup)
        } else {
            None
        };
        match write(self.store) {
            Ok(()) => {
                self.done.push(Restore {
                    name: name.to_string(),
                    path,
                    backup,
                });
                Ok(())
            }
            Err(err) => {
                if let Some(backup) = &backup
                    && let Err(restore_err) = fs::copy(backup, &path)
                {
                    return Err(err.context(format!(
                        "写入 {name} 失败，且恢复原文件失败（备份在 {}）: {restore_err}",
                        backup.display()
                    )));
                }
                Err(err.context(format!("写入 {name} 失败")))
            }
        }
    }

    /// 唯一备份名：`<文件名>.bak-<时间戳>.json`，同一秒重复导入时追加序号。
    fn next_backup_path(&self, name: &str) -> PathBuf {
        let mut index = 1;
        loop {
            let suffix = if index == 1 {
                String::new()
            } else {
                format!("-{index}")
            };
            let candidate = self
                .store
                .file_path(&format!("{name}.bak-{}{suffix}.json", self.stamp));
            if !candidate.exists() {
                return candidate;
            }
            index += 1;
        }
    }

    /// 回滚本次已写入的文件（逆序，尽力而为），返回成功恢复的文件名。
    fn rollback(&self) -> Vec<String> {
        let mut restored = Vec::new();
        for item in self.done.iter().rev() {
            let result = match &item.backup {
                Some(backup) => fs::copy(backup, &item.path).map(|_| ()),
                None => match fs::remove_file(&item.path) {
                    Ok(()) => Ok(()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(err) => Err(err),
                },
            };
            if result.is_ok() {
                restored.push(item.name.clone());
            }
        }
        restored
    }

    /// 本次导入产生的备份文件名（相对数据目录）。
    fn backup_names(&self) -> Vec<String> {
        self.done
            .iter()
            .filter_map(|item| item.backup.as_ref())
            .filter_map(|backup| backup.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .collect()
    }
}

/// 尽力刷盘：备份文件同样希望落到磁盘，失败不影响导入。
fn sync_file(path: &Path) {
    if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
        let _ = file.sync_all();
    }
}

/// 回滚已写入的存档，并给原始错误补上下文。
fn abort_with_rollback(writer: &ImportWriter<'_>, err: anyhow::Error) -> anyhow::Error {
    let restored = writer.rollback();
    if restored.is_empty() {
        err.context("导入中止；没有存档被改动")
    } else {
        err.context(format!("导入中止，已回滚: {}", restored.join("、")))
    }
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
    val.insert(
        KEY_PRACTICE_ARTICLE.to_string(),
        article_envelope_for_web(store)?,
    );

    let backup = serde_json::json!({
        "version": WEB_BACKUP_VERSION,
        "val": Value::Object(val),
    });
    Ok(serde_json::to_string_pretty(&backup)?)
}

/// 把本地文章断点包装成网页版能安全读取的信封。
///
/// 上游 `PracticeArticleCache` 要求 `practiceData` 里有三个数字游标，且
/// `statStoreData` 非空（文章页离页时会写 `statStoreData.spend`）；但 TUI
/// 只记扁平句子下标，也没有网页版的文章 id。因此 Web 字段填 0（等价「从头
/// 开始」），真正的 TUI 断点放进 `tuiPracticeData`，由 Web 原样保存并在
/// 再次导出时带回（网页版一旦推进句子就会重写缓存，扩展随之丢失）。
///
/// 本地文件缺失、版本不是 1、或断点不可用（`articleId` / `total` 缺失）时
/// 仍写空信封——与上游「没有文章断点」的表示一致。
fn article_envelope_for_web(store: &FileStore) -> Result<Value> {
    let empty = || empty_envelope(PRACTICE_ARTICLE_CACHE_VERSION);
    let Some(envelope) = read_envelope_raw(store, FILE_PRACTICE_ARTICLE)? else {
        return Ok(empty());
    };
    if envelope.get("version").and_then(Value::as_u64)
        != Some(u64::from(PRACTICE_ARTICLE_CACHE_VERSION))
    {
        return Ok(empty());
    }
    let Some(cache) = envelope
        .get("val")
        .and_then(|value| value.get("practiceData"))
        .and_then(article_cache_from_value)
    else {
        return Ok(empty());
    };

    // Web 的 `PracticeState` 与本地同名同构；只带用时，避免把句子数冒充成
    // 词数。`startDate` 用本地信封的保存时间，Web 统计里不会出现 1970。
    let stat = PracticeState {
        start_date: updated_at_ms(&envelope),
        spend: cache.spend,
        ..PracticeState::default()
    };

    let mut val = serde_json::Map::new();
    val.insert(
        "practiceData".to_string(),
        serde_json::json!({
            "sectionIndex": 0,
            "sentenceIndex": 0,
            "wordIndex": 0,
        }),
    );
    val.insert("statStoreData".to_string(), serde_json::to_value(stat)?);
    val.insert(
        KEY_TUI_PRACTICE_DATA.to_string(),
        serde_json::to_value(&cache)?,
    );

    Ok(serde_json::json!({
        "version": PRACTICE_ARTICLE_CACHE_VERSION,
        "val": Value::Object(val),
    }))
}

/// 按来源导入备份里的文章断点。
///
/// - TUI 来源（`tuiPracticeData`，或手工塞进来的 TUI 原生 `practiceData`）：
///   规范化写入 `practice-article.json`；
/// - 空占位（`{}` / `null`）：不动本地，兼容修复前 TUI 导出的旧备份；
/// - 网页版来源的三阶游标：没有文章 id，无法映射，报告为丢弃。
fn import_article_envelope(
    writer: &mut ImportWriter<'_>,
    envelope: &Value,
    report: &mut ImportReport,
) -> Result<()> {
    let found = envelope
        .get("version")
        .and_then(Value::as_u64)
        .map(|raw| u32::try_from(raw).unwrap_or(u32::MAX))
        .unwrap_or(0);
    if found > PRACTICE_ARTICLE_CACHE_VERSION {
        report.skipped.push(format!(
            "{KEY_PRACTICE_ARTICLE}（版本 {found} > {PRACTICE_ARTICLE_CACHE_VERSION}）"
        ));
        return Ok(());
    }
    let Some(value) = envelope.get("val").filter(|value| value.is_object()) else {
        return Ok(());
    };
    let cache = value
        .get(KEY_TUI_PRACTICE_DATA)
        .and_then(article_cache_from_value)
        .or_else(|| value.get("practiceData").and_then(article_cache_from_value));

    if let Some(cache) = cache {
        let local = ArticleCacheEnvelope {
            practice_data: cache,
            stat_store_data: None,
        };
        writer.write_article(FILE_PRACTICE_ARTICLE, Some(&local))?;
        report.written.push(KEY_PRACTICE_ARTICLE.to_string());
    } else if value.as_object().is_some_and(|object| !object.is_empty()) {
        report.discarded.push(format!(
            "{KEY_PRACTICE_ARTICLE}（网页版只记录段落/句/词游标，没有文章 id，无法对应到 TUI 的文章断点）"
        ));
    }
    Ok(())
}

/// 从 JSON 值解析出一个可用的 TUI 文章断点。
///
/// 与 `App::refresh_article_resume` 的过滤条件一致：没有文章 id 或总句数的
/// 缓存无法在列表里定位文章，直接视为不可用。
fn article_cache_from_value(value: &Value) -> Option<ArticleCache> {
    let cache: ArticleCache = serde_json::from_value(value.clone()).ok()?;
    (cache.article_id > 0 && cache.total > 0).then_some(cache)
}

/// 本地信封的保存时间（毫秒）；缺失或格式不对时回退 0。
fn updated_at_ms(envelope: &Value) -> i64 {
    envelope
        .get("updated_at")
        .and_then(Value::as_str)
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.timestamp_millis())
        .unwrap_or(0)
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
