//! 自定义练习流程的存档（对应上游 `localStorage['PracticeFlowV2']`）。
//!
//! 上游把自建流程放在浏览器 localStorage 里，键是 `PracticeFlowV2`，值是
//! `{ activeId, flows: { [id]: { config, name, createdAt, updatedAt } } }`。
//! 本程序改存数据目录下的 `flows.json`，结构保持一致。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::domain::flow::PracticeFlowConfig;

/// 存档文件名。
pub const FILE_NAME: &str = "flows.json";

/// 一条自建流程。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredFlow {
    /// 流程名称
    pub name: String,
    /// 流程定义
    pub config: PracticeFlowConfig,
    /// 创建时间（毫秒时间戳）
    pub created_at: i64,
    /// 更新时间（毫秒时间戳）
    pub updated_at: i64,
}

/// 自建流程集合。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FlowStore {
    /// 当前启用的流程 id；为空表示使用内置流程
    pub active_id: String,
    /// 流程 id → 流程
    pub flows: BTreeMap<String, StoredFlow>,
}

impl FlowStore {
    /// 读取数据目录里的流程存档；文件不存在或损坏时返回空集合。
    pub fn load(home: &Path) -> Self {
        let path = home.join(FILE_NAME);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// 写回数据目录。
    pub fn save(&self, home: &Path) -> Result<()> {
        let path = home.join(FILE_NAME);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录失败: {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self)?;
        crate::store::write_atomic(&path, &text)
    }

    /// 当前启用的自建流程。
    pub fn active(&self) -> Option<&StoredFlow> {
        if self.active_id.is_empty() {
            return None;
        }
        self.flows.get(&self.active_id)
    }

    /// 启用某个流程（传空串表示回到内置流程）。
    pub fn activate(&mut self, id: &str) {
        self.active_id = id.to_string();
    }

    /// 新增或覆盖一条流程，返回它的 id。
    pub fn upsert(&mut self, id: &str, flow: StoredFlow) {
        self.flows.insert(id.to_string(), flow);
    }

    /// 删除一条流程；被删的是当前启用流程时回到内置流程。
    pub fn remove(&mut self, id: &str) {
        self.flows.remove(id);
        if self.active_id == id {
            self.active_id.clear();
        }
    }
}
