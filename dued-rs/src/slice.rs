use std::collections::{HashSet, VecDeque};
use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::effects::tag_effects;

pub fn slice_symbol(conn: &Connection, query: &str, depth: i64) -> Value {
    let matches = find_symbols(conn, query);
    if matches.is_empty() {
        return json!({"query": query, "error": "symbol not found", "symbols": []});
    }
    let root = matches[0].clone();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((root["id"].as_i64().unwrap(), 0));
    let mut nodes = Vec::new();
    while let Some((sid, d)) = queue.pop_front() {
        if seen.contains(&sid) || d > depth {
            continue;
        }
        seen.insert(sid);
        let mut stmt = conn
            .prepare(
                r#"
            SELECT s.id, s.name, s.start_line, s.signature, s.effects, s.cognitive, s.body, s.file_id, f.relpath
            FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.id = ?
            "#,
            )
            .unwrap();
        let row = stmt
            .query_row([sid], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, String>(8)?,
                ))
            })
            .ok();
        let Some(row) = row else {
            continue;
        };
        let effects: Value = serde_json::from_str(&row.4).unwrap_or(json!([]));
        nodes.push(json!({
            "id": row.0,
            "name": row.1,
            "relpath": row.8,
            "start_line": row.2,
            "signature": row.3,
            "effects": effects,
            "cognitive": row.5,
            "depth": d,
        }));
        let mut estmt = conn
            .prepare("SELECT dst_name, dst_file_id FROM edges WHERE src_symbol_id = ? AND kind = 'call'")
            .unwrap();
        for edge in estmt
            .query_map([sid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))
            .unwrap()
            .flatten()
        {
            if let Some(dst_file) = edge.1 {
                let mut dstmt = conn.prepare("SELECT id FROM symbols WHERE name = ? AND file_id = ?").unwrap();
                for dest in dstmt.query_map(params![edge.0, dst_file], |r| r.get::<_, i64>(0)).unwrap().flatten() {
                    queue.push_back((dest, d + 1));
                }
            } else {
                let mut dstmt = conn.prepare("SELECT id FROM symbols WHERE name = ?").unwrap();
                for dest in dstmt.query_map([&edge.0], |r| r.get::<_, i64>(0)).unwrap().flatten() {
                    queue.push_back((dest, d + 1));
                }
            }
        }
    }
    let root_name = root["name"].as_str().unwrap_or("");
    let root_file = root["file_id"].as_i64().unwrap_or(0);
    let mut cstmt = conn
        .prepare(
            r#"
        SELECT s.name, f.relpath, s.start_line
        FROM edges e JOIN symbols s ON s.id = e.src_symbol_id JOIN files f ON f.id = s.file_id
        WHERE e.dst_name = ? AND e.kind = 'call' AND (e.dst_file_id = ? OR e.dst_file_id IS NULL)
        "#,
        )
        .unwrap();
    let callers: Vec<Value> = cstmt
        .query_map(params![root_name, root_file], |r| {
            Ok(json!({"name": r.get::<_, String>(0)?, "relpath": r.get::<_, String>(1)?, "start_line": r.get::<_, i64>(2)?}))
        })
        .unwrap()
        .flatten()
        .collect();
    let files: Vec<String> = {
        let mut v: Vec<String> = nodes.iter().filter_map(|n| n["relpath"].as_str().map(|s| s.to_string())).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut all_effects: Vec<String> = nodes
        .iter()
        .filter_map(|n| n["effects"].as_array().cloned())
        .flatten()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    all_effects.sort();
    all_effects.dedup();
    let tests = test_map(conn, root_name, root["relpath"].as_str().unwrap_or(""));
    let body = root["body"].as_str().unwrap_or("");
    let taint = taint_lite(body, root["signature"].as_str().unwrap_or(""), &all_effects);
    let cost: i64 = nodes
        .iter()
        .filter_map(|n| n["id"].as_i64())
        .map(|id| {
            conn.query_row("SELECT cost_hint FROM symbols WHERE id = ?", [id], |r| r.get::<_, i64>(0))
                .unwrap_or(0)
        })
        .sum();
    json!({
        "query": query,
        "root": {"name": root_name, "relpath": root["relpath"], "signature": root["signature"]},
        "symbols": nodes,
        "callers": callers,
        "files": files,
        "effects": all_effects,
        "blast_radius": files.len(),
        "tests": tests,
        "taint": taint,
        "cost_hint": cost,
    })
}

fn find_symbols(conn: &Connection, query: &str) -> Vec<Value> {
    if let Some((path, name)) = query.rsplit_once("::") {
        let mut stmt = conn
            .prepare(
                r#"
            SELECT s.id, s.name, s.signature, s.body, s.file_id, f.relpath
            FROM symbols s JOIN files f ON f.id = s.file_id
            WHERE f.relpath = ? AND s.name = ?
            "#,
            )
            .unwrap();
        return stmt
            .query_map(params![path, name], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "signature": r.get::<_, String>(2)?,
                    "body": r.get::<_, String>(3)?,
                    "file_id": r.get::<_, i64>(4)?,
                    "relpath": r.get::<_, String>(5)?,
                }))
            })
            .unwrap()
            .flatten()
            .collect();
    }
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.signature, s.body, s.file_id, f.relpath
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.name = ? ORDER BY f.relpath
        "#,
        )
        .unwrap();
    stmt.query_map([query], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "name": r.get::<_, String>(1)?,
            "signature": r.get::<_, String>(2)?,
            "body": r.get::<_, String>(3)?,
            "file_id": r.get::<_, i64>(4)?,
            "relpath": r.get::<_, String>(5)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

fn test_map(conn: &Connection, name: &str, relpath: &str) -> Vec<Value> {
    let stem = Path::new(relpath)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let needle = name.to_lowercase();
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.name, f.relpath, s.start_line
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.is_test = 1 OR f.is_test = 1
        "#,
        )
        .unwrap();
    let mut found = Vec::new();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
        .unwrap()
        .flatten()
    {
        let blob = format!("{} {}", row.0, row.1).to_lowercase();
        if blob.contains(&needle) || blob.contains(&stem) {
            found.push(json!({"name": row.0, "relpath": row.1, "start_line": row.2}));
        }
    }
    found.truncate(20);
    found
}

fn taint_lite(body: &str, signature: &str, effects: &[String]) -> Value {
    let sinks: Vec<String> = tag_effects(body)
        .into_iter()
        .filter(|s| effects.contains(s))
        .collect();
    let mut params_list = Vec::new();
    if let (Some(a), Some(b)) = (signature.find('('), signature.rfind(')')) {
        if b > a {
            for part in signature[a + 1..b].split(',') {
                let token = part.trim().split(':').next().unwrap_or("").split('=').next().unwrap_or("").trim();
                if !token.is_empty() && token != "self" && token != "cls" {
                    params_list.push(token.to_string());
                }
            }
        }
    }
    let reaching: Vec<String> = params_list
        .iter()
        .filter(|p| body.contains(p.as_str()) && !sinks.is_empty())
        .cloned()
        .collect();
    json!({"sinks": sinks, "params": params_list, "params_may_reach_sinks": reaching})
}
