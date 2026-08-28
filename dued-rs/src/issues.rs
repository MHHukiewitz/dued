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
        // Docs, tests, fixtures, assets, cursor config, and markdown/json QA
        // co-change with product code by design. Those pairs are not surgery.
        if is_shotgun_noise_partner(&row.0) || is_shotgun_noise_partner(&row.1) {
            continue;
        }
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

/// True when a coupling partner is docs/QA noise rather than production surgery.
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

pub fn list_issues(conn: &Connection, limit: i64) -> Vec<Value> {
    if limit <= 0 {
        return Vec::new();
    }
    let mut stmt = conn
        .prepare(
            r#"
        SELECT i.kind, i.detail, i.score, f.relpath, s.name, s.start_line
        FROM issues i
        LEFT JOIN files f ON f.id = i.file_id
        LEFT JOIN symbols s ON s.id = i.symbol_id
        ORDER BY i.score DESC
        "#,
        )
        .unwrap();
    let all: Vec<Value> = stmt
        .query_map([], |r| {
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
        .collect();
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
        ] {
            conn.execute(
                "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (?1, ?2, 'rust', 'd', 10, 20, 0)",
                params![id, path],
            )
            .unwrap();
        }
        // docs/QA/assets/cursor noise must not become shotgun_surgery.
        for (a, b) in [
            ("docs/design/flow.md", "src/game/graph_bridge.rs"),
            ("src/game/graph_bridge.rs", "tests/fixtures/scenarios/a.toml"),
            ("assets/locales/en.json", "src/game/graph_bridge.rs"),
            (".cursor/rules.md", "src/game/graph_bridge.rs"),
            ("AGENTS.md", "src/game/graph_bridge.rs"),
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
            !shotguns
                .iter()
                .any(|d| d.contains("docs/") || d.contains("fixtures") || d.contains("assets/") || d.contains(".cursor")),
            "{shotguns:?}"
        );
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
        assert!(!is_shotgun_noise_partner("crates/mainnet_graph/src/lib.rs"));
        assert!(!is_shotgun_noise_partner("src/game/graph_bridge.rs"));
        assert!(!is_shotgun_noise_partner("src/game/state.rs"));
    }
}
