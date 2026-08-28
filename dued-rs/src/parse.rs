use tree_sitter::{Node, Parser, Tree};

use crate::metrics::complexity;

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: String,
    pub docstring: String,
    pub body: String,
    pub cyclomatic: i64,
    pub cognitive: i64,
    pub nesting: i64,
    pub nargs: i64,
    pub is_public: bool,
    pub is_entry: bool,
    pub is_test: bool,
}

#[derive(Default, Debug)]
pub struct Extracted {
    pub symbols: Vec<Symbol>,
    pub imports: Vec<String>,
    pub calls: Vec<(String, String)>,
    pub import_modules: Vec<String>,
    pub ast_nodes: i64,
}

fn text(source: &[u8], node: Node) -> String {
    String::from_utf8_lossy(&source[node.start_byte()..node.end_byte()]).into_owned()
}

fn count_ast(node: Node) -> i64 {
    let mut n = 1;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        n += count_ast(child);
    }
    n
}

fn parse_tree(language: &str, path_suffix: &str, source: &[u8]) -> Option<Tree> {
    thread_local! {
        static PY: std::cell::RefCell<Option<Parser>> = const { std::cell::RefCell::new(None) };
        static TS: std::cell::RefCell<Option<Parser>> = const { std::cell::RefCell::new(None) };
        static TSX: std::cell::RefCell<Option<Parser>> = const { std::cell::RefCell::new(None) };
        static JS: std::cell::RefCell<Option<Parser>> = const { std::cell::RefCell::new(None) };
        static RS: std::cell::RefCell<Option<Parser>> = const { std::cell::RefCell::new(None) };
    }
    fn with_parser(cell: &std::cell::RefCell<Option<Parser>>, lang: tree_sitter::Language, source: &[u8]) -> Option<Tree> {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let mut parser = Parser::new();
            parser.set_language(&lang).ok()?;
            *slot = Some(parser);
        }
        slot.as_mut()?.parse(source, None)
    }
    if language == "python" {
        PY.with(|cell| with_parser(cell, tree_sitter_python::LANGUAGE.into(), source))
    } else if language == "rust" {
        RS.with(|cell| with_parser(cell, tree_sitter_rust::LANGUAGE.into(), source))
    } else if language == "typescript" && matches!(path_suffix, ".tsx" | ".jsx") {
        TSX.with(|cell| with_parser(cell, tree_sitter_typescript::LANGUAGE_TSX.into(), source))
    } else if language == "typescript" && matches!(path_suffix, ".js" | ".jsx") {
        JS.with(|cell| with_parser(cell, tree_sitter_javascript::LANGUAGE.into(), source))
    } else {
        TS.with(|cell| with_parser(cell, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), source))
    }
}

fn count_params_src(params: Option<Node>, source: &[u8]) -> i64 {
    let Some(params) = params else {
        return 0;
    };
    let mut count = 0;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() == "self_parameter" {
            continue;
        }
        if matches!(
            child.kind(),
            "identifier"
                | "typed_parameter"
                | "default_parameter"
                | "parameter"
                | "required_parameter"
                | "optional_parameter"
                | "rest_parameter"
        ) {
            let name = text(source, child);
            if matches!(name.as_str(), "self" | "cls" | "&self" | "&mut self") {
                continue;
            }
            count += 1;
        }
    }
    count
}

const ENTRY_NAMES: &[&str] = &["main", "cli", "app", "handler", "run"];

fn python_docstring(fn_node: Node, source: &[u8]) -> String {
    let Some(body) = fn_node.child_by_field_name("body") else {
        return String::new();
    };
    let Some(first) = body.child(0) else {
        return String::new();
    };
    if first.kind() == "expression_statement" && first.child_count() > 0 {
        if let Some(inner) = first.child(0) {
            if inner.kind() == "string" {
                return text(source, inner).trim_matches(|c| c == ' ' || c == '"' || c == '\'').to_string();
            }
        }
    }
    String::new()
}

