use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Instant;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::clones::{find_clones, find_embed_clones};
use crate::cost::apply_cost_hints;
use crate::effects::apply_effects;
use crate::embed::{embed_symbols, mismatch_flags};
use crate::fingerprints::apply_fingerprints;
use crate::git_hist::analyze_history;
use crate::graph::resolve_and_store_edges;
use crate::hollow::apply_hollow;
use crate::inventory::inventory;
use crate::issues::apply_issues;
use crate::names::analyze_names;
use crate::parse::parse_source;
use crate::progress::{note, stage, Bar};
use crate::rank::compute_rank;
use crate::risks::apply_risks;
use crate::store::{connect, delete_file_row, meta_i64, set_meta, PARSER_VERSION};
use crate::walk::walk_repo;

pub fn run_scan(
    repo: &Path,
    max_files: Option<usize>,
    budget_seconds: Option<f64>,
    with_git: bool,
    with_embed: bool,
    model_name: &str,
) -> Value {
    let started = Instant::now();
    stage("walk source files");
    let files = dedupe_by_relpath(walk_repo(repo, max_files));
    note(&format!("found {} source files", files.len()));
    let conn = connect(repo);
    conn.execute_batch("BEGIN IMMEDIATE")
        .expect("begin scan transaction");
    let mut old: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT relpath, digest FROM files").unwrap();
        for row in stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .flatten()
        {
            old.insert(row.0, row.1);
        }
    }
    let current: HashSet<String> = files.iter().map(|f| f.relpath.clone()).collect();
    for relpath in old.keys() {
        if !current.contains(relpath) {
            delete_file_row(&conn, relpath);
        }
    }
    // Missing or older parser_version means digests alone are not enough: new
    // extract rules (e.g. Rust structs) must re-parse even when bytes match.
    let stored_parser = meta_i64(&conn, "parser_version").unwrap_or(0);
    let parser_stale = stored_parser != PARSER_VERSION;
    let mut reused = 0i64;
    let mut dirty = Vec::new();
    for src in &files {
        if let Some(budget) = budget_seconds {
            if started.elapsed().as_secs_f64() > budget {
                break;
            }
        }
        if !parser_stale && old.get(&src.relpath) == Some(&src.digest) {
            reused += 1;
            continue;
        }
        delete_file_row(&conn, &src.relpath);
        dirty.push(src);
    }
    stage(&format!(
        "parse + metrics ({} dirty, {} reused)",
        dirty.len(),
        reused
    ));
    let mut parsed = 0i64;
    let dirty_total = dirty.len();
    let mut parse_bar = Bar::new("parse", dirty_total);
    {
        let mut insert_file = conn
            .prepare(
                "INSERT INTO files(relpath, language, digest, loc, size, is_test, tokens, ast_nodes) VALUES (?,?,?,?,?,?,?,?)",
            )
            .unwrap();
        let mut insert_symbol = conn
            .prepare(
                r#"
                INSERT INTO symbols(file_id, name, kind, start_line, end_line, signature, docstring, body,
                    cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test)
                VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                "#,
            )
            .unwrap();
        let mut insert_call = conn
            .prepare("INSERT INTO call_facts(src_file_id, src_symbol_id, callee) VALUES (?,?,?)")
            .unwrap();
        let mut insert_import = conn
            .prepare("INSERT INTO import_facts(src_file_id, module_hint) VALUES (?,?)")
            .unwrap();
        for src in dirty {
            if let Some(budget) = budget_seconds {
                if started.elapsed().as_secs_f64() > budget {
                    break;
                }
            }
            // Delete again immediately before INSERT. Covers leftover rows and
            // any duplicate dirty relpath so UNIQUE files.relpath cannot fire.
            delete_file_row(&conn, &src.relpath);
            let text = fs::read(&src.path).unwrap_or_default();
            let suffix = src
                .path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let extracted = parse_source(&src.language, &suffix, &text);
            insert_file
                .execute(params![
                    src.relpath,
                    src.language,
                    src.digest,
                    src.loc,
                    src.size,
                    src.is_test as i64,
                    src.tokens,
                    extracted.ast_nodes
                ])
                .expect("insert dirty file row");
            let file_id = conn.last_insert_rowid();
            let mut calls_by_owner: HashMap<&str, Vec<&str>> = HashMap::new();
            for (owner, callee) in &extracted.calls {
                calls_by_owner.entry(owner.as_str()).or_default().push(callee.as_str());
            }
            for symbol in &extracted.symbols {
                let mut entry = symbol.is_entry || src.relpath.ends_with("main.py") || src.relpath.ends_with("main.rs");
                if src.relpath.contains("app/") && src.relpath.ends_with("route.ts") && symbol.is_public {
                    entry = true;
                }
                insert_symbol
                    .execute(params![
                        file_id,
                        symbol.name,
                        symbol.kind,
                        symbol.start_line,
                        symbol.end_line,
                        symbol.signature,
                        symbol.docstring,
                        symbol.body,
                        symbol.cyclomatic,
                        symbol.cognitive,
                        symbol.nesting,
                        symbol.nargs,
                        symbol.is_public as i64,
                        entry as i64,
                        (symbol.is_test || src.is_test) as i64
                    ])
                    .unwrap();
                let sid = conn.last_insert_rowid();
                if let Some(callees) = calls_by_owner.get(symbol.name.as_str()) {
                    for callee in callees {
                        insert_call.execute(params![file_id, sid, callee]).ok();
                    }
                }
            }
            for module in &extracted.import_modules {
                insert_import.execute(params![file_id, module]).ok();
            }
            parsed += 1;
            parse_bar.tick(&src.relpath);
        }
    }
    parse_bar.finish();
    stage("build call graph and metrics");
    resolve_and_store_edges(&conn);
    apply_effects(&conn);
    apply_cost_hints(&conn);
    apply_fingerprints(&conn);
    apply_risks(&conn);
    let git_info = if with_git {
        stage("git history");
        analyze_history(repo, &conn)
    } else {
        json!({"enabled": false})
    };
    stage("rank + name health");
    compute_rank(&conn);
    analyze_names(&conn);
    let hollow = apply_hollow(&conn);
    let mut clones = find_clones(&conn);
    let mut mismatches = Vec::new();
    let model_used = if !with_embed {
        "none"
    } else if crate::embed::use_stub(model_name) {
        "stub"
    } else {
        model_name
    };
    if with_embed {
        if crate::embed::use_stub(model_name) {
            stage("embed symbols (stub vectors)");
        } else {
            stage("embed symbols (Jina)");
        }
        embed_symbols(&conn, model_name, true);
        mismatches = mismatch_flags(&conn);
        clones.extend(find_embed_clones(&conn));
    }
    stage("flag issues and inventory");
    let issues = apply_issues(&conn);
    let inv = inventory(&conn, repo);
    set_meta(&conn, "repo", &json!(repo.display().to_string()));
    set_meta(&conn, "model", &json!(model_used));
    set_meta(&conn, "parser_version", &Value::from(PARSER_VERSION));
    conn.execute_batch("COMMIT")
        .expect("commit scan transaction");
    summary(&conn, parsed, reused, clones.len(), hollow.len(), mismatches.len(), issues.len(), git_info, inv, started, model_used)
}

