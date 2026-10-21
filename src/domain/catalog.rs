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
            length: if self.length > 0 {
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
}

impl Catalog {
    pub fn load(paths: &Paths) -> Result<Self> {
        let mut resources: Vec<DictResource> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();

        for root in [&paths.assets, &paths.home] {
            let file = root.join("list").join("word.json");
            if !file.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("读取词库目录失败: {}", file.display()))?;
            let parsed: Vec<DictResource> = serde_json::from_str(&text)
                .with_context(|| format!("解析词库目录失败: {}", file.display()))?;
            for resource in parsed {
                if !resource.is_word_dict() {
                    continue;
                }
                let key = resource.id_string();
                if key.is_empty() {
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
            return Err(anyhow!(
                "词库目录为空，请检查 {} 或数据目录下的 list/word.json",
                paths.assets.join("list").join("word.json").display()
            ));
        }

        Ok(Self {
            assets: paths.assets.clone(),
            home: paths.home.clone(),
            resources,
        })
    }

    pub fn resources(&self) -> &[DictResource] {
        &self.resources
    }

    pub fn find(&self, id: &str) -> Option<&DictResource> {
        self.resources
            .iter()
            .find(|resource| resource.matches_id(id))
    }

    /// 词条文件路径：数据目录优先，其次资源目录。
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
        let mut words: Vec<Word> = serde_json::from_str(&text)
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
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&text)
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
        crate::store::write_atomic(&file, &text)
    }
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
    crate::store::write_atomic(&file, &text)?;

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
    let text = std::fs::read_to_string(source)
        .with_context(|| format!("读取词库失败: {}", source.display()))?;
    let mut words: Vec<Word> = serde_json::from_str(&text)
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

    let target = catalog.user_dict_file(&file_name);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    std::fs::copy(source, &target)
        .with_context(|| format!("复制词库失败: {} → {}", source.display(), target.display()))?;

    let stem = file_name.trim_end_matches(".json").to_string();
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
        extra: serde_json::Map::new(),
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
