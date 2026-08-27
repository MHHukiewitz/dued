use std::sync::OnceLock;

use regex::Regex;
use rusqlite::Connection;
use serde_json::{json, Value};

fn core_lines(body: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') || line.starts_with("//") || line.starts_with("/*") {
            continue;
        }
        if line.starts_with("\"\"\"") || line.starts_with("'''") {
            continue;
        }
        if line.starts_with("def ")
            || line.starts_with("fn ")
            || line.starts_with("pub fn ")
            || line.starts_with("export ")
            || line.starts_with("function ")
        {
            continue;
        }
        if matches!(line, "}" | "{" | "):" | ") {" | ") -> None:" | ") -> None {") {
            continue;
        }
        lines.push(line.to_string());
    }
    lines
}

pub fn is_hollow(body: &str, docstring: &str) -> String {
    let core = core_lines(body);
    if core.is_empty() {
        return "empty_body".into();
    }
    let joined = core.join(" ");
    static EMPTY: OnceLock<Regex> = OnceLock::new();
    let empty = EMPTY.get_or_init(|| {
        Regex::new(r"^(?i)(pass|\.\.\.|return|return None|return;|return \(\)| \{\s*\}|;\s*)$").unwrap()
    });
    if empty.is_match(joined.trim()) {
        return "empty_body".into();
    }
    if !docstring.is_empty() && docstring.len() > 80 && joined.len() < 40 {
        return "doc_oversells".into();
    }
    if core.len() <= 2
        && core.iter().all(|line| {
            matches!(
                line.as_str(),
                "pass" | "..." | "return" | "return None" | "return;" | "NotImplemented"
            ) || line.starts_with("raise NotImplemented")
        })
    {
        return "stub".into();
    }
    String::new()
}

pub fn hollow_symbols(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.body, s.docstring, s.signature, s.start_line, f.relpath
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.is_test = 0 AND f.is_test = 0
        "#,
        )
        .unwrap();
    let mut found = Vec::new();
    let rows: Vec<(i64, String, String, String, String, i64, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .unwrap()
        .flatten()
        .collect();
    drop(stmt);
    let mut bar = crate::progress::Bar::new("hollow", rows.len());
    for row in rows {
        bar.tick(&row.1);
        let reason = is_hollow(&row.2, &row.3);
        if reason.is_empty() {
            continue;
        }
        found.push(json!({
            "id": row.0,
            "name": row.1,
            "relpath": row.6,
            "start_line": row.5,
            "signature": row.4,
            "reason": reason,
        }));
    }
    bar.finish();
    found
}

pub fn apply_hollow(conn: &Connection) -> Vec<Value> {
    let found = hollow_symbols(conn);
    for item in &found {
        conn.execute(
            "INSERT INTO name_flags(symbol_id, kind, detail, score) VALUES (?,?,?,?)",
            rusqlite::params![
                item["id"].as_i64().unwrap(),
                "hollow",
                item["reason"].as_str().unwrap_or(""),
                1.0
            ],
        )
        .ok();
    }
    found
}
