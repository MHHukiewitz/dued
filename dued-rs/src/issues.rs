use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

const BOUNDARY: &[&str] = &["cli.py", "store.py", "profile.py", "git_hist.py", "/io/", "/db/", "http"];

pub fn apply_issues(conn: &Connection) -> Vec<Value> {
    conn.execute("DELETE FROM issues", []).ok();
    let mut found = Vec::new();
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.cognitive, s.fan_out, s.fan_in, s.effects, s.start_line, s.end_line,
               f.id, f.relpath
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE f.is_test = 0 AND s.is_test = 0
        "#,
        )
        .unwrap();
    let rows: Vec<(i64, String, i64, i64, i64, String, i64, i64, i64, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        })
        .unwrap()
        .flatten()
        .collect();
    let mut bar = crate::progress::Bar::new("issues", rows.len());
    for row in &rows {
        bar.tick(&row.1);
        let loc = (row.7 - row.6 + 1).max(1);
        let score = row.2 as f64 * (1.0 + row.3 as f64 / 5.0) * (1.0 + loc as f64 / 80.0);
        if row.2 >= 15 || score >= 40.0 {
            let detail = format!("god function cognitive={} fan_out={} loc={}", row.2, row.3, loc);
            add(conn, &mut found, Some(row.0), Some(row.8), "god_function", &detail, score, &row.9, &row.1);
        }
        let effects: Vec<String> = serde_json::from_str(&row.5).unwrap_or_default();
        let io: Vec<&str> = effects
            .iter()
            .map(|s| s.as_str())
            .filter(|t| matches!(*t, "filesystem" | "network" | "db" | "process"))
            .collect();
        if !io.is_empty() && row.4 >= 2 && !BOUNDARY.iter().any(|m| row.9.contains(m)) {
            let detail = format!("I/O {io:?} mixed into core (fan_in={})", row.4);
            add(
                conn,
                &mut found,
                Some(row.0),
                Some(row.8),
                "effect_in_core",
                &detail,
                row.4 as f64,
                &row.9,
                &row.1,
            );
        }
    }
    bar.finish();
    let mut stmt = conn
        .prepare(
            r#"
        SELECT f.id, f.relpath, COUNT(s.id), COALESCE(SUM(s.cognitive), 0)
        FROM files f LEFT JOIN symbols s ON s.file_id = f.id
        WHERE f.is_test = 0 GROUP BY f.id
        "#,
        )
        .unwrap();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, f64>(3)?)))
        .unwrap()
        .flatten()
    {
        if row.3 >= 40.0 || row.2 >= 20 {
            let detail = format!("god module symbols={} cognitive={}", row.2, row.3);
            add(conn, &mut found, None, Some(row.0), "god_module", &detail, row.3, &row.1, "");
        }
    }
    let mut stmt = conn.prepare("SELECT file_a, file_b, shared, strength FROM git_coupling").unwrap();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, f64>(3)?)))
        .unwrap()
        .flatten()
    {
        if far_apart(&row.0, &row.1) && row.3 >= 0.4 {
            let detail = format!("{} <-> {} shared={}", row.0, row.1, row.2);
            let fid: Option<i64> = conn
                .query_row("SELECT id FROM files WHERE relpath = ?", [&row.0], |r| r.get(0))
                .ok();
            add(conn, &mut found, None, fid, "shotgun_surgery", &detail, row.3, &row.0, "");
        }
    }
    found
}

fn far_apart(a: &str, b: &str) -> bool {
    let pa = Path::new(a).components().next().map(|c| c.as_os_str().to_string_lossy().into_owned());
    let pb = Path::new(b).components().next().map(|c| c.as_os_str().to_string_lossy().into_owned());
    pa.is_some() && pb.is_some() && pa != pb
}

fn add(
    conn: &Connection,
    found: &mut Vec<Value>,
    symbol_id: Option<i64>,
    file_id: Option<i64>,
    kind: &str,
    detail: &str,
    score: f64,
    relpath: &str,
    name: &str,
) {
    conn.execute(
        "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (?,?,?,?,?)",
        params![symbol_id, file_id, kind, detail, score],
    )
    .ok();
    found.push(json!({"kind": kind, "detail": detail, "score": score, "relpath": relpath, "name": name}));
}

pub fn list_issues(conn: &Connection, limit: i64) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT i.kind, i.detail, i.score, f.relpath, s.name, s.start_line
        FROM issues i
        LEFT JOIN files f ON f.id = i.file_id
        LEFT JOIN symbols s ON s.id = i.symbol_id
        ORDER BY i.score DESC
        LIMIT ?
        "#,
        )
        .unwrap();
    stmt.query_map([limit], |r| {
        Ok(json!({
            "kind": r.get::<_, String>(0)?,
            "detail": r.get::<_, String>(1)?,
            "score": r.get::<_, f64>(2)?,
            "relpath": r.get::<_, Option<String>>(3)?,
            "name": r.get::<_, Option<String>>(4)?,
            "start_line": r.get::<_, Option<i64>>(5)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}