/// Keep one entry per relpath (last wins). Duplicate walk hits would otherwise
/// delete once then INSERT twice and panic on UNIQUE files.relpath.
fn dedupe_by_relpath(files: Vec<crate::walk::SourceFile>) -> Vec<crate::walk::SourceFile> {
    let mut by_rel: HashMap<String, crate::walk::SourceFile> = HashMap::new();
    for src in files {
        by_rel.insert(src.relpath.clone(), src);
    }
    let mut out: Vec<_> = by_rel.into_values().collect();
    out.sort_by(|a, b| a.relpath.cmp(&b.relpath));
    out
}

fn summary(
    conn: &Connection,
    parsed: i64,
    reused: i64,
    clones: usize,
    hollow: usize,
    mismatches: usize,
    issues: usize,
    git_info: Value,
    inv: Value,
    started: Instant,
    model_used: &str,
) -> Value {
    let files: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
    let symbols: i64 = conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0)).unwrap();
    let edges: i64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0)).unwrap();
    json!({
        "files": files,
        "symbols": symbols,
        "edges": edges,
        "parsed": parsed,
        "reused": reused,
        "clones": clones,
        "hollow": hollow,
        "mismatches": mismatches,
        "issues": issues,
        "inventory": inv,
        "git": git_info,
        "elapsed_seconds": (started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0,
        "engine": "dued",
        "model": model_used,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issues::list_issues;
    use crate::store::{connect, meta_i64, set_meta, PARSER_VERSION};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let repo = std::env::temp_dir().join(format!("dued-scan-{label}-{nanos}"));
        fs::create_dir_all(&repo).unwrap();
        repo
    }

    fn write_py(repo: &Path, rel: &str, body: &str) {
        let path = repo.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    fn god_fn_source(tag: &str) -> String {
        // Enough branches for god_function scoring on rescan.
        let mut body = format!("def messy_{tag}(x):\n");
        for i in 0..20 {
            body.push_str(&format!("    if x == {i}:\n        return {i}\n"));
        }
        body.push_str("    return x\n");
        body
    }

    #[test]
    fn dedupe_by_relpath_keeps_last() {
        let a = crate::walk::SourceFile {
            path: Path::new("a.py").to_path_buf(),
            relpath: "a.py".into(),
            language: "python".into(),
            size: 1,
            digest: "first".into(),
            is_test: false,
            loc: 1,
            tokens: 1,
        };
        let mut b = a.clone();
        b.digest = "second".into();
        let out = dedupe_by_relpath(vec![a, b]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].digest, "second");
    }

    #[test]
    fn dirty_rescan_updates_in_place_without_unique_panic() {
        let repo = temp_repo("dirty");
        write_py(&repo, "core.py", &god_fn_source("v1"));
        write_py(&repo, "ok.py", "def fine():\n    return 1\n");
        let first = run_scan(&repo, None, None, false, false, "stub");
        assert_eq!(first["parsed"], 2);
        assert_eq!(first["reused"], 0);

        write_py(&repo, "core.py", &god_fn_source("v2"));
        // Leftover-row case: a stale files row must not block the dirty INSERT.
        // delete_file_row before INSERT clears it (same path as a failed prior delete).
        let conn = connect(&repo);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM files WHERE relpath = 'core.py'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        drop(conn);

        let second = run_scan(&repo, None, None, true, false, "stub");
        assert_eq!(second["parsed"], 1);
        assert_eq!(second["reused"], 1);
        assert_eq!(second["files"], 2);

        let conn = connect(&repo);
        let digest: String = conn
            .query_row("SELECT digest FROM files WHERE relpath = 'core.py'", [], |r| r.get(0))
            .unwrap();
        assert!(!digest.is_empty());
        let issues = list_issues(&conn, 40);
        for row in &issues {
            if row["kind"] == "god_function" {
                assert!(row["relpath"].as_str().is_some(), "{row}");
                assert!(row["name"].as_str().is_some(), "{row}");
                assert!(!row["relpath"].as_str().unwrap().is_empty());
                assert!(!row["name"].as_str().unwrap().is_empty());
            }
        }
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn parser_version_bump_reparses_matching_digests_for_rust_types() {
        let repo = temp_repo("parser-bump");
        let path = repo.join("types.rs");
        fs::write(
            &path,
            r#"
pub struct TheStruct {
    pub id: u64,
}

impl TheStruct {
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}
"#,
        )
        .unwrap();

        let first = run_scan(&repo, None, None, false, false, "stub");
        assert_eq!(first["parsed"], 1);
        assert_eq!(first["reused"], 0);

        let conn = connect(&repo);
        let kind: String = conn
            .query_row(
                "SELECT kind FROM symbols WHERE name = 'TheStruct'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kind, "struct");
        // Simulate an index built before Rust type extraction existed.
        conn.execute("DELETE FROM symbols WHERE kind = 'struct'", [])
            .unwrap();
        set_meta(&conn, "parser_version", &Value::from(0i64));
        let missing: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbols WHERE name = 'TheStruct'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(missing, 0);
        drop(conn);

        let second = run_scan(&repo, None, None, false, false, "stub");
        assert_eq!(second["parsed"], 1, "{second}");
        assert_eq!(second["reused"], 0, "{second}");

        let conn = connect(&repo);
        let stored = meta_i64(&conn, "parser_version");
        assert_eq!(stored, Some(PARSER_VERSION));
        let kind: String = conn
            .query_row(
                "SELECT kind FROM symbols WHERE name = 'TheStruct'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kind, "struct");
        let sliced = crate::slice::slice_symbol(&conn, "TheStruct", 2);
        assert!(sliced.get("error").is_none(), "{sliced}");
        assert_eq!(sliced["symbols"][0]["name"], "TheStruct");
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn dirty_rescan_with_git_and_preexisting_row_survives_unique() {
        let repo = temp_repo("git-dirty");
        write_py(&repo, "a.py", "def a():\n    return 1\n");
        Command::new("git").args(["init"]).current_dir(&repo).status().unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).status().unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .status()
            .unwrap();

        let first = run_scan(&repo, None, None, true, false, "stub");
        assert_eq!(first["parsed"], 1);

        // Simulate a half-cleaned index: issues point at the file, row still present.
        let conn = connect(&repo);
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) \
             SELECT s.id, f.id, 'god_function', 'stale', 99.0 FROM symbols s JOIN files f ON f.id = s.file_id LIMIT 1",
            [],
        )
        .unwrap();
        drop(conn);

        write_py(&repo, "a.py", "def a():\n    return 2\n");
        let second = run_scan(&repo, None, None, true, false, "stub");
        assert_eq!(second["parsed"], 1);
        assert_eq!(second["reused"], 0);

        let conn = connect(&repo);
        let issues = list_issues(&conn, 40);
        for row in &issues {
            assert!(row["relpath"].as_str().is_some() || row["kind"] != "god_function", "{row}");
            if row["kind"] == "god_function" {
                assert!(row["name"].as_str().is_some(), "{row}");
            }
        }
        let _ = fs::remove_dir_all(repo);
    }

    fn write_rs(repo: &Path, rel: &str, body: &str) {
        let path = repo.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    fn pad_rust(base: &str, target_bytes: usize) -> String {
        // Grow file bytes with comments, not thousands of same-stem fns.
        // Many `pad_fn_*` names collapse into one names.rs stem bucket and O(n²).
        let mut out = String::from(base);
        let mut i = 0u32;
        while out.len() < target_bytes {
            out.push_str(&format!(
                "// padding line {i} keep large Rust sources in the index under scan\n"
            ));
            i += 1;
        }
        out
    }

    #[test]
    fn large_rust_sources_enter_files_and_slice_resolves() {
        crate::progress::set_quiet(true);
        let repo = temp_repo("large-rust");
        // A few large-bodied pub fns share a leading `pub` token (clone-bucket stress)
        // without exploding analyze_names stem groups.
        let mut access = String::from(
            "pub struct WholesaleContract {\n    pub id: u64,\n    pub qty: i64,\n}\n\n\
             impl WholesaleContract {\n    pub fn apply(&self) -> u64 { self.id }\n}\n\n\
             pub fn apply_purchase_wholesale(c: &WholesaleContract) -> u64 {\n    c.apply()\n}\n\n\
             pub fn new() -> i32 { 0 }\n\n",
        );
        for i in 0..40 {
            let pad = "x".repeat(120);
            access.push_str(&format!(
                "pub fn access_rule_{i}(state: &mut i32) -> i32 {{\n    let mut acc = *state;\n    // {pad}\n    acc = acc.wrapping_add({i});\n    *state = acc;\n    acc\n}}\n"
            ));
        }
        let access = pad_rust(&access, 130_805);
        let mut state = String::from(
            "pub struct GameState {\n    pub tick: u64,\n}\n\n\
             impl GameState {\n    pub fn step(&mut self) { self.tick += 1; }\n}\n\n\
             pub fn new() -> GameState { GameState { tick: 0 } }\n\n",
        );
        for i in 0..40 {
            let pad = "y".repeat(120);
            state.push_str(&format!(
                "pub fn state_op_{i}(tick: &mut u64) {{\n    let mut acc = *tick;\n    // {pad}\n    acc = acc.wrapping_add({i});\n    *tick = acc;\n}}\n"
            ));
        }
        let state = pad_rust(&state, 300_868);
        write_rs(&repo, "src/game/access_network.rs", &access);
        write_rs(&repo, "src/game/state.rs", &state);
        assert!(repo.join("src/game/access_network.rs").metadata().unwrap().len() >= 130_000);
        assert!(repo.join("src/game/state.rs").metadata().unwrap().len() >= 300_000);

        Command::new("git").args(["init"]).current_dir(&repo).status().unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).status().unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .status()
            .unwrap();

        let started = Instant::now();
        let summary = run_scan(&repo, None, None, true, false, "stub");
        assert!(
            started.elapsed().as_secs() < 60,
            "scan --git hung or was too slow: {:?}",
            started.elapsed()
        );
        assert_eq!(summary["files"], 2);
        assert!(summary["parsed"].as_i64().unwrap() >= 2);

        let conn = connect(&repo);
        let paths: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT relpath FROM files ORDER BY relpath")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .flatten()
                .collect()
        };
        assert!(
            paths.iter().any(|p| p.ends_with("access_network.rs")),
            "{paths:?}"
        );
        assert!(paths.iter().any(|p| p.ends_with("state.rs")), "{paths:?}");
        let max_size: i64 = conn
            .query_row("SELECT MAX(size) FROM files", [], |r| r.get(0))
            .unwrap();
        assert!(max_size >= 300_000, "max size {max_size}");

        let sliced = crate::slice::slice_symbol(&conn, "WholesaleContract", 4);
        assert!(sliced.get("error").is_none(), "{sliced}");
        assert_eq!(sliced["root"]["name"], "WholesaleContract");
        assert!(
            sliced["root"]["relpath"]
                .as_str()
                .unwrap()
                .ends_with("access_network.rs"),
            "{sliced}"
        );
        let names: Vec<&str> = sliced["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row["name"].as_str())
            .collect();
        assert!(names.contains(&"WholesaleContract"), "{names:?}");
        let blast = sliced["blast_radius"].as_u64().or_else(|| sliced["blast_radius"].as_i64().map(|n| n as u64)).unwrap();
        assert!(blast < 10, "blast_radius exploded: {sliced}");
        assert!(!names.iter().any(|n| n.starts_with("access_rule_")), "{names:?}");
        assert!(!names.iter().any(|n| n.starts_with("state_op_")), "{names:?}");
        assert!(!names.contains(&"new"), "{names:?}");

        let ambiguous = crate::slice::slice_symbol(&conn, "new", 4);
        assert_eq!(
            ambiguous.get("error").and_then(|v| v.as_str()),
            Some("ambiguous symbol name; qualify as path::name")
        );
        let _ = fs::remove_dir_all(repo);
    }
}
