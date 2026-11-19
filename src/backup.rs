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
//! 多文件写入中途失败时按备份原子回滚。文章断点需要按来源分流：TUI 自己导出的
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
use crate::fs::copy_atomic;
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
///
/// 所有条目都用本地文件名（`dict.json`、`practice-article.json` …）称呼存档，
/// 与用户能在数据目录里看到的文件一致；备份里的键名（`PracticeSaveWord` 等）
/// 只在文档的字段映射表里出现。
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    /// 实际写入的存档（文件名）
    pub written: Vec<String>,
    /// 因版本不受支持而跳过的存档（文件名 + 原因）
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
    // 与 import-dict 同一上限：外部备份整读前先预检大小，超限给「文件过大」。
    let text = crate::fs::read_text_limited(file, crate::fs::IMPORT_SIZE_LIMIT)
        .with_context(|| format!("读取 {} 失败", file.display()))?;
    // 读取容忍开头的 UTF-8 BOM（网页版导出的文件被记事本另存过很常见）。
    let backup: Value = serde_json::from_str(crate::fs::strip_utf8_bom(&text))
        .with_context(|| format!("{} 不是合法的 JSON", file.display()))?;
    import_web_value(store, &backup)
}

/// 从已解析的 JSON 导入。
pub fn import_web_value(store: &FileStore, backup: &Value) -> Result<ImportReport> {
    // 「缺字段」与「字段在、类型不对」是两种失败：混成一句「缺少 version
    // 字段」会让人去找一个根本不缺的字段。u64 超出 u32 仍按「版本高于支持」
    // 处理（下面的比较用 u32::MAX 兜底）。
    let version_field = backup
        .get("version")
        .ok_or_else(|| anyhow!("备份缺少 version 字段"))?;
    let raw = version_field
        .as_u64()
        .ok_or_else(|| anyhow!("备份的 version 字段不是非负整数（实际：{version_field}）"))?;
    let version = u32::try_from(raw).unwrap_or(u32::MAX);
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
                report.skipped.push(format!(
                    "{file_name}（备份版本 {found} 高于本程序支持的 {expected}）"
                ));
                continue;
            }
            // 练习缓存只认当前版本：低版本 TUI 也读不懂，写进去只会覆盖本地断点。
            if key == KEY_PRACTICE_WORD && found != expected {
                report.skipped.push(format!(
                    "{file_name}（备份版本 {found} 与本程序支持的 {expected} 不符，不受支持）"
                ));
                continue;
            }
        }

        // 本地存档版本比本程序支持的新时不覆盖。dict / setting 在运行时本来
        // 就受只读保护；两份练习缓存没有这层保护（版本不符时运行时只是忽略，
        // 保存时反而会被降级覆盖），导入按同一口径跳过，别动本地文件。
        if let Some(local) = store.envelope_version(file_name)
            && local > expected
        {
            report
                .skipped
                .push(local_version_too_new(file_name, local, expected));
            continue;
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
            report.rejected.push(format!("{file_name}（{reason}）"));
            continue;
        }

        if let Err(err) = writer.write_raw(file_name, &text) {
            return Err(abort_with_rollback(&writer, err));
        }
        report.written.push(file_name.to_string());
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

impl Restore {
    /// 备份文件名（相对数据目录）；导入前不存在的存档没有备份。
    fn backup_name(&self) -> Option<String> {
        self.backup
            .as_ref()
            .and_then(|backup| backup.file_name())
            .map(|name| name.to_string_lossy().to_string())
    }
}

/// 一次回滚的结果。
///
/// 分成两份是因为「改过没有」与「回滚成没成」是两件事：`done` 非空就说明
/// 磁盘上已经是导入后的内容，此时哪怕回滚全部失败也不能说「没有存档被改动」。
#[derive(Debug, Default)]
struct RollbackOutcome {
    /// 已恢复（或删除）的文件，按回滚顺序（逆序）
    restored: Vec<RestoredSave>,
    /// 没能恢复的文件，附带失败原因与它的备份
    failed: Vec<RollbackFailure>,
}

/// 一份已回滚成功的存档。
#[derive(Debug)]
struct RestoredSave {
    name: String,
    /// 备份文件名；`None` 表示导入前文件不存在，回滚时已删除。
    backup: Option<String>,
}

