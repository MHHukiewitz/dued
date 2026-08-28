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
use crate::store::{connect, delete_file_row, set_meta};
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
    let mut reused = 0i64;
    let mut dirty = Vec::new();
    for src in &files {
        if let Some(budget) = budget_seconds {
            if started.elapsed().as_secs_f64() > budget {
                break;
            }
        }
        if old.get(&src.relpath) == Some(&src.digest) {
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
    use crate::store::connect;
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
}
