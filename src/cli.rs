//! 命令行入口：参数、doctor、task 预览、词库导入导出（import-dict / export-dict）。

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};

use crate::bootstrap;
use crate::domain::catalog::{Catalog, DictResource, import_dict as do_import_dict};
use crate::domain::rng;
use crate::domain::study_task::create_study_task;
use crate::domain::types::{Dict, Word, WordPracticeMode};
use crate::paths::Paths;

/// `tw task` 缺省使用的随机种子。
///
/// 预览刻意是确定性的：同一天、同一份存档预览两次给出同一份任务列表，便于
/// 对照「真实练习为什么和预览不一样」。真实练习缺省用随机种子，两者的差别
/// 会在预览输出里说明（见 [`task_preview_to`]）。
const PREVIEW_SEED: u64 = 42;

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
    /// 直接进入指定词库（id 或 enName；内置词典可用 wordCollect / wordWrong / wordKnown）
    #[arg(long, global = true, value_name = "DICT_ID")]
    pub dict: Option<String>,
    /// 启动时覆盖练习模式（system/free/review/shuffle/identify/dictation/listen）
    #[arg(long, global = true, value_name = "MODE")]
    pub mode: Option<String>,
    /// 启动时覆盖主题（内置 dark/light/high-contrast/midnight/paper/ikb 或 <home>/themes 下的文件名）
    #[arg(long, global = true, value_name = "NAME")]
    pub theme: Option<String>,
    /// 随机种子（复现问题用）
    #[arg(long, global = true, value_name = "N")]
    pub seed: Option<u64>,
    /// 清空当前练习缓存
    #[arg(long, global = true)]
    pub reset_practice: bool,
    /// 运行日志文件（缺省 <home>/logs/tw-<日期>.log）
    #[arg(long, global = true, value_name = "FILE", conflicts_with = "no_log")]
    pub log: Option<PathBuf>,
    /// 关闭运行日志（崩溃报告仍写入日志目录）
    #[arg(long, global = true)]
    pub no_log: bool,
    /// 日志级别：error / warn / info / debug
    #[arg(
        long,
        global = true,
        value_name = "LEVEL",
        default_value = "info",
        value_parser = crate::logging::parse_level
    )]
    pub log_level: crate::logging::Level,
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
    /// 导出词库词条为 JSON（import-dict 的逆操作）
    ExportDict {
        /// 词库 id 或 enName；缺省取全局 --dict，再缺省取当前学习词库
        #[arg(value_name = "DICT_ID")]
        id: Option<String>,
        /// 输出路径，缺省写到标准输出
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// 导入网页版导出的备份 JSON（BackupData v5）
    ImportWeb { file: PathBuf },
    /// 导出成网页版能导入的备份 JSON
    ExportWeb {
        /// 输出路径，缺省写到标准输出
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
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
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    doctor_to(&mut out, paths)
}

/// 把排障信息写进 `out`。
///
/// 单独拆出来是为了让测试能断言实际打印的内容：`println!` 在测试进程里抓不到，
/// 而「doctor 报的词数」与「程序实际加载的词数」必须是同一个数字。
pub fn doctor_to(out: &mut impl Write, paths: &Paths) -> Result<()> {
    writeln!(out, "{}", crate::version_line())?;
    writeln!(out, "数据目录: {}", paths.home.display())?;
    writeln!(out, "资源目录: {}", paths.assets.display())?;
    writeln!(
        out,
        "日志目录: {}（--log 可指定文件，--no-log 关闭）",
        paths.logs_dir().display()
    )?;

    let catalog = crate::domain::catalog::Catalog::load(paths)?;
    writeln!(out, "\n词库（{} 本）：", catalog.resources().len())?;
    for resource in catalog.resources() {
        let file = catalog.dict_file(resource);
        // 词数必须走真正的加载路径 `Catalog::load_words`：它用
        // `retain(Word::is_usable)` 过滤空词条。这里若自己解析 JSON 再 `len()`，
        // 含空词条的词库就会被多算——doctor 的数字比程序实际加载的大，
        // 排障时最容易误判。
        let summary = if file.is_none() {
            "<缺失>".to_string()
        } else {
            match catalog.load_words(resource) {
                Ok(words) => format!("{} 词", words.len()),
                Err(err) => format!("! {err:#}"),
            }
        };
        writeln!(
            out,
            "  - {:<12} {:<10} {:<8} {:<6} {}",
            resource.id_string(),
            resource.en_name,
            resource.name,
            resource.length,
            summary
        )?;
        writeln!(
            out,
            "      文件: {}",
            file.as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<缺失>".to_string())
        )?;
    }
    // 被跳过的目录项不会出现在上面的列表里：不打印的话，用户只会看到「词库少了
    // 一本」，而不知道是自己手改的目录项不合格。
    for warning in catalog.warnings() {
        writeln!(out, "  ! {warning}")?;
    }

    let store = crate::store::FileStore::new(&paths.home)?;
    writeln!(out, "\n存档：")?;
    for (name, size) in store.describe_files() {
        match size {
            Some(size) => writeln!(out, "  - {name}: {size} bytes")?,
            None => writeln!(out, "  - {name}: 不存在")?,
        }
    }

    let themes = crate::theme::ThemeRegistry::load(&paths.themes_dir());
    writeln!(out, "\n主题（{} 个自定义）：", themes.custom_count())?;
    writeln!(out, "  目录: {}", paths.themes_dir().display())?;
    for id in themes.ids() {
        writeln!(out, "  - {:<16} {}", id, themes.label(id))?;
    }
    for warning in themes.warnings() {
        writeln!(out, "  ! {warning}")?;
    }
    Ok(())
}

pub fn task_preview(paths: &Paths, cli: &Cli) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    task_preview_to(&mut out, paths, cli)
}

