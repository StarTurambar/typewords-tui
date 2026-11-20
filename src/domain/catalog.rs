//! 词库目录与词条文件加载。
//!
//! 资源布局与 Web 端一致：
//! - `<root>/list/word.json`：词库目录
//! - `<root>/dicts/<language>/word/<url>`：词条数据
//!
//! 查找顺序：数据目录（用户 `import-dict`）优先，其次只读资源目录。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::domain::types::{Dict, Word};
use crate::paths::Paths;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DictResource {
    pub id: serde_json::Value,
    #[serde(alias = "en_name")]
    pub en_name: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub length: usize,
    pub category: String,
    pub tags: Vec<String>,
    pub language: String,
    pub translate_language: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub version: Option<u32>,
    pub cover: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl DictResource {
    pub fn id_string(&self) -> String {
        match &self.id {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Number(number) => number.to_string(),
            _ => String::new(),
        }
    }

    pub fn language_or_default(&self) -> &str {
        if self.language.is_empty() {
            "en"
        } else {
            &self.language
        }
    }

    /// 目录项里的 `url` / `language` 不能安全拼进 `<home>/dicts` 时的原因。
    ///
    /// 这两个字段是 `<home>/list/word.json` 里原样读取的字符串，用户手工编辑
    /// 就能写进 `../../x.json`、`/etc/x.json`、`C:\x.json` 之类的东西，而
    /// [`Catalog::dict_file`] / [`Catalog::dict_write_file`] 直接把它们 `join`
    /// 到数据目录上——词条的读写会逃出 `<home>/dicts`。`import-dict` 生成的
    /// url 恒为单段文件名（且导入前已拒绝路径分隔符），所以这条只可能来自
    /// 手工编辑：装载时跳过并告警，而不是到写盘时才发现路径不对。
    fn unsafe_path_reason(&self) -> Option<String> {
        if !is_single_path_segment(&self.url) {
            return Some(format!(
                "url「{}」不是单段文件名（不能含路径分隔符，不能是 . 或 ..）",
                self.url
            ));
        }
        // 空 language 合法：`language_or_default` 会补成 `en`。
        if !self.language.is_empty() && !is_single_path_segment(&self.language) {
            return Some(format!(
                "language「{}」不是单段目录名（不能含路径分隔符，不能是 . 或 ..）",
                self.language
            ));
        }
        None
    }

    pub fn matches_id(&self, id: &str) -> bool {
        !id.is_empty()
            && (self.id_string() == id || (!self.en_name.is_empty() && self.en_name == id))
    }

    pub fn is_word_dict(&self) -> bool {
        self.kind
            .as_deref()
            .map(|kind| kind == "word")
            .unwrap_or(true)
    }

    /// 是否是用户自建的词典（目录项里的 `custom` 标记）。
    pub fn is_custom(&self) -> bool {
        self.extra
            .get("custom")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }

    /// 转为可持久化的 Dict（词表由调用方补）。
    pub fn to_dict(&self, words: Vec<Word>) -> Dict {
        Dict {
            id: self.id_string(),
            en_name: self.en_name.clone(),
            name: if self.name.is_empty() {
                self.id_string()
            } else {
                self.name.clone()
            },
            description: self.description.clone(),
            url: self.url.clone(),
            // 词数是词条数据的派生量：拿到词表就以词条数为准，登记值只在
            // 词表为空（空的自建词典、还没加载）时兜底。反过来的优先级会让
            // 编辑 / 删除词条后登记值把 `length` 顶回原值，主页磁贴与词库页
            // 从此各显示一个数。
            length: if words.is_empty() {
                self.length
            } else {
                words.len()
            },
            category: self.category.clone(),
            tags: self.tags.clone(),
            language: self.language_or_default().to_string(),
            cover: self.cover.clone(),
            per_day_study_number: 20,
            // `custom` 必须跟着走：存档落盘前会清掉非自建词典的词表
            // （`BaseState::shake_common_dict`），标记丢了自建词典的词表
            // 就被清空了。
            custom: self.is_custom(),
            words,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct Catalog {
    assets: PathBuf,
    home: PathBuf,
    resources: Vec<DictResource>,
    /// 装载时被跳过的目录项（路径不安全）。与 `ThemeRegistry::warnings` 同一个
    /// 模式：领域层不弹 UI、不写日志，把「哪里被跳过、为什么」交给上层展示。
    warnings: Vec<String>,
}

impl Catalog {
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut resources: Vec<DictResource> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut warnings: Vec<String> = Vec::new();

        for root in [&paths.assets, &paths.home] {
            let file = root.join("list").join("word.json");
            if !file.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("读取词库目录失败: {}", file.display()))?;
            let parsed: Vec<DictResource> = serde_json::from_str(crate::fs::strip_utf8_bom(&text))
                .with_context(|| format!("解析词库目录失败: {}", file.display()))?;
            for resource in parsed {
                if !resource.is_word_dict() {
                    continue;
                }
                let key = resource.id_string();
                if key.is_empty() {
                    continue;
                }
                // 内置目录与用户目录走同一条装载路径：只跳过真正不安全的项，
                // 正常条目（url 是单段文件名、language 为空或单段）一律不受影响。
                if let Some(reason) = resource.unsafe_path_reason() {
                    warnings.push(format!("词库目录项「{key}」已跳过：{reason}"));
                    continue;
                }
                match index.get(&key) {
                    Some(existing) => resources[*existing] = resource,
                    None => {
                        index.insert(key, resources.len());
                        resources.push(resource);
                    }
                }
            }
        }

        if resources.is_empty() {
            let mut message = format!(
                "词库目录为空，请检查 {} 或数据目录下的 list/word.json",
                paths.assets.join("list").join("word.json").display()
            );
            // 全是「被跳过的目录项」时「目录为空」会把人引向错误的方向：
            // 目录文件明明有内容，是每个条目都不合格。
            if !warnings.is_empty() {
                message.push_str(&format!(
                    "；{} 个目录项因路径不安全被跳过：{}",
                    warnings.len(),
                    warnings.join("；")
                ));
            }
            return Err(anyhow!(message));
        }

        Ok(Self {
            assets: paths.assets.clone(),
            home: paths.home.clone(),
            resources,
            warnings,
        })
    }

    pub fn resources(&self) -> &[DictResource] {
        &self.resources
    }

    /// 装载时被跳过的目录项说明（目前只有路径不安全这一种）。
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn find(&self, id: &str) -> Option<&DictResource> {
        self.resources
            .iter()
            .find(|resource| resource.matches_id(id))
    }

    /// 词条文件路径：数据目录优先，其次资源目录。
    ///
    /// 这里的 `join` 不再做路径校验：`url` / `language` 在 [`Catalog::load`] 里
    /// 已按单段路径组件筛过（见 `DictResource::unsafe_path_reason`），不安全的
    /// 目录项根本进不了 `resources`。别处构造 `DictResource` 时同样要过那道筛。
    pub fn dict_file(&self, resource: &DictResource) -> Option<PathBuf> {
        let relative = Path::new("dicts")
            .join(resource.language_or_default())
            .join("word")
            .join(&resource.url);
        [self.home.join(&relative), self.assets.join(&relative)]
            .into_iter()
            .find(|path| path.is_file())
    }

    /// 读取并解析词条数据。
    ///
    /// 词条文件存在但没有可用单词时不报错：新建的自建词典一开始就是空的，
    /// 用户要往里加词——报错会让「新建词典」直接失败。
    pub fn load_words(&self, resource: &DictResource) -> Result<Vec<Word>> {
        let file = self.dict_file(resource).ok_or_else(|| {
            anyhow!(
                "找不到词库文件: {}（url={}）",
                resource.id_string(),
                resource.url
            )
        })?;
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("读取词库文件失败: {}", file.display()))?;
        // 读取容忍开头的 UTF-8 BOM（外部编辑 / Windows 记事本常见）。
        let mut words: Vec<Word> = serde_json::from_str(crate::fs::strip_utf8_bom(&text))
            .with_context(|| format!("解析词库文件失败: {}", file.display()))?;
        words.retain(Word::is_usable);
        Ok(words)
    }

    pub fn user_dict_file(&self, file_name: &str) -> PathBuf {
        self.home
            .join("dicts")
            .join("en")
            .join("word")
            .join(file_name)
    }

    /// 词条文件的**写入**路径：始终落在数据目录。
    ///
    /// 读取时资源目录是兜底（`dict_file`），但写入绝不能改写资源目录里的
    /// 内置词库——那些文件随发布包分发，用户改一个词就再也拿不回来，
    /// `tw import-dict` 也只能写数据目录。写入数据目录后，读取会优先命中
    /// 它，天然形成「用户覆盖内置」的语义。
    ///
    /// 与 [`Catalog::dict_file`] 一样，路径安全依赖装载时的单段校验：`url` 与
    /// `language` 都是用户可手改的目录文件字段，不能在这里才补救。
    pub fn dict_write_file(&self, resource: &DictResource) -> PathBuf {
        let relative = Path::new("dicts")
            .join(resource.language_or_default())
            .join("word")
            .join(&resource.url);
        self.home.join(relative)
    }

    fn user_catalog_file(&self) -> PathBuf {
        self.home.join("list").join("word.json")
    }

    pub fn read_user_catalog(&self) -> Result<Vec<serde_json::Value>> {
        let file = self.user_catalog_file();
        if !file.is_file() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("读取用户词库目录失败: {}", file.display()))?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(crate::fs::strip_utf8_bom(&text))
            .with_context(|| format!("解析用户词库目录失败: {}", file.display()))?;
        Ok(parsed)
    }

    pub fn write_user_catalog(&self, value: &[serde_json::Value]) -> Result<()> {
        let file = self.user_catalog_file();
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录失败: {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(value)?;
        crate::fs::write_atomic(&file, &text)
    }

    /// 把词条文件的真实词数写回用户词库目录里已登记的条目。
    ///
    /// 词条被编辑 / 删除后，`list/word.json` 的 `length` 是同一个「词数」的
    /// 另一个来源；只改词条文件会让没打开过这本词库的界面（推荐列表、
    /// `tw doctor`）继续显示旧值。内置词库的词条文件随发布包分发、目录里
    /// 没有对应条目，这里自然跳过——它们的词数由「词条优先」的显示规则
    /// 兜底。返回是否真的写了文件（没有条目、长度没变都不写）。
    pub fn sync_user_catalog_length(&self, id: &str, length: usize) -> Result<bool> {
        let mut entries = self.read_user_catalog()?;
        let mut changed = false;
        for entry in &mut entries {
            let Ok(resource) = serde_json::from_value::<DictResource>(entry.clone()) else {
                continue;
            };
            if !resource.matches_id(id) {
                continue;
            }
            let current = entry
                .get("length")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            if current == length as u64 {
                continue;
            }
            if let Some(object) = entry.as_object_mut() {
                object.insert("length".to_string(), serde_json::Value::from(length));
                changed = true;
            }
        }
        if changed {
            self.write_user_catalog(&entries)?;
        }
        Ok(changed)
    }
}

/// `value` 是否是「恰好一个普通路径段」：不含路径分隔符、不是 `.` / `..`、
/// 不带盘符或根前缀。
///
/// 用 `Path::components` 而不是手写 `contains(['/', '\\'])`：Windows 上
/// `C:escape.json`（盘符前缀）与 `\\server\share`（UNC）都不是单段，手写
/// 分隔符检查会漏掉前者，`join` 之后路径的语义也随之变形。空串没有组件，
/// 同样判为不合格。
fn is_single_path_segment(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

/// 在数据目录里新建一本自定义词典，返回登记好的目录项。
///
/// 自定义词典与 `import-dict` 导入的词典一样落在 `<home>` 下，因此会自动出现在
/// 词库页；词条存在 `<home>/dicts/en/word/<id>.json`。
pub fn create_custom_dict(paths: &Paths, name: &str, words: Vec<Word>) -> Result<DictResource> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("词典名称不能为空"));
    }
    let catalog = Catalog::load(paths)?;
    let id = unique_custom_id(&catalog, name);
    let file_name = format!("{id}.json");
    let text = serde_json::to_string_pretty(&words)?;
    let file = catalog.user_dict_file(&file_name);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    crate::fs::write_atomic(&file, &text)?;

    let mut entries = catalog.read_user_catalog()?;
    let mut resource = DictResource {
        id: serde_json::Value::String(id.clone()),
        en_name: id.clone(),
        name: name.to_string(),
        description: "自建词库".to_string(),
        url: file_name,
        length: words.len(),
        category: "我的词库".to_string(),
        tags: vec!["自建".to_string()],
        language: "en".to_string(),
        translate_language: "zh_CN".to_string(),
        kind: Some("word".to_string()),
        version: Some(1),
        cover: None,
        extra: serde_json::Map::new(),
    };
    // `custom` 标记既要写进目录文件，也要留在返回的 resource 上——
    // 调用方直接用它 `to_dict` 建词典，标记丢了自建词典的词表会被清空。
    resource
        .extra
        .insert("custom".to_string(), serde_json::Value::Bool(true));
    let value = serde_json::to_value(&resource)?;
    entries.retain(|entry| {
        entry
            .get("id")
            .and_then(|id| id.as_str())
            .map(|id| id != resource.id_string())
            .unwrap_or(true)
    });
    entries.push(value);
    catalog.write_user_catalog(&entries)?;
    Ok(resource)
}