/// 一份没能回滚的存档。
#[derive(Debug)]
struct RollbackFailure {
    name: String,
    /// 备份文件名；`None` 表示导入前文件不存在，回滚时本来要删掉它。
    backup: Option<String>,
    error: anyhow::Error,
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
            // 备份本身也走原子写：`fs::copy` 中途失败会留下半截 `.bak`，
            // 而这份 `.bak` 正是回滚与手工恢复的唯一依据。
            copy_atomic(&path, &backup).with_context(|| format!("备份 {} 失败", path.display()))?;
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
                    && let Err(restore_err) = copy_atomic(backup, &path)
                {
                    return Err(err.context(format!(
                        "写入 {name} 失败，且恢复原文件失败（备份在 {}）: {restore_err:#}",
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

    /// 回滚本次已写入的文件（逆序，尽力而为）。
    ///
    /// 恢复同样走原子写：中断时要么是完整的旧内容，要么仍是导入后的内容，
    /// 不会留下半截存档。单个文件失败不影响其余文件继续回滚。
    fn rollback(&self) -> RollbackOutcome {
        let mut outcome = RollbackOutcome::default();
        for item in self.done.iter().rev() {
            let result = match &item.backup {
                Some(backup) => copy_atomic(backup, &item.path),
                None => match fs::remove_file(&item.path) {
                    Ok(()) => Ok(()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(err) => Err(err.into()),
                },
            };
            match result {
                Ok(()) => outcome.restored.push(RestoredSave {
                    name: item.name.clone(),
                    backup: item.backup_name(),
                }),
                Err(error) => outcome.failed.push(RollbackFailure {
                    name: item.name.clone(),
                    backup: item.backup_name(),
                    error,
                }),
            }
        }
        outcome
    }

    /// 本次导入产生的备份文件名（相对数据目录）。
    fn backup_names(&self) -> Vec<String> {
        self.done.iter().filter_map(Restore::backup_name).collect()
    }
}

/// 回滚已写入的存档，并给原始错误补上下文。
///
/// 「有没有改过」只看 `done`，不看回滚成功的数量：全部回滚失败时磁盘上仍是
/// 导入后的内容，报「没有存档被改动」会把用户引向错误的数据。回滚失败的文件
/// 逐个点名，并给出对应的 `.bak`，方便手工还原。
fn abort_with_rollback(writer: &ImportWriter<'_>, err: anyhow::Error) -> anyhow::Error {
    if writer.done.is_empty() {
        return err.context("导入中止；没有存档被改动");
    }
    let outcome = writer.rollback();
    let mut notes = Vec::new();
    if !outcome.restored.is_empty() {
        let listed: Vec<String> = outcome
            .restored
            .iter()
            .map(|save| match &save.backup {
                Some(backup) => format!("{}（备份 {backup}）", save.name),
                None => format!("{}（导入前不存在，已删除）", save.name),
            })
            .collect();
        notes.push(format!("已回滚: {}", listed.join("、")));
    }
    if !outcome.failed.is_empty() {
        let listed: Vec<String> = outcome
            .failed
            .iter()
            .map(|failure| match &failure.backup {
                Some(backup) => format!("{}（{:#}；备份 {backup}）", failure.name, failure.error),
                None => format!(
                    "{}（{:#}；导入前不存在，可手工删除）",
                    failure.name, failure.error
                ),
            })
            .collect();
        notes.push(format!("未能回滚: {}", listed.join("、")));
    }
    // 备份名是相对数据目录的，把目录一并带上才找得到文件。
    err.context(format!(
        "导入中止（数据目录 {}），{}",
        writer.store.home().display(),
        notes.join("；")
    ))
}

/// `tw export-web` 的产物：可直接重定向的 JSON 文本与需要提示的警告。
///
/// 警告与文本分开，标准输出才能保持「纯 JSON」：脚本可以直接
/// `tw export-web > backup.json`，提示由调用方打到 stderr。
#[derive(Clone, Debug)]
pub struct WebExport {
    pub text: String,
    /// 被空信封占位的存档（每条含文件名与原因）
    pub warnings: Vec<String>,
}

/// 把当前数据目录导出成网页版能导入的备份 JSON。
///
/// 导出是抢救数据的通道：某份存档坏了更需要它能用，因此读不到 / 解析失败
/// 的存档退成空信封，并各留一条警告，而不是让整次导出失败。
pub fn export_web(store: &FileStore) -> Result<WebExport> {
    let mut warnings = Vec::new();
    let dict = read_or_placeholder(store, "dict.json", DICT_STATE_VERSION, &mut warnings);
    let setting = read_or_placeholder(store, "setting.json", SETTING_VERSION, &mut warnings);
    let practice = read_or_placeholder(
        store,
        "practice-word.json",
        PRACTICE_CACHE_VERSION,
        &mut warnings,
    );

    let mut val = serde_json::Map::new();
    val.insert(KEY_SETTING.to_string(), setting);
    val.insert(KEY_DICT.to_string(), dict);
    val.insert(KEY_PRACTICE_WORD.to_string(), practice);
    val.insert(
        KEY_PRACTICE_ARTICLE.to_string(),
        article_envelope_for_web(store, &mut warnings)?,
    );

    let backup = serde_json::json!({
        "version": WEB_BACKUP_VERSION,
        "val": Value::Object(val),
    });
    Ok(WebExport {
        text: serde_json::to_string_pretty(&backup)?,
        warnings,
    })
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
fn article_envelope_for_web(store: &FileStore, warnings: &mut Vec<String>) -> Result<Value> {
    let empty = || empty_envelope(PRACTICE_ARTICLE_CACHE_VERSION);
    let envelope = match read_envelope_raw(store, FILE_PRACTICE_ARTICLE) {
        RawSave::Value(value) => value,
        RawSave::Missing => return Ok(empty()),
        RawSave::Unavailable(reason) => {
            warnings.push(format!(
                "{FILE_PRACTICE_ARTICLE} 不可解析，已用空信封占位：{reason}"
            ));
            return Ok(empty());
        }
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

/// 本地存档版本高于本程序支持版本时的跳过说明（导入路径共用）。
///
/// `dict.json` / `setting.json` 运行时受只读保护；两份练习缓存版本不符时运行时
/// 只是忽略、保存时更容易被降级覆盖，导入同样不去动本地文件。提示里点明本地
/// 版本、支持版本，并给出「先备份 / 升级」的下一步。
fn local_version_too_new(file_name: &str, local: u32, expected: u32) -> String {
    format!(
        "本地 {file_name} 版本 {local} 高于本程序支持的 {expected}，已保留原文件不覆盖；建议先备份数据目录，升级程序后再导入"
    )
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
            "{FILE_PRACTICE_ARTICLE}（备份版本 {found} 高于本程序支持的 {PRACTICE_ARTICLE_CACHE_VERSION}）"
        ));
        return Ok(());
    }
    let Some(value) = envelope.get("val").filter(|value| value.is_object()) else {
        // 信封在、`val` 却不是对象：不能静默返回——用户会以为这条断点已经
        // 导入了。`{}` 是「没有文章断点」的合法占位（不报丢弃），缺失 /
        // `null` / 其它类型则是读不懂的内容，记「已丢弃」并说明原因。
        let reason = match envelope.get("val") {
            None => "缺少 val 字段".to_string(),
            Some(value) => format!("val 不是对象（实际是 {}）", json_kind(value)),
        };
        report.discarded.push(format!(
            "{FILE_PRACTICE_ARTICLE}（{reason}，无法还原文章断点）"
        ));
        return Ok(());
    };
    let cache = value
        .get(KEY_TUI_PRACTICE_DATA)
        .and_then(article_cache_from_value)
        .or_else(|| value.get("practiceData").and_then(article_cache_from_value));

    if let Some(cache) = cache {
        // 本地文章断点版本更高时不覆盖。检查放在确认「备份确实带了一个可写
        // 的断点」之后：空占位 / 网页版游标本来就不会写本地文件，提前报
        // 「已跳过」是假消息（本地断点 v2 时，任何不带断点的备份都会刷一条）。
        if let Some(local) = writer.store.envelope_version(FILE_PRACTICE_ARTICLE)
            && local > PRACTICE_ARTICLE_CACHE_VERSION
        {
            report.skipped.push(local_version_too_new(
                FILE_PRACTICE_ARTICLE,
                local,
                PRACTICE_ARTICLE_CACHE_VERSION,
            ));
            return Ok(());
        }
        let local = ArticleCacheEnvelope {
            practice_data: cache,
            stat_store_data: None,
        };
        writer.write_article(FILE_PRACTICE_ARTICLE, Some(&local))?;
        report.written.push(FILE_PRACTICE_ARTICLE.to_string());
    } else if value.as_object().is_some_and(|object| !object.is_empty()) {
        report.discarded.push(format!(
            "{FILE_PRACTICE_ARTICLE}（网页版只记录段落/句/词游标，没有文章 id，无法对应到 TUI 的文章断点）"
        ));
    }
    Ok(())
}

/// JSON 值的类型名，用于「实际是 X」这类报告文案。
fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "布尔值",
        Value::Number(_) => "数字",
        Value::String(_) => "字符串",
        Value::Array(_) => "数组",
        Value::Object(_) => "对象",
    }
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

/// `read_envelope_raw` 的三类结果。
///
/// 「缺失」是正常状态，「读不到 / 解析失败」是坏档：导出时两者都退成空信封，
/// 但只有后者要留一条警告。用 `Result` 把两者混在 `Err` 里正是导出整体
/// 失败（问题 3）的根源。
enum RawSave {
    Missing,
    /// 文件在，但读不出来、不是合法 JSON，或不是存档信封（含原因）
    Unavailable(String),
    Value(Value),
}

/// 读取一份存档的原始 JSON；文件不存在时返回 `Missing`。
///
/// 直接 `read_to_string` 而不是先 `exists()`：后者是 TOCTOU（检查与读取之间
/// 文件可能被删 / 换成目录），也会把「路径是目录」这类 IO 错误误判成不存在，
/// 于是导出一份缺了这份存档的备份。与 `FileStore::read_text` 同一套判定：
/// 容忍开头的 UTF-8 BOM，只含空白的文件等同缺失。
fn read_envelope_raw(store: &FileStore, name: &str) -> RawSave {
    let path = store.file_path(name);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return RawSave::Missing,
        Err(err) => return RawSave::Unavailable(format!("读取失败：{err}")),
    };
    let text = crate::fs::strip_utf8_bom(&text);
    if text.trim().is_empty() {
        return RawSave::Missing;
    }
    match serde_json::from_str::<Value>(text) {
        // 能解析但不是信封（数组、缺 version / val）同样导不回来：原样导出
        // 只会产出一份自己都拒绝导入的备份，所以与解析失败同一处理。
        Ok(value) => match envelope_shape_error(&value) {
            Some(reason) => RawSave::Unavailable(reason),
            None => RawSave::Value(value),
        },
        Err(err) => RawSave::Unavailable(format!("不是合法的 JSON：{err}")),
    }
}

/// 判断解析出来的 JSON 是否像一个存档信封：对象、`version` 是非负整数、
/// 且有 `val` 键（`val` 允许为 `null`，那是练习缓存的「没有缓存」）。
///
/// 只检查形状，不检查版本高低：版本高于本程序的存档仍原样导出，导入侧会按
/// 「版本过高」跳过，不会悄悄降级。
fn envelope_shape_error(value: &Value) -> Option<String> {
    let Value::Object(object) = value else {
        return Some(format!("不是存档信封（实际是 {}）", json_kind(value)));
    };
    match object.get("version") {
        None => Some("不是存档信封（缺少 version 字段）".to_string()),
        Some(version) if version.as_u64().is_none() => Some(format!(
            "不是存档信封（version 不是非负整数，实际是 {version}）"
        )),
        Some(_) if !object.contains_key("val") => Some("不是存档信封（缺少 val 字段）".to_string()),
        Some(_) => None,
    }
}

/// 读一份存档；缺失或读不出来时退成空信封，读不出来的再留一条警告。
fn read_or_placeholder(
    store: &FileStore,
    name: &str,
    version: u32,
    warnings: &mut Vec<String>,
) -> Value {
    match read_envelope_raw(store, name) {
        RawSave::Value(value) => value,
        RawSave::Missing => empty_envelope(version),
        RawSave::Unavailable(reason) => {
            warnings.push(format!("{name} 不可解析，已用空信封占位：{reason}"));
            empty_envelope(version)
        }
    }
}

/// 空的存档信封。
fn empty_envelope(version: u32) -> Value {
    serde_json::json!({ "val": {}, "version": version })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一份「备份还在、目标位置却已无法还原」的存档。
    ///
    /// 目标被目录占住后原子写的 rename 必然失败，模拟权限变化 / 文件被占用
    /// 这类回滚失败（真实触发要磁盘满、权限或 Windows 文件锁）。
    fn stuck_save(store: &FileStore, name: &str, stamp: &str) -> Restore {
        let backup = store.file_path(&format!("{name}.bak-{stamp}.json"));
        fs::write(&backup, "{\"val\":{},\"version\":4}").expect("写备份");
        let path = store.file_path(name);
        fs::create_dir(&path).expect("建目录");
        Restore {
            name: name.to_string(),
            path,
            backup: Some(backup),
        }
    }

    /// 回滚写盘失败时不能宣称「没有存档被改动」，还要点名文件与它的 .bak。
    #[test]
    fn a_failed_import_that_cannot_roll_back_points_at_the_backup_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf()).expect("数据目录");
        let stamp = "20260101-120000";
        let writer = ImportWriter {
            store: &store,
            stamp: stamp.to_string(),
            done: vec![stuck_save(&store, "dict.json", stamp)],
        };

        let err = abort_with_rollback(&writer, anyhow!("写入 setting.json 失败"));
        let detail = format!("{err:#}");

        assert!(
            !detail.contains("没有存档被改动"),
            "回滚失败时磁盘上已是导入后的内容，不能这么说: {detail}"
        );
        assert!(
            detail.contains("未能回滚"),
            "应单独列出失败的文件: {detail}"
        );
        assert!(detail.contains("dict.json"), "应点名失败的文件: {detail}");
        assert!(
            detail.contains("dict.json.bak-20260101-120000.json"),
            "应给出备份路径: {detail}"
        );
    }

    /// 部分成功时，既要报告回滚成功的文件，也要点名没回滚的文件。
    #[test]
    fn a_partial_rollback_lists_both_the_restored_and_the_stuck_saves() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf()).expect("数据目录");
        let stamp = "20260101-120000";

