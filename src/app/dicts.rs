//! 词库页条目：三本内置词典 + 词库资源目录。
//!
//! 词库页的行、键盘光标、鼠标命中与「设为当前 / 打开词条页」都按**同一条
//! 列表**寻址。内置词典（收藏 / 错词 / 已掌握）排在资源词库之前，因此
//! `dict_cursor` 是**条目下标**而不是 Catalog 下标——任何用到它去查词库的
//! 地方都必须先经过 [`App::dict_entry_at`] / [`App::dict_entries`]，
//! 直接索引 `Catalog::resources()` 会整体错位一行。
//!
//! 内置词典的词条存在 `dict.json` 里（`system: true`），资源词库的词条按需
//! 从词条文件加载；两者的「当前」「词数」「进度」口径因此在同一处收敛。

use crate::domain::constants::system_dict_hint;
use crate::domain::types::Dict;

use super::App;

/// 词库页一行的来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictEntryKind {
    /// 内置词典（收藏 / 错词 / 已掌握）：`index` 是 `word.book_list` 下标。
    System,
    /// 词库资源：`index` 是 `Catalog::resources()` 下标。
    Resource,
}

/// 词库页一行的展示数据。
///
/// 全部是拥有的值：渲染、详情面板与底栏文案都只读这一份快照，不再各自去翻
/// `Catalog` 与 `book_list`——两处各算一遍必然漂移（行上写着 12 词、
/// 详情面板写着 20 词）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DictEntry {
    pub kind: DictEntryKind,
    /// `word.book_list` 下标（内置词典）或 `Catalog::resources()` 下标（词库资源）。
    pub index: usize,
    pub name: String,
    pub id: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    /// 词数：内置词典取实际词条数，资源取目录里登记的长度。
    pub length: usize,
    /// 学习进度百分比（内置词典平时不推进，界面不给它们画进度条）。
    pub percent: u32,
    /// 是否是当前学习词典。
    pub current: bool,
    /// 是否是内置词典（等价于 `kind == DictEntryKind::System`，给界面直接判断用）。
    pub system: bool,
    /// 每日词数；资源词库还没成为当前词典时为 `None`（详情面板据此提示）。
    pub per_day: Option<usize>,
}

impl App {
    /// 词库页的全部条目：内置词典在前，词库资源在后。
    pub fn dict_entries(&self) -> Vec<DictEntry> {
        let base = &self.boot.base;
        let study = base.study_dict();
        let study_index = base.study_dict_index();
        let mut entries = Vec::new();

        for (index, dict) in base
            .word
            .book_list
            .iter()
            .enumerate()
            .filter(|(_, dict)| dict.system)
        {
            entries.push(DictEntry {
                kind: DictEntryKind::System,
                index,
                name: dict.name.clone(),
                id: dict.id.clone(),
                description: system_dict_hint(&dict.id).to_string(),
                category: "内置词典".to_string(),
                tags: Vec::new(),
                length: dict.effective_length(),
                percent: dict.progress_percent(),
                current: study_index == Some(index),
                system: true,
                per_day: Some(dict.per_day()),
            });
        }

        for (index, resource) in self.boot.catalog.resources().iter().enumerate() {
            let id = resource.id_string();
            let owned = base
                .word
                .book_list
                .iter()
                .find(|dict| dict.matches_id(&id) || dict.matches_id(&resource.en_name));
            // 资源词库按 id / enName 认「当前」：`select_dict` 用的是
            // `set_study_dict`，可能更新既有条目也可能追加，下标不保证稳定。
            let current = study
                .map(|study| study.matches_id(&id) || study.matches_id(&resource.en_name))
                .unwrap_or(false);
            entries.push(DictEntry {
                kind: DictEntryKind::Resource,
                index,
                name: resource.name.clone(),
                id,
                description: resource.description.clone(),
                category: resource.category.clone(),
                tags: resource.tags.clone(),
                length: if resource.length > 0 {
                    resource.length
                } else {
                    owned.map(Dict::effective_length).unwrap_or(0)
                },
                percent: owned.map(Dict::progress_percent).unwrap_or(0),
                current,
                system: false,
                per_day: owned.map(Dict::per_day),
            });
        }

        entries
    }

    /// 词库页第 `position` 行的条目。
    pub fn dict_entry_at(&self, position: usize) -> Option<DictEntry> {
        self.dict_entries().into_iter().nth(position)
    }

    /// 词库页条目总数（光标上界）。
    pub fn dict_entry_count(&self) -> usize {
        self.dict_entries().len()
    }

    /// 当前学习词典在词库页里的位置；没有学习词典时返回 `None`。
    ///
    /// 进入词库页时用它把光标停在当前词典上——列表第一行现在是内置词典
    /// 「收藏」，停在 0 会让一次 Enter 就把学习词典换成收藏。
    pub fn dict_entry_of_study(&self) -> Option<usize> {
        self.dict_entries().iter().position(|entry| entry.current)
    }
}
