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
