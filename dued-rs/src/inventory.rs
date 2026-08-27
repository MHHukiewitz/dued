use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Value};

pub fn package_map(repo: &Path) -> Vec<Value> {
    let mut packs = Vec::new();
    if repo.join("pyproject.toml").is_file() {
        packs.push(json!({"kind": "python", "path": "pyproject.toml", "extras": []}));
    }
    if repo.join("Cargo.toml").is_file() {
        packs.push(json!({"kind": "rust", "path": "Cargo.toml", "members": []}));
    }
    if repo.join("package.json").is_file() {
        packs.push(json!({"kind": "node", "path": "package.json"}));
    }
    packs
}

pub fn inventory(conn: &Connection, repo: &Path) -> Value {
    let prod: (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(loc),0), COALESCE(SUM(tokens),0) FROM files WHERE is_test = 0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or((0, 0, 0));
    let tests: (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(loc),0) FROM files WHERE is_test = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 0));
    let public: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols WHERE is_public = 1 AND is_test = 0", [], |r| r.get(0))
        .unwrap_or(0);
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.name, s.signature, f.relpath, s.start_line
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.is_entry = 1 AND f.is_test = 0
        ORDER BY f.relpath, s.start_line
        "#,
        )
        .unwrap();
    let entries: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "name": r.get::<_, String>(0)?,
                "signature": r.get::<_, String>(1)?,
                "relpath": r.get::<_, String>(2)?,
                "start_line": r.get::<_, i64>(3)?,
            }))
        })
        .unwrap()
        .flatten()
        .collect();
    let mut stmt = conn
        .prepare("SELECT language, COUNT(*), SUM(loc) FROM files GROUP BY language")
        .unwrap();
    let languages: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({"language": r.get::<_, String>(0)?, "n": r.get::<_, i64>(1)?, "loc": r.get::<_, i64>(2)?}))
        })
        .unwrap()
        .flatten()
        .collect();
    let prod_loc = if prod.1 == 0 { 1.0 } else { prod.1 as f64 };
    json!({
        "languages": languages,
        "prod_files": prod.0,
        "test_files": tests.0,
        "prod_loc": prod.1,
        "test_loc": tests.1,
        "test_to_code": ((tests.1 as f64) / prod_loc * 1000.0).round() / 1000.0,
        "tokens": prod.2,
        "public_symbols": public,
        "entry_points": entries,
        "packages": package_map(repo),
    })
}