fn python_import_names(node: Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    if node.kind() == "import_from_statement" {
        if let Some(module) = node.child_by_field_name("module_name") {
            names.push(text(source, module).split('.').next().unwrap_or("").to_string());
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "dotted_name" || child.kind() == "identifier" {
            names.push(text(source, child).split('.').next().unwrap_or("").to_string());
        }
    }
    names
}

fn leading_comment(node: Node, source: &[u8]) -> String {
    if let Some(prev) = node.prev_named_sibling() {
        if matches!(prev.kind(), "comment" | "line_comment" | "block_comment") {
            return text(source, prev)
                .trim_start_matches(|c| c == '/' || c == '#' || c == '!' || c == ' ')
                .trim()
                .to_string();
        }
    }
    String::new()
}

/// Outer attrs sit as named siblings before the item in tree-sitter-rust (`#[test]` then `fn`).
fn leading_attrs(node: Node, source: &[u8]) -> String {
    let mut stack = Vec::new();
    let mut prev = node.prev_named_sibling();
    while let Some(p) = prev {
        if p.kind() != "attribute_item" {
            break;
        }
        stack.push(text(source, p));
        prev = p.prev_named_sibling();
    }
    let mut attrs = String::new();
    for a in stack.into_iter().rev() {
        attrs.push_str(&a);
        attrs.push(' ');
    }
    attrs
}

fn is_rust_test_attr(attrs: &str) -> bool {
    attrs.contains("#[test]") || attrs.contains("::test]")
}

fn strip_turbofish(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find("::<") {
        out.push_str(&rest[..idx]);
        rest = &rest[idx + 3..];
        let mut depth = 1usize;
        let mut end = 0usize;
        for (i, ch) in rest.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + ch.len_utf8();
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = if end > 0 { &rest[end..] } else { "" };
    }
    out.push_str(rest);
    out
}

fn is_code_ident(name: &str) -> bool {
    if name.is_empty() || name.starts_with('<') || name.chars().any(char::is_whitespace) {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn call_ident(node: Node, source: &[u8]) -> String {
    let fn_node = node.child_by_field_name("function").or_else(|| node.child(0));
    let Some(fn_node) = fn_node else {
        return String::new();
    };
    let before_paren = text(source, fn_node).split('(').next().unwrap_or("").to_string();
    let without_turbo = strip_turbofish(&before_paren);
    let name = without_turbo
        .rsplit("::")
        .next()
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("")
        .trim();
    if is_code_ident(name) {
        name.to_string()
    } else {
        String::new()
    }
}

fn collect_calls(node: Node, source: &[u8], owner: &str, out: &mut Extracted) {
    if node.kind() == "call" || node.kind() == "call_expression" {
        let name = call_ident(node, source);
        if !name.is_empty() {
            out.calls.push((owner.to_string(), name));
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calls(child, source, owner, out);
    }
}

fn push_fn(
    out: &mut Extracted,
    source: &[u8],
    node: Node,
    name: String,
    kind: &str,
    sig: String,
    doc: String,
    nargs: i64,
    is_public: bool,
    is_entry: bool,
    is_test: bool,
) {
    let (cyc, cog, nest) = complexity(node, source, &name);
    out.symbols.push(Symbol {
        name: name.clone(),
        kind: kind.to_string(),
        start_line: node.start_position().row as i64 + 1,
        end_line: node.end_position().row as i64 + 1,
        signature: sig,
        docstring: doc,
        body: text(source, node),
        cyclomatic: cyc,
        cognitive: cog,
        nesting: nest,
        nargs,
        is_public,
        is_entry,
        is_test,
    });
    collect_calls(node, source, &name, out);
}

fn walk_python(node: Node, source: &[u8], out: &mut Extracted, class_name: Option<&str>, forced_entry: bool) {
    if node.kind() == "import_statement" || node.kind() == "import_from_statement" {
        out.imports.push(text(source, node).lines().next().unwrap_or("").to_string());
        out.import_modules.extend(python_import_names(node, source));
    } else if node.kind() == "decorated_definition" {
        let deco = text(source, node);
        let is_cmd = deco.contains(".command") || deco.contains("callback") || deco.contains("@app.");
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_python(child, source, out, class_name, is_cmd);
        }
        return;
    } else if node.kind() == "function_definition" {
        let name = node
            .child_by_field_name("name")
            .map(|n| text(source, n))
            .unwrap_or_else(|| "anonymous".into());
        let params = node.child_by_field_name("parameters");
        let nargs = count_params_src(params, source);
        let mut sig = format!("def {name}{}", params.map(|p| text(source, p)).unwrap_or_else(|| "()".into()));
        if let Some(cname) = class_name {
            sig = format!("class {cname}: {sig}");
        }
        let is_entry = forced_entry || ENTRY_NAMES.contains(&name.as_str()) || name == "__main__";
        push_fn(
            out,
            source,
            node,
            name.clone(),
            if class_name.is_some() { "method" } else { "function" },
            sig,
            python_docstring(node, source),
            nargs,
            !name.starts_with('_'),
            is_entry,
            name.starts_with("test_"),
        );
        return;
    } else if node.kind() == "class_definition" {
        let cname = node
            .child_by_field_name("name")
            .map(|n| text(source, n))
            .unwrap_or_else(|| "Anonymous".into());
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                walk_python(child, source, out, Some(&cname), false);
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_python(child, source, out, class_name, forced_entry);
    }
}

fn maybe_arrow_fn(node: Node, source: &[u8], out: &mut Extracted) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Some(value) = child.child_by_field_name("value") else {
            continue;
        };
        if value.kind() != "arrow_function" && value.kind() != "function" {
            continue;
        }
        let name = text(source, name_node);
        let nargs = count_params_src(value.child_by_field_name("parameters"), source);
        push_fn(
            out,
            source,
            node,
            name.clone(),
            "function",
            format!("const {name} = ..."),
            leading_comment(node, source),
            nargs,
            !name.starts_with('_'),
            ENTRY_NAMES.contains(&name.as_str()),
            name.starts_with("test"),
        );
        collect_calls(value, source, &name, out);
    }
}

fn walk_ts(node: Node, source: &[u8], out: &mut Extracted) {
    if node.kind() == "import_statement" {
        out.imports.push(text(source, node).lines().next().unwrap_or("").to_string());
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string" {
                out.import_modules
                    .push(text(source, child).trim_matches(|c| c == '"' || c == '\'').to_string());
            }
        }
    } else if matches!(node.kind(), "function_declaration" | "method_definition" | "function_signature") {
        let name = node
            .child_by_field_name("name")
            .map(|n| text(source, n))
            .unwrap_or_else(|| "anonymous".into());
        let nargs = count_params_src(node.child_by_field_name("parameters"), source);
        let sig = text(source, node)
            .split('{')
            .next()
            .unwrap_or("")
            .split("=>")
            .next()
            .unwrap_or("")
            .trim()
            .chars()
            .take(240)
            .collect();
        let is_entry = ENTRY_NAMES.contains(&name.as_str()) || matches!(name.as_str(), "GET" | "POST" | "PUT" | "DELETE");
        push_fn(
            out,
            source,
            node,
            name.clone(),
            "function",
            sig,
            leading_comment(node, source),
            nargs,
            !name.starts_with('_'),
            is_entry,
            name.starts_with("test") || name.ends_with("Test"),
        );
        return;
    } else if node.kind() == "lexical_declaration" {
        maybe_arrow_fn(node, source, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_ts(child, source, out);
    }
}

