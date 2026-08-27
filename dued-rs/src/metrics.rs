use tree_sitter::Node;

const CYCLO_TYPES: &[&str] = &[
    "if_statement",
    "if_expression",
    "elif_clause",
    "else_if_clause",
    "while_statement",
    "while_expression",
    "for_statement",
    "for_expression",
    "for_in_statement",
    "loop_expression",
    "match_statement",
    "match_expression",
    "switch_statement",
    "catch_clause",
    "except_clause",
    "conditional_expression",
    "ternary_expression",
    "case_clause",
    "match_arm",
];

const COGNITIVE_STRUCT: &[&str] = &[
    "if_statement",
    "if_expression",
    "while_statement",
    "while_expression",
    "for_statement",
    "for_expression",
    "for_in_statement",
    "loop_expression",
    "match_statement",
    "match_expression",
    "switch_statement",
    "catch_clause",
    "except_clause",
    "conditional_expression",
    "ternary_expression",
];

const COGNITIVE_HYBRID: &[&str] = &["elif_clause", "else_clause", "else_if_clause"];

const NEST_RAISERS: &[&str] = &[
    "if_statement",
    "if_expression",
    "while_statement",
    "while_expression",
    "for_statement",
    "for_expression",
    "for_in_statement",
    "loop_expression",
    "match_statement",
    "match_expression",
    "switch_statement",
    "catch_clause",
    "except_clause",
    "conditional_expression",
    "ternary_expression",
    "function_definition",
    "function_item",
    "method_definition",
    "arrow_function",
    "lambda",
];

fn node_text<'a>(source: &'a [u8], node: Node) -> &'a str {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("")
}

fn bool_sequences(node: Node, source: &[u8]) -> i64 {
    if node.kind() != "boolean_operator" && node.kind() != "binary_expression" {
        return 0;
    }
    let text = node_text(source, node);
    if !text.contains("&&") && !text.contains("||") && !text.contains(" and ") && !text.contains(" or ") {
        return 0;
    }
    let mut ops = Vec::new();
    for token in ["&&", "||", " and ", " or "] {
        if text.contains(token) {
            ops.push(token.trim());
        }
    }
    if ops.is_empty() {
        return 0;
    }
    let mixed = (ops.contains(&"and") || ops.contains(&"&&")) && (ops.contains(&"or") || ops.contains(&"||"));
    if mixed {
        2
    } else {
        1
    }
}

fn call_name(node: Node, source: &[u8]) -> String {
    if let Some(fn_node) = node.child_by_field_name("function") {
        return node_text(source, fn_node)
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_string();
    }
    if node.child_count() > 0 {
        if let Some(first) = node.child(0) {
            return node_text(source, first)
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_string();
        }
    }
    String::new()
}

pub fn complexity(root: Node, source: &[u8], symbol_name: &str) -> (i64, i64, i64) {
    let mut cyclomatic = 1;
    let mut cognitive = 0;
    let mut max_nest = 0;
    fn walk(
        node: Node,
        nest: i64,
        root: Node,
        source: &[u8],
        symbol_name: &str,
        cyclomatic: &mut i64,
        cognitive: &mut i64,
        max_nest: &mut i64,
    ) {
        *max_nest = (*max_nest).max(nest);
        let ntype = node.kind();
        if CYCLO_TYPES.contains(&ntype) {
            *cyclomatic += 1;
        }
        if ntype == "binary_expression" || ntype == "boolean_operator" {
            let extra = bool_sequences(node, source);
            *cyclomatic += extra;
            *cognitive += extra;
        }
        let child_nest = if COGNITIVE_STRUCT.contains(&ntype) {
            *cognitive += 1 + nest;
            nest + 1
        } else if COGNITIVE_HYBRID.contains(&ntype) {
            *cognitive += 1;
            nest
        } else if NEST_RAISERS.contains(&ntype) && node.id() != root.id() {
            nest + 1
        } else {
            nest
        };
        if ntype == "call" || ntype == "call_expression" {
            let ident = call_name(node, source);
            if ident == symbol_name {
                *cognitive += 1;
                *cyclomatic += 1;
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(
                child,
                child_nest,
                root,
                source,
                symbol_name,
                cyclomatic,
                cognitive,
                max_nest,
            );
        }
    }
    walk(
        root,
        0,
        root,
        source,
        symbol_name,
        &mut cyclomatic,
        &mut cognitive,
        &mut max_nest,
    );
    (cyclomatic, cognitive, max_nest)
}