/// [`task_preview`] 的实现。
///
/// 单独拆出来是为了让测试能断言实际打印的内容（`println!` 在测试进程里抓不到），
/// 与 [`doctor_to`] 同一个理由。
///
/// 预览不是只读操作：装载路径与 TUI 启动完全相同，会按词表实际词数校正学习
/// 进度，并在断点缓存失效时删掉 `practice-word.json`。这类副作用与装载期的
/// 告警（存档被隔离、FSRS 步进回退、词库目录项被跳过）都必须在输出里说明——
/// 预览命令悄悄改数据、又把提示吞掉，是排障时最难发现的组合。
pub fn task_preview_to(out: &mut impl Write, paths: &Paths, cli: &Cli) -> Result<()> {
    let mut boot = bootstrap::load(paths, cli.dict.as_deref())?;
    boot.ensure_study_dict()?;
    let mut rng = rng::seeded(cli.seed.unwrap_or(PREVIEW_SEED));
    let result = create_study_task(&boot.base, &boot.settings, &mut rng);

    for notice in &boot.notices {
        writeln!(out, "提示: {notice}")?;
    }
    let dict = boot
        .study_dict()
        .ok_or_else(|| anyhow!("没有当前学习词典"))?;
    let (dict_line, progress_line) = dict_progress_lines(dict);
    writeln!(out, "{dict_line}")?;
    writeln!(out, "{progress_line}")?;
    if boot.practice_cache_cleared {
        writeln!(out, "学习进度已按词表校准，失效的断点缓存已清除")?;
    }
    writeln!(
        out,
        "今日任务: 新词 {} / 复习 {}（到期 {}，随机补 {}）",
        result.task_words.new.len(),
        result.task_words.review.len(),
        result.due_review_count,
        result.random_review_count
    )?;
    for word in result.task_words.new.iter().take(5) {
        writeln!(out, "  new: {}  {}", word.word, word.trans_joined())?;
    }
    for word in result.task_words.review.iter().take(5) {
        writeln!(out, "  review: {}  {}", word.word, word.trans_joined())?;
    }
    // 预览是确定性的，真实练习默认随机：两者的抽取顺序可以不同。不说清楚，
    // 用户会以为「预览和实际练的不是同一批词」是 bug。
    match cli.seed {
        Some(seed) => writeln!(
            out,
            "预览使用随机种子 {seed}（--seed 指定；真实练习用同一个值才能复现）"
        )?,
        None => writeln!(
            out,
            "预览使用固定随机种子 {PREVIEW_SEED}（真实练习默认随机；可用 --seed N 固定）"
        )?,
    }
    Ok(())
}

