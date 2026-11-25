//! 文档一致性检查：README、CHANGELOG.md 与 docs/ 中的相对链接必须指向真实存在的文件，
//! 链接里的 `#锚点` 必须命中目标文档的标题，docs/README.md 必须登记 docs/ 下的全部 Markdown 文档，CHANGELOG.md 必须遵循
//! Keep a Changelog 且是发布说明的唯一来源。
//!
//! 检查只依赖本地文件系统，随 `cargo test` 在所有平台运行，防止文档重构后留下死链。

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    // 仓库根的 Markdown 一起校验：逐个点名会漏掉新加的根文档
    // （CONTRIBUTING.md 就曾因此完全不在检查范围内）。
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                files.push(path);
            }
        }
    }
    collect_markdown(&root.join("docs"), &mut files);
    files.sort();
    files.into_iter().filter(|path| path.is_file()).collect()
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_markdown(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

/// 提取 Markdown 行内链接（`[text](target)` / `![alt](target)`）的目标；
/// 跳过围栏代码块，避免把示例里的链接当成真实链接。
fn markdown_link_targets(content: &str) -> Vec<String> {
    let mut visible = String::new();
    let mut in_fence = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            visible.push_str(line);
            visible.push('\n');
        }
    }

    let bytes = visible.as_bytes();
    let mut targets = Vec::new();
    let mut cursor = 0;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] != b']' || bytes[cursor + 1] != b'(' {
            cursor += 1;
            continue;
        }
        let start = cursor + 2;
        let Some(offset) = visible[start..].find(')') else {
            break;
        };
        let target = visible[start..start + offset]
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>');
        if !target.is_empty() {
            targets.push(target.to_string());
        }
        cursor = start + offset + 1;
    }
    targets
}

fn is_external(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("tel:")
        || target.starts_with("data:")
        || target.starts_with('#')
}

/// 把链接解析为仓库内的路径；外部链接与纯锚点返回 `None`。
fn resolve_link(root: &Path, document: &Path, target: &str) -> Option<PathBuf> {
    if is_external(target) {
        return None;
    }
    let path = target.split(['#', '?']).next().unwrap_or("").trim();
    if path.is_empty() {
        return None;
    }
    if let Some(absolute) = path.strip_prefix('/') {
        return Some(root.join(absolute));
    }
    let base = document.parent().unwrap_or(root);
    Some(base.join(path))
}

/// 把标题文本转成 GitHub 风格的锚点：小写、去掉 Markdown 标记与标点、空格转连字符。
fn slugify(heading: &str) -> String {
    let mut out = String::new();
    for ch in heading.to_lowercase().chars() {
        if matches!(ch, '`' | '*' | '[' | ']' | '(' | ')') {
            continue;
        }
        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    out
}

/// 收集文档内全部标题锚点；重名标题按 GitHub 的规则追加 `-1`、`-2`。
fn heading_slugs(content: &str) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let Some(rest) = line.trim_start().strip_prefix('#') else {
            continue;
        };
        let extra = rest.chars().take_while(|ch| *ch == '#').count();
        let Some(text) = rest[extra..].strip_prefix(' ') else {
            continue;
        };
        let base = slugify(text.trim());
        let mut candidate = base.clone();
        let mut suffix = 1;
        while slugs.contains(&candidate) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        slugs.push(candidate);
    }
    slugs
}

