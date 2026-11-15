//! 文档一致性检查：README 与 docs/ 中的相对链接必须指向真实存在的文件，
//! 且 docs/README.md 必须登记 docs/ 下的全部 Markdown 文档。
//!
//! 检查只依赖本地文件系统，随 `cargo test` 在所有平台运行，防止文档重构后留下死链。

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![root.join("README.md")];
    collect_markdown(&root.join("docs"), &mut files);
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
            if candidate.ends_with(".rs") {
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