fn walk_rust(node: Node, source: &[u8], out: &mut Extracted) {
    if node.kind() == "use_declaration" {
        let t = text(source, node);
        out.imports.push(t.lines().next().unwrap_or("").to_string());
        out.import_modules.push(t);
    } else if node.kind() == "function_item" {
        let name = node
            .child_by_field_name("name")
            .map(|n| text(source, n))
            .unwrap_or_else(|| "anonymous".into());
        let nargs = count_params_src(node.child_by_field_name("parameters"), source);
        let mut vis = false;
        let mut attrs = leading_attrs(node, source);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                vis = true;
            }
            if child.kind() == "attribute_item" {
                attrs.push_str(&text(source, child));
                attrs.push(' ');
            }
        }
        let sig: String = text(source, node)
            .split('{')
            .next()
            .unwrap_or("")
            .trim()
            .chars()
            .take(240)
            .collect();
        push_fn(
            out,
            source,
            node,
            name.clone(),
            "function",
            sig,
            leading_comment(node, source),
            nargs,
            vis || name == "main",
            name == "main" || attrs.contains("main]"),
            is_rust_test_attr(&attrs) || name.starts_with("test_"),
        );
        return;
    } else if matches!(
        node.kind(),
        "struct_item" | "enum_item" | "type_item" | "trait_item" | "union_item"
    ) {
        // Types must enter `symbols` so `slice WholesaleContract` (and peers) resolve.
        let name = node
            .child_by_field_name("name")
            .map(|n| text(source, n))
            .unwrap_or_else(|| "anonymous".into());
        let mut vis = false;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                vis = true;
            }
        }
        let kind = match node.kind() {
            "struct_item" => "struct",
            "enum_item" => "enum",
            "type_item" => "type",
            "trait_item" => "trait",
            "union_item" => "union",
            _ => "type",
        };
        let sig: String = text(source, node)
            .split('{')
            .next()
            .unwrap_or("")
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .chars()
            .take(240)
            .collect();
        push_fn(
            out,
            source,
            node,
            name,
            kind,
            sig,
            leading_comment(node, source),
            0,
            vis,
            false,
            false,
        );
        // Still walk children so methods inside impl are separate; struct fields are not.
        // For type items, children are fields/variants — do not recurse into them as fns.
        return;
    } else if node.kind() == "impl_item" {
        // Walk impl bodies for methods; the impl type name itself is not a symbol here.
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_rust(child, source, out);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_rust(child, source, out);
    }
}

