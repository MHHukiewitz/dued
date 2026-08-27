use std::collections::{HashMap, HashSet};
use std::path::Path;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use serde_json::Value;

use crate::paths::set_python_layout;
use crate::store::connect;

fn dumps(value: &Value) -> String {
    serde_json::to_string(value).unwrap()
}

fn dumps_vec(values: &[Value]) -> String {
    serde_json::to_string(values).unwrap()
}

fn with_conn<T>(repo: &str, f: impl FnOnce(&rusqlite::Connection) -> T) -> T {
    let conn = connect(Path::new(repo));
    f(&conn)
}

#[pyfunction]
fn init_index(repo: &str) {
    let _ = connect(Path::new(repo));
}

#[pyfunction]
#[pyo3(signature = (repo, max_files=None))]
fn walk_repo(repo: &str, max_files: Option<usize>) -> String {
    let files = crate::walk::walk_repo(Path::new(repo), max_files);
    let rows: Vec<Value> = files
        .iter()
        .map(|f| {
            serde_json::json!({
                "path": f.path.display().to_string(),
                "relpath": f.relpath,
                "language": f.language,
                "size": f.size,
                "digest": f.digest,
                "is_test": f.is_test,
                "loc": f.loc,
                "tokens": f.tokens,
            })
        })
        .collect();
    dumps_vec(&rows)
}

#[pyfunction]
fn parse_source(language: &str, path_suffix: &str, source: &[u8]) -> String {
    let extracted = crate::parse::parse_source(language, path_suffix, source);
    let symbols: Vec<Value> = extracted
        .symbols
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "kind": s.kind,
                "start_line": s.start_line,
                "end_line": s.end_line,
                "signature": s.signature,
                "docstring": s.docstring,
                "body": s.body,
                "cyclomatic": s.cyclomatic,
                "cognitive": s.cognitive,
                "nesting": s.nesting,
                "nargs": s.nargs,
                "is_public": s.is_public,
                "is_entry": s.is_entry,
                "is_test": s.is_test,
            })
        })
        .collect();
    dumps(&serde_json::json!({
        "symbols": symbols,
        "imports": extracted.imports,
        "calls": extracted.calls,
        "import_modules": extracted.import_modules,
        "ast_nodes": extracted.ast_nodes,
    }))
}

#[pyfunction]
#[pyo3(signature = (repo, max_files, budget_seconds, with_git, with_embed, model_name))]
fn run_scan(
    repo: &str,
    max_files: Option<usize>,
    budget_seconds: Option<f64>,
    with_git: bool,
    with_embed: bool,
    model_name: &str,
) -> String {
    dumps(&crate::scan::run_scan(
        Path::new(repo),
        max_files,
        budget_seconds,
        with_git,
        with_embed,
        model_name,
    ))
}

#[pyfunction]
fn choose_call_targets(
    callee: &str,
    targets: Vec<(i64, i64)>,
    src_file_id: i64,
    langs: HashMap<i64, String>,
) -> Vec<(i64, i64)> {
    crate::graph::choose_call_targets(callee, &targets, src_file_id, &langs)
}

#[pyfunction]
fn resolve_and_store_edges(repo: &str) {
    with_conn(repo, crate::graph::resolve_and_store_edges);
}

#[pyfunction]
fn file_graph(repo: &str) -> String {
    with_conn(repo, |conn| {
        let (nodes, edges) = crate::graph::file_graph(conn);
        let mut node_list: Vec<i64> = nodes.into_iter().collect();
        node_list.sort();
        dumps(&serde_json::json!({"nodes": node_list, "edges": edges}))
    })
}

#[pyfunction]
#[pyo3(signature = (nodes, edges, personalize, damping, rounds))]
fn pagerank(
    nodes: Vec<i64>,
    edges: Vec<(i64, i64)>,
    personalize: Option<HashMap<i64, f64>>,
    damping: f64,
    rounds: usize,
) -> HashMap<i64, f64> {
    let set: HashSet<i64> = nodes.into_iter().collect();
    crate::graph::pagerank(&set, &edges, personalize.as_ref(), damping, rounds)
}

#[pyfunction]
fn dead_symbols(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::dead::dead_symbols(conn)))
}

#[pyfunction]
fn dead_files(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::dead::dead_files(conn)))
}

#[pyfunction]
fn dead_report(repo: &str) -> String {
    with_conn(repo, |conn| dumps(&crate::dead::dead_report(conn)))
}

#[pyfunction]
fn slice_symbol(repo: &str, query: &str, depth: i64) -> String {
    with_conn(repo, |conn| dumps(&crate::slice::slice_symbol(conn, query, depth)))
}

#[pyfunction]
fn apply_issues(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::issues::apply_issues(conn)))
}

#[pyfunction]
fn list_issues(repo: &str, limit: i64) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::issues::list_issues(conn, limit)))
}