#[test]
fn markdown_relative_links_point_to_existing_files() {
    let root = repo_root();
    let mut broken = Vec::new();
    for document in markdown_files(&root) {
        let content = fs::read_to_string(&document).unwrap_or_default();
        for target in markdown_link_targets(&content) {
            let Some(resolved) = resolve_link(&root, &document, &target) else {
                continue;
            };
            if !resolved.exists() {
                let from = document.strip_prefix(&root).unwrap_or(document.as_path());
                broken.push(format!("{}: {target}", from.display()));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "以下文档链接指向不存在的文件（共 {} 条）：\n{}",
        broken.len(),
        broken.join("\n")
    );
}

#[test]
fn markdown_anchors_point_to_existing_headings() {
    let root = repo_root();
    let mut broken = Vec::new();
    for document in markdown_files(&root) {
        let content = fs::read_to_string(&document).unwrap_or_default();
        for target in markdown_link_targets(&content) {
            let Some((path, fragment)) = target.split_once('#') else {
                continue;
            };
            if is_external(path) {
                continue;
            }
            let fragment = fragment.split('?').next().unwrap_or(fragment);
            if fragment.is_empty() {
                continue;
            }
            let resolved = if path.is_empty() {
                document.clone()
            } else {
                match resolve_link(&root, &document, path) {
                    Some(resolved) if resolved.extension().is_some_and(|ext| ext == "md") => {
                        resolved
                    }
                    _ => continue,
                }
            };
            // 文件缺失由链接测试负责报错，这里只判锚点。
            let Ok(target_content) = fs::read_to_string(&resolved) else {
                continue;
            };
            if !heading_slugs(&target_content)
                .iter()
                .any(|slug| slug == fragment)
            {
                let from = document.strip_prefix(&root).unwrap_or(document.as_path());
                broken.push(format!("{}: {target}", from.display()));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "以下文档锚点指向不存在的标题（共 {} 条）：\n{}",
        broken.len(),
        broken.join("\n")
    );
}

#[test]
fn docs_index_lists_every_document() {
    let root = repo_root();
    let index = fs::read_to_string(root.join("docs/README.md")).expect("docs/README.md 应存在");
    let docs_dir = root.join("docs");
    let mut missing = Vec::new();
    for document in markdown_files(&root) {
        let Ok(relative) = document.strip_prefix(&docs_dir) else {
            continue;
        };
        if relative == Path::new("README.md") {
            continue;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        if !index.contains(&format!("]({relative})")) {
            missing.push(relative);
        }
    }
    assert!(
        missing.is_empty(),
        "docs/README.md 未登记以下文档：\n{}",
        missing.join("\n")
    );
}

/// 从 Cargo.toml 的 [package] 段读取 version。
fn package_version(root: &Path) -> String {
    let cargo = fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml 应存在");
    let mut in_package = false;
    for line in cargo.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if in_package && let Some(value) = line.strip_prefix("version = ") {
            return value.trim().trim_matches('"').to_string();
        }
    }
    panic!("Cargo.toml 的 [package] 段应有 version");
}

/// 文档里的「对应版本」必须与 Cargo.toml 一致，避免发版后文档仍写旧版本。
#[test]
fn docs_version_matches_cargo_version() {
    let root = repo_root();
    let version = package_version(&root);
    let index = fs::read_to_string(root.join("docs/README.md")).expect("docs/README.md 应存在");
    let expected = format!("typewords-tui {version}");
    assert!(
        index.contains(&expected),
        "docs/README.md 的「对应版本」应包含 {expected}（Cargo.toml 当前版本）"
    );
}

/// 收集文档中提到的 src/...rs 路径（只取 ASCII 路径片段）。
fn source_paths(content: &str) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut paths = Vec::new();
    let mut cursor = 0;
    while cursor + 4 <= bytes.len() {
        if &bytes[cursor..cursor + 4] == b"src/" {
            let start = cursor;
            let mut end = cursor + 4;
            while end < bytes.len() {
                let byte = bytes[end];
                if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'/' | b'.' | b'-') {
                    end += 1;
                } else {
                    break;
                }
            }
            let candidate = &content[start..end];
            // `src/...rs` 这类带省略号的写法是占位符（描述「引用源码」规则时用），不是真实路径。
            if candidate.ends_with(".rs") && !candidate.contains("...") {
                paths.push(candidate.to_string());
            }
            cursor = end;
        } else {
            cursor += 1;
        }
    }
    paths
}

/// 文档引用的源码文件必须存在：重命名模块后，旧路径会在这里立刻暴露。
#[test]
fn referenced_source_paths_exist() {
    let root = repo_root();
    let mut broken = Vec::new();
    for document in markdown_files(&root) {
        let content = fs::read_to_string(&document).unwrap_or_default();
        for path in source_paths(&content) {
            if !root.join(&path).is_file() {
                let from = document.strip_prefix(&root).unwrap_or(document.as_path());
                broken.push(format!("{}: {path}", from.display()));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "以下文档引用的源码路径不存在（共 {} 条）：\n{}",
        broken.len(),
        broken.join("\n")
    );
}

/// 从文本中挑出形如 `x86_64-unknown-linux-musl` 的 Rust target triple。
fn release_targets(content: &str) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    for token in content.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))) {
        let known_suffix = ["-gnu", "-musl", "-msvc", "-darwin"]
            .iter()
            .any(|suffix| token.ends_with(suffix));
        if known_suffix
            && token.matches('-').count() >= 2
            && !targets.iter().any(|known| known.as_str() == token)
        {
            targets.push(token.to_string());
        }
    }
    targets
}

