//! 分层约束：领域层与主题层不能依赖上层模块。
//!
//! `src/domain/` 必须与终端无关，也不能依赖 `app`/`ui`/`store`/`theme`——
//! 这样它才能被单测与 `tw task` 直接复用。早期的自建词典功能让
//! `domain/catalog.rs` 调用了 `store::write_atomic`，形成 `domain ↔ store`
//! 的环；原子写因此挪到更低层的 `src/fs.rs`。
//!
//! 检查用 `syn` 解析源码，而不是手写词法扫描：早期版本把 `'` 当字符字面量
//! 定界符，Rust 的生命周期标注（`&'a str`）会让它把两个 `'` 之间的整段代码
//! 当成字面量丢掉——`domain/typing.rs` 只剩不到 10% 的文本真正被检查。

use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::Visit;

/// domain 的禁止清单：上层模块与终端相关 crate。
const DOMAIN_FORBIDDEN: &[&str] = &[
    "crate::app",
    "crate::ui",
    "crate::store",
    "crate::theme",
    "ratatui",
    "crossterm",
];

/// theme 只处理“主题是什么”，渲染与状态都在上层。
const THEME_FORBIDDEN: &[&str] = &["crate::app", "crate::ui", "ratatui", "crossterm"];

fn source_files(dir: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
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

/// 解析源码，返回其中真正引用到的禁止依赖根（如 `crate::store`、`ratatui`）。
///
/// 注释与字符串字面量里的路径会被 `syn` 自然忽略；`use` 分组、全限定路径、
/// `extern crate` 以及宏体里的 `crate::…` 都会被检查到。
fn forbidden_references(source: &str, forbidden: &[&str]) -> Vec<String> {
    let file = syn::parse_file(source).expect("源码必须能被 syn 解析");
    let mut visitor = ForbiddenVisitor {
        forbidden,
        found: Vec::new(),
    };
    visitor.visit_file(&file);
    let mut found = visitor.found;
    found.sort();
    found.dedup();
    found
}

struct ForbiddenVisitor<'a> {
    forbidden: &'a [&'a str],
    found: Vec<String>,
}

impl ForbiddenVisitor<'_> {
    fn record(&mut self, root: String) {
        if self.forbidden.iter().any(|forbidden| *forbidden == root) {
            self.found.push(root);
        }
    }

    fn record_use_tree(&mut self, prefix: &mut Vec<String>, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.record_use_tree(prefix, &path.tree);
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                prefix.push(name.ident.to_string());
                self.record(use_root(prefix));
                prefix.pop();
            }
            syn::UseTree::Rename(rename) => {
                prefix.push(rename.ident.to_string());
                self.record(use_root(prefix));
                prefix.pop();
            }
            syn::UseTree::Glob(_) => self.record(use_root(prefix)),
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.record_use_tree(prefix, item);
                }
            }
        }
    }

    /// 宏体与属性参数都是未解析的 token 流，单独扫一遍 `crate::…` 与外层 crate 名。
    ///
    /// syn 默认的 `visit_token_stream` 是空实现；宏调用（`visit_macro`）和
    /// 属性参数（`visit_meta_list`）最终都会走到这里，因此只需覆写这一处。
    fn record_token_stream(&mut self, tokens: TokenStream) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let mut index = 0;
        while index < trees.len() {
            match &trees[index] {
                TokenTree::Group(group) => self.record_token_stream(group.stream()),
                TokenTree::Ident(ident) if ident == "crate" => {
                    for root in crate_paths_in(&trees[index..]) {
                        self.record(root);
                    }
                }
                TokenTree::Ident(ident) => self.record(ident.to_string()),
                _ => {}
            }
            index += 1;
        }
    }
}

impl<'ast> Visit<'ast> for ForbiddenVisitor<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(root) = path_root(path) {
            self.record(root);
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_use_tree(&mut self, tree: &'ast syn::UseTree) {
        let mut prefix = Vec::new();
        self.record_use_tree(&mut prefix, tree);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        self.record(item.ident.to_string());
        syn::visit::visit_item_extern_crate(self, item);
    }

    fn visit_token_stream(&mut self, tokens: &'ast TokenStream) {
        self.record_token_stream(tokens.clone());
    }
}

/// 路径的依赖根：`crate::store::FileStore` → `crate::store`，`ratatui::Color` → `ratatui`。
fn path_root(path: &syn::Path) -> Option<String> {
    let mut segments = path.segments.iter();
    let first = segments.next()?.ident.to_string();
    if first != "crate" {
        return Some(first);
    }
    let second = segments.next()?.ident.to_string();
    Some(format!("crate::{second}"))
}

/// 把 `use` 树已累积的路径前缀归一成依赖根。
fn use_root(prefix: &[String]) -> String {
    match prefix {
        [first, second, ..] if first.as_str() == "crate" => format!("crate::{second}"),
        [first, ..] => first.clone(),
        [] => String::new(),
    }
}

