use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::dead::dead_symbols;
use crate::issues::list_issues;
use crate::rank::reading_order;
use crate::slice::slice_symbol;

pub fn review_pack(conn: &Connection, dest: &Path, slice_query: Option<&str>) -> PathBuf {
    fs::create_dir_all(dest).ok();
    let order = reading_order(conn, 15);
    let mut dead = dead_symbols(conn);
    dead.truncate(15);
    let mut questions = Vec::new();
    for item in order.iter().take(8) {
        questions.push(format!(
            "What side effects does `{}::{}` have, and are they at a boundary?",
            item["relpath"].as_str().unwrap_or(""),
            item["name"].as_str().unwrap_or("")
        ));
        if item["cognitive"].as_i64().unwrap_or(0) >= 10 {
            questions.push(format!(
                "Can `{}` in `{}` be split? Cognitive complexity is {}.",
                item["name"].as_str().unwrap_or(""),
                item["relpath"].as_str().unwrap_or(""),
                item["cognitive"]
            ));
        }
    }
    if !dead.is_empty() {
        questions.push("Which listed dead symbols are public API, and which can be removed?".into());
    }
    let issues = list_issues(conn, 10);
    for item in issues.iter().take(5) {
        questions.push(format!(
            "Is `{}` in `{}` a real refactor target? {}",
            item["kind"].as_str().unwrap_or(""),
            item["relpath"].as_str().unwrap_or(""),
            item["detail"].as_str().unwrap_or("")
        ));
    }
    let mut sessions = Vec::new();
    let mut chunk = Vec::new();
    let mut load = 0;
    for item in &order {
        let cost = 10 + item["cognitive"].as_i64().unwrap_or(0);
        if load + cost > 45 && !chunk.is_empty() {
            sessions.push(Value::Array(chunk));
            chunk = Vec::new();
            load = 0;
        }
        chunk.push(item.clone());
        load += cost;
    }
    if !chunk.is_empty() {
        sessions.push(Value::Array(chunk));
    }
    let mut pack = json!({
        "reading_order": order,
        "questions": questions,
        "sessions": sessions,
        "dead_symbols": dead,
        "issues": issues,
    });
    if let Some(q) = slice_query {
        pack["behavior"] = slice_symbol(conn, q, 4);
    }
    let mut reading = String::from("# Guided reading order\n\n");
    for (i, item) in order.iter().enumerate() {
        reading.push_str(&format!(
            "{}. `{}::{}` — {}\n",
            i + 1,
            item["relpath"].as_str().unwrap_or(""),
            item["name"].as_str().unwrap_or(""),
            item["why"].as_str().unwrap_or("")
        ));
    }
    fs::write(dest.join("reading_order.md"), reading).ok();
    let qmd = format!(
        "# Review questions\n\n{}\n",
        questions.iter().map(|q| format!("- {q}")).collect::<Vec<_>>().join("\n")
    );
    fs::write(dest.join("questions.md"), qmd).ok();
    fs::write(dest.join("review.json"), serde_json::to_string_pretty(&pack).unwrap()).ok();
    dest.to_path_buf()
}
