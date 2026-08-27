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
    let files = walk_repo(repo, max_files);
    note(&format!("found {} source files", files.len()));
    let conn = connect(repo);
    conn.execute_batch("BEGIN IMMEDIATE").ok();
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
                .unwrap();
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
    conn.execute_batch("COMMIT").ok();
    summary(&conn, parsed, reused, clones.len(), hollow.len(), mismatches.len(), issues.len(), git_info, inv, started, model_used)
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