/// 从 `crate` 之后的 token 里取出依赖根，兼容 `crate::store` 与 `crate::{store, ui}`。
fn crate_paths_in(trees: &[TokenTree]) -> Vec<String> {
    let is_colon = |token: Option<&TokenTree>| matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == ':');
    if !is_colon(trees.get(1)) || !is_colon(trees.get(2)) {
        return Vec::new();
    }
    match trees.get(3) {
        Some(TokenTree::Ident(ident)) => vec![format!("crate::{ident}")],
        Some(TokenTree::Group(group)) => group_item_roots(group.stream()),
        _ => Vec::new(),
    }
}

/// `crate::{app, store::FileStore, ui}` 里每一项的根。
fn group_item_roots(stream: TokenStream) -> Vec<String> {
    let mut roots = Vec::new();
    let mut at_item_start = true;
    for token in stream {
        match token {
            TokenTree::Punct(punct) if punct.as_char() == ',' => at_item_start = true,
            TokenTree::Ident(ident) if at_item_start => {
                roots.push(format!("crate::{ident}"));
                at_item_start = false;
            }
            _ => at_item_start = false,
        }
    }
    roots
}

fn assert_layer_is_clean(dir: &str, forbidden: &[&str]) {
    let files = source_files(dir);
    assert!(!files.is_empty(), "{dir} 下没有找到 .rs 文件：分层守卫失效");
    for path in files {
        let text = std::fs::read_to_string(&path).expect("读源码");
        let found = forbidden_references(&text, forbidden);
        assert!(
            found.is_empty(),
            "{} 里出现了 {}：这一层不能依赖上层模块或终端 crate",
            path.display(),
            found.join("、")
        );
    }
}

#[test]
fn the_domain_layer_does_not_depend_on_upper_layers() {
    assert_layer_is_clean("src/domain", DOMAIN_FORBIDDEN);
}

#[test]
fn the_theme_layer_does_not_depend_on_upper_layers() {
    assert_layer_is_clean("src/theme", THEME_FORBIDDEN);
}

/// 守卫自身必须有回归测试：`use crate::store::X;` 要被识别出来。
///
/// 这条用例针对早期版本的真实缺陷——把 `'` 当字符字面量定界符，
/// 于是 `&'static str` 之后的整段代码都被当成字面量丢弃，守卫通过却什么都没查。
#[test]
fn the_layering_guard_detects_a_forbidden_use_statement() {
    let source = r#"
        fn f(x: &'static str) { }
        use crate::store::FileStore;
    "#;
    assert_eq!(
        forbidden_references(source, DOMAIN_FORBIDDEN),
        ["crate::store"]
    );
}

#[test]
fn the_layering_guard_sees_code_after_lifetimes_and_char_literals() {
    let source = r#"
        fn label<'a>(value: &'a str) -> &'a str {
            let quote = '"';
            let mark = 'x';
            let _ = (quote, mark);
            value
        }

        fn load() {
            let _ = crate::store::FileStore::new();
        }
    "#;
    assert_eq!(
        forbidden_references(source, DOMAIN_FORBIDDEN),
        ["crate::store"]
    );
}

#[test]
fn the_layering_guard_ignores_comments_and_string_literals() {
    let source = r#"
        // use crate::store::FileStore;
        /* crate::ui::render 只出现在注释里 */
        const HINT: &str = "crate::app::run";
        fn f() {}
    "#;
    assert!(forbidden_references(source, DOMAIN_FORBIDDEN).is_empty());
}

#[test]
fn the_layering_guard_sees_grouped_imports() {
    let source = "use crate::{app, store::FileStore, ui};";
    assert_eq!(
        forbidden_references(source, DOMAIN_FORBIDDEN),
        ["crate::app", "crate::store", "crate::ui"]
    );
}

#[test]
fn the_layering_guard_sees_references_inside_macros() {
    let source = "fn f() { assert!(crate::store::FileStore::new().is_ok()); }";
    assert_eq!(
        forbidden_references(source, DOMAIN_FORBIDDEN),
        ["crate::store"]
    );
}

/// 属性参数也是 token 流：`#[attr(crate::store::X)]` 不能被漏掉。
///
/// syn 默认的 `visit_token_stream` 是空实现，只覆写 `visit_macro` 会漏掉
/// 属性里的路径（旧的手写扫描器反而能抓到）。
#[test]
fn the_layering_guard_sees_references_inside_attribute_tokens() {
    let source = "#[some_attr(crate::store::FileStore)] fn f() {}";
    assert_eq!(
        forbidden_references(source, DOMAIN_FORBIDDEN),
        ["crate::store"]
    );
}

#[test]
fn the_layering_guard_detects_terminal_crates() {
    assert_eq!(
        forbidden_references("use ratatui::style::Color;", DOMAIN_FORBIDDEN),
        ["ratatui"]
    );
    assert_eq!(
        forbidden_references(
            "fn f() -> crossterm::Result<()> { todo!() }",
            DOMAIN_FORBIDDEN
        ),
        ["crossterm"]
    );
}
