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
    // Bare-name queries that hit multiple symbols must stay explicit. Expanding the first
    // match silently turns `slice new` into an arbitrary root and a huge blast radius.
    if matches.len() > 1 && !query.contains("::") {
        let candidates: Vec<Value> = matches
            .iter()
            .map(|m| {
                json!({
                    "name": m["name"],
                    "relpath": m["relpath"],
                    "signature": m["signature"],
                    "start_line": m.get("start_line").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        return json!({
            "query": query,
            "error": "ambiguous symbol name; qualify as path::name",
            "candidates": candidates,
            "symbols": [],
            "files": [],
            "effects": [],
            "blast_radius": 0,
            "callers": [],
            "tests": [],
            "taint": {"sinks": [], "params": [], "params_may_reach_sinks": []},
            "cost_hint": 0,
        });
    }
    let root = matches[0].clone();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((root["id"].as_i64().unwrap(), 0));
    let mut nodes = Vec::new();
    let mut unresolved_callees: Vec<String> = Vec::new();
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
            // Unresolved edges keep dst_file_id NULL on purpose (generic / ambiguous callees).
            // Do not fan out to every same-named symbol in the index — that pollutes unique-name slices.
            let Some(dst_file) = edge.1 else {
                unresolved_callees.push(edge.0);
                continue;
            };
            let mut dstmt = conn
                .prepare("SELECT id FROM symbols WHERE name = ? AND file_id = ?")
                .unwrap();
            for dest in dstmt
                .query_map(params![edge.0, dst_file], |r| r.get::<_, i64>(0))
                .unwrap()
                .flatten()
            {
                queue.push_back((dest, d + 1));
            }
        }
    }
    unresolved_callees.sort();
    unresolved_callees.dedup();
    let root_name = root["name"].as_str().unwrap_or("");
    let root_file = root["file_id"].as_i64().unwrap_or(0);
    // Unique names: also keep callers whose call edge left dst_file_id NULL.
    // Issue #1 tightened callers to `dst_file_id = root` only; that dropped real
    // cross-file callers (e.g. Game `impl` → imported crate fn) when resolution
    // stored NULL. Outgoing BFS above still skips NULL — do not undo #1 blast fix.
    // Ambiguous / path-qualified roots stay file-exact so NULL edges do not attach
    // to every overload.
    let name_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols WHERE name = ?", [root_name], |r| r.get(0))
        .unwrap_or(0);
    let caller_sql = if name_count == 1 {
        r#"
        SELECT s.name, f.relpath, s.start_line
        FROM edges e JOIN symbols s ON s.id = e.src_symbol_id JOIN files f ON f.id = s.file_id
        WHERE e.dst_name = ? AND e.kind = 'call'
          AND (e.dst_file_id = ? OR e.dst_file_id IS NULL)
        "#
    } else {
        r#"
        SELECT s.name, f.relpath, s.start_line
        FROM edges e JOIN symbols s ON s.id = e.src_symbol_id JOIN files f ON f.id = s.file_id
        WHERE e.dst_name = ? AND e.kind = 'call' AND e.dst_file_id = ?
        "#
    };
    let mut cstmt = conn.prepare(caller_sql).unwrap();
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
        "unresolved_callees": unresolved_callees,
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
            SELECT s.id, s.name, s.signature, s.body, s.file_id, f.relpath, s.start_line
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
                    "start_line": r.get::<_, i64>(6)?,
                }))
            })
            .unwrap()
            .flatten()
            .collect();
    }
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.signature, s.body, s.file_id, f.relpath, s.start_line
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
            "start_line": r.get::<_, i64>(6)?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::connect;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo() -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let repo = std::env::temp_dir().join(format!("dued-slice-test-{nanos}"));
        fs::create_dir_all(repo.join("dued")).unwrap();
        repo
    }

    fn seed_collision_index(conn: &Connection) {
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (1, 'bridge.rs', 'rust', 'a', 40, 100, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (2, 'noise_a.rs', 'rust', 'b', 80, 200, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (3, 'noise_b.rs', 'rust', 'c', 80, 200, 0)",
            [],
        )
        .unwrap();
        // Unique entry that calls common method names (left unresolved on purpose).
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (1, 1, 'sync_graph_access_layers', 'function', 1, 20, 'fn sync_graph_access_layers()', '', 'fn sync_graph_access_layers() { ensure_graph_world(); new(); get(); as_str(); is_empty(); default(); }', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (2, 1, 'ensure_graph_world', 'function', 22, 30, 'fn ensure_graph_world()', '', 'fn ensure_graph_world() {}', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        // Same-file unique callee that should expand.
        conn.execute(
            "INSERT INTO edges(src_file_id, src_symbol_id, dst_file_id, dst_name, kind) VALUES (1, 1, 1, 'ensure_graph_world', 'call')",
            [],
        )
        .unwrap();
        // Unresolved generic callees — previously exploded the slice.
        for name in ["new", "get", "as_str", "is_empty", "default"] {
            conn.execute(
                "INSERT INTO edges(src_file_id, src_symbol_id, dst_file_id, dst_name, kind) VALUES (1, 1, NULL, ?1, 'call')",
                params![name],
            )
            .unwrap();
        }
        // Noise: many same-named methods with heavy effects in other files.
        let noise: &[(&str, i64, i64, &str)] = &[
            ("new", 2, 10, "[\"filesystem\"]"),
            ("default", 2, 11, "[\"network\"]"),
            ("get", 2, 12, "[\"process\"]"),
            ("as_str", 2, 13, "[\"unsafe\"]"),
            ("is_empty", 2, 14, "[\"global_mutate\"]"),
            ("new", 3, 15, "[\"filesystem\"]"),
            ("get", 3, 16, "[\"network\"]"),
        ];
        for (i, (name, file_id, line, effects)) in noise.iter().enumerate() {
            let id = 100 + i as i64;
            conn.execute(
                "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
                 VALUES (?1, ?2, ?3, 'method', ?4, ?4, ?5, '', '', 1, 1, 0, 0, 1, 0, 0, ?6)",
                params![id, file_id, name, line, format!("fn {name}()"), effects],
            )
            .unwrap();
        }
    }

    #[test]
    fn unique_name_slice_ignores_unresolved_generic_callees() {
        let repo = temp_repo();
        let conn = connect(&repo);
        seed_collision_index(&conn);
        let sliced = slice_symbol(&conn, "sync_graph_access_layers", 4);
        assert!(sliced.get("error").is_none(), "{sliced}");
        let names: Vec<&str> = sliced["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["name"].as_str())
            .collect();
        assert_eq!(names, vec!["sync_graph_access_layers", "ensure_graph_world"]);
        assert_eq!(sliced["blast_radius"], 1);
        let effects = sliced["effects"].as_array().unwrap();
        assert!(effects.is_empty(), "unexpected effects from noise methods: {effects:?}");
        let unresolved = sliced["unresolved_callees"].as_array().unwrap();
        let unresolved: Vec<&str> = unresolved.iter().filter_map(|v| v.as_str()).collect();
        assert!(unresolved.contains(&"new"));
        assert!(unresolved.contains(&"default"));
        assert!(unresolved.contains(&"get"));
        assert!(unresolved.contains(&"as_str"));
        assert!(unresolved.contains(&"is_empty"));
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn ambiguous_bare_name_requires_qualification() {
        let repo = temp_repo();
        let conn = connect(&repo);
        seed_collision_index(&conn);
        let sliced = slice_symbol(&conn, "new", 4);
        assert_eq!(
            sliced["error"].as_str().unwrap(),
            "ambiguous symbol name; qualify as path::name"
        );
        assert!(sliced["candidates"].as_array().unwrap().len() >= 2);
        assert_eq!(sliced["blast_radius"], 0);

        let qualified = slice_symbol(&conn, "noise_a.rs::new", 1);
        assert!(qualified.get("error").is_none(), "{qualified}");
        let names: Vec<&str> = qualified["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["name"].as_str())
            .collect();
        assert_eq!(names, vec!["new"]);
        assert_eq!(qualified["root"]["relpath"], "noise_a.rs");
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn same_file_resolved_generic_still_expands() {
        let repo = temp_repo();
        let conn = connect(&repo);
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (1, 'bridge.rs', 'rust', 'a', 40, 100, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (2, 'noise.rs', 'rust', 'b', 80, 200, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (1, 1, 'apply_op', 'function', 1, 10, 'fn apply_op()', '', '', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (2, 1, 'new', 'method', 12, 20, 'fn new()', '', '', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (3, 2, 'new', 'method', 1, 30, 'fn new()', '', 'fs::read', 1, 1, 0, 0, 1, 0, 0, '[\"filesystem\"]')",
            [],
        )
        .unwrap();
        // Resolved same-file generic (what choose_call_targets returns for unique in-file new).
        conn.execute(
            "INSERT INTO edges(src_file_id, src_symbol_id, dst_file_id, dst_name, kind) VALUES (1, 1, 1, 'new', 'call')",
            [],
        )
        .unwrap();
        // Unresolved cross-file generic must not expand.
        conn.execute(
            "INSERT INTO edges(src_file_id, src_symbol_id, dst_file_id, dst_name, kind) VALUES (1, 1, NULL, 'get', 'call')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (4, 2, 'get', 'method', 40, 50, 'fn get()', '', '', 1, 1, 0, 0, 1, 0, 0, '[\"network\"]')",
            [],
        )
        .unwrap();

        let sliced = slice_symbol(&conn, "apply_op", 4);
        let names: Vec<&str> = sliced["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["name"].as_str())
            .collect();
        assert_eq!(names, vec!["apply_op", "new"]);
        assert_eq!(sliced["root"]["relpath"], "bridge.rs");
        assert_eq!(sliced["blast_radius"], 1);
        let effects = sliced["effects"].as_array().unwrap();
        assert!(effects.is_empty(), "{effects:?}");
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn unique_name_keeps_unresolved_cross_file_caller() {
        // Models graph_bridge → allocate_customers when the call edge left dst_file_id NULL.
        let repo = temp_repo();
        let conn = connect(&repo);
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (1, 'crates/mainnet_graph/src/prototype/allocate.rs', 'rust', 'a', 40, 100, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (2, 'src/game/graph_bridge.rs', 'rust', 'b', 80, 200, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (1, 1, 'allocate_customers', 'function', 1, 20, 'fn allocate_customers()', '', '', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (2, 2, 'apply_competitive_alloc_jobs', 'function', 100, 120, 'fn apply_competitive_alloc_jobs()', '', 'allocate_customers()', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        // Unresolved unique-name edge (dst_file_id NULL) — must still surface as a caller.
        conn.execute(
            "INSERT INTO edges(src_file_id, src_symbol_id, dst_file_id, dst_name, kind) VALUES (2, 2, NULL, 'allocate_customers', 'call')",
            [],
        )
        .unwrap();

        let sliced = slice_symbol(&conn, "allocate_customers", 4);
        assert!(sliced.get("error").is_none(), "{sliced}");
        let callers = sliced["callers"].as_array().unwrap();
        let names: Vec<&str> = callers.iter().filter_map(|c| c["name"].as_str()).collect();
        assert!(
            names.contains(&"apply_competitive_alloc_jobs"),
            "unique-name NULL edge dropped impl caller: {callers:?}"
        );
        assert_eq!(callers[0]["relpath"], "src/game/graph_bridge.rs");
        // Outgoing blast must stay tight (root file only; no NULL fan-out).
        assert_eq!(sliced["blast_radius"], 1);
        assert_eq!(sliced["files"].as_array().unwrap().len(), 1);
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn ambiguous_qualified_root_ignores_unresolved_callers() {
        let repo = temp_repo();
        let conn = connect(&repo);
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (1, 'a.rs', 'rust', 'a', 10, 10, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (2, 'b.rs', 'rust', 'b', 10, 10, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files(id, relpath, language, digest, loc, size, is_test) VALUES (3, 'caller.rs', 'rust', 'c', 10, 10, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (1, 1, 'process', 'function', 1, 5, 'fn process()', '', '', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (2, 2, 'process', 'function', 1, 5, 'fn process()', '', '', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symbols(id, file_id, name, kind, start_line, end_line, signature, docstring, body, cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test, effects)
             VALUES (3, 3, 'entry', 'function', 1, 5, 'fn entry()', '', 'process()', 1, 1, 0, 0, 1, 0, 0, '[]')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO edges(src_file_id, src_symbol_id, dst_file_id, dst_name, kind) VALUES (3, 3, NULL, 'process', 'call')",
            [],
        )
        .unwrap();

        let sliced = slice_symbol(&conn, "a.rs::process", 1);
        assert!(sliced.get("error").is_none(), "{sliced}");
        let callers = sliced["callers"].as_array().unwrap();
        assert!(
            callers.is_empty(),
            "ambiguous NULL caller must not attach to a qualified overload: {callers:?}"
        );
        let _ = fs::remove_dir_all(&repo);
    }
}