pub fn parse_source(language: &str, path_suffix: &str, source: &[u8]) -> Extracted {
    let Some(tree) = parse_tree(language, path_suffix, source) else {
        return Extracted::default();
    };
    let root = tree.root_node();
    let mut extracted = Extracted {
        ast_nodes: count_ast(root),
        ..Extracted::default()
    };
    match language {
        "python" => walk_python(root, source, &mut extracted, None, false),
        "rust" => walk_rust(root, source, &mut extracted),
        _ => walk_ts(root, source, &mut extracted),
    }
    extracted
}

#[cfg(test)]
mod rust_type_tests {
    use super::*;

    #[test]
    fn call_ident_rejects_string_chop_garbage() {
        let src = br#"
fn resolve_deploy_anchor_pop() {
    let _ = "Need compute capacity (datacenter)".into();
}
"#;
        let extracted = parse_source("rust", ".rs", src);
        let callees: Vec<&str> = extracted
            .calls
            .iter()
            .filter(|(owner, _)| owner == "resolve_deploy_anchor_pop")
            .map(|(_, c)| c.as_str())
            .collect();
        assert!(
            !callees.iter().any(|c| c.contains(char::is_whitespace) || c.starts_with('<')),
            "garbage callees: {callees:?}"
        );
        assert!(!callees.iter().any(|c| c.starts_with("Need")), "{callees:?}");
    }

    #[test]
    fn call_ident_turbofish_uses_function_name() {
        let src = br#"
fn caller() {
    let _ = foo::<Row>(1);
}
fn foo<T>(_x: i32) {}
"#;
        let extracted = parse_source("rust", ".rs", src);
        let callees: Vec<&str> = extracted
            .calls
            .iter()
            .filter(|(owner, _)| owner == "caller")
            .map(|(_, c)| c.as_str())
            .collect();
        assert!(callees.contains(&"foo"), "{callees:?}");
        assert!(!callees.iter().any(|c| c.starts_with('<') || c.contains("Row")), "{callees:?}");
    }

    #[test]
    fn extracts_rust_struct_and_impl_method() {
        let src = br#"
pub struct WholesaleContract {
    pub id: u64,
    pub qty: i64,
}

impl WholesaleContract {
    pub fn apply(&self) -> u64 { self.id }
}

pub fn apply_purchase_wholesale(c: &WholesaleContract) -> u64 {
    c.apply()
}
"#;
        let extracted = parse_source("rust", ".rs", src);
        let names: Vec<&str> = extracted.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"WholesaleContract"), "{names:?}");
        assert!(names.contains(&"apply"), "{names:?}");
        assert!(names.contains(&"apply_purchase_wholesale"), "{names:?}");
        let kind = extracted
            .symbols
            .iter()
            .find(|s| s.name == "WholesaleContract")
            .map(|s| s.kind.as_str());
        assert_eq!(kind, Some("struct"));
    }

    #[test]
    fn extracts_rust_enum_and_type_alias() {
        let src = br#"
pub enum AccessTier { Free, Paid }
pub type WholesaleId = u64;
"#;
        let extracted = parse_source("rust", ".rs", src);
        let by_name: std::collections::HashMap<&str, &str> = extracted
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.as_str()))
            .collect();
        assert_eq!(by_name.get("AccessTier"), Some(&"enum"));
        assert_eq!(by_name.get("WholesaleId"), Some(&"type"));
    }

    #[test]
    fn rust_test_attr_sets_is_test() {
        let src = b"#[test]\nfn rings_one() {}\n\nfn not_a_test() {}\n";
        let extracted = parse_source("rust", ".rs", src);
        let by_name: std::collections::HashMap<&str, bool> = extracted
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.is_test))
            .collect();
        assert_eq!(by_name.get("rings_one"), Some(&true), "{by_name:?}");
        assert_eq!(by_name.get("not_a_test"), Some(&false), "{by_name:?}");
    }
}