#[pyfunction]
fn tokenize_name(name: &str) -> Vec<String> {
    crate::names::tokenize_name(name)
}

#[pyfunction]
fn analyze_names(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::names::analyze_names(conn)))
}

#[pyfunction]
fn find_clones(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::clones::find_clones(conn)))
}

#[pyfunction]
fn find_embed_clones(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::clones::find_embed_clones(conn)))
}

#[pyfunction]
fn label_clusters(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::clones::label_clusters(conn)))
}

#[pyfunction]
fn embed_symbols(repo: &str, model_name: &str, only_missing: bool) {
    with_conn(repo, |conn| crate::embed::embed_symbols(conn, model_name, only_missing));
}

#[pyfunction]
fn mismatch_flags(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::embed::mismatch_flags(conn)))
}

#[pyfunction]
fn similar_to(repo: &str, query: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::embed::similar_to(conn, query)))
}

#[pyfunction]
fn export_label_csv(repo: &str, dest: &str) -> usize {
    with_conn(repo, |conn| crate::embed::export_label_csv(conn, Path::new(dest)))
}

#[pyfunction]
fn use_stub(model_name: &str) -> bool {
    crate::embed::use_stub(model_name)
}

#[pyfunction]
fn compute_rank(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::rank::compute_rank(conn)))
}

#[pyfunction]
fn reading_order(repo: &str, limit: i64) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::rank::reading_order(conn, limit)))
}

#[pyfunction]
fn analyze_history(repo: &str) -> String {
    with_conn(repo, |conn| dumps(&crate::git_hist::analyze_history(Path::new(repo), conn)))
}

#[pyfunction]
fn history_report(repo: &str) -> String {
    with_conn(repo, |conn| dumps(&crate::git_hist::history_report(conn)))
}

#[pyfunction]
#[pyo3(signature = (repo, dest, slice_files=None))]
fn write_heatmap(repo: &str, dest: &str, slice_files: Option<Vec<String>>) -> String {
    with_conn(repo, |conn| {
        dumps(&crate::heatmap::write_heatmap(
            conn,
            Path::new(dest),
            slice_files.as_deref(),
        ))
    })
}

#[pyfunction]
fn write_report_dir(repo: &str, extra_json: &str) -> String {
    let extra: Value = serde_json::from_str(extra_json).unwrap_or(Value::Object(Default::default()));
    with_conn(repo, |conn| {
        crate::reports::write_report_dir(Path::new(repo), conn, extra)
            .display()
            .to_string()
    })
}

#[pyfunction]
fn refresh_report(repo: &str) -> String {
    with_conn(repo, |conn| {
        crate::reports::refresh_report(Path::new(repo), conn)
            .display()
            .to_string()
    })
}

#[pyfunction]
#[pyo3(signature = (repo, dest, slice_query=None))]
fn review_pack(repo: &str, dest: &str, slice_query: Option<&str>) -> String {
    with_conn(repo, |conn| {
        crate::review::review_pack(conn, Path::new(dest), slice_query)
            .display()
            .to_string()
    })
}

#[pyfunction]
fn ingest_profile(repo: &str, profile_path: &str) -> String {
    with_conn(repo, |conn| dumps(&crate::profile::ingest_profile(conn, Path::new(profile_path))))
}

#[pyfunction]
#[pyo3(signature = (repo, lang, pid, command, dest, duration))]
fn launch_or_attach(
    repo: &str,
    lang: &str,
    pid: Option<i32>,
    command: Vec<String>,
    dest: &str,
    duration: i32,
) -> PyResult<String> {
    crate::profile::launch_or_attach(Path::new(repo), lang, pid, &command, Path::new(dest), duration)
        .map(|p| p.display().to_string())
        .map_err(PyRuntimeError::new_err)
}

#[pyfunction]
fn package_map(repo: &str) -> String {
    dumps_vec(&crate::inventory::package_map(Path::new(repo)))
}

#[pyfunction]
fn inventory(repo: &str) -> String {
    with_conn(repo, |conn| dumps(&crate::inventory::inventory(conn, Path::new(repo))))
}

#[pyfunction]
fn tag_effects(body: &str) -> Vec<String> {
    crate::effects::tag_effects(body)
}

#[pyfunction]
fn apply_effects(repo: &str) {
    with_conn(repo, crate::effects::apply_effects);
}

#[pyfunction]
fn tag_risks(name: &str, body: &str, signature: &str) -> Vec<String> {
    crate::risks::tag_risks(name, body, signature)
}

#[pyfunction]
fn apply_risks(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::risks::apply_risks(conn)))
}

#[pyfunction]
fn cost_hint(body: &str) -> i64 {
    crate::cost::cost_hint(body)
}

#[pyfunction]
fn apply_cost_hints(repo: &str) {
    with_conn(repo, crate::cost::apply_cost_hints);
}

