//! 本地存档：JSON 原子写、版本校验、损坏隔离。
//!
//! 文件名与 Web 的 IndexedDB key 对应，便于将来互通：
//! - dict.json             ↔ typing-word-dict（版本 4）
//! - setting.json          ↔ typing-word-setting（版本 23）
//! - practice-word.json    ↔ PracticeSaveWord（版本 2）
//! - practice-article.json ↔ PracticeSaveArticle（版本 1）
//! - flows.json            ↔ localStorage['PracticeFlowV2']（无版本号）

use std::fs;
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
    /// 文件损坏，已隔离。
    Corrupt(String),
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

    pub fn notice(&self) -> Option<String> {
        match &self.status {
            LoadStatus::Corrupt(reason) => {
                Some(format!("存档已损坏，已隔离并恢复默认值：{reason}"))
            }
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
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "state.json".to_string());
        let target = path.with_file_name(format!("{file_name}.corrupt-{stamp}.json"));
        fs::rename(path, &target)
            .with_context(|| format!("隔离损坏存档失败: {}", path.display()))?;
        Ok(target)
    }

    /// 读入存档文件，但**只解析版本头**（`version` / `updated_at`），不碰 `val`。
    ///
    /// 版本判定必须先于载荷反序列化：高版本存档的 `val` 结构可能已经变了，
    /// 先整体反序列化会把「版本过高」误判成「损坏」——隔离原文件后，退出时
    /// 再用默认值写回同名路径，正好毁掉版本号要保护的新版本数据。
    fn read_raw(&self, name: &str) -> Result<Option<RawEnvelope>> {
        let path = self.file_path(name);
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("读取存档失败: {}", path.display()))?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        let header: EnvelopeHeader = serde_json::from_str(&text)
            .with_context(|| format!("解析存档失败: {}", path.display()))?;
        Ok(Some(RawEnvelope {
            text,
            version: header.version,
            // 只保留能用于展示的字符串形式；其它类型视为没有时间戳。
            updated_at: header
                .updated_at
                .and_then(|value| value.as_str().map(str::to_string)),
        }))
    }

    /// 版本合格后才反序列化整份信封，取出 `val`。
    fn decode_val<T: DeserializeOwned>(&self, name: &str, raw: &RawEnvelope) -> Result<T> {
        let envelope: SaveEnvelope<T> = serde_json::from_str(&raw.text)
            .with_context(|| format!("解析存档失败: {}", self.file_path(name).display()))?;
        Ok(envelope.val)
    }

    /// 隔离坏档，并生成给用户看的原因（成功时带上隔离后的路径）。
    fn quarantine_reason(&self, name: &str, err: &anyhow::Error) -> String {
        let path = self.file_path(name);
        match self.quarantine(&path) {
            Ok(target) => format!("{err}（已移动到 {}）", target.display()),
            Err(quarantine_err) => format!("{err}（隔离失败: {quarantine_err}）"),
        }
    }

    fn corrupt<T: Default>(&self, name: &str, err: &anyhow::Error) -> Loaded<T> {
        Loaded {
            value: T::default(),
            status: LoadStatus::Corrupt(self.quarantine_reason(name, err)),
            updated_at: None,
        }
    }

    fn load_typed<T: DeserializeOwned + Default>(
        &self,
        name: &str,
        expected_version: u32,
    ) -> Loaded<T> {
        let raw = match self.read_raw(name) {
            Ok(Some(raw)) => raw,
            Ok(None) => {
                return Loaded {
                    value: T::default(),
                    status: LoadStatus::Missing,
                    updated_at: None,
                };
            }
            Err(err) => return self.corrupt(name, &err),
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
    /// 版本相符但解析失败才隔离丢弃。
    fn load_cache<T: DeserializeOwned + Default>(
        &self,
        name: &str,
        expected_version: u32,
        corrupt_reason: &str,
    ) -> Loaded<T> {
        let raw = match self.read_raw(name) {
            Ok(Some(raw)) => raw,
            Ok(None) => {
                return Loaded {
                    value: T::default(),
                    status: LoadStatus::Missing,
                    updated_at: None,
                };
            }
            Err(_) => {
                let path = self.file_path(name);
                let _ = self.quarantine(&path);
                return Loaded {
                    value: T::default(),
                    status: LoadStatus::Corrupt(corrupt_reason.to_string()),
                    updated_at: None,
                };
            }
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
            Err(_) => {
                let path = self.file_path(name);
                let _ = self.quarantine(&path);
                Loaded {
                    value: T::default(),
                    status: LoadStatus::Corrupt(corrupt_reason.to_string()),
                    updated_at: None,
                }
            }
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
            "practice-word.json",
            PRACTICE_CACHE_VERSION,
            "练习缓存损坏，已丢弃",
        )
    }

    pub fn save_practice(&self, cache: Option<&PracticeCacheCompact>) -> Result<()> {
        match cache {
            Some(value) => {
                self.save_typed("practice-word.json", &Some(value), PRACTICE_CACHE_VERSION)
            }
            None => self.remove_practice(),
        }
    }

    pub fn remove_practice(&self) -> Result<()> {
        let path = self.file_path("practice-word.json");
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("删除练习缓存失败: {}", path.display()))?;
        }
        Ok(())
    }

    pub fn practice_exists(&self) -> bool {
        self.file_path("practice-word.json").is_file()
    }

    /// 读取文章练习缓存（`practice-article.json`）。
    ///
    /// 与单词练习缓存分开存：两者的进度互不相干，混在一个文件里会让
    /// 「清空单词练习进度」误删文章的跟打进度。
    pub fn load_practice_article(&self) -> Loaded<Option<ArticleCacheEnvelope>> {
        // 缓存损坏时直接丢弃，不阻断启动。
        self.load_cache(
            "practice-article.json",
            PRACTICE_ARTICLE_CACHE_VERSION,
            "文章练习缓存损坏，已丢弃",
        )
    }

    pub fn save_practice_article(&self, cache: Option<&ArticleCacheEnvelope>) -> Result<()> {
        match cache {
            Some(value) => self.save_typed(
                "practice-article.json",
                &Some(value),
                PRACTICE_ARTICLE_CACHE_VERSION,
            ),
            None => self.remove_practice_article(),
        }
    }

    pub fn remove_practice_article(&self) -> Result<()> {
        let path = self.file_path("practice-article.json");
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("删除文章练习缓存失败: {}", path.display()))?;
        }
        Ok(())
    }

    pub fn practice_article_exists(&self) -> bool {
        self.file_path("practice-article.json").is_file()
    }

    /// 读取自建流程存档（`flows.json`）。
    ///
    /// 这份存档没有版本号（结构与上游 localStorage 的 `PracticeFlowV2`
    /// 一致），损坏时隔离成 `*.corrupt-<时间戳>.json` 并回退到空集合。
    pub fn load_flows(&self) -> Loaded<FlowStore> {
        let path = self.file_path(FLOW_FILE_NAME);
        if !path.is_file() {
            return Loaded {
                value: FlowStore::default(),
                status: LoadStatus::Missing,
                updated_at: None,
            };
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                let reason = match self.quarantine(&path) {
                    Ok(target) => format!("{err}（已移动到 {}）", target.display()),
                    Err(quarantine_err) => format!("{err}（隔离失败: {quarantine_err}）"),
                };
                return Loaded {
                    value: FlowStore::default(),
                    status: LoadStatus::Corrupt(reason),
                    updated_at: None,
                };
            }
        };
        if text.trim().is_empty() {
            return Loaded {
                value: FlowStore::default(),
                status: LoadStatus::Missing,
                updated_at: None,
            };
        }
        match serde_json::from_str::<FlowStore>(&text) {
            Ok(value) => Loaded {
                value,
                status: LoadStatus::Loaded,
                updated_at: None,
            },
            Err(err) => {
                let reason = match self.quarantine(&path) {
                    Ok(target) => format!("{err}（已移动到 {}）", target.display()),
                    Err(quarantine_err) => format!("{err}（隔离失败: {quarantine_err}）"),
                };
                Loaded {
                    value: FlowStore::default(),
                    status: LoadStatus::Corrupt(reason),
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
            "practice-word.json",
            "practice-article.json",
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
