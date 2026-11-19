//! 本地存档：JSON 原子写、版本校验、损坏隔离。
//!
//! 文件名与 Web 的 IndexedDB key 对应，便于将来互通：
//! - dict.json             ↔ typing-word-dict（版本 4）
//! - setting.json          ↔ typing-word-setting（版本 23）
//! - practice-word.json    ↔ PracticeSaveWord（版本 2）
//! - practice-article.json ↔ PracticeSaveArticle（版本 1）
//! - flows.json            ↔ localStorage['PracticeFlowV2']（无版本号）

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::domain::article::ArticleCacheEnvelope;
use crate::domain::constants::{
    DICT_STATE_VERSION, PRACTICE_ARTICLE_CACHE_VERSION, PRACTICE_CACHE_VERSION, SETTING_VERSION,
};
use crate::domain::flow::store::{FILE_NAME as FLOW_FILE_NAME, FlowStore};
use crate::domain::types::{BaseState, PracticeCacheCompact, SettingState};

/// 原子写（实现在 `crate::fs`，与 `domain` 共用，避免 `domain ↔ store` 成环）。
pub use crate::fs::write_atomic;

/// 单词练习断点缓存的文件名。
///
/// 导出成常量，是因为「读不出来的缓存不能删 / 不能写」的判定分散在 app 层：
/// 文件名在两边各写一遍字面量，拼错就等于静默关掉保护。
pub const PRACTICE_FILE_NAME: &str = "practice-word.json";

/// 文章跟打断点缓存的文件名。
pub const PRACTICE_ARTICLE_FILE_NAME: &str = "practice-article.json";

#[derive(Serialize, Deserialize, Debug)]
pub struct SaveEnvelope<T> {
    pub val: T,
    pub version: u32,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Loaded,
    Missing,
    /// 文件损坏（版本头 / 载荷解析失败），已隔离。
    Corrupt(String),
    /// 存档读不出来（IO 错误：权限不足、被同步盘 / 杀毒软件独占、路径是目录等）。
    ///
    /// 读不到 ≠ 内容坏了：**不隔离、不改写、不算损坏**。隔离会把好档改名
    /// 移走，再用默认值写回同名路径更会丢数据。调用方按「本次运行只读」处理；
    /// `dict.json` / `setting.json` 在启动时直接中止（见 `bootstrap::load`）。
    Unreadable(String),
    /// 存档版本高于本程序支持的版本，不会覆盖原文件。
    VersionTooNew {
        found: u32,
        expected: u32,
    },
    /// 存档版本不是本程序支持的版本（过低或不兼容），已忽略且不移动文件。
    UnsupportedVersion {
        found: u32,
        expected: u32,
    },
}

#[derive(Clone, Debug)]
pub struct Loaded<T> {
    pub value: T,
    pub status: LoadStatus,
    pub updated_at: Option<String>,
}

impl<T> Loaded<T> {
    pub fn is_ok(&self) -> bool {
        matches!(self.status, LoadStatus::Loaded | LoadStatus::Missing)
    }

    /// 断点缓存的提示：损坏时只说「哪份缓存被丢弃」。
    ///
    /// `notice()` 的「存档已损坏，已隔离并恢复默认值」是词典 / 设置存档的口径；
    /// 缓存损坏的后果是丢弃，套上那层前缀会在一句话里出现两次「损坏」，
    /// 也与 persistence.md 记的文案对不上。
    pub fn cache_notice(&self) -> Option<String> {
        match &self.status {
            LoadStatus::Corrupt(reason) => Some(reason.clone()),
            _ => self.notice(),
        }
    }

    pub fn notice(&self) -> Option<String> {
        match &self.status {
            LoadStatus::Corrupt(reason) => {
                Some(format!("存档已损坏，已隔离并恢复默认值：{reason}"))
            }
            LoadStatus::Unreadable(reason) => Some(format!(
                "存档读不出来：{reason}；可能被同步盘 / 杀毒软件占用，本次运行不会改写它"
            )),
            LoadStatus::VersionTooNew { found, expected } => Some(format!(
                "存档版本过高（{found} > {expected}），本次不会覆盖，请升级后使用"
            )),
            LoadStatus::UnsupportedVersion { found, expected } => Some(format!(
                "存档版本不受支持（{found} != {expected}），已忽略；下次保存会覆盖"
            )),
            _ => None,
        }
    }
}