/// 发布矩阵里的每个构建目标都要写进 README 与使用手册的平台支持表，
/// 并且发布格式统一为 tar.gz + sha256（含聚合的 SHA256SUMS）。
#[test]
fn release_targets_and_checksums_are_documented() {
    let root = repo_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect(".github/workflows/release.yml 应存在");

    let targets = release_targets(&workflow);
    assert!(
        !targets.is_empty(),
        "release.yml 应至少声明一个构建目标（target triple）"
    );
    assert!(
        workflow.contains(".tar.gz") && workflow.contains("SHA256SUMS"),
        "release.yml 应统一产出 tar.gz，并生成聚合校验文件 SHA256SUMS"
    );

    assert!(
        !workflow.contains(".zip"),
        "release.yml 应统一产出 tar.gz，不应再产出 zip 发布包"
    );

    for document in ["README.md", "docs/usage.md"] {
        let content = fs::read_to_string(root.join(document)).expect("文档应存在");
        assert!(
            content.contains("tar.gz"),
            "{document} 应说明发布包为 tar.gz"
        );
        for target in &targets {
            assert!(
                content.contains(target.as_str()),
                "{document} 未写明发布目标 {target}（见 .github/workflows/release.yml）"
            );
            let archive = format!("typewords-tui-vX.Y.Z-{target}.tar.gz");
            assert!(
                content.contains(&archive),
                "{document} 未写明归档名模板 {archive}"
            );
        }
    }

    // 发布流程写在开发指南里，改 matrix 时开发文档不能落后。
    let development =
        fs::read_to_string(root.join("docs/development.md")).expect("docs/development.md 应存在");
    for target in &targets {
        assert!(
            development.contains(target.as_str()),
            "docs/development.md 未写明发布目标 {target}"
        );
    }

    let usage = fs::read_to_string(root.join("docs/usage.md")).expect("docs/usage.md 应存在");
    assert!(
        usage.contains("SHA256SUMS"),
        "docs/usage.md 应说明聚合校验文件 SHA256SUMS 的用法"
    );
}

/// 行级切分 workflow 的 jobs: 段：返回 (作业名, 该作业的原始行)。
/// 只需要看懂本仓库这两个 workflow（作业名缩进 2 格、needs / run 在作业内），不引入 YAML 依赖。
fn workflow_jobs(workflow: &str) -> Vec<(String, Vec<String>)> {
    let mut jobs: Vec<(String, Vec<String>)> = Vec::new();
    let mut in_jobs = false;
    let mut current: Option<(String, Vec<String>)> = None;
    for line in workflow.lines() {
        let line = line.trim_end();
        if line.trim() == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if !line.is_empty() && !line.starts_with(' ') {
            in_jobs = false;
            jobs.extend(current.take());
            continue;
        }
        if let Some(header) = line.strip_prefix("  ")
            && !header.starts_with(' ')
            && let Some(name) = header.strip_suffix(':')
        {
            jobs.extend(current.take());
            current = Some((name.trim().to_string(), Vec::new()));
            continue;
        }
        if let Some((_, lines)) = current.as_mut() {
            lines.push(line.to_string());
        }
    }
    jobs.extend(current.take());
    jobs
}

/// 作业里 cargo ... 形式的检查命令（跳过注释行），按出现顺序去重。
fn cargo_commands(lines: &[String]) -> Vec<String> {
    let mut commands: Vec<String> = Vec::new();
    for line in lines {
        let mut text = line.trim();
        if text.starts_with('#') {
            continue;
        }
        if let Some(rest) = text.strip_prefix("- ") {
            text = rest.trim();
        }
        if let Some(rest) = text.strip_prefix("run:") {
            text = rest.trim();
        }
        if text.starts_with("cargo ") && !commands.iter().any(|known| known == text) {
            commands.push(text.to_string());
        }
    }
    commands
}

