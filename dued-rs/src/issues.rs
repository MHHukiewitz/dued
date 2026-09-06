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
        // Core = game/sim/domain (and other non-boundary product code).
        // UI, scripts, and tools do I/O by design; they are not core.
        if !io.is_empty()
            && row.4 >= 2
            && !is_effect_boundary_path(&row.9)
            && !is_effect_non_core_path(&row.9)
        {
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
            let name = file_basename(&row.1);
            add(conn, &mut found, None, Some(row.0), "god_module", &detail, row.3, &row.1, &name);
        }
    }
    let mut stmt = conn.prepare("SELECT file_a, file_b, shared, strength FROM git_coupling").unwrap();
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, f64>(3)?)))
        .unwrap()
        .flatten()
    {
        // Docs, tests, fixtures, assets, cursor config, markdown/json QA,
        // Cargo lock/manifests, and shell starters co-change with product
        // code by design. Those pairs are not surgery.
        if is_shotgun_noise_partner(&row.0) || is_shotgun_noise_partner(&row.1) {
            continue;
        }
        if far_apart(&row.0, &row.1) && row.3 >= 0.4 {
            let detail = format!("{} <-> {} shared={}", row.0, row.1, row.2);
            let fid: Option<i64> = conn
                .query_row("SELECT id FROM files WHERE relpath = ?", [&row.0], |r| r.get(0))
                .ok();
            let name = file_basename(&row.0);
            add(conn, &mut found, None, fid, "shotgun_surgery", &detail, row.3, &row.0, &name);
        }
    }
    found
}

fn far_apart(a: &str, b: &str) -> bool {
    let pa = Path::new(a).components().next().map(|c| c.as_os_str().to_string_lossy().into_owned());
    let pb = Path::new(b).components().next().map(|c| c.as_os_str().to_string_lossy().into_owned());
    pa.is_some() && pb.is_some() && pa != pb
}

/// True when the path is an expected I/O boundary (CLI, store, HTTP, etc.).
fn is_effect_boundary_path(path: &str) -> bool {
    BOUNDARY.iter().any(|m| path.contains(m))
}

/// True when the path is presentation or tooling, not game/sim/domain core.
fn is_effect_non_core_path(path: &str) -> bool {
    Path::new(path).components().any(|c| {
        let part = c.as_os_str().to_string_lossy().to_lowercase();
        matches!(part.as_str(), "ui" | "scripts" | "tools")
    })
}