impl<T: Default> Loaded<T> {
    /// 文件不存在或只有空白：不隔离、不报错。
    fn missing() -> Self {
        Self {
            value: T::default(),
            status: LoadStatus::Missing,
            updated_at: None,
        }
    }

    /// 文件读不出来：默认值只用于本次运行，**不会**写回磁盘。
    fn unreadable(reason: String) -> Self {
        Self {
            value: T::default(),
            status: LoadStatus::Unreadable(reason),
            updated_at: None,
        }
    }
}

/// 存档信封的版本头：只解析 `version` / `updated_at`，故意不声明 `val`。
///
/// serde 默认忽略未知字段，因此高版本存档里结构已经变化的 `val` 不会
/// 影响这里的解析。`updated_at` 用 `Value` 接住：它只是展示用的元数据，
/// 未来版本即使换类型（例如改成毫秒时间戳）也不能让「版本过高」退化成
/// 「损坏」——否则高版本存档仍会被隔离、再被默认值覆盖。
#[derive(Deserialize)]
struct EnvelopeHeader {
    version: u32,
    #[serde(default)]
    updated_at: Option<serde_json::Value>,
}

/// 已读入内存、但只解析了版本头的信封。
struct RawEnvelope {
    text: String,
    version: u32,
    updated_at: Option<String>,
}

/// `read_raw` 的结果：不存在、读不到、读到了但版本头解析失败，或解析成功。
///
/// 这三种「失败」的处理完全不同：不存在是正常状态，读不到是只读，只有
/// 版本头解析失败才隔离。用 `Result` 把前两者混在 `Err` 里正是问题 2 的根源。
enum RawRead {
    Missing,
    Unreadable(String),
    Invalid(anyhow::Error),
    Text(RawEnvelope),
}

