use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};

const GENERIC_CALLEES: &[&str] = &[
    "new",
    "clone",
    "clone_from",
    "default",
    "from",
    "into",
    "try_from",
    "try_into",
    "fmt",
    "drop",
    "hash",
    "eq",
    "ne",
    "cmp",
    "partial_cmp",
    "as_ref",
    "as_mut",
    "borrow",
    "borrow_mut",
    "deref",
    "deref_mut",
    "index",
    "index_mut",
    "push",
    "pop",
    "get",
    "get_mut",
    "set",
    "insert",
    "remove",
    "len",
    "is_empty",
    "as_str",
    "to_string",
    "to_owned",
    "to_vec",
    "into_iter",
    "iter",
    "next",
    "map",
    "filter",
    "collect",
    "unwrap",
    "expect",
    "ok",
    "err",
    "update",
    "build",
    "create",
    "init",
    "parse",
    "load",
    "save",
    "write",
    "read",
    "send",
    "recv",
    "lock",
];

pub fn is_generic_callee(name: &str) -> bool {
    GENERIC_CALLEES.iter().any(|g| g.eq_ignore_ascii_case(name))
}

pub fn choose_call_targets(
    callee: &str,
    targets: &[(i64, i64)],
    src_file_id: i64,
    lang_by_file: &HashMap<i64, String>,
) -> Vec<(i64, i64)> {
    let same_file: Vec<(i64, i64)> = targets.iter().copied().filter(|t| t.1 == src_file_id).collect();
    if same_file.len() == 1 {
        return same_file;
    }
    if same_file.len() > 1 {
        return Vec::new();
    }
    if targets.is_empty() {
        return Vec::new();
    }
    let src_lang = lang_by_file.get(&src_file_id);
    let same_lang: Vec<(i64, i64)> = targets
        .iter()
        .copied()
        .filter(|t| lang_by_file.get(&t.1) == src_lang)
        .collect();
    let pool = if same_lang.is_empty() {
        targets.to_vec()
    } else {
        same_lang
    };
    if is_generic_callee(callee) || pool.len() > 1 {
        return Vec::new();
    }
    pool
}

pub fn resolve_and_store_edges(conn: &Connection) {
    conn.execute("DELETE FROM edges", []).ok();
    let mut name_to_symbols: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT id, file_id, name FROM symbols").unwrap();
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))).unwrap();
    for row in rows.flatten() {
        name_to_symbols.entry(row.2).or_default().push((row.0, row.1));
    }
    let mut files: Vec<(i64, String, String)> = Vec::new();
    let mut stmt = conn.prepare("SELECT id, relpath, language FROM files").unwrap();
    let frows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    for row in frows.flatten() {
        files.push(row);
    }
    let lang_by_file: HashMap<i64, String> = files.iter().map(|(id, _, lang)| (*id, lang.clone())).collect();

    let mut stmt = conn.prepare("SELECT src_file_id, src_symbol_id, callee FROM call_facts").unwrap();
    let calls: Vec<(i64, i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .flatten()
        .collect();
    drop(stmt);
    let mut insert_edge = conn
        .prepare("INSERT INTO edges(src_file_id, src_symbol_id, dst_file_id, dst_name, kind) VALUES (?,?,?,?,?)")
        .unwrap();
    let mut call_bar = crate::progress::Bar::new("graph calls", calls.len());
    for (src_file_id, src_symbol_id, callee) in calls {
        call_bar.tick(&callee);
        let targets = name_to_symbols.get(&callee).cloned().unwrap_or_default();
        if targets.is_empty() {
            insert_edge
                .execute(params![src_file_id, src_symbol_id, None::<i64>, callee, "call"])
                .ok();
            continue;
        }
        let chosen = choose_call_targets(&callee, &targets, src_file_id, &lang_by_file);
        if chosen.is_empty() {
            insert_edge
                .execute(params![src_file_id, src_symbol_id, None::<i64>, callee, "call"])
                .ok();
            continue;
        }
        for (_sid, dst_file_id) in chosen {
            insert_edge
                .execute(params![src_file_id, src_symbol_id, dst_file_id, callee, "call"])
                .ok();
        }
    }
    call_bar.finish();

    let mut stmt = conn.prepare("SELECT src_file_id, module_hint FROM import_facts").unwrap();
    let imports: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().flatten().collect();
    drop(stmt);
    let mut import_bar = crate::progress::Bar::new("imports", imports.len());
    for (src_file_id, module_hint) in imports {
        let hint = module_hint.replace('\\', "/").trim_matches(|c| c == '"' || c == '\'').to_string();
        let stem = hint.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
        for (id, rel, _) in &files {
            if !stem.is_empty()
                && (rel.ends_with(&format!("/{stem}.py"))
                    || rel.ends_with(&format!("/{stem}.ts"))
                    || rel.ends_with(&format!("/{stem}.rs"))
                    || rel == &format!("{stem}.py"))
            {
                insert_edge
                    .execute(params![src_file_id, None::<i64>, *id, stem, "import"])
                    .ok();
            }
        }
        import_bar.tick(&hint);
    }
    import_bar.finish();
    drop(insert_edge);

    conn.execute("UPDATE symbols SET fan_in = 0, fan_out = 0", []).ok();
    let mut fan_in: HashMap<i64, i64> = HashMap::new();
    let mut fan_out: HashMap<i64, i64> = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT src_symbol_id, dst_file_id, dst_name FROM edges WHERE kind='call'")
        .unwrap();
    let edges = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    for row in edges.flatten() {
        if let Some(sid) = row.0 {
            *fan_out.entry(sid).or_insert(0) += 1;
        }
        let Some(dst_file_id) = row.1 else {
            continue;
        };
        let Some(targets) = name_to_symbols.get(&row.2) else {
            continue;
        };
        let matches: Vec<i64> = targets.iter().filter(|(_, fid)| *fid == dst_file_id).map(|(sid, _)| *sid).collect();
        if matches.len() == 1 {
            *fan_in.entry(matches[0]).or_insert(0) += 1;
        }
    }
    drop(stmt);
    let mut update_in = conn.prepare("UPDATE symbols SET fan_in = ? WHERE id = ?").unwrap();
    let mut fan_bar = crate::progress::Bar::new("fan-in/out", fan_in.len() + fan_out.len());
    for (sid, value) in fan_in {
        update_in.execute(params![value, sid]).ok();
        fan_bar.tick("");
    }
    drop(update_in);
    let mut update_out = conn.prepare("UPDATE symbols SET fan_out = ? WHERE id = ?").unwrap();
    for (sid, value) in fan_out {
        update_out.execute(params![value, sid]).ok();
        fan_bar.tick("");
    }
    fan_bar.finish();
}

