//! 命令行入口：参数、doctor、task 预览、import-dict。

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};

use crate::bootstrap;
use crate::domain::catalog::import_dict as do_import_dict;
use crate::domain::rng;
use crate::domain::study_task::create_study_task;
use crate::domain::types::WordPracticeMode;
use crate::paths::Paths;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "tw",
    version,
    about = "Type Words 终端版单词练习",
    long_about = "Type Words TUI：在终端里按 FSRS 计划练习英语单词。"
)]
pub struct Cli {
    /// 词库资源目录（需包含 list/word.json）
    #[arg(long, global = true, value_name = "DIR")]
    pub assets: Option<PathBuf>,
    /// 数据目录（存档、导入词库）
    #[arg(long, global = true, value_name = "DIR")]
    pub home: Option<PathBuf>,
    /// 直接进入指定词库（id 或 enName）
    #[arg(long, value_name = "DICT_ID")]
    pub dict: Option<String>,
    /// 启动时覆盖练习模式（system/free/review/shuffle/identify/dictation/listen）
    #[arg(long, value_name = "MODE")]
    pub mode: Option<String>,
    /// 启动时覆盖主题（dark/light/high-contrast 或 <home>/themes 下的文件名）
    #[arg(long, value_name = "NAME")]
    pub theme: Option<String>,
    /// 随机种子（复现问题用）
    #[arg(long, value_name = "N")]
    pub seed: Option<u64>,
    /// 清空当前练习缓存
    #[arg(long)]
    pub reset_practice: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// 打印解析到的路径与词库状态
    Doctor,
    /// 预览今日任务（不启动界面）
    Task,
    /// 导入词库 JSON 到数据目录
    ImportDict { file: PathBuf },
}

pub fn parse_mode(value: &str) -> Result<WordPracticeMode> {
    let normalized = value.trim().to_lowercase();
    let mode = match normalized.as_str() {
        "system" | "smart" | "学习" => WordPracticeMode::System,
        "free" | "自由" => WordPracticeMode::Free,
        "review" | "复习" => WordPracticeMode::Review,
        "shuffle" | "随机" => WordPracticeMode::Shuffle,
        "identify" | "自测" => WordPracticeMode::IdentifyOnly,
        "dictation" | "默写" => WordPracticeMode::DictationOnly,
        "listen" | "听写" => WordPracticeMode::ListenOnly,
        other => return Err(anyhow!("未知练习模式: {other}")),
    };
    Ok(mode)
}

pub fn doctor(paths: &Paths) -> Result<()> {
    println!("{}", crate::version_line());
    println!("数据目录: {}", paths.home.display());
    println!("资源目录: {}", paths.assets.display());

    let catalog = crate::domain::catalog::Catalog::load(paths)?;
    println!("\n词库（{} 本）：", catalog.resources().len());
    for resource in catalog.resources() {
        let file = catalog.dict_file(resource);
        let words = file
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Vec<crate::domain::types::Word>>(&text).ok())
            .map(|words| words.len());
        println!(
            "  - {:<12} {:<10} {:<8} {:<6} {:?}",
            resource.id_string(),
            resource.en_name,
            resource.name,
            resource.length.to_string(),
            words.map(|count| format!("{count} 词"))
        );
        println!(
            "      文件: {}",
            file.map(|path| path.display().to_string())
                .unwrap_or_else(|| "<缺失>".to_string())
        );
    }

    let store = crate::store::FileStore::new(&paths.home)?;
    println!("\n存档：");
    for (name, size) in store.describe_files() {
        match size {
            Some(size) => println!("  - {name}: {size} bytes"),
            None => println!("  - {name}: 不存在"),
        }
    }

    let themes = crate::theme::ThemeRegistry::load(&paths.themes_dir());
    println!("\n主题（{} 个自定义）：", themes.custom_count());
    println!("  目录: {}", paths.themes_dir().display());
    for id in themes.ids() {
        println!("  - {:<16} {}", id, themes.label(id));
    }
    for warning in themes.warnings() {
        println!("  ! {warning}");
    }
    Ok(())
}

pub fn task_preview(paths: &Paths, cli: &Cli) -> Result<()> {
    let mut boot = bootstrap::load(paths, cli.dict.as_deref())?;
    boot.ensure_study_dict()?;
    let mut rng = rng::seeded(cli.seed.unwrap_or(42));
    let result = create_study_task(&boot.base, &boot.settings, &mut rng);

    let dict = boot
        .study_dict()
        .ok_or_else(|| anyhow!("没有当前学习词典"))?;
    println!(
        "词库: {}（{} 词，每日 {} 词）",
        dict.name,
        dict.length,
        dict.per_day()
    );
    println!(
        "学习进度: {}/{} ({}%){}",
        dict.last_learn_index,
        dict.length,
        dict.progress_percent(),
        if dict.complete { "，已完成" } else { "" }
    );
    println!(
        "今日任务: 新词 {} / 复习 {}（到期 {}，随机补 {}）",
        result.task_words.new.len(),
        result.task_words.review.len(),
        result.due_review_count,
        result.random_review_count
    );
    for word in result.task_words.new.iter().take(5) {
        println!("  new: {}  {}", word.word, word.trans_joined());
    }
    for word in result.task_words.review.iter().take(5) {
        println!("  review: {}  {}", word.word, word.trans_joined());
    }
    Ok(())
}

pub fn import_dict(paths: &Paths, file: &Path) -> Result<()> {
    let resource = do_import_dict(paths, file)?;
    println!(
        "已导入词库: {}（{} 词）→ {}",
        resource.name,
        resource.length,
        file.display()
    );
    println!("使用 tw --dict {} 切换到该词库。", resource.id_string());
    Ok(())
}