/// `tw task` 的词库行与进度行。
///
/// 两行的词数与百分比都取有效词数（[`Dict::effective_length`] 经
/// [`crate::domain::types::DictProgress`]）：早期实现这里写的是 `dict.length`，
/// 与界面上的百分比各用一套分母。
fn dict_progress_lines(dict: &Dict) -> (String, String) {
    let progress = dict.progress();
    (
        format!(
            "词库: {}（{} 词，每日 {} 词）",
            dict.name,
            progress.total,
            dict.per_day()
        ),
        format!(
            "学习进度: {}/{} ({}%){}",
            progress.learned,
            progress.total,
            progress.percent(),
            if progress.complete {
                "，已完成"
            } else {
                ""
            }
        ),
    )
}

/// 导入网页版导出的备份。
pub fn import_web(paths: &Paths, file: &Path) -> Result<()> {
    let stdout = std::io::stdout();
    let mut sink = stdout.lock();
    import_web_to(&mut sink, paths, file)
}

/// [`import_web`] 的实现。
///
/// 单独拆出来是为了让测试能断言实际打印的内容（`println!` 在测试进程里抓不到），
/// 与 [`export_web_to`] 同一个理由。
pub fn import_web_to(sink: &mut impl Write, paths: &Paths, file: &Path) -> Result<()> {
    let store = crate::store::FileStore::new(paths.home.clone())?;
    let report = crate::backup::import_web(&store, file)?;
    // 「没有可导入的存档」只在备份里确实一条都没有时成立；全部跳过 / 拒绝 /
    // 丢弃时先打这句，再逐条说明为什么没导入，读起来是自相矛盾的。
    if report.written.is_empty()
        && report.skipped.is_empty()
        && report.rejected.is_empty()
        && report.discarded.is_empty()
    {
        writeln!(sink, "没有可导入的存档。")?;
    }
    for name in &report.written {
        writeln!(sink, "已导入: {name}")?;
    }
    for name in &report.skipped {
        writeln!(sink, "已跳过: {name}")?;
    }
    for name in &report.rejected {
        writeln!(sink, "已拒绝: {name}")?;
    }
    for name in &report.discarded {
        writeln!(sink, "已丢弃: {name}")?;
    }
    for name in &report.backups {
        writeln!(sink, "已备份原存档: {name}")?;
    }
    if !report.rejected.is_empty() {
        writeln!(sink, "被拒绝的存档没有写入，原文件保持不变。")?;
    }
    if !report.written.is_empty() {
        writeln!(sink, "\n请重启程序让新存档生效。")?;
    }
    Ok(())
}

/// 导出成网页版能导入的备份。
pub fn export_web(paths: &Paths, out: Option<&Path>) -> Result<()> {
    let stdout = std::io::stdout();
    let mut sink = stdout.lock();
    export_web_to(&mut sink, paths, out)
}

