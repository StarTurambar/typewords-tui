//! 分层约束：领域层不能依赖上层模块。
//!
//! `src/domain/` 必须与终端无关，也不能依赖 `app`/`ui`/`store`/`theme`——
//! 这样它才能被单测与 `tw task` 直接复用。早期的自建词典功能让
//! `domain/catalog.rs` 调用了 `store::write_atomic`，形成 `domain ↔ store`
//! 的环；原子写因此挪到更低层的 `src/fs.rs`。

use std::path::Path;

fn source_files(dir: &str) -> Vec<std::path::PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new(dir), &mut out);
    out.sort();
    out
}

/// 去掉行注释与字符串字面量，避免注释里的 `crate::store` 被当成真的依赖。
fn strip_comments_and_strings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut in_char = false;
    while let Some(character) = chars.next() {
        match character {
            '/' if !in_string && !in_char && chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if !in_string && !in_char && chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            '"' if !in_char => {
                in_string = !in_string;
            }
            '\'' if !in_string => {
                in_char = !in_char;
            }
            _ => {
                if !in_string && !in_char {
                    out.push(character);
                }
            }
        }
    }
    out
}

#[test]
fn the_domain_layer_does_not_depend_on_upper_layers() {
    let forbidden = [
        "crate::app",
        "crate::ui",
        "crate::store",
        "crate::theme",
        "ratatui",
        "crossterm",
    ];
    for path in source_files("src/domain") {
        let text = std::fs::read_to_string(&path).expect("读源码");
        let code = strip_comments_and_strings(&text);
        for needle in forbidden {
            assert!(
                !code.contains(needle),
                "{} 里出现了 {needle}：领域层不能依赖上层模块",
                path.display()
            );
        }
    }
}

/// 领域层也不能引用终端相关的 crate。
#[test]
fn the_domain_layer_does_not_use_terminal_crates() {
    for path in source_files("src/domain") {
        let text = std::fs::read_to_string(&path).expect("读源码");
        let code = strip_comments_and_strings(&text);
        assert!(
            !code.contains("use ratatui") && !code.contains("use crossterm"),
            "{} 引入了终端 crate",
            path.display()
        );
    }
}
