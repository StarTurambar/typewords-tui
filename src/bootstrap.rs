//! 启动装载：读存档 + 词库，确保当前学习词典可用。

use anyhow::{Result, anyhow};

use crate::domain::catalog::Catalog;
use crate::domain::types::{BaseState, Dict, SettingState};
use crate::logging;
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
    /// 本次装载是否删除了失效的断点缓存（进度按词表校正过）。
    ///
    /// 删除发生在 [`Bootstrapped::normalize_study_progress`] 里：词表比存档登记的
    /// 短、`lastLearnIndex` 越界时，旧断点属于校正前的进度，续练会把夹回的进度
    /// 再算回去。这是**写副作用**，`tw task` 预览与 TUI 启动都走这条路，所以
    /// 命令行的输出要把它说出来（预览命令不该悄悄删用户的文件）。
    pub practice_cache_cleared: bool,
    /// 词典存档版本过高时为 true：本次运行不覆盖 dict.json。
    pub readonly_dict: bool,
    /// 设置存档版本过高时为 true：本次运行不覆盖 setting.json。
    pub readonly_settings: bool,
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
        self.normalize_study_progress();
        Ok(())
    }

    /// 按词表实际词数校准当前学习词典的词数与进度（对齐上游 `words.vue` init）。
    ///
    /// 词库目录里的 `length` 与存档里的 `lastLearnIndex` 都是「登记」性质的值：
    /// 词表文件删过词、或换成更短的词库后，它们会大于真实词数。学完判定一律按
    /// 实际词数走（`Dict::effective_length`），装载时先把当前词典校正一次；
    /// 校正过进度说明旧断点属于校正前的进度，顺手丢弃（上游同一处调
    /// `resetCacheData()`，否则续练会把夹回的进度再算回去）。
    ///
    /// 内置词典（收藏 / 错词 / 已掌握）不参与校正：它们的词条就存在存档里，
    /// `length` 由词条增删同步维护，上游也不做这一步。
    ///
    /// 真的删掉了一个缓存文件时把 [`Bootstrapped::practice_cache_cleared`] 置位：
    /// `tw task` 是写入口，删了用户的文件就必须在输出里说明。
    fn normalize_study_progress(&mut self) {
        let repaired = match self.base.study_dict_mut() {
            Some(dict) if !dict.system => dict.normalize_progress(),
            _ => false,
        };
        if !repaired {
            return;
        }
        // 读不出来的缓存不能删：与 `App::clear_practice_cache` 同一条不变量
        // ——文件还在，内容只是本次读不到，删掉等于替用户丢弃好档。
        if !self
            .store
            .cache_discardable(crate::store::PRACTICE_FILE_NAME)
        {
            logging::warn(format!(
                "练习缓存 {} 读不出来，校正进度时未删除；请关闭占用它的程序后重启",
                crate::store::PRACTICE_FILE_NAME
            ));
            return;
        }
        // 只有真的删掉了一个文件才算「本次清除了缓存」：文件本来就不存在时
        // 预览命令不该报一句「已清除」。删除失败同样不算，并留一条日志。
        let existed = self.store.practice_exists();
        match self.store.remove_practice() {
            Ok(()) => self.practice_cache_cleared = existed,
            Err(err) => logging::warn(format!("校正进度时删除练习缓存失败: {err:#}")),
        }
    }

    /// 确保当前词典已加载词表；为空则按 id 重新加载或选第一本词库。
    ///
    /// 内置词典（收藏 / 错词 / 已掌握）是例外：它们的词条存在存档里，没有
    /// 「按 id 从资源文件重新加载」这一步，词条为空也是合法状态。如果按普通
    /// 词库的规则回退到 cet4 / 第一本，用户上次特意选的收藏/错词会在重启后
    /// 悄悄消失。
    pub fn ensure_study_dict(&mut self) -> Result<()> {
        if self
            .base
            .study_dict()
            .map(|dict| dict.system)
            .unwrap_or(false)
        {
            return Ok(());
        }
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
    if let LoadStatus::Unreadable(reason) = &dict_loaded.status {
        // 读不到 ≠ 内容坏了：不能隔离，更不能拿默认值继续跑、退出时覆盖原档。
        anyhow::bail!(
            "读取词典存档失败：{reason}；请关闭占用该文件的程序（同步盘 / 杀毒软件）后重试"
        );
    }
    let setting_loaded = store.load_settings();
    if let LoadStatus::Unreadable(reason) = &setting_loaded.status {
        anyhow::bail!(
            "读取设置存档失败：{reason}；请关闭占用该文件的程序（同步盘 / 杀毒软件）后重试"
        );
    }

    let mut notices = Vec::new();
    if let Some(notice) = dict_loaded.notice() {
        notices.push(notice);
    }
    if let Some(notice) = setting_loaded.notice() {
        notices.push(notice);
    }
    // 步进串是「仍需手改 setting.json」的字段：无法解析时 `Fsrs::new` 会把
    // 整组回退成内置默认值。回退本身是静默的，这里把「哪些没被采用」推进
    // notices——启动时是 Warning Toast，同时由下面的循环写进运行日志。
    let invalid_steps = crate::domain::fsrs::invalid_steps(&setting_loaded.value.fsrs_parameters);
    if !invalid_steps.is_empty() {
        notices.push(format!(
            "FSRS 步进无法解析（{}），已整组回退默认步进：学习 1m / 10m、重学 10m",
            invalid_steps.join(" / ")
        ));
    }

    let catalog = Catalog::load(paths)?;
    let readonly_dict = matches!(dict_loaded.status, LoadStatus::VersionTooNew { .. });
    let readonly_settings = matches!(setting_loaded.status, LoadStatus::VersionTooNew { .. });
    let themes = ThemeRegistry::load(&paths.themes_dir());
    // 词库目录项因 url / language 不是单段路径被跳过：领域层只返回 warnings，
    // 展示交给这里（启动 Toast + 日志）与 `tw doctor` / `tw task` 的输出。
    notices.extend(catalog.warnings().iter().cloned());
    notices.extend(themes.warnings().iter().cloned());

    logging::info(format!(
        "存档装载: dict.json {:?}, setting.json {:?}",
        dict_loaded.status, setting_loaded.status
    ));
    logging::info(format!(
        "词库目录: {} 本；自定义主题: {} 个",
        catalog.resources().len(),
        themes.custom_count()
    ));
    for notice in &notices {
        // 这些告警在界面里只显示 3.2 秒的 Toast，用户回头排障时全靠日志。
        logging::warn(notice);
    }

    let mut bootstrapped = Bootstrapped {
        store,
        base: dict_loaded.value,
        settings: setting_loaded.value,
        catalog,
        themes,
        notices,
        practice_cache_cleared: false,
        readonly_dict,
        readonly_settings,
    };

    if let Some(id) = dict_id {
        // 内置词典（收藏 / 错词 / 已掌握）不在 Catalog 里，没有「按 id 加载词表」
        // 这一步——词条就在存档里，直接把 studyIndex 指过去即可。
        // 因此 tw --dict wordCollect 与词库页 Enter 得到同一结果。
        match bootstrapped.base.system_dict_index(id) {
            Some(index) => bootstrapped.base.set_study_index(index),
            None => bootstrapped.select_dict(id)?,
        }
    } else {
        bootstrapped.ensure_study_dict()?;
    }
    // 自建词典与内置词典不会从资源重载词表，也要按词表实际词数校准一次；
    // 资源词库在上面的 select_dict 里已经校准过（此处幂等）。
    bootstrapped.normalize_study_progress();
    if let Some(dict) = bootstrapped.study_dict() {
        logging::info(format!(
            "当前学习词典: {}（{} 词，每日 {} 词）",
            dict.name,
            dict.effective_length(),
            dict.per_day()
        ));
    }
    Ok(bootstrapped)
}