/// 隔离文件名：`<文件名>.corrupt-<stamp>.json`，同一秒重名时追加 `-2`、`-3`……
///
/// 不能直接 `rename` 到固定名：`rename` 会替换既有目标，同一秒第二次隔离
/// 就会把上一次的隔离副本（坏数据的手工恢复依据）静默覆盖掉。与
/// `backup::ImportWriter::next_backup_path` 同一套占名思路。
fn unique_corrupt_path(path: &Path, stamp: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "state.json".to_string());
    let mut index = 1;
    loop {
        let suffix = if index == 1 {
            String::new()
        } else {
            format!("-{index}")
        };
        let candidate = path.with_file_name(format!("{file_name}.corrupt-{stamp}{suffix}.json"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Clone, Debug)]
pub struct FileStore {
    home: PathBuf,
}

impl FileStore {
    pub fn new(home: impl Into<PathBuf>) -> Result<Self> {
        let home = home.into();
        fs::create_dir_all(&home)
            .with_context(|| format!("创建数据目录失败: {}", home.display()))?;
        Ok(Self { home })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn file_path(&self, name: &str) -> PathBuf {
        self.home.join(name)
    }

    fn quarantine(&self, path: &Path) -> Result<PathBuf> {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let target = unique_corrupt_path(path, &stamp);
        fs::rename(path, &target)
            .with_context(|| format!("隔离损坏存档失败: {}", path.display()))?;
        Ok(target)
    }

    /// 读取存档原文：不存在 / 只有空白 → `Ok(None)`，IO 失败 → `Err(原因)`。
    ///
    /// 直接 `fs::read_to_string` 而不是先 `is_file()`：后者是 TOCTOU（检查与
    /// 读取之间文件可能被删 / 换成目录），而且会把「路径是目录」这类 IO 错误
    /// 误判成「不存在」，于是拿默认值继续跑、退出时再覆盖原路径。
    fn read_text(&self, name: &str) -> std::result::Result<Option<String>, String> {
        let path = self.file_path(name);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(format!("{}（{err}）", path.display())),
        };
        // 读取容忍开头的 UTF-8 BOM（Windows 记事本等工具的默认行为）；
        // 只剥一个字符，写回时序列化器本来就不会带 BOM。无 BOM 是常见情况，
        // 直接交出原 String，避免为每份存档多拷一次。
        let text = match text.strip_prefix('\u{feff}') {
            Some(rest) => rest.to_string(),
            None => text,
        };
        // 空白判定放在剥离之后：只含 BOM 的文件与空文件一样是「缺失」，
        // 不能拿空串去解析、再把好档判成损坏。
        if text.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(text))
    }

    /// 读入存档文件，但**只解析版本头**（`version` / `updated_at`），不碰 `val`。
    ///
    /// 版本判定必须先于载荷反序列化：高版本存档的 `val` 结构可能已经变了，
    /// 先整体反序列化会把「版本过高」误判成「损坏」——隔离原文件后，退出时
    /// 再用默认值写回同名路径，正好毁掉版本号要保护的新版本数据。
    fn read_raw(&self, name: &str) -> RawRead {
        let text = match self.read_text(name) {
            Ok(None) => return RawRead::Missing,
            Err(reason) => return RawRead::Unreadable(reason),
            Ok(Some(text)) => text,
        };
        match serde_json::from_str::<EnvelopeHeader>(&text) {
            Ok(header) => RawRead::Text(RawEnvelope {
                text,
                version: header.version,
                // 只保留能用于展示的字符串形式；其它类型视为没有时间戳。
                updated_at: header
                    .updated_at
                    .and_then(|value| value.as_str().map(str::to_string)),
            }),
            Err(err) => RawRead::Invalid(
                anyhow::Error::new(err)
                    .context(format!("解析存档失败: {}", self.file_path(name).display())),
            ),
        }
    }

    /// 读取一份存档信封的版本头（`version` 字段），不碰 `val`。
    ///
    /// 只解析 `version` / `updated_at`：高版本存档的载荷结构可能已经变了。
    /// 文件不存在 / 读不到 / 版本头解析失败都返回 `None`——那几种情况由装载
    /// 路径（`load_*`）分类处理，这里只回答「读得到版本号吗」。给「写前先看
    /// 一眼」的入口复用（`tw import-web` 覆盖前的只读保护）。
    pub fn envelope_version(&self, name: &str) -> Option<u32> {
        match self.read_raw(name) {
            RawRead::Text(raw) => Some(raw.version),
            _ => None,
        }
    }

    /// 版本合格后才反序列化整份信封，取出 `val`。
    fn decode_val<T: DeserializeOwned>(&self, name: &str, raw: &RawEnvelope) -> Result<T> {
        let envelope: SaveEnvelope<T> = serde_json::from_str(&raw.text)
            .with_context(|| format!("解析存档失败: {}", self.file_path(name).display()))?;
        Ok(envelope.val)
    }

    /// 隔离坏档，并生成给用户看的原因（成功时带上隔离后的路径）。
    fn quarantine_reason(&self, name: &str, reason: &str) -> String {
        let path = self.file_path(name);
        match self.quarantine(&path) {
            Ok(target) => format!("{reason}（已移动到 {}）", target.display()),
            Err(quarantine_err) => format!("{reason}（隔离失败: {quarantine_err}）"),
        }
    }

    fn corrupt<T: Default>(&self, name: &str, err: &anyhow::Error) -> Loaded<T> {
        let reason = format!("{err}");
        Loaded {
            value: T::default(),
            status: LoadStatus::Corrupt(self.quarantine_reason(name, &reason)),
            updated_at: None,
        }
    }

    /// 断点缓存损坏：文案保留「哪份缓存 + 丢弃原因」，隔离结果由
    /// `quarantine_reason` 追加，用户能据此找到隔离副本。
    fn corrupt_cache<T: Default>(&self, name: &str, label: &str, err: &anyhow::Error) -> Loaded<T> {
        let reason = format!("{label}：{err}");
        Loaded {
            value: T::default(),
            status: LoadStatus::Corrupt(self.quarantine_reason(name, &reason)),
            updated_at: None,
        }
    }

    fn load_typed<T: DeserializeOwned + Default>(
        &self,
        name: &str,
        expected_version: u32,
    ) -> Loaded<T> {
        let raw = match self.read_raw(name) {
            RawRead::Missing => return Loaded::missing(),
            // 读不到 ≠ 内容坏了：原文件保留，本次按只读处理。
            RawRead::Unreadable(reason) => return Loaded::unreadable(reason),
            RawRead::Invalid(err) => return self.corrupt(name, &err),
            RawRead::Text(raw) => raw,
        };
        // 版本过高时直接返回，绝不解析 `val`，也不隔离文件。
        if raw.version > expected_version {
            return Loaded {
                value: T::default(),
                status: LoadStatus::VersionTooNew {
                    found: raw.version,
                    expected: expected_version,
                },
                updated_at: raw.updated_at.clone(),
            };
        }
        match self.decode_val(name, &raw) {
            Ok(value) => Loaded {
                value,
                status: LoadStatus::Loaded,
                updated_at: raw.updated_at,
            },
            Err(err) => self.corrupt(name, &err),
        }
    }

    /// 断点缓存的装载：版本不符时忽略（不解析 `val`、不移动文件），
    /// 版本相符但解析失败才隔离丢弃；读不到则原样保留、本次只读。
    fn load_cache<T: DeserializeOwned + Default>(
        &self,
        name: &str,
        expected_version: u32,
        corrupt_label: &str,
    ) -> Loaded<T> {
        let raw = match self.read_raw(name) {
            RawRead::Missing => return Loaded::missing(),
            RawRead::Unreadable(reason) => return Loaded::unreadable(reason),
            RawRead::Invalid(err) => return self.corrupt_cache(name, corrupt_label, &err),
            RawRead::Text(raw) => raw,
        };
        if raw.version != expected_version {
            return Loaded {
                value: T::default(),
                status: LoadStatus::UnsupportedVersion {
                    found: raw.version,
                    expected: expected_version,
                },
                updated_at: raw.updated_at.clone(),
            };
        }
        match self.decode_val(name, &raw) {
            Ok(value) => Loaded {
                value,
                status: LoadStatus::Loaded,
                updated_at: raw.updated_at,
            },
            Err(err) => self.corrupt_cache(name, corrupt_label, &err),
        }
    }

    fn save_typed<T: Serialize>(&self, name: &str, value: &T, version: u32) -> Result<()> {
        let envelope = SaveEnvelope {
            val: value,
            version,
            updated_at: Some(now_rfc3339()),
        };
        let text = serde_json::to_string(&envelope)?;
        write_atomic(&self.file_path(name), &text)
    }

    /// 原样写入一份存档（用于网页版备份导入，保留未建模的字段）。
    ///
    /// 调用方必须先用目标存档类型做一次反序列化校验（见
    /// `backup::import_web_value`）；这里只做字节级原子替换、不解析内容，
    /// 以免把可读但不兼容的备份写进数据目录。目前只有导入路径使用。
    pub(crate) fn write_envelope_raw(&self, name: &str, text: &str) -> Result<()> {
        write_atomic(&self.file_path(name), text)
    }

    pub fn load_dict(&self) -> Loaded<BaseState> {
        self.load_typed("dict.json", DICT_STATE_VERSION)
    }

    pub fn save_dict(&self, state: &BaseState) -> Result<()> {
        let mut saved = state.clone();
        saved.shake_common_dict();
        self.save_typed("dict.json", &saved, DICT_STATE_VERSION)
    }

    pub fn load_settings(&self) -> Loaded<SettingState> {
        self.load_typed("setting.json", SETTING_VERSION)
    }

    pub fn save_settings(&self, settings: &SettingState) -> Result<()> {
        self.save_typed("setting.json", settings, SETTING_VERSION)
    }

    pub fn load_practice(&self) -> Loaded<Option<PracticeCacheCompact>> {
        // 练习缓存损坏时直接丢弃，不阻断启动。
        self.load_cache(
            PRACTICE_FILE_NAME,
            PRACTICE_CACHE_VERSION,
            "练习缓存损坏，已丢弃",
        )
    }

    pub fn save_practice(&self, cache: Option<&PracticeCacheCompact>) -> Result<()> {
        match cache {
            Some(value) => {
                self.save_typed(PRACTICE_FILE_NAME, &Some(value), PRACTICE_CACHE_VERSION)
            }
            None => self.remove_practice(),
        }
    }

    /// 一份缓存读不出来的原因；读得出来（或本来就不存在）时为 `None`。
    ///
    /// 只走读取判定，不解析、不隔离，因此没有副作用——给「删除 / 写入前先探
    /// 一下」的入口用（`--reset-practice` 与启动时的进度校正）。
    pub fn cache_read_error(&self, name: &str) -> Option<String> {
        self.read_text(name).err()
    }

    /// 缓存能否安全删除：读得出来（或本来就不存在）时为 true。
    ///
    /// 「读不出来的缓存不能删」是 store 层的判定：文件还在，内容只是本次读不到
    /// （被同步盘 / 杀毒软件占用），删掉等于替用户丢弃好档。
    pub fn cache_discardable(&self, name: &str) -> bool {
        self.cache_read_error(name).is_none()
    }

    pub fn remove_practice(&self) -> Result<()> {
        let path = self.file_path(PRACTICE_FILE_NAME);
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("删除练习缓存失败: {}", path.display()))?;
        }
        Ok(())
    }

    pub fn practice_exists(&self) -> bool {
        self.file_path(PRACTICE_FILE_NAME).is_file()
    }

    /// 读取文章练习缓存（`practice-article.json`）。
    ///
    /// 与单词练习缓存分开存：两者的进度互不相干，混在一个文件里会让
    /// 「清空单词练习进度」误删文章的跟打进度。
    pub fn load_practice_article(&self) -> Loaded<Option<ArticleCacheEnvelope>> {
        // 缓存损坏时直接丢弃，不阻断启动。
        self.load_cache(
            PRACTICE_ARTICLE_FILE_NAME,
            PRACTICE_ARTICLE_CACHE_VERSION,
            "文章练习缓存损坏，已丢弃",
        )
    }

    pub fn save_practice_article(&self, cache: Option<&ArticleCacheEnvelope>) -> Result<()> {
        match cache {
            Some(value) => self.save_typed(
                PRACTICE_ARTICLE_FILE_NAME,
                &Some(value),
                PRACTICE_ARTICLE_CACHE_VERSION,
            ),
            None => self.remove_practice_article(),
        }
    }

    pub fn remove_practice_article(&self) -> Result<()> {
        let path = self.file_path(PRACTICE_ARTICLE_FILE_NAME);
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("删除文章练习缓存失败: {}", path.display()))?;
        }
        Ok(())
    }

    pub fn practice_article_exists(&self) -> bool {
        self.file_path(PRACTICE_ARTICLE_FILE_NAME).is_file()
    }

    /// 读取自建流程存档（`flows.json`）。
    ///
    /// 这份存档没有版本号（结构与上游 localStorage 的 `PracticeFlowV2`
    /// 一致），解析失败时隔离成 `*.corrupt-<时间戳>.json` 并回退到空集合；
    /// 读不到（IO 错误）时不隔离、不删除，本次运行按只读处理。
    pub fn load_flows(&self) -> Loaded<FlowStore> {
        let text = match self.read_text(FLOW_FILE_NAME) {
            Ok(None) => return Loaded::missing(),
            Err(reason) => return Loaded::unreadable(reason),
            Ok(Some(text)) => text,
        };
        match serde_json::from_str::<FlowStore>(&text) {
            Ok(value) => Loaded {
                value,
                status: LoadStatus::Loaded,
                updated_at: None,
            },
            Err(err) => {
                let err = anyhow::Error::new(err).context(format!(
                    "解析存档失败: {}",
                    self.file_path(FLOW_FILE_NAME).display()
                ));
                let reason = format!("{err}");
                Loaded {
                    value: FlowStore::default(),
                    status: LoadStatus::Corrupt(self.quarantine_reason(FLOW_FILE_NAME, &reason)),
                    updated_at: None,
                }
            }
        }
    }

    /// 写回自建流程存档。
    pub fn save_flows(&self, flows: &FlowStore) -> Result<()> {
        let text = serde_json::to_string_pretty(flows)?;
        write_atomic(&self.file_path(FLOW_FILE_NAME), &text)
    }

    /// 供 doctor 展示：列出已知存档文件与大小。
    ///
    /// 顺序与模块文档一致（词典 → 设置 → 单词断点 → 文章断点 → 自建流程）。
    /// 这里漏掉一份存档，`tw doctor` 就会在排障时看不到它对应的断点或进度。
    pub fn describe_files(&self) -> Vec<(String, Option<u64>)> {
        [
            "dict.json",
            "setting.json",
            PRACTICE_FILE_NAME,
            PRACTICE_ARTICLE_FILE_NAME,
            FLOW_FILE_NAME,
        ]
        .iter()
        .map(|name| {
            let path = self.file_path(name);
            let size = fs::metadata(&path).ok().map(|meta| meta.len());
            ((*name).to_string(), size)
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_names_do_not_overwrite_an_existing_copy_in_the_same_second() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dict.json");
        let first = unique_corrupt_path(&path, "20260101-120000");
        assert_eq!(
            first.file_name().unwrap(),
            "dict.json.corrupt-20260101-120000.json"
        );
        // 同一秒的第一次隔离已经落盘：第二次必须换名，不能把它覆盖掉。
        fs::write(&first, "{ 上一次的隔离副本 }").unwrap();

        let second = unique_corrupt_path(&path, "20260101-120000");
        assert_eq!(
            second.file_name().unwrap(),
            "dict.json.corrupt-20260101-120000-2.json"
        );
        assert_ne!(first, second);
        assert!(
            fs::read_to_string(&first).unwrap().contains("上一次"),
            "既有隔离副本不能被静默覆盖"
        );
    }

    #[test]
    fn an_unreadable_cache_is_reported_as_unreadable_instead_of_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path()).unwrap();
        // 路径做成目录：Unix 得到 EISDIR，Windows 得到拒绝访问，都走 IO 分支。
        fs::create_dir(store.file_path("practice-word.json")).unwrap();

        let loaded = store.load_practice();
        assert!(matches!(loaded.status, LoadStatus::Unreadable(_)));
        assert!(store.file_path("practice-word.json").is_dir());
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .all(|entry| !entry.file_name().to_string_lossy().contains("corrupt")),
            "读不到不能隔离"
        );
    }

    /// 只含一个 BOM 的文件等于空文件：是「缺失」，不是「损坏」。
    ///
    /// 剥离 BOM 必须先于空白判定，否则空串会被送进解析器、再按损坏隔离。
    #[test]
    fn a_file_with_only_a_bom_counts_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path()).unwrap();
        fs::write(store.file_path("dict.json"), "\u{feff}").unwrap();

        let loaded = store.load_dict();
        assert!(matches!(loaded.status, LoadStatus::Missing));
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .all(|entry| !entry.file_name().to_string_lossy().contains("corrupt")),
            "只有 BOM 的文件不该被隔离"
        );
    }

    #[test]
    fn a_cache_that_cannot_be_read_is_not_discardable() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path()).unwrap();
        let path = store.file_path(PRACTICE_FILE_NAME);

        // 不存在：删也无所谓。
        assert!(store.cache_discardable(PRACTICE_FILE_NAME));
        fs::write(&path, "{\"val\":null,\"version\":2}").unwrap();
        assert!(store.cache_discardable(PRACTICE_FILE_NAME));

        // 路径是目录：读不出来，不能删（与 App::clear_practice_cache 同一条不变量）。
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(!store.cache_discardable(PRACTICE_FILE_NAME));
        assert!(store.cache_read_error(PRACTICE_FILE_NAME).is_some());
    }
}
