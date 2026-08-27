use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::graph::{file_graph, is_generic_callee, pagerank};

pub fn compute_rank(conn: &Connection) -> Vec<Value> {
    let (nodes, edges) = file_graph(conn);
    let mut personalize: HashMap<i64, f64> = HashMap::new();
    let mut stmt = conn.prepare("SELECT file_id FROM symbols WHERE is_entry = 1").unwrap();
    for file_id in stmt.query_map([], |r| r.get::<_, i64>(0)).unwrap().flatten() {
        *personalize.entry(file_id).or_insert(0.0) += 5.0;
    }
    let pers = if personalize.is_empty() {
        None
    } else {
        Some(&personalize)
    };
    let scores = pagerank(&nodes, &edges, pers, 0.85, 40);
    let mut bar = crate::progress::Bar::new("rank", scores.len());
    for (file_id, score) in scores {
        let cog: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cognitive), 0) FROM symbols WHERE file_id = ?",
                [file_id],
                |r| r.get(0),
            )
            .unwrap_or(0.0);
        let churn: f64 = conn
            .query_row("SELECT churn FROM files WHERE id = ?", [file_id], |r| r.get(0))
            .unwrap_or(0.0);
        let profile: f64 = conn
            .query_row("SELECT profile_total FROM files WHERE id = ?", [file_id], |r| r.get(0))
            .unwrap_or(0.0);
        let mut hotspot = (1.0 + churn) * (1.0 + cog / 10.0);
        if profile != 0.0 {
            hotspot *= 1.0 + profile;
        }
        conn.execute(
            "UPDATE files SET pagerank = ?, hotspot = ? WHERE id = ?",
            params![score, hotspot, file_id],
        )
        .ok();
        bar.tick("");
    }
    bar.finish();
    let mut stmt = conn
        .prepare(
            r#"
        SELECT f.relpath, f.language, f.loc, f.pagerank, f.hotspot, f.churn, f.is_test,
               COALESCE(SUM(s.cognitive), 0) AS cognitive,
               COALESCE(SUM(s.cyclomatic), 0) AS cyclomatic,
               COUNT(s.id) AS symbols
        FROM files f
        LEFT JOIN symbols s ON s.file_id = f.id
        GROUP BY f.id
        ORDER BY (f.pagerank * (1.0 + COALESCE(SUM(s.cognitive), 0) / 20.0) + f.hotspot * 0.01) DESC
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "relpath": r.get::<_, String>(0)?,
            "language": r.get::<_, String>(1)?,
            "loc": r.get::<_, i64>(2)?,
            "pagerank": r.get::<_, f64>(3)?,
            "hotspot": r.get::<_, f64>(4)?,
            "churn": r.get::<_, i64>(5)?,
            "is_test": r.get::<_, i64>(6)?,
            "cognitive": r.get::<_, f64>(7)?,
            "cyclomatic": r.get::<_, f64>(8)?,
            "symbols": r.get::<_, i64>(9)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

pub fn reading_order(conn: &Connection, limit: i64) -> Vec<Value> {
    let fetch = (limit * 20).max(80);
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.name, s.signature, s.kind, s.cognitive, s.cyclomatic, s.fan_in, s.fan_out,
               s.start_line, f.relpath, f.pagerank, s.is_entry, s.effects
        FROM symbols s
        JOIN files f ON f.id = s.file_id
        WHERE f.is_test = 0 AND s.is_test = 0
        ORDER BY (s.is_entry * 3.0 + f.pagerank * 10.0 + s.cognitive * 0.2 + s.fan_in * 0.3) DESC
        LIMIT ?
        "#,
        )
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([fetch], |r| {
            let is_entry: i64 = r.get(10)?;
            let fan_in: i64 = r.get(5)?;
            let cognitive: i64 = r.get(3)?;
            let pagerank: f64 = r.get(9)?;
            let name: String = r.get(0)?;
            let relpath: String = r.get(8)?;
            let mut why = Vec::new();
            if is_entry != 0 {
                why.push("entry point".to_string());
            }
            if fan_in > 2 {
                why.push(format!("used by {fan_in} callers"));
            }
            if cognitive >= 8 {
                why.push(format!("cognitive complexity {cognitive}"));
            }
            if pagerank > 0.05 {
                why.push("high graph centrality".to_string());
            }
            let why = if why.is_empty() {
                "ranked by PageRank and complexity".to_string()
            } else {
                why.join(", ")
            };
            Ok(json!({
                "name": name,
                "signature": r.get::<_, String>(1)?,
                "kind": r.get::<_, String>(2)?,
                "cognitive": cognitive,
                "cyclomatic": r.get::<_, i64>(4)?,
                "fan_in": fan_in,
                "fan_out": r.get::<_, i64>(6)?,
                "start_line": r.get::<_, i64>(7)?,
                "relpath": relpath,
                "pagerank": pagerank,
                "is_entry": is_entry,
                "effects": r.get::<_, String>(11)?,
                "why": why,
            }))
        })
        .unwrap()
        .flatten()
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in rows {
        let name = item["name"].as_str().unwrap_or("");
        let is_entry = item["is_entry"].as_i64().unwrap_or(0) != 0;
        if is_generic_callee(name) && !is_entry {
            continue;
        }
        let key = format!("{}::{name}", item["relpath"].as_str().unwrap_or(""));
        if !seen.insert(key) {
            continue;
        }
        out.push(item);
        if out.len() as i64 >= limit {
            break;
        }
    }
    out
}