/// True when a coupling partner is docs/QA / lockfile / starter noise
/// rather than production surgery.
fn is_shotgun_noise_partner(path: &str) -> bool {
    let path = Path::new(path);
    if path.components().any(|c| {
        let part = c.as_os_str().to_string_lossy().to_lowercase();
        matches!(
            part.as_str(),
            "docs"
                | "doc"
                | "tests"
                | "test"
                | "fixtures"
                | "fixture"
                | "assets"
                | "asset"
                | ".cursor"
                | "locales"
                | "locale"
        )
    }) {
        return true;
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_ascii_lowercase();
        // Lockfiles and crate manifests churn with product trees by design.
        if lower == "cargo.lock" || lower == "cargo.toml" {
            return true;
        }
        // Shell starters (start.sh, start-dev.sh, …) are wiring, not surgery.
        if lower.starts_with("start") && lower.ends_with(".sh") {
            return true;
        }
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
    {
        Some(ext) if matches!(ext.as_str(), "md" | "markdown" | "json") => true,
        _ => false,
    }
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

/// File basename for file-level issue kinds (not a fabricated symbol name).
pub(crate) fn file_basename(relpath: &str) -> String {
    Path::new(relpath)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(relpath)
        .to_string()
}

/// Fill `name` / `start_line` for issue JSON rows.
///
/// Symbol-backed kinds keep the joined symbol fields. File-level kinds
/// (`god_module`, `shotgun_surgery`, …) have `symbol_id` NULL, so fall back
/// to the file basename and the earliest symbol line in that file (or 1).
pub(crate) fn resolve_issue_anchor(
    relpath: Option<&str>,
    symbol_name: Option<String>,
    symbol_start: Option<i64>,
    first_symbol_line: Option<i64>,
) -> (Option<String>, Option<i64>) {
    if symbol_name.is_some() {
        let start = symbol_start.or(first_symbol_line).or(Some(1));
        return (symbol_name, start);
    }
    let Some(path) = relpath else {
        return (None, None);
    };
    let name = file_basename(path);
    let start = first_symbol_line.unwrap_or(1);
    (Some(name), Some(start))
}

/// List issues ordered by score descending.
///
/// `limit` is a **per-kind** cap (top N by score within each kind).
/// Pass a negative `limit` to return every row (no per-kind cap).
/// Pass `0` for an empty list.
pub fn list_issues(conn: &Connection, limit: i64) -> Vec<Value> {
    if limit == 0 {
        return Vec::new();
    }
    let mut stmt = conn
        .prepare(
            r#"
        SELECT i.kind, i.detail, i.score, f.relpath, s.name, s.start_line,
               (SELECT MIN(ss.start_line) FROM symbols ss WHERE ss.file_id = i.file_id)
        FROM issues i
        LEFT JOIN files f ON f.id = i.file_id
        LEFT JOIN symbols s ON s.id = i.symbol_id
        ORDER BY i.score DESC
        "#,
        )
        .unwrap();
    let all: Vec<Value> = stmt
        .query_map([], |r| {
            let relpath: Option<String> = r.get(3)?;
            let symbol_name: Option<String> = r.get(4)?;
            let symbol_start: Option<i64> = r.get(5)?;
            let first_symbol_line: Option<i64> = r.get(6)?;
            let (name, start_line) = resolve_issue_anchor(
                relpath.as_deref(),
                symbol_name,
                symbol_start,
                first_symbol_line,
            );
            Ok(json!({
                "kind": r.get::<_, String>(0)?,
                "detail": r.get::<_, String>(1)?,
                "score": r.get::<_, f64>(2)?,
                "relpath": relpath,
                "name": name,
                "start_line": start_line,
            }))
        })
        .unwrap()
        .flatten()
        .collect();
    if limit < 0 {
        return all;
    }
    select_per_kind(all, limit as usize)
}

/// Apply `limit` per kind (top N by score within each kind).
/// Scores are not one scale across kinds, so a global LIMIT only returns
/// `god_function` rows and hides `effect_in_core` / `shotgun_surgery`.
fn select_per_kind(all: Vec<Value>, per_kind: usize) -> Vec<Value> {
    use std::collections::HashMap;

    if all.is_empty() || per_kind == 0 {
        return Vec::new();
    }

    let mut kind_order: Vec<String> = Vec::new();
    let mut by_kind: HashMap<String, Vec<Value>> = HashMap::new();
    for item in all {
        let kind = item["kind"].as_str().unwrap_or("").to_string();
        if !kind_order.iter().any(|k| k == &kind) {
            kind_order.push(kind.clone());
        }
        by_kind.entry(kind).or_default().push(item);
    }

    let mut out: Vec<Value> = Vec::new();
    for kind in kind_order {
        let Some(rows) = by_kind.get_mut(&kind) else {
            continue;
        };
        // Already sorted by score DESC from the SQL ORDER BY.
        let take = per_kind.min(rows.len());
        out.extend(rows.drain(..take));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::connect;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo() -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let repo = std::env::temp_dir().join(format!("dued-issues-test-{nanos}"));
        fs::create_dir_all(repo.join("dued")).unwrap();
        repo
    }

    fn seed_crowded_issues(conn: &Connection) {
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (1, 'core/engine.py', 'python', 'a', 100, 200, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (2, 'ui/view.py', 'python', 'b', 40, 80, 0)",
            [],
        )
        .unwrap();
        for i in 0..50 {
            conn.execute(
                "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (NULL, 1, 'god_function', ?1, ?2)",
                params![format!("god {i}"), 10000.0 - i as f64],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (NULL, 1, 'god_module', 'god module symbols=20', 50.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (NULL, 1, 'effect_in_core', 'I/O mixed into core', 20.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (NULL, 2, 'shotgun_surgery', 'core/engine.py <-> ui/view.py', 0.8)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn list_issues_includes_low_score_kinds_under_limit() {
        let repo = temp_repo();
        let conn = connect(&repo);
        seed_crowded_issues(&conn);
        let listed = list_issues(&conn, 40);
        let kinds: std::collections::HashSet<&str> = listed
            .iter()
            .filter_map(|row| row["kind"].as_str())
            .collect();
        assert!(kinds.contains("god_function"), "{kinds:?}");
        assert!(kinds.contains("god_module"), "{kinds:?}");
        assert!(kinds.contains("effect_in_core"), "{kinds:?}");
        assert!(kinds.contains("shotgun_surgery"), "{kinds:?}");
        let gods = listed.iter().filter(|r| r["kind"] == "god_function").count();
        assert_eq!(gods, 40);
        assert_eq!(
            listed
                .iter()
                .filter(|r| r["kind"] == "effect_in_core")
                .count(),
            1
        );
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn list_issues_negative_limit_returns_all_rows() {
        let repo = temp_repo();
        let conn = connect(&repo);
        seed_crowded_issues(&conn);
        let listed = list_issues(&conn, -1);
        let gods = listed.iter().filter(|r| r["kind"] == "god_function").count();
        assert_eq!(gods, 50);
        assert_eq!(listed.len(), 53);
        assert!(list_issues(&conn, 0).is_empty());
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn select_per_kind_keeps_minority_kinds() {
        let mut rows = Vec::new();
        for i in 0..30 {
            rows.push(json!({"kind": "god_function", "score": 10000.0 - i as f64, "detail": i}));
        }
        rows.push(json!({"kind": "god_module", "score": 50.0, "detail": "mod"}));
        rows.push(json!({"kind": "effect_in_core", "score": 20.0, "detail": "io"}));
        rows.push(json!({"kind": "shotgun_surgery", "score": 0.8, "detail": "pair"}));
        let picked = select_per_kind(rows, 10);
        let kinds: std::collections::HashSet<&str> = picked
            .iter()
            .filter_map(|row| row["kind"].as_str())
            .collect();
        assert_eq!(picked.iter().filter(|r| r["kind"] == "god_function").count(), 10);
        assert!(kinds.contains("effect_in_core"));
        assert!(kinds.contains("shotgun_surgery"));
        assert!(kinds.contains("god_module"));
        assert!(kinds.contains("god_function"));
    }

    #[test]
    fn shotgun_skips_docs_qa_noise_keeps_production_rs_pair() {
        let repo = temp_repo();
        let conn = connect(&repo);
        for (id, path) in [
            (1, "crates/mainnet_graph/src/lib.rs"),
            (2, "src/game/graph_bridge.rs"),
            (3, "docs/design/flow.md"),
            (4, "tests/fixtures/scenarios/a.toml"),
            (5, "assets/locales/en.json"),
            (6, ".cursor/rules.md"),
            (7, "Cargo.lock"),
            (8, "crates/mainnet_graph/Cargo.toml"),
            (9, "scripts/start-dev.sh"),
        ] {
            conn.execute(
                "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (?1, ?2, 'rust', 'd', 10, 20, 0)",
                params![id, path],
            )
            .unwrap();
        }
        // docs/QA/assets/cursor/Cargo/starter noise must not become shotgun_surgery.
        for (a, b) in [
            ("docs/design/flow.md", "src/game/graph_bridge.rs"),
            ("src/game/graph_bridge.rs", "tests/fixtures/scenarios/a.toml"),
            ("assets/locales/en.json", "src/game/graph_bridge.rs"),
            (".cursor/rules.md", "src/game/graph_bridge.rs"),
            ("AGENTS.md", "src/game/graph_bridge.rs"),
            ("Cargo.lock", "src/game/graph_bridge.rs"),
            ("crates/mainnet_graph/Cargo.toml", "src/game/graph_bridge.rs"),
            ("scripts/start-dev.sh", "src/game/graph_bridge.rs"),
            ("start.sh", "crates/mainnet_graph/src/lib.rs"),
        ] {
            conn.execute(
                "INSERT INTO git_coupling(file_a, file_b, shared, strength) VALUES (?1, ?2, 5, 1.0)",
                params![a, b],
            )
            .unwrap();
        }
        // Two far-apart production .rs files still can.
        conn.execute(
            "INSERT INTO git_coupling(file_a, file_b, shared, strength) VALUES ('crates/mainnet_graph/src/lib.rs', 'src/game/graph_bridge.rs', 9, 0.9)",
            [],
        )
        .unwrap();

        let found = apply_issues(&conn);
        let shotguns: Vec<&str> = found
            .iter()
            .filter(|r| r["kind"] == "shotgun_surgery")
            .filter_map(|r| r["detail"].as_str())
            .collect();
        assert_eq!(shotguns.len(), 1, "{shotguns:?}");
        assert!(
            shotguns[0].contains("crates/mainnet_graph/src/lib.rs")
                && shotguns[0].contains("src/game/graph_bridge.rs"),
            "{shotguns:?}"
        );
        assert!(
            !shotguns.iter().any(|d| {
                d.contains("docs/")
                    || d.contains("fixtures")
                    || d.contains("assets/")
                    || d.contains(".cursor")
                    || d.contains("Cargo.lock")
                    || d.contains("Cargo.toml")
                    || d.contains("start")
            }),
            "{shotguns:?}"
        );
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn list_issues_file_level_kinds_get_basename_and_start_line() {
        let repo = temp_repo();
        let conn = connect(&repo);
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (1, 'core/engine.py', 'python', 'a', 100, 200, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (2, 'ui/view.py', 'python', 'b', 40, 80, 0)",
            [],
        )
        .unwrap();
        // Earliest symbol in engine.py starts at line 3.
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects, fan_in, fan_out)
             VALUES (10, 1, 'run', 'function', 3, 40, 'def run()', '', 'pass', 1, 20, 1, 0, 1, 0, 0, '[]', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects, fan_in, fan_out)
             VALUES (11, 1, 'helper', 'function', 50, 60, 'def helper()', '', 'pass', 1, 1, 1, 0, 1, 0, 0, '[]', 0, 0)",
            [],
        )
        .unwrap();
        // File-level rows: no symbol_id (this is the #37 bug path).
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (NULL, 1, 'god_module', 'god module symbols=20', 50.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (NULL, 2, 'shotgun_surgery', 'core/engine.py <-> ui/view.py', 0.8)",
            [],
        )
        .unwrap();
        // Symbol-backed row must keep the real symbol name/line.
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (10, 1, 'god_function', 'god function', 90.0)",
            [],
        )
        .unwrap();

        let listed = list_issues(&conn, -1);
        let god_mod = listed.iter().find(|r| r["kind"] == "god_module").unwrap();
        assert_eq!(god_mod["name"], "engine.py");
        assert_eq!(god_mod["start_line"], 3);
        assert_eq!(god_mod["relpath"], "core/engine.py");

        let shotgun = listed.iter().find(|r| r["kind"] == "shotgun_surgery").unwrap();
        assert_eq!(shotgun["name"], "view.py");
        // No symbols in ui/view.py → start_line falls back to 1.
        assert_eq!(shotgun["start_line"], 1);
        assert_eq!(shotgun["relpath"], "ui/view.py");

        let god_fn = listed.iter().find(|r| r["kind"] == "god_function").unwrap();
        assert_eq!(god_fn["name"], "run");
        assert_eq!(god_fn["start_line"], 3);

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn resolve_issue_anchor_uses_basename_not_fake_symbol() {
        let (name, start) = resolve_issue_anchor(Some("crates/foo/src/lib.rs"), None, None, Some(7));
        assert_eq!(name.as_deref(), Some("lib.rs"));
        assert_eq!(start, Some(7));
        let (name, start) = resolve_issue_anchor(Some("core/engine.py"), None, None, None);
        assert_eq!(name.as_deref(), Some("engine.py"));
        assert_eq!(start, Some(1));
        let (name, start) = resolve_issue_anchor(Some("core/engine.py"), Some("run".into()), Some(3), Some(1));
        assert_eq!(name.as_deref(), Some("run"));
        assert_eq!(start, Some(3));
        assert_eq!(file_basename("a/b/c.py"), "c.py");
    }

    #[test]
    fn apply_issues_god_module_and_shotgun_carry_basename() {
        let repo = temp_repo();
        let conn = connect(&repo);
        // Build a crowded module so god_module fires.
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (1, 'core/engine.py', 'python', 'a', 200, 400, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (2, 'ui/view.py', 'python', 'b', 40, 80, 0)",
            [],
        )
        .unwrap();
        for i in 0..20 {
            conn.execute(
                "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects, fan_in, fan_out)
                 VALUES (?1, 1, ?2, 'function', ?3, ?4, 'def x()', '', 'pass', 1, 3, 1, 0, 1, 0, 0, '[]', 0, 0)",
                params![i + 1, format!("fn_{i}"), i * 10 + 1, i * 10 + 5],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO git_coupling(file_a, file_b, shared, strength) VALUES ('core/engine.py', 'ui/view.py', 5, 0.9)",
            [],
        )
        .unwrap();

        let found = apply_issues(&conn);
        let god_mod = found.iter().find(|r| r["kind"] == "god_module").unwrap();
        assert_eq!(god_mod["name"], "engine.py");
        let shotgun = found.iter().find(|r| r["kind"] == "shotgun_surgery").unwrap();
        assert_eq!(shotgun["name"], "engine.py");

        // list_issues must also populate start_line for these kinds.
        let listed = list_issues(&conn, -1);
        let gm = listed.iter().find(|r| r["kind"] == "god_module").unwrap();
        assert_eq!(gm["name"], "engine.py");
        assert_eq!(gm["start_line"], 1);
        let ss = listed.iter().find(|r| r["kind"] == "shotgun_surgery").unwrap();
        assert_eq!(ss["name"], "engine.py");
        assert_eq!(ss["start_line"], 1);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn is_shotgun_noise_partner_matches_denylist() {
        assert!(is_shotgun_noise_partner("docs/design/flow.md"));
        assert!(is_shotgun_noise_partner("tests/fixtures/scenarios/a.toml"));
        assert!(is_shotgun_noise_partner("src/tests/helper.rs"));
        assert!(is_shotgun_noise_partner("assets/locales/en.json"));
        assert!(is_shotgun_noise_partner(".cursor/rules.md"));
        assert!(is_shotgun_noise_partner("AGENTS.md"));
        assert!(is_shotgun_noise_partner("config/settings.json"));
        assert!(is_shotgun_noise_partner("Cargo.lock"));
        assert!(is_shotgun_noise_partner("Cargo.toml"));
        assert!(is_shotgun_noise_partner("crates/foo/Cargo.toml"));
        assert!(is_shotgun_noise_partner("start.sh"));
        assert!(is_shotgun_noise_partner("scripts/start-dev.sh"));
        assert!(is_shotgun_noise_partner("bin/start_server.sh"));
        assert!(!is_shotgun_noise_partner("crates/mainnet_graph/src/lib.rs"));
        assert!(!is_shotgun_noise_partner("src/game/graph_bridge.rs"));
        assert!(!is_shotgun_noise_partner("src/game/state.rs"));
        // Not a starter: must begin with start, not contain it mid-name.
        assert!(!is_shotgun_noise_partner("scripts/restart.sh"));
        assert!(!is_shotgun_noise_partner("scripts/bootstrap.sh"));
    }

    #[test]
    fn is_effect_non_core_path_matches_ui_scripts_tools() {
        assert!(is_effect_non_core_path("src/ui/renderer.rs"));
        assert!(is_effect_non_core_path("src/ui/mod.rs"));
        assert!(is_effect_non_core_path("scripts/bench.py"));
        assert!(is_effect_non_core_path("tools/codegen.rs"));
        assert!(is_effect_non_core_path("UI/View.swift"));
        assert!(!is_effect_non_core_path("src/game/state.rs"));
        assert!(!is_effect_non_core_path("src/sim/step.rs"));
        assert!(!is_effect_non_core_path("src/domain/model.rs"));
        assert!(!is_effect_non_core_path("core/engine.py"));
        assert!(!is_effect_non_core_path("crates/mainnet_graph/src/lib.rs"));
        // Substring alone is not enough: path segment must be ui/scripts/tools.
        assert!(!is_effect_non_core_path("src/circuit/guide.rs"));
        assert!(!is_effect_non_core_path("src/toolsmith/mod.rs"));
    }

    #[test]
    fn effect_in_core_skips_ui_scripts_tools_keeps_domain() {
        let repo = temp_repo();
        let conn = connect(&repo);
        let effects = r#"["filesystem"]"#;
        for (id, path) in [
            (1, "src/game/persist.rs"),
            (2, "src/sim/io_bridge.rs"),
            (3, "src/domain/store_wrap.rs"),
            (4, "src/ui/save_dialog.rs"),
            (5, "scripts/export_state.py"),
            (6, "tools/dump_db.rs"),
            (7, "core/engine.py"),
            (8, "src/cli.py"),
        ] {
            conn.execute(
                "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (?1, ?2, 'rust', 'd', 40, 80, 0)",
                params![id, path],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects, fan_in, fan_out)
                 VALUES (?1, ?1, 'load', 'function', 1, 20, 'fn load()', '', 'open(path)', 1, 1, 1, 1, 1, 0, 0, ?2, 3, 0)",
                params![id, effects],
            )
            .unwrap();
        }

        let found = apply_issues(&conn);
        let effect_paths: Vec<&str> = found
            .iter()
            .filter(|r| r["kind"] == "effect_in_core")
            .filter_map(|r| r["relpath"].as_str())
            .collect();
        assert!(
            effect_paths.contains(&"src/game/persist.rs"),
            "{effect_paths:?}"
        );
        assert!(
            effect_paths.contains(&"src/sim/io_bridge.rs"),
            "{effect_paths:?}"
        );
        assert!(
            effect_paths.contains(&"src/domain/store_wrap.rs"),
            "{effect_paths:?}"
        );
        assert!(
            effect_paths.contains(&"core/engine.py"),
            "{effect_paths:?}"
        );
        assert!(
            !effect_paths.iter().any(|p| p.contains("/ui/") || *p == "src/ui/save_dialog.rs"),
            "{effect_paths:?}"
        );
        assert!(
            !effect_paths.iter().any(|p| p.starts_with("scripts/") || p.starts_with("tools/")),
            "{effect_paths:?}"
        );
        // Existing BOUNDARY substrings still suppress the flag.
        assert!(
            !effect_paths.iter().any(|p| p.ends_with("cli.py")),
            "{effect_paths:?}"
        );
        let _ = fs::remove_dir_all(repo);
    }
}
