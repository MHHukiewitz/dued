use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::fingerprints::fingerprint_overlap;

fn split_camel(part: &str) -> Vec<String> {
    let chars: Vec<char> = part.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        } else if chars[i].is_ascii_lowercase() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_lowercase() {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        } else if chars[i].is_ascii_uppercase() {
            if i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase() {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_lowercase() {
                    i += 1;
                }
                tokens.push(chars[start..i].iter().collect());
            } else {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_uppercase() {
                    if i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase() {
                        break;
                    }
                    i += 1;
                }
                tokens.push(chars[start..i].iter().collect());
            }
        } else {
            i += 1;
        }
    }
    tokens
}

pub fn tokenize_name(name: &str) -> Vec<String> {
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    let split = SPLIT.get_or_init(|| Regex::new(r"[_\-\s]+").unwrap());
    let mut tokens = Vec::new();
    for part in split.split(name) {
        for token in split_camel(part) {
            tokens.push(token.to_lowercase());
        }
    }
    tokens
        .into_iter()
        .filter(|t| !t.is_empty() && !matches!(t.as_str(), "get" | "set" | "the" | "a"))
        .collect()
}

fn stem_family(tokens: &[String]) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    if tokens.len() == 1 {
        return tokens[0].clone();
    }
    let last = tokens.last().unwrap();
    if matches!(last.as_str(), "model" | "dto" | "service" | "handler" | "repo" | "view") {
        last.clone()
    } else {
        tokens[0].clone()
    }
}

pub fn analyze_names(conn: &Connection) -> Vec<Value> {
    conn.execute("DELETE FROM name_flags", []).ok();
    let mut stmt = conn
        .prepare("SELECT id, name, signature, fingerprint, fan_in, cognitive FROM symbols")
        .unwrap();
    let symbols: Vec<(i64, String, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(3)?, r.get(5)?)))
        .unwrap()
        .flatten()
        .collect();
    drop(stmt);
    let mut insert_flag = conn
        .prepare("INSERT INTO name_flags(symbol_id, kind, detail, score) VALUES (?,?,?,?)")
        .unwrap();
    let mut by_stem: HashMap<String, Vec<(i64, String, String, i64)>> = HashMap::new();
    let mut by_name: HashMap<String, Vec<(i64, String, String, i64)>> = HashMap::new();
    let mut bar = crate::progress::Bar::new("names", symbols.len());
    for row in &symbols {
        let tokens = tokenize_name(&row.1);
        by_stem.entry(stem_family(&tokens)).or_default().push(row.clone());
        by_name.entry(row.1.to_lowercase()).or_default().push(row.clone());
        if tokens.len() >= 5 && row.3 >= 8 {
            insert_flag
                .execute(params![
                    row.0,
                    "long_name",
                    "long name plus high cognitive complexity",
                    tokens.len() as f64
                ])
                .ok();
        }
        bar.tick(&row.1);
    }
    bar.finish();
    let mut flags = Vec::new();
    for (name, group) in &by_name {
        if group.len() < 2 {
            continue;
        }
        for i in 0..group.len() {
            for b in group.iter().skip(i + 1) {
                let overlap = fingerprint_overlap(&group[i].2, &b.2);
                if overlap < 0.35 {
                    let detail = format!("same name {name} with distant behavior ({overlap:.2})");
                    insert_flag
                        .execute(params![group[i].0, "homonym", detail, 1.0 - overlap])
                        .ok();
                    flags.push(json!({"kind": "homonym", "detail": detail, "score": 1.0 - overlap}));
                }
            }
        }
    }
    let suffixes = ["model", "dto", "service", "handler", "repo"];
    for (stem, group) in &by_stem {
        if stem.is_empty() || group.len() < 2 {
            continue;
        }
        let kinds: std::collections::HashSet<String> = group
            .iter()
            .map(|r| tokenize_name(&r.1).last().cloned().unwrap_or_default())
            .collect();
        if suffixes.iter().any(|s| kinds.contains(*s)) && group.len() >= 2 {
            continue;
        }
        let mut overlaps = Vec::new();
        for i in 0..group.len() {
            for b in group.iter().skip(i + 1) {
                overlaps.push(fingerprint_overlap(&group[i].2, &b.2));
            }
        }
        if !overlaps.is_empty() && overlaps.iter().sum::<f64>() / (overlaps.len() as f64) < 0.3 {
            for row in group {
                insert_flag
                    .execute(params![
                        row.0,
                        "same_stem_diff_behavior",
                        format!("stem '{stem}' used for unlike functions"),
                        0.7
                    ])
                    .ok();
            }
        }
    }
    drop(insert_flag);
    let mut stmt = conn
        .prepare(
            r#"
        SELECT n.kind, n.detail, n.score, s.name, f.relpath, s.start_line
        FROM name_flags n JOIN symbols s ON s.id = n.symbol_id JOIN files f ON f.id = s.file_id
        ORDER BY n.score DESC
        "#,
        )
        .unwrap();
    for row in stmt
        .query_map([], |r| {
            Ok(json!({
                "kind": r.get::<_, String>(0)?,
                "detail": r.get::<_, String>(1)?,
                "score": r.get::<_, f64>(2)?,
                "name": r.get::<_, String>(3)?,
                "relpath": r.get::<_, String>(4)?,
                "start_line": r.get::<_, i64>(5)?,
            }))
        })
        .unwrap()
        .flatten()
    {
        flags.push(row);
    }
    flags
}