/// 生成不与现有词典冲突的自定义词典 id。
fn unique_custom_id(catalog: &Catalog, name: &str) -> String {
    let mut index = 1usize;
    loop {
        let id = format!("custom-{index}");
        let taken = catalog
            .resources()
            .iter()
            .any(|resource| resource.id_string() == id)
            || catalog
                .read_user_catalog()
                .ok()
                .map(|entries| {
                    entries.iter().any(|entry| {
                        entry.get("id").and_then(|id| id.as_str()) == Some(id.as_str())
                    })
                })
                .unwrap_or(false);
        if !taken {
            return id;
        }
        index += 1;
        let _ = name;
    }
}

/// 把外部词库 JSON 导入数据目录，并注册到用户词库目录。
pub fn import_dict(paths: &Paths, source: &Path) -> Result<DictResource> {
    // 外部文件整读前先预检大小：误喂一个几 GB 的「JSON」时给「文件过大」，
    // 而不是等系统内存耗尽把进程杀掉。上限只约束导入入口，不约束自家存档。
    let text = crate::fs::read_text_limited(source, crate::fs::IMPORT_SIZE_LIMIT)
        .with_context(|| format!("读取词库失败: {}", source.display()))?;
    // 读取容忍开头的 UTF-8 BOM；写出的词条文件同样不带 BOM。
    let text = crate::fs::strip_utf8_bom(&text);
    let mut words: Vec<Word> = serde_json::from_str(text)
        .with_context(|| format!("解析词库失败（需要词条数组 JSON）: {}", source.display()))?;
    words.retain(Word::is_usable);
    if words.is_empty() {
        return Err(anyhow!("词库中没有可用单词: {}", source.display()));
    }

    let catalog = Catalog::load(paths)?;
    let file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| anyhow!("无法识别的文件名: {}", source.display()))?;
    if !file_name.ends_with(".json") {
        return Err(anyhow!("词库文件需要是 .json 结尾: {file_name}"));
    }
    // 目录项 id 取文件名去掉 .json。文件名只有 `.json` 时 stem 为空，
    // `Catalog::load` 会跳过空 id 的条目：词条文件与目录项都写进去，词库页 /
    // `tw doctor` / `--dict` 却永远看不到它，CLI 还报「已导入」。
    // 路径分隔符这条只是防御：`file_name()` 在 Windows 上不会含分隔符，Linux
    // 上也只挡得住字面反斜杠——它不是目录穿越防护。
    let stem = file_name.trim_end_matches(".json");
    if stem.is_empty() {
        return Err(anyhow!(
            "词库文件名不能是 .json（需要 <名称>.json）: {}",
            source.display()
        ));
    }
    if stem.contains(['/', '\\']) {
        return Err(anyhow!("词库文件名不能包含路径分隔符: {file_name}"));
    }

    let target = catalog.user_dict_file(&file_name);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    // 复用上面读到的原文并走原子写：直接 `fs::copy` 会在目标文件上就地截断，
    // 进程中途被杀就留下半个词库文件；源与目标同路径（把数据目录里的词库再
    // 导入一次）时更是先截断再读，内容会丢。
    crate::fs::write_atomic(&target, text)
        .with_context(|| format!("复制词库失败: {} → {}", source.display(), target.display()))?;

    let stem = stem.to_string();
    let mut extra = serde_json::Map::new();
    // 导入的词库要标成自建：它的词条文件在数据目录里，不是随包分发的内置
    // 资源。缺了这个标记，落盘前的 `shake_common_dict` 会把词表清空——
    // 换个词库或改一个词再重启，整本词典就没了。
    extra.insert("custom".to_string(), serde_json::Value::Bool(true));
    let resource = DictResource {
        id: serde_json::Value::String(stem.clone()),
        en_name: stem.clone(),
        name: stem.clone(),
        description: format!("导入词库（{} 词）", words.len()),
        url: file_name.clone(),
        length: words.len(),
        category: "自定义".to_string(),
        tags: vec!["导入".to_string()],
        language: "en".to_string(),
        translate_language: "zh_CN".to_string(),
        kind: Some("word".to_string()),
        version: Some(1),
        cover: None,
        extra,
    };

    let mut user_catalog = catalog.read_user_catalog()?;
    let mut value = serde_json::to_value(&resource)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("extra");
    }
    // 判重必须按解析后的 id 语义比较：serde_json::Value::to_string() 会给字符串 id
    // 带上引号（"foo" != foo），旧实现因此永远匹配不上，重复导入会追加重复目录项。
    let existing = user_catalog.iter().position(|item| {
        serde_json::from_value::<DictResource>(item.clone())
            .map(|entry| entry.id_string() == resource.id_string())
            .unwrap_or(false)
    });
    match existing {
        Some(index) => user_catalog[index] = value,
        None => user_catalog.push(value),
    }
    catalog.write_user_catalog(&user_catalog)?;

    Ok(resource)
}
