use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::dead::{dead_files, dead_symbols};
use crate::explorer;
use crate::heatmap::write_heatmap;
use crate::hollow::hollow_symbols;
use crate::inventory::inventory;
use crate::issues::list_issues;
use crate::names::analyze_names;
use crate::paths::{new_report_dir, newest_report_dir};
use crate::progress::note;
use crate::rank::reading_order;

pub fn write_report_dir(repo: &Path, conn: &Connection, extra: Value) -> PathBuf {
    let dest = new_report_dir(repo);
    fill_report(repo, conn, &dest, extra);
    dest
}

pub fn refresh_report(repo: &Path, conn: &Connection) -> PathBuf {
    if let Some(dest) = newest_report_dir(repo) {
        note("rebuild explorer from the existing index");
        fill_report(repo, conn, &dest, json!({}));
        dest
    } else {
        write_report_dir(repo, conn, json!({}))
    }
}

fn fill_report(repo: &Path, conn: &Connection, dest: &Path, extra: Value) {
    fs::create_dir_all(dest).ok();
    let heatmap = write_heatmap(conn, &dest.join("heatmap.svg"), None);
    explorer::write_explorer(repo, conn, dest, extra.clone());
    write_brief_files(repo, conn, dest, extra, heatmap);
}

fn write_brief_files(repo: &Path, conn: &Connection, dest: &Path, extra: Value, heatmap: Value) {
    let files: Vec<Value> = {
        let mut stmt = conn
            .prepare("SELECT language, COUNT(*) AS n, SUM(loc) AS loc FROM files GROUP BY language")
            .unwrap();
        stmt.query_map([], |r| {
            Ok(json!({"language": r.get::<_, String>(0)?, "n": r.get::<_, i64>(1)?, "loc": r.get::<_, i64>(2)?}))
        })
        .unwrap()
        .flatten()
        .collect()
    };
    let symbols_n: i64 = conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0)).unwrap();
    let order = reading_order(conn, 15);
    let dead = {
        let mut d = dead_symbols(conn);
        d.truncate(30);
        d
    };
    let issues = list_issues(conn, 40);
    let inv = inventory(conn, repo);
    let mut dead_f = dead_files(conn);
    dead_f.truncate(20);
    let mut hollow = hollow_symbols(conn);
    hollow.truncate(20);
    let mut names = analyze_names(conn);
    names.truncate(40);
    let questions = review_questions(&order, &dead, &issues);
    let file_count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
    let stamp = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let mut brief = json!({
        "repo": repo.display().to_string(),
        "generated_at": stamp,
        "languages": files,
        "files": file_count,
        "symbols": symbols_n,
        "inventory": inv,
        "reading_order": order,
        "dead_symbols": dead,
        "dead_files": dead_f,
        "hollow": hollow,
        "issues": issues,
        "names": names,
        "questions": questions,
        "heatmap": heatmap,
        "engine": "dued",
        "explorer": "report.html",
        "data_dir": "data",
    });
    if let Value::Object(extra_map) = extra {
        if let Value::Object(obj) = &mut brief {
            obj.extend(extra_map);
        }
    }
    fs::write(dest.join("index.json"), serde_json::to_string_pretty(&brief).unwrap()).ok();
    fs::write(dest.join("rank.json"), serde_json::to_string_pretty(&order).unwrap()).ok();
    note(&format!("HTML explorer {}", dest.join("report.html").display()));
    fs::write(
        dest.join("agent.json"),
        serde_json::to_string_pretty(&json!({
            "must_read": order.iter().take(8).cloned().collect::<Vec<_>>(),
            "languages": brief["languages"],
            "inventory": inv,
            "top_issues": issues.iter().take(8).cloned().collect::<Vec<_>>(),
            "dead_code_count": dead.len(),
            "effects_hint": "use dued slice <symbol> before changing behavior",
            "engine": "dued",
            "explorer": dest.join("report.html").display().to_string(),
        }))
        .unwrap(),
    )
    .ok();
}

fn review_questions(order: &[Value], dead: &[Value], issues: &[Value]) -> Vec<String> {
    let mut questions = Vec::new();
    for item in order.iter().take(8) {
        questions.push(format!(
            "What side effects does `{}::{}` have, and are they at a boundary?",
            item["relpath"].as_str().unwrap_or(""),
            item["name"].as_str().unwrap_or("")
        ));
    }
    if !dead.is_empty() {
        questions.push("Which listed dead symbols are public API, and which can be removed?".into());
    }
    for item in issues.iter().take(5) {
        questions.push(format!(
            "Is `{}` in `{}` a real refactor target? {}",
            item["kind"].as_str().unwrap_or(""),
            item["relpath"].as_str().unwrap_or(""),
            item["detail"].as_str().unwrap_or("")
        ));
    }
    questions
}
