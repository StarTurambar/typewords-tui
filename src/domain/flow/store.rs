//! 自定义练习流程的存档（对应上游 `localStorage['PracticeFlowV2']`）。
//!
//! 上游把自建流程放在浏览器 localStorage 里，键是 `PracticeFlowV2`，值是
//! `{ activeId, flows: { [id]: { config, name, createdAt, updatedAt } } }`。
//! 本程序改存数据目录下的 `flows.json`，结构保持一致。
//!
//! 文件读写由 `store` 层负责（`FileStore::load_flows` / `save_flows`），
//! 领域层只管数据结构——损坏文件的隔离也在 store 层做。

use std::collections::BTreeMap;

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

    /// 分配一个没被占用的自建流程 id。
    ///
    /// 流程数量会因删除而减少，直接用 `flows.len() + 1` 会在删除后与已有流程
    /// 撞号，把别人的配置覆盖掉。这里逐个试探直到找到空位。
    pub fn unique_custom_id(&self) -> String {
        let mut index = 1usize;
        loop {
            let id = format!("custom-{index}");
            if !self.flows.contains_key(&id) {
                return id;
            }
            index += 1;
        }
    }

    /// 删除一条流程；被删的是当前启用流程时回到内置流程。
    pub fn remove(&mut self, id: &str) {
        self.flows.remove(id);
        if self.active_id == id {
            self.active_id.clear();
        }
    }
}