/// [`export_web`] 的实现。
///
/// 单独拆出来是为了让测试能断言实际打印的内容（`println!` 在测试进程里抓不到），
/// 与 [`export_dict_to`] 同一个理由；`--out` 缺省时输出就是可直接重定向的备份 JSON。
pub fn export_web_to(sink: &mut impl Write, paths: &Paths, out: Option<&Path>) -> Result<()> {
    let store = crate::store::FileStore::new(paths.home.clone())?;
    let export = crate::backup::export_web(&store)?;
    // 警告走 stderr：stdout 必须保持「纯 JSON」，脚本才能直接重定向。
    for warning in &export.warnings {
        eprintln!("警告：{warning}");
    }
    match out {
        Some(path) => {
            // 与 export-dict 一样走原子写：中途失败不会把目标文件截成半截，
            // 也不会留下一份写了一半、看起来却像备份的 JSON。
            crate::fs::write_atomic(path, &format!("{}\n", export.text))
                .with_context(|| format!("写入 {} 失败", path.display()))?;
            writeln!(sink, "已导出: {}", path.display())?;
        }
        None => writeln!(sink, "{}", export.text)?,
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

/// `tw export-dict` 的导出目标：词库名称与词条。
struct ExportTarget {
    name: String,
    words: Vec<Word>,
}

/// 导出词库词条为词条数组 JSON（`import-dict` 的逆操作）。
pub fn export_dict(paths: &Paths, id: Option<&str>, out: Option<&Path>) -> Result<()> {
    let stdout = std::io::stdout();
    let mut sink = stdout.lock();
    export_dict_to(&mut sink, paths, id, out)
}

/// [`export_dict`] 的实现。
///
/// 单独拆出来是为了让测试能断言实际打印的内容（`println!` 在测试进程里抓不到），
/// 与 [`doctor_to`] 同一个理由；`--out` 缺省时输出就是可直接再导入的 JSON 本体。
pub fn export_dict_to(
    sink: &mut impl Write,
    paths: &Paths,
    id: Option<&str>,
    out: Option<&Path>,
) -> Result<()> {
    let target = resolve_export_target(paths, id)?;
    let text = serde_json::to_string_pretty(&target.words)?;
    match out {
        Some(path) => {
            // 与 import-dict 一样走原子写：中途失败不会把目标文件截成半截，
            // 也不会把一份写了一半的词库留在磁盘上。
            crate::fs::write_atomic(path, &format!("{text}\n"))
                .with_context(|| format!("写入词库文件失败: {}", path.display()))?;
            writeln!(
                sink,
                "已导出词库: {}（{} 词）→ {}",
                target.name,
                target.words.len(),
                path.display()
            )?;
        }
        None => writeln!(sink, "{text}")?,
    }
    Ok(())
}

/// 解析要导出的词库。
///
/// 先查词库目录（内置词库与 `import-dict` 导入的词库都在这里，词条来自词条
/// 文件）；再查存档——收藏 / 错词 / 已掌握三本内置词典的**词条只在
/// `dict.json` 里**，没有词条文件，词库目录里也查不到它们。`id` 缺省时导出
/// 当前学习词库。
fn resolve_export_target(paths: &Paths, id: Option<&str>) -> Result<ExportTarget> {
    let catalog = Catalog::load(paths)?;
    if let Some(id) = id
        && let Some(resource) = catalog.find(id)
    {
        return target_from_catalog(&catalog, resource);
    }

    let loaded = crate::store::FileStore::new(paths.home.clone())?.load_dict();
    if let crate::store::LoadStatus::Unreadable(reason) = &loaded.status {
        // 读不到 ≠ 内容坏了：说清原因，别让「文件被占用」显示成
        // 「还没有当前学习词库」——后者会把人引向错误的方向（以为数据没了）。
        return Err(anyhow!(
            "读取词典存档失败：{reason}；请关闭占用该文件的程序（同步盘 / 杀毒软件）后重试"
        ));
    }
    let base = loaded.value;
    let dict = match id {
        Some(id) => base
            .system_dict_index(id)
            .and_then(|index| base.word.book_list.get(index)),
        None => base.study_dict(),
    };
    let Some(dict) = dict else {
        return Err(match id {
            Some(id) => anyhow!("未找到词库: {id}（用 tw doctor 查看全部词库 id）"),
            None => anyhow!("还没有当前学习词库，请指定 DICT_ID（tw doctor 可列出全部词库 id）"),
        });
    };

    // 普通词库的词表在存档落盘时被清空（`shake_common_dict` 只留元数据），
    // 按 id / enName 回词条文件补齐；自建词库与内置词典的词条就在存档里。
    let words = if dict.words.is_empty() {
        match catalog
            .find(&dict.id)
            .or_else(|| catalog.find(&dict.en_name))
        {
            Some(resource) => catalog.load_words(resource)?,
            None => Vec::new(),
        }
    } else {
        dict.words.clone()
    };
    let name = if dict.name.is_empty() {
        dict.id.clone()
    } else {
        dict.name.clone()
    };
    export_target(name, words)
}

fn target_from_catalog(catalog: &Catalog, resource: &DictResource) -> Result<ExportTarget> {
    let name = if resource.name.is_empty() {
        resource.id_string()
    } else {
        resource.name.clone()
    };
    export_target(name, catalog.load_words(resource)?)
}

/// 收尾：过滤空词条并拒绝空词库。
///
/// 空词条既练不到，也会让导出的词数比界面上的数字大；空词库导出成 `[]`
/// 之后 `import-dict` 又会拒收（它要求至少一个可用词），与其造出一个导不
/// 回去的文件，不如当场报错。
fn export_target(name: String, mut words: Vec<Word>) -> Result<ExportTarget> {
    words.retain(Word::is_usable);
    if words.is_empty() {
        return Err(anyhow!("词库中没有可用单词: {name}"));
    }
    Ok(ExportTarget { name, words })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::types::Word;

    /// `tw task` 的词数与百分比必须同源：存档里 `length` 比实际词表大时，
    /// 旧实现会打印出「12 词 … 6/20 (30%)」这种自相矛盾的组合。
    #[test]
    fn task_preview_uses_the_effective_word_count_as_denominator() {
        let words: Vec<Word> = (0..12)
            .map(|index| Word::new(format!("w{index}")))
            .collect();
        let dict = Dict {
            name: "测试词库".to_string(),
            length: 20,
            last_learn_index: 6,
            words,
            ..Dict::default()
        };
        let (dict_line, progress_line) = dict_progress_lines(&dict);
        assert!(
            dict_line.contains("（12 词"),
            "词数应取有效词数：{dict_line}"
        );
        assert_eq!(progress_line, "学习进度: 6/12 (50%)");
    }

    /// 全局参数放在子命令前后都要能解析。
    ///
    /// 文档示例是 `tw task --seed 42`：`seed` / `dict` / `mode` / `theme` /
    /// `reset-practice` 早期没有 `global = true`，clap 只认
    /// `tw --seed 42 task`，用户照抄文档会得到
    /// `error: unexpected argument '--seed' found`（退出码 2）。
    #[test]
    fn global_options_parse_before_and_after_the_subcommand() {
        for args in [
            vec!["tw", "task", "--seed", "42"],
            vec!["tw", "--seed", "42", "task"],
        ] {
            let cli = Cli::try_parse_from(&args)
                .unwrap_or_else(|err| panic!("{args:?} 应能解析，实际报错: {err}"));
            assert_eq!(cli.seed, Some(42), "{args:?}");
            assert!(matches!(cli.command, Some(Command::Task)), "{args:?}");
        }

        let cli = Cli::try_parse_from([
            "tw",
            "task",
            "--dict",
            "cet4",
            "--mode",
            "review",
            "--theme",
            "dark",
            "--seed",
            "7",
            "--reset-practice",
            "--home",
            "H",
        ])
        .expect("子命令之后的全局参数都应被接受");
        assert_eq!(cli.dict.as_deref(), Some("cet4"));
        assert_eq!(cli.mode.as_deref(), Some("review"));
        assert_eq!(cli.theme.as_deref(), Some("dark"));
        assert_eq!(cli.seed, Some(7));
        assert!(cli.reset_practice);
        assert_eq!(cli.home.as_deref(), Some(Path::new("H")));

        // `export-dict` 缺省用全局 `--dict`：写在子命令之后同样要落到 cli.dict。
        let cli = Cli::try_parse_from(["tw", "export-dict", "--dict", "cet4"])
            .expect("export-dict 之后也应接受 --dict");
        assert_eq!(cli.dict.as_deref(), Some("cet4"));
        assert!(matches!(
            cli.command,
            Some(Command::ExportDict { id: None, .. })
        ));
    }
}
