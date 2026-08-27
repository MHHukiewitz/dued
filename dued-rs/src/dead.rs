use rusqlite::Connection;
use serde_json::{json, Value};

use crate::hollow::hollow_symbols;

pub fn dead_symbols(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.kind, s.signature, s.start_line, f.relpath, s.fan_in, s.is_public, s.is_entry, s.is_test
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.fan_in = 0 AND s.is_entry = 0 AND s.is_test = 0 AND f.is_test = 0
          AND s.name NOT IN ('main', 'app', 'cli', 'setup', 'run')
        ORDER BY s.is_public DESC, f.relpath, s.start_line
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "name": r.get::<_, String>(1)?,
            "kind": r.get::<_, String>(2)?,
            "signature": r.get::<_, String>(3)?,
            "start_line": r.get::<_, i64>(4)?,
            "relpath": r.get::<_, String>(5)?,
            "fan_in": r.get::<_, i64>(6)?,
            "is_public": r.get::<_, i64>(7)?,
            "is_entry": r.get::<_, i64>(8)?,
            "is_test": r.get::<_, i64>(9)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

pub fn dead_files(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT f.relpath, f.language, f.loc, f.pagerank
        FROM files f
        WHERE f.is_test = 0
          AND f.id NOT IN (SELECT DISTINCT dst_file_id FROM edges WHERE dst_file_id IS NOT NULL)
          AND f.id NOT IN (SELECT DISTINCT file_id FROM symbols WHERE is_entry = 1)
        ORDER BY f.relpath
        "#,
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok(json!({
            "relpath": r.get::<_, String>(0)?,
            "language": r.get::<_, String>(1)?,
            "loc": r.get::<_, i64>(2)?,
            "pagerank": r.get::<_, f64>(3)?,
        }))
    })
    .unwrap()
    .flatten()
    .collect()
}

pub fn dead_report(conn: &Connection) -> Value {
    json!({
        "symbols": dead_symbols(conn),
        "files": dead_files(conn),
        "hollow": hollow_symbols(conn),
    })
}
