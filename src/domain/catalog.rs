//! 词库目录与词条文件加载。
//!
//! 资源布局与 Web 端一致：
//! - `<root>/list/word.json`：词库目录
//! - `<root>/dicts/<language>/word/<url>`：词条数据
//!
//! 查找顺序：数据目录（用户 `import-dict`）优先，其次只读资源目录。

use std::collections::HashMap;
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
        if words.is_empty() {
            return Err(anyhow!("词库文件没有可用单词: {}", file.display()));
        }
        Ok(words)
    }

    pub fn user_dict_file(&self, file_name: &str) -> PathBuf {
        self.home
            .join("dicts")
            .join("en")
            .join("word")
            .join(file_name)
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
    let existing = user_catalog
        .iter()
        .position(|item| item.get("id").map(|id| id.to_string()) == Some(resource.id_string()));
    match existing {
        Some(index) => user_catalog[index] = value,
        None => user_catalog.push(value),
    }
    catalog.write_user_catalog(&user_catalog)?;

    Ok(resource)
}