pub fn file_graph(conn: &Connection) -> (HashSet<i64>, Vec<(i64, i64)>) {
    let mut nodes = HashSet::new();
    let mut stmt = conn.prepare("SELECT id FROM files").unwrap();
    for id in stmt.query_map([], |r| r.get::<_, i64>(0)).unwrap().flatten() {
        nodes.insert(id);
    }
    let mut edges = Vec::new();
    let mut stmt = conn.prepare("SELECT src_file_id, dst_file_id FROM edges WHERE dst_file_id IS NOT NULL").unwrap();
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).unwrap().flatten() {
        if row.0 != row.1 {
            edges.push(row);
        }
    }
    (nodes, edges)
}

pub fn pagerank(
    nodes: &HashSet<i64>,
    edges: &[(i64, i64)],
    personalize: Option<&HashMap<i64, f64>>,
    damping: f64,
    rounds: usize,
) -> HashMap<i64, f64> {
    if nodes.is_empty() {
        return HashMap::new();
    }
    let mut incoming: HashMap<i64, Vec<i64>> = nodes.iter().map(|n| (*n, Vec::new())).collect();
    let mut outdeg: HashMap<i64, i64> = nodes.iter().map(|n| (*n, 0)).collect();
    for (src, dst) in edges {
        if nodes.contains(src) && nodes.contains(dst) {
            incoming.entry(*dst).or_default().push(*src);
            *outdeg.entry(*src).or_insert(0) += 1;
        }
    }
    let n = nodes.len() as f64;
    let mut base: HashMap<i64, f64> = nodes.iter().map(|id| (*id, 1.0 / n)).collect();
    if let Some(pers) = personalize {
        let total: f64 = nodes.iter().map(|id| pers.get(id).copied().unwrap_or(0.0)).sum::<f64>().max(1.0);
        base = nodes.iter().map(|id| (*id, pers.get(id).copied().unwrap_or(0.0) / total)).collect();
    }
    let mut score = base.clone();
    for _ in 0..rounds {
        let mut nxt = HashMap::new();
        for nid in nodes {
            let mut inbound = 0.0;
            for src in incoming.get(nid).unwrap_or(&Vec::new()) {
                let deg = outdeg.get(src).copied().unwrap_or(0).max(1) as f64;
                inbound += score.get(src).copied().unwrap_or(0.0) / deg;
            }
            nxt.insert(*nid, (1.0 - damping) * base.get(nid).copied().unwrap_or(0.0) + damping * inbound);
        }
        score = nxt;
    }
    score
}

pub fn _optional_file_id(conn: &Connection, relpath: &str) -> Option<i64> {
    conn.query_row("SELECT id FROM files WHERE relpath = ?", [relpath], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn langs() -> HashMap<i64, String> {
        HashMap::from([(1, "rust".into()), (2, "rust".into()), (3, "python".into())])
    }

    #[test]
    fn new_does_not_fan_out_across_files() {
        let targets = vec![(10, 1), (11, 2)];
        let chosen = choose_call_targets("new", &targets, 1, &langs());
        assert_eq!(chosen, vec![(10, 1)]);
        let chosen = choose_call_targets("new", &targets, 9, &langs());
        assert!(chosen.is_empty());
    }

    #[test]
    fn multiple_same_file_overloads_stay_unresolved() {
        let targets = vec![(10, 1), (12, 1), (11, 2)];
        let chosen = choose_call_targets("new", &targets, 1, &langs());
        assert!(chosen.is_empty());
    }

    #[test]
    fn unique_cross_file_name_resolves() {
        let targets = vec![(20, 2)];
        let chosen = choose_call_targets("get_user", &targets, 1, &langs());
        assert_eq!(chosen, vec![(20, 2)]);
    }

    #[test]
    fn ambiguous_non_generic_stays_unresolved() {
        let targets = vec![(20, 2), (21, 2)];
        let chosen = choose_call_targets("process", &targets, 1, &langs());
        assert!(chosen.is_empty());
    }
}