        // dict.json 能正常还原。
        let dict_path = store.file_path("dict.json");
        fs::write(&dict_path, "导入后的 dict").expect("写 dict");
        let dict_backup = store.file_path(&format!("dict.json.bak-{stamp}.json"));
        fs::write(&dict_backup, "导入前的 dict").expect("写 dict 备份");

        // setting.json 的位置被目录占住，还原必然失败。
        let setting_save = stuck_save(&store, "setting.json", stamp);

        let writer = ImportWriter {
            store: &store,
            stamp: stamp.to_string(),
            // done 按写入顺序记录，回滚是逆序：先试 setting，再试 dict。
            done: vec![
                Restore {
                    name: "dict.json".to_string(),
                    path: dict_path.clone(),
                    backup: Some(dict_backup),
                },
                setting_save,
            ],
        };

        let err = abort_with_rollback(&writer, anyhow!("写入 practice-word.json 失败"));
        let detail = format!("{err:#}");

        assert!(detail.contains("已回滚: dict.json"), "{detail}");
        assert!(detail.contains("未能回滚: setting.json"), "{detail}");
        assert!(
            detail.contains("setting.json.bak-20260101-120000.json"),
            "失败项要给出备份路径: {detail}"
        );
        assert_eq!(
            fs::read_to_string(&dict_path).expect("读 dict"),
            "导入前的 dict",
            "能还原的文件要真的还原"
        );
    }

    /// 一份都没写成就老老实实说「没有存档被改动」。
    #[test]
    fn aborting_before_any_write_reports_that_nothing_changed() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf()).expect("数据目录");
        let writer = ImportWriter {
            store: &store,
            stamp: "20260101-120000".to_string(),
            done: Vec::new(),
        };

        let err = abort_with_rollback(&writer, anyhow!("写入 dict.json 失败"));
        assert!(
            format!("{err:#}").contains("没有存档被改动"),
            "还没写任何文件时这句才成立"
        );
    }
}