/// 读取作业的 needs：支持 needs: a、needs: [a, b] 与换行列表三种写法。
fn job_needs(lines: &[String]) -> Vec<String> {
    for (index, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix("needs:") else {
            continue;
        };
        let rest = rest.trim();
        if rest.starts_with('[') {
            return rest
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|item| item.trim().trim_matches('"').to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }
        if rest.is_empty() {
            return lines[index + 1..]
                .iter()
                .map_while(|follow| follow.trim().strip_prefix("- "))
                .map(|item| item.trim().trim_matches('"').to_string())
                .collect();
        }
        return vec![rest.trim_matches('"').to_string()];
    }
    Vec::new()
}

/// 发布前必须跑完整套检查：打包只能发生在 fmt / clippy / test 全绿之后。
///
/// 这是「推 tag 就发版」那道缺口的回归测试：release.yml 的 package 作业必须显式依赖
/// 跑过与 ci.yml 相同命令的作业，且 release 作业必须依赖 package。
#[test]
fn release_runs_the_ci_suite_before_packaging() {
    let root = repo_root();
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml"))
        .expect(".github/workflows/ci.yml 应存在");
    let release = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect(".github/workflows/release.yml 应存在");

    let ci_commands: Vec<String> = workflow_jobs(&ci)
        .iter()
        .flat_map(|(_, lines)| cargo_commands(lines))
        .collect();
    assert!(
        !ci_commands.is_empty(),
        "ci.yml 应声明至少一条 cargo 检查命令"
    );

    let release_jobs = workflow_jobs(&release);
    let mut verify_names: Vec<String> = Vec::new();
    let mut verify_commands: Vec<String> = Vec::new();
    for (name, lines) in &release_jobs {
        if name.starts_with("verify") {
            verify_names.push(name.clone());
            verify_commands.extend(cargo_commands(lines));
        }
    }
    assert!(
        !verify_names.is_empty(),
        "release.yml 应在打包前用 verify* 作业跑一遍检查"
    );
    for command in &ci_commands {
        assert!(
            verify_commands.contains(command),
            "release.yml 的 verify 作业缺少 ci.yml 的命令「{command}」：推 tag 前必须先跑完整套 CI"
        );
    }

    let package = release_jobs
        .iter()
        .find(|(name, _)| name == "package")
        .expect("release.yml 应有 package 作业");
    let package_needs = job_needs(&package.1);
    for name in &verify_names {
        assert!(
            package_needs.contains(name),
            "package 必须 needs: {name}，否则测试红灯也会照常打包发布"
        );
    }

    let publish = release_jobs
        .iter()
        .find(|(name, _)| name == "release")
        .expect("release.yml 应有 release 作业");
    assert!(
        job_needs(&publish.1).iter().any(|dep| dep == "package"),
        "release 作业必须 needs: package，不能绕过打包与冒烟"
    );
}

/// 从 Cargo.toml 的 [package] 段读取 repository，用于核对 CHANGELOG.md 底部的链接定义。
fn package_repository(root: &Path) -> String {
    let cargo = fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml 应存在");
    let mut in_package = false;
    for line in cargo.lines() {
        let line = line.trim();
        if line == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if in_package && let Some(value) = line.strip_prefix("repository = ") {
            return value.trim().trim_matches('"').to_string();
        }
    }
    panic!("Cargo.toml 的 [package] 段应有 repository");
}

/// CHANGELOG.md 的一个版本小节。
struct ChangelogSection {
    /// `Unreleased` 或形如 `0.3.3` 的版本号。
    version: String,
    /// 发布日期；`[Unreleased]` 为 `None`。
    date: Option<String>,
    /// 小节正文（不含标题行与文件底部的链接定义）。
    body: Vec<String>,
}

