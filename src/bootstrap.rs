//! 启动装载：读存档 + 词库，确保当前学习词典可用。

use anyhow::{Result, anyhow};

use crate::domain::catalog::Catalog;
use crate::domain::types::{BaseState, Dict, SettingState};
use crate::paths::Paths;
use crate::store::{FileStore, LoadStatus};
use crate::theme::ThemeRegistry;

pub struct Bootstrapped {
    pub store: FileStore,
    pub base: BaseState,
    pub settings: SettingState,
    pub catalog: Catalog,
    pub themes: ThemeRegistry,
    pub notices: Vec<String>,
    /// 词典存档版本过高时为 true：本次运行不覆盖 dict.json。
    pub readonly_dict: bool,
}

impl Bootstrapped {
    /// 切换当前学习词典（加载词表并保留既有进度）。
    pub fn select_dict(&mut self, id: &str) -> Result<()> {
        let resource = self
            .catalog
            .find(id)
            .cloned()
            .ok_or_else(|| anyhow!("未找到词库: {id}"))?;
        let words = self.catalog.load_words(&resource)?;
        let mut dict = resource.to_dict(words);
        if let Some(existing) = self
            .base
            .word
            .book_list
            .iter()
            .find(|item| item.matches_id(&dict.id) || item.matches_id(&resource.en_name))
        {
            dict.last_learn_index = existing.last_learn_index;
            dict.per_day_study_number = existing.per_day_study_number;
            dict.complete = existing.complete;
            dict.statistics = existing.statistics.clone();
        }
        if dict.per_day_study_number == 0 {
            dict.per_day_study_number = 20;
        }
        if dict.per_day_study_number > dict.words.len() {
            dict.per_day_study_number = dict.words.len().max(1);
        }
        self.base.set_study_dict(dict);
        Ok(())
    }

    /// 确保当前词典已加载词表；为空则按 id 重新加载或选第一本词库。
    pub fn ensure_study_dict(&mut self) -> Result<()> {
        let need_load = self
            .base
            .study_dict()
            .map(|dict| dict.words.is_empty())
            .unwrap_or(true);
        if !need_load {
            return Ok(());
        }
        let id = self
            .base
            .study_dict()
            .map(|dict| {
                if dict.id.is_empty() {
                    dict.en_name.clone()
                } else {
                    dict.id.clone()
                }
            })
            .unwrap_or_default();
        let fallback = if !id.is_empty() && self.catalog.find(&id).is_some() {
            id
        } else if self.catalog.find("cet4").is_some() {
            "cet4".to_string()
        } else {
            self.catalog
                .resources()
                .first()
                .map(|resource| resource.id_string())
                .ok_or_else(|| anyhow!("没有可用词库"))?
        };
        self.select_dict(&fallback)
    }

    pub fn study_dict(&self) -> Option<&Dict> {
        self.base.study_dict()
    }
}

pub fn load(paths: &Paths, dict_id: Option<&str>) -> Result<Bootstrapped> {
    let store = FileStore::new(&paths.home)?;
    let dict_loaded = store.load_dict();
    let setting_loaded = store.load_settings();

    let mut notices = Vec::new();
    if let Some(notice) = dict_loaded.notice() {
        notices.push(notice);
    }
    if let Some(notice) = setting_loaded.notice() {
        notices.push(notice);
    }

    let catalog = Catalog::load(paths)?;
    let readonly_dict = matches!(dict_loaded.status, LoadStatus::VersionTooNew { .. });
    let themes = ThemeRegistry::load(&paths.themes_dir());
    notices.extend(themes.warnings().iter().cloned());

    let mut bootstrapped = Bootstrapped {
        store,
        base: dict_loaded.value,
        settings: setting_loaded.value,
        catalog,
        themes,
        notices,
        readonly_dict,
    };

    if let Some(id) = dict_id {
        bootstrapped.select_dict(id)?;
    } else {
        bootstrapped.ensure_study_dict()?;
    }
    Ok(bootstrapped)
}