#[pyfunction]
fn is_hollow(body: &str, docstring: &str) -> String {
    crate::hollow::is_hollow(body, docstring)
}

#[pyfunction]
fn hollow_symbols(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::hollow::hollow_symbols(conn)))
}

#[pyfunction]
fn apply_hollow(repo: &str) -> String {
    with_conn(repo, |conn| dumps_vec(&crate::hollow::apply_hollow(conn)))
}

#[pyfunction]
fn fingerprint_symbol(
    name: &str,
    effects: Vec<String>,
    fan_in: i64,
    fan_out: i64,
    cyclomatic: i64,
    cognitive: i64,
    callees: Vec<String>,
) -> String {
    crate::fingerprints::fingerprint_symbol(name, &effects, fan_in, fan_out, cyclomatic, cognitive, &callees)
}

#[pyfunction]
fn fingerprint_overlap(a: &str, b: &str) -> f64 {
    crate::fingerprints::fingerprint_overlap(a, b)
}

#[pyfunction]
fn apply_fingerprints(repo: &str) {
    with_conn(repo, crate::fingerprints::apply_fingerprints);
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    set_python_layout();
    m.add("DEFAULT_MODEL", crate::embed::DEFAULT_MODEL)?;
    m.add_function(wrap_pyfunction!(init_index, m)?)?;
    m.add_function(wrap_pyfunction!(walk_repo, m)?)?;
    m.add_function(wrap_pyfunction!(parse_source, m)?)?;
    m.add_function(wrap_pyfunction!(run_scan, m)?)?;
    m.add_function(wrap_pyfunction!(choose_call_targets, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_and_store_edges, m)?)?;
    m.add_function(wrap_pyfunction!(file_graph, m)?)?;
    m.add_function(wrap_pyfunction!(pagerank, m)?)?;
    m.add_function(wrap_pyfunction!(dead_symbols, m)?)?;
    m.add_function(wrap_pyfunction!(dead_files, m)?)?;
    m.add_function(wrap_pyfunction!(dead_report, m)?)?;
    m.add_function(wrap_pyfunction!(slice_symbol, m)?)?;
    m.add_function(wrap_pyfunction!(apply_issues, m)?)?;
    m.add_function(wrap_pyfunction!(list_issues, m)?)?;
    m.add_function(wrap_pyfunction!(tokenize_name, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_names, m)?)?;
    m.add_function(wrap_pyfunction!(find_clones, m)?)?;
    m.add_function(wrap_pyfunction!(find_embed_clones, m)?)?;
    m.add_function(wrap_pyfunction!(label_clusters, m)?)?;
    m.add_function(wrap_pyfunction!(embed_symbols, m)?)?;
    m.add_function(wrap_pyfunction!(mismatch_flags, m)?)?;
    m.add_function(wrap_pyfunction!(similar_to, m)?)?;
    m.add_function(wrap_pyfunction!(export_label_csv, m)?)?;
    m.add_function(wrap_pyfunction!(use_stub, m)?)?;
    m.add_function(wrap_pyfunction!(compute_rank, m)?)?;
    m.add_function(wrap_pyfunction!(reading_order, m)?)?;
    m.add_function(wrap_pyfunction!(analyze_history, m)?)?;
    m.add_function(wrap_pyfunction!(history_report, m)?)?;
    m.add_function(wrap_pyfunction!(write_heatmap, m)?)?;
    m.add_function(wrap_pyfunction!(write_report_dir, m)?)?;
    m.add_function(wrap_pyfunction!(refresh_report, m)?)?;
    m.add_function(wrap_pyfunction!(review_pack, m)?)?;
    m.add_function(wrap_pyfunction!(ingest_profile, m)?)?;
    m.add_function(wrap_pyfunction!(launch_or_attach, m)?)?;
    m.add_function(wrap_pyfunction!(package_map, m)?)?;
    m.add_function(wrap_pyfunction!(inventory, m)?)?;
    m.add_function(wrap_pyfunction!(tag_effects, m)?)?;
    m.add_function(wrap_pyfunction!(apply_effects, m)?)?;
    m.add_function(wrap_pyfunction!(tag_risks, m)?)?;
    m.add_function(wrap_pyfunction!(apply_risks, m)?)?;
    m.add_function(wrap_pyfunction!(cost_hint, m)?)?;
    m.add_function(wrap_pyfunction!(apply_cost_hints, m)?)?;
    m.add_function(wrap_pyfunction!(is_hollow, m)?)?;
    m.add_function(wrap_pyfunction!(hollow_symbols, m)?)?;
    m.add_function(wrap_pyfunction!(apply_hollow, m)?)?;
    m.add_function(wrap_pyfunction!(fingerprint_symbol, m)?)?;
    m.add_function(wrap_pyfunction!(fingerprint_overlap, m)?)?;
    m.add_function(wrap_pyfunction!(apply_fingerprints, m)?)?;
    Ok(())
}