/// 按 `## [版本]` 标题切出小节；文件底部的链接定义（`[0.3.3]: https://…`）不计入正文。
fn changelog_sections(content: &str) -> Vec<ChangelogSection> {
    let mut sections: Vec<ChangelogSection> = Vec::new();
    for line in content.lines() {
        let line = line.trim_end();
        let heading = line
            .strip_prefix("## [")
            .and_then(|rest| rest.find(']').map(|end| (rest, end)));
        if let Some((rest, end)) = heading {
            let tail = rest[end + 1..].trim();
            let date = tail.strip_prefix('-').map(|date| date.trim().to_string());
            sections.push(ChangelogSection {
                version: rest[..end].to_string(),
                date,
                body: Vec::new(),
            });
            continue;
        }
        if let Some(section) = sections.last_mut() {
            // 底部的链接定义（`[0.3.3]: https://…`）不属于任何版本小节。
            if line.starts_with('[') && line.contains("]: ") {
                continue;
            }
            section.body.push(line.to_string());
        }
    }
    sections
}

/// 日历日期：只做格式与取值范围检查，够用即可。
fn is_iso_date(date: &str) -> bool {
    if !date.is_ascii() || date.len() != 10 {
        return false;
    }
    let bytes = date.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Ok(year) = date[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = date[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = date[8..10].parse::<u32>() else {
        return false;
    };
    year >= 2020 && (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// 解析 `x.y.z`，用于校验版本严格倒序。
fn semver_key(version: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// CHANGELOG.md 必须遵循 Keep a Changelog：`[Unreleased]` 打头、版本从新到旧、
/// 日期合法、分类只用标准六类、最新版本与 Cargo.toml 一致、最早版本 0.1.0 不能丢。
#[test]
fn changelog_follows_keep_a_changelog() {
    const CATEGORIES: [&str; 6] = ["新增", "变更", "弃用", "移除", "修复", "安全"];

    let root = repo_root();
    let content = fs::read_to_string(root.join("CHANGELOG.md")).expect("仓库根应有 CHANGELOG.md");

    let title = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();
    assert_eq!(title, "# 变更日志", "CHANGELOG.md 的标题应为 `# 变更日志`");
    assert!(
        content.contains("Keep a Changelog") && content.contains("keepachangelog.com"),
        "CHANGELOG.md 应说明格式来自 Keep a Changelog 并链到规范"
    );
    assert!(
        content.contains("语义化版本"),
        "CHANGELOG.md 应说明版本号遵循语义化版本"
    );

    let sections = changelog_sections(&content);
    assert!(
        sections.len() >= 2,
        "CHANGELOG.md 应至少有 [Unreleased] 与一个已发布版本"
    );
    assert_eq!(
        sections[0].version, "Unreleased",
        "[Unreleased] 应排在最前面，新改动先记在这里"
    );
    assert!(sections[0].date.is_none(), "[Unreleased] 不带发布日期");

    for section in &sections {
        for line in &section.body {
            let Some(heading) = line.strip_prefix("### ") else {
                continue;
            };
            assert!(
                CATEGORIES.contains(&heading.trim()),
                "{} 使用了非 Keep a Changelog 的分类「{}」，只能用：{}",
                section.version,
                heading.trim(),
                CATEGORIES.join(" / ")
            );
        }
        if section.version == "Unreleased" {
            continue;
        }
        assert!(
            section.body.iter().any(|line| line.starts_with("- ")),
            "{} 下没有任何条目：发版时把 [Unreleased] 里的改动移到这里",
            section.version
        );
        let date = section.date.as_deref().unwrap_or_else(|| {
            panic!(
                "{} 缺少发布日期，标题应写成 `## [{}] - YYYY-MM-DD`",
                section.version, section.version
            )
        });
        assert!(
            is_iso_date(date),
            "{} 的发布日期「{}」不是 YYYY-MM-DD",
            section.version,
            date
        );
    }

    // 版本必须从新到旧排列、没有重复，且最早的一版是 0.1.0（历史不能缺失）。
    let mut previous_version: Option<(u64, u64, u64)> = None;
    let mut previous_date: Option<String> = None;
    let mut versions: Vec<&str> = Vec::new();
    for section in sections.iter().skip(1) {
        let version = section.version.as_str();
        let key =
            semver_key(version).unwrap_or_else(|| panic!("版本号「{version}」不是 x.y.z 形式"));
        if let Some(previous) = previous_version {
            assert!(
                key < previous,
                "版本应从新到旧排列：{version} 排在更新的版本之后"
            );
        }
        let date = section.date.clone().unwrap_or_default();
        if let Some(previous) = previous_date.as_deref() {
            assert!(
                date.as_str() <= previous,
                "{version} 的发布日期 {date} 比更新版本还晚，版本小节应从新到旧"
            );
        }
        previous_version = Some(key);
        previous_date = Some(date);
        versions.push(version);
    }

    let cargo_version = package_version(&root);
    assert_eq!(
        versions.first().copied(),
        Some(cargo_version.as_str()),
        "CHANGELOG.md 最新的已发布版本应与 Cargo.toml 的 version 一致：发版时先把 [Unreleased] 的条目移到 `## [{cargo_version}]` 下"
    );
    assert_eq!(
        versions.last().copied(),
        Some("0.1.0"),
        "CHANGELOG.md 必须保留最早版本 0.1.0 的记录，早期历史不能缺失"
    );

    // 每个版本小节在文件底部都有一行链接定义：与上一版的比对（最早一版指向 Release 页）。
    let repository = package_repository(&root);
    let unreleased = format!("[Unreleased]: {repository}/compare/v{}...HEAD", versions[0]);
    assert!(
        content.contains(&unreleased),
        "CHANGELOG.md 底部缺少链接定义：{unreleased}"
    );
    for (index, version) in versions.iter().enumerate() {
        let expected = match versions.get(index + 1) {
            Some(previous) => format!("[{version}]: {repository}/compare/v{previous}...v{version}"),
            None => format!("[{version}]: {repository}/releases/tag/v{version}"),
        };
        assert!(
            content.contains(&expected),
            "CHANGELOG.md 底部缺少链接定义：{expected}"
        );
    }
}

/// 发布说明只能有一个来源：CHANGELOG.md。release.yml 不得回退到单独目录或
/// `--generate-notes`，且打包前就要拦住「忘了写变更日志」的 tag。
#[test]
fn release_notes_come_from_the_changelog() {
    let root = repo_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect(".github/workflows/release.yml 应存在");
    assert!(
        workflow.contains("CHANGELOG.md"),
        "release.yml 的发布说明应来自 CHANGELOG.md"
    );
    assert!(
        workflow.contains("scripts/changelog.sh"),
        "release.yml 应调用 scripts/changelog.sh 提取发布说明，避免在 YAML 里再写一份解析逻辑"
    );
    assert!(
        !workflow.contains(".github/release-notes"),
        "发布说明只允许一个来源：release.yml 不得再读取 .github/release-notes/（该目录已冻结为历史归档）"
    );
    assert!(
        !workflow.contains("--generate-notes"),
        "发布说明必须来自 CHANGELOG.md，不能退回 --generate-notes"
    );

    let jobs = workflow_jobs(&workflow);
    let guard = jobs
        .iter()
        .find(|(name, _)| name == "guard")
        .expect("release.yml 应有 guard 作业");
    assert!(
        guard.1.iter().any(|line| line.contains("changelog.sh")),
        "guard 作业应校验 CHANGELOG.md 里有该版本的条目，否则忘写变更日志也能发版"
    );

    let script =
        fs::read_to_string(root.join("scripts/changelog.sh")).expect("scripts/changelog.sh 应存在");
    assert!(
        script.starts_with("#!/usr/bin/env bash"),
        "scripts/changelog.sh 应是 bash 脚本"
    );
    assert!(
        script.contains("set -euo pipefail"),
        "scripts/changelog.sh 应开启 set -euo pipefail"
    );

    // 历史公告若还留在仓库里，只能是冻结的归档：必须写明唯一来源是 CHANGELOG.md。
    let legacy = root.join(".github/release-notes");
    if legacy.is_dir() {
        let readme = fs::read_to_string(legacy.join("README.md")).expect(
            ".github/release-notes/ 只保留历史归档，需要 README.md 说明唯一来源是 CHANGELOG.md",
        );
        assert!(
            readme.contains("CHANGELOG.md"),
            ".github/release-notes/README.md 应指向 CHANGELOG.md"
        );
    }
}
