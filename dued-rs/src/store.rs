use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::paths::{db_path, index_dir};

pub const SCHEMA_VERSION: i64 = 2;

/// Bump when parse rules change what symbols a file yields for the same bytes.
/// Scan treats a mismatch as all walked files dirty (digest match alone is not enough).
pub const PARSER_VERSION: i64 = 3;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    relpath TEXT UNIQUE NOT NULL,
    language TEXT NOT NULL,
    digest TEXT NOT NULL,
    loc INTEGER NOT NULL,
    size INTEGER NOT NULL,
    is_test INTEGER NOT NULL,
    tokens INTEGER NOT NULL DEFAULT 0,
    ast_nodes INTEGER NOT NULL DEFAULT 0,
    pagerank REAL NOT NULL DEFAULT 0,
    hotspot REAL NOT NULL DEFAULT 0,
    churn INTEGER NOT NULL DEFAULT 0,
    authors INTEGER NOT NULL DEFAULT 0,
    bus_factor INTEGER NOT NULL DEFAULT 0,
    bursts INTEGER NOT NULL DEFAULT 0,
    age_days INTEGER NOT NULL DEFAULT 0,
    first_seen TEXT NOT NULL DEFAULT '',
    last_seen TEXT NOT NULL DEFAULT '',
    profile_self REAL NOT NULL DEFAULT 0,
    profile_total REAL NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    signature TEXT NOT NULL,
    docstring TEXT NOT NULL,
    body TEXT NOT NULL,
    cyclomatic INTEGER NOT NULL,
    cognitive INTEGER NOT NULL,
    nesting INTEGER NOT NULL,
    nargs INTEGER NOT NULL,
    is_public INTEGER NOT NULL,
    is_entry INTEGER NOT NULL,
    is_test INTEGER NOT NULL,
    fan_in INTEGER NOT NULL DEFAULT 0,
    fan_out INTEGER NOT NULL DEFAULT 0,
    effects TEXT NOT NULL DEFAULT '[]',
    risks TEXT NOT NULL DEFAULT '[]',
    cost_hint INTEGER NOT NULL DEFAULT 0,
    fingerprint TEXT NOT NULL DEFAULT '',
    embed_sig BLOB,
    embed_doc BLOB,
    embed_body BLOB,
    FOREIGN KEY(file_id) REFERENCES files(id)
);
CREATE TABLE IF NOT EXISTS edges (
    id INTEGER PRIMARY KEY,
    src_file_id INTEGER NOT NULL,
    src_symbol_id INTEGER,
    dst_file_id INTEGER,
    dst_name TEXT NOT NULL,
    kind TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS call_facts (
    src_file_id INTEGER NOT NULL,
    src_symbol_id INTEGER NOT NULL,
    callee TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS import_facts (
    src_file_id INTEGER NOT NULL,
    module_hint TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS clones (
    id INTEGER PRIMARY KEY,
    symbol_a INTEGER NOT NULL,
    symbol_b INTEGER NOT NULL,
    score REAL NOT NULL,
    method TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS name_flags (
    id INTEGER PRIMARY KEY,
    symbol_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    detail TEXT NOT NULL,
    score REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS git_coupling (
    id INTEGER PRIMARY KEY,
    file_a TEXT NOT NULL,
    file_b TEXT NOT NULL,
    shared INTEGER NOT NULL,
    strength REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS issues (
    id INTEGER PRIMARY KEY,
    symbol_id INTEGER,
    file_id INTEGER,
    kind TEXT NOT NULL,
    detail TEXT NOT NULL,
    score REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst_name);
"#;

pub fn connect(repo: &Path) -> Connection {
    fs::create_dir_all(index_dir(repo)).ok();
    let conn = Connection::open(db_path(repo)).expect("open sqlite");
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;
         PRAGMA foreign_keys = OFF;",
    )
    .expect("pragma");
    conn.execute_batch(SCHEMA).expect("schema");
    migrate(&conn);
    conn
}

fn migrate(conn: &Connection) {
    let current: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = ?", ["schema_version"], |r| r.get(0))
        .ok();
    let version = current
        .as_deref()
        .and_then(|v| serde_json::from_str::<i64>(v).ok())
        .unwrap_or(0);
    if version == SCHEMA_VERSION {
        return;
    }
    for table in [
        "issues",
        "name_flags",
        "clones",
        "edges",
        "call_facts",
        "import_facts",
        "git_coupling",
        "symbols",
        "files",
    ] {
        let _ = conn.execute(&format!("DROP TABLE IF EXISTS {table}"), []);
    }
    conn.execute_batch(SCHEMA).expect("recreate schema");
    set_meta(conn, "schema_version", &Value::from(SCHEMA_VERSION));
}

pub fn set_meta(conn: &Connection, key: &str, value: &Value) {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value.to_string()],
    )
    .ok();
}

pub fn meta_i64(conn: &Connection, key: &str) -> Option<i64> {
    let raw: String = conn
        .query_row("SELECT value FROM meta WHERE key = ?", [key], |r| r.get(0))
        .ok()?;
    serde_json::from_str::<i64>(&raw).ok()
}

pub fn delete_file_row(conn: &Connection, relpath: &str) {
    let id: Option<i64> = conn
        .query_row("SELECT id FROM files WHERE relpath = ?", [relpath], |r| r.get(0))
        .ok();
    let Some(fid) = id else {
        return;
    };
    // Child rows first. issues/name_flags/clones must go too, or a failed scan
    // leaves JOIN nulls and a leftover files row can trip UNIQUE on rescan.
    conn.execute(
        "DELETE FROM name_flags WHERE symbol_id IN (SELECT id FROM symbols WHERE file_id = ?)",
        [fid],
    )
    .expect("delete name_flags for file");
    conn.execute(
        "DELETE FROM clones WHERE symbol_a IN (SELECT id FROM symbols WHERE file_id = ?) \
         OR symbol_b IN (SELECT id FROM symbols WHERE file_id = ?)",
        params![fid, fid],
    )
    .expect("delete clones for file");
    conn.execute(
        "DELETE FROM issues WHERE file_id = ? OR symbol_id IN (SELECT id FROM symbols WHERE file_id = ?)",
        params![fid, fid],
    )
    .expect("delete issues for file");
    conn.execute("DELETE FROM call_facts WHERE src_file_id = ?", [fid])
        .expect("delete call_facts for file");
    conn.execute("DELETE FROM import_facts WHERE src_file_id = ?", [fid])
        .expect("delete import_facts for file");
    conn.execute(
        "DELETE FROM edges WHERE src_file_id = ? OR dst_file_id = ?",
        params![fid, fid],
    )
    .expect("delete edges for file");
    conn.execute("DELETE FROM symbols WHERE file_id = ?", [fid])
        .expect("delete symbols for file");
    conn.execute("DELETE FROM files WHERE id = ?", [fid])
        .expect("delete files row");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo() -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let repo = std::env::temp_dir().join(format!("dued-store-test-{nanos}"));
        fs::create_dir_all(repo.join("dued")).unwrap();
        repo
    }

    #[test]
    fn delete_file_row_removes_files_and_issue_children() {
        let repo = temp_repo();
        let conn = connect(&repo);
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) \
             VALUES (1, 'a.py', 'python', 'd1', 10, 20, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, \
             cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test) \
             VALUES (10, 1, 'big', 'function', 1, 40, 'def big()', '', 'pass', 1, 20, 1, 0, 1, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (10, 1, 'god_function', 'x', 50.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO name_flags(symbol_id, kind, detail, score) VALUES (10, 'hollow', 'x', 1.0)",
            [],
        )
        .unwrap();
        delete_file_row(&conn, "a.py");
        let files: i64 = conn
            .query_row("SELECT COUNT(*) FROM files WHERE relpath = 'a.py'", [], |r| r.get(0))
            .unwrap();
        let issues: i64 = conn
            .query_row("SELECT COUNT(*) FROM issues", [], |r| r.get(0))
            .unwrap();
        let symbols: i64 = conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))
            .unwrap();
        assert_eq!(files, 0);
        assert_eq!(issues, 0);
        assert_eq!(symbols, 0);
        // Same relpath can be inserted again after delete (UNIQUE must be free).
        conn.execute(
            "INSERT INTO files(relpath, language, digest, loc, size, is_test) \
             VALUES ('a.py', 'python', 'd2', 11, 22, 0)",
            [],
        )
        .unwrap();
        let _ = fs::remove_dir_all(repo);
    }
}
