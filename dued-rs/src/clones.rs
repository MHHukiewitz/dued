use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};
use serde_json::{json, Value};

/// Bodies longer than this are truncated for token-clone scoring only.
/// Full bodies stay in `symbols`; this keeps Jaccard work bounded.
const MAX_CLONE_BODY_CHARS: usize = 4_096;

/// Hard cap on pairwise Jaccard comparisons per scan.
/// Prevents O(n²) stalls when many symbols share a coarse bucket key.
const MAX_CLONE_COMPARISONS: usize = 25_000;

/// Cap symbols compared inside one bucket (pairwise within the cap).
const MAX_BUCKET_SYMBOLS: usize = 200;

const SKIP_LEAD_TOKENS: &[&str] = &[
    "pub", "fn", "async", "unsafe", "const", "static", "extern", "crate", "super",
    "def", "class", "export", "function", "type", "interface", "struct", "enum", "impl",
    "mod", "use", "let", "mut", "return", "the", "a", "self",
];

fn tokens(text: &str) -> Vec<String> {
    let mut buf = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            buf.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        buf.push(current);
    }
    buf.into_iter()
        .filter(|t| !matches!(t.as_str(), "self" | "return" | "the" | "a"))
        .collect()
}

fn truncate_body(body: &str) -> &str {
    if body.len() <= MAX_CLONE_BODY_CHARS {
        return body;
    }
    let mut end = MAX_CLONE_BODY_CHARS;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    &body[..end]
}

fn shingles(toks: &[String], size: usize) -> HashSet<String> {
    if toks.len() < size {
        return if toks.is_empty() {
            HashSet::new()
        } else {
            HashSet::from([toks.join(" ")])
        };
    }
    (0..=toks.len() - size).map(|i| toks[i..i + size].join(" ")).collect()
}

fn jaccard(sa: &HashSet<String>, sb: &HashSet<String>) -> f64 {
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(sb).count() as f64;
    let union = sa.union(sb).count() as f64;
    inter / union
}

/// Prefer a content token over language keywords so Rust `pub fn …` bodies
/// do not all collapse into one giant `"pub"` bucket.
fn bucket_key(name: &str, toks: &[String]) -> String {
    for t in toks {
        if !SKIP_LEAD_TOKENS.contains(&t.as_str()) {
            return t.chars().take(12).collect();
        }
    }
    name.chars().take(12).collect()
}

struct CloneRow {
    id: i64,
    name: String,
    relpath: String,
    shingles: HashSet<String>,
}

pub fn find_clones(conn: &Connection) -> Vec<Value> {
    conn.execute("DELETE FROM clones", []).ok();
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.body, f.relpath
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE length(s.body) > 80
        "#,
        )
        .unwrap();
    let rows: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .flatten()
        .collect();

    let mut buckets: HashMap<String, Vec<CloneRow>> = HashMap::new();
    for (id, name, body, relpath) in rows {
        let toks = tokens(truncate_body(&body));
        let key = bucket_key(&name, &toks);
        let shingles = shingles(&toks, 5);
        if shingles.is_empty() {
            continue;
        }
        buckets.entry(key).or_default().push(CloneRow {
            id,
            name,
            relpath,
            shingles,
        });
    }

    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut comparisons = 0usize;
    let pair_budget = MAX_CLONE_COMPARISONS.min(estimate_pairs(&buckets)).max(1);
    let mut bar = crate::progress::Bar::new("clones", pair_budget);
    'outer: for group in buckets.values() {
        let limit = group.len().min(MAX_BUCKET_SYMBOLS);
        for i in 0..limit {
            for j in (i + 1)..limit {
                if comparisons >= MAX_CLONE_COMPARISONS {
                    break 'outer;
                }
                comparisons += 1;
                if comparisons % 64 == 0 {
                    bar.set(comparisons.min(pair_budget), "");
                }
                let a = &group[i];
                let b = &group[j];
                let pair = (a.id.min(b.id), a.id.max(b.id));
                if seen.contains(&pair) {
                    continue;
                }
                let score = jaccard(&a.shingles, &b.shingles);
                if score < 0.55 {
                    continue;
                }
                seen.insert(pair);
                conn.execute(
                    "INSERT INTO clones(symbol_a, symbol_b, score, method) VALUES (?,?,?,?)",
                    params![a.id, b.id, score, "token"],
                )
                .ok();
                found.push(json!({
                    "a": format!("{}::{}", a.relpath, a.name),
                    "b": format!("{}::{}", b.relpath, b.name),
                    "score": score,
                    "method": "token",
                }));
            }
        }
    }
    bar.set(comparisons.min(pair_budget), "");
    bar.finish();
    found
}

fn estimate_pairs(buckets: &HashMap<String, Vec<CloneRow>>) -> usize {
    let mut n = 0usize;
    for group in buckets.values() {
        let m = group.len().min(MAX_BUCKET_SYMBOLS);
        n = n.saturating_add(m.saturating_mul(m.saturating_sub(1)) / 2);
        if n >= MAX_CLONE_COMPARISONS {
            return MAX_CLONE_COMPARISONS;
        }
    }
    n.max(1)
}

pub fn find_embed_clones(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.embed_body, f.relpath
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.embed_body IS NOT NULL AND length(s.body) > 80
        "#,
        )
        .unwrap();
    let rows: Vec<(i64, String, Vec<u8>, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .flatten()
        .collect();
    let mut found = Vec::new();
    let mut comparisons = 0usize;
    let mut bar = crate::progress::Bar::new("embed clones", rows.len());
    for i in 0..rows.len() {
        bar.tick(&rows[i].1);
        for b in rows.iter().skip(i + 1) {
            if comparisons >= MAX_CLONE_COMPARISONS {
                bar.finish();
                return found;
            }
            comparisons += 1;
            let score = cosine(&rows[i].2, &b.2);
            if score < 0.92 {
                continue;
            }
            conn.execute(
                "INSERT INTO clones(symbol_a, symbol_b, score, method) VALUES (?,?,?,?)",
                params![rows[i].0, b.0, score, "embed"],
            )
            .ok();
            found.push(json!({
                "a": format!("{}::{}", rows[i].3, rows[i].1),
                "b": format!("{}::{}", b.3, b.1),
                "score": score,
                "method": "embed",
            }));
        }
    }
    bar.finish();
    found
}

fn cosine(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.len() % 4 != 0 {
        return 0.0;
    }
    let fa: Vec<f32> = a.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    let fb: Vec<f32> = b.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    let dot: f32 = fa.iter().zip(&fb).map(|(x, y)| x * y).sum();
    let na = fa.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = fb.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na * nb)) as f64
    }
}

pub fn label_clusters(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT s.id, s.name, s.embed_body, f.relpath
        FROM symbols s JOIN files f ON f.id = s.file_id
        WHERE s.embed_body IS NOT NULL
        "#,
        )
        .unwrap();
    let rows: Vec<(i64, String, Vec<u8>, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .flatten()
        .collect();
    if rows.len() < 4 {
        return Vec::new();
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for row in &rows {
        for t in tokens(&row.1) {
            *counts.entry(t).or_insert(0) += 1;
        }
    }
    let mut common: Vec<(String, usize)> = counts.into_iter().collect();
    common.sort_by(|a, b| b.1.cmp(&a.1));
    let label = common.iter().take(3).map(|(t, _)| t.as_str()).collect::<Vec<_>>().join(" ");
    vec![json!({
        "id": 0,
        "label": if label.is_empty() { "cluster-0".into() } else { label },
        "size": rows.len(),
        "members": rows.iter().take(12).map(|r| format!("{}::{}", r.3, r.1)).collect::<Vec<_>>(),
    })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::connect;
    use std::fs;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    fn temp_repo(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let repo = std::env::temp_dir().join(format!("dued-clones-{label}-{nanos}"));
        fs::create_dir_all(&repo).unwrap();
        repo
    }

    fn seed_pub_bucket(conn: &Connection, n: usize, body_pad: usize) {
        conn.execute(
            "INSERT INTO files(relpath, language, digest, loc, size, is_test, tokens, ast_nodes) \
             VALUES ('big.rs', 'rust', 'd', 1, 1, 0, 1, 1)",
            [],
        )
        .unwrap();
        let file_id = conn.last_insert_rowid();
        let pad = "x".repeat(body_pad);
        for i in 0..n {
            let body = format!(
                "pub fn helper_{i}(a: i32) -> i32 {{\n    let v = a + {i};\n    {pad}\n    v\n}}\n"
            );
            conn.execute(
                "INSERT INTO symbols(file_id, name, kind, start_line, end_line, signature, docstring, body, \
                 cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test) \
                 VALUES (?, ?, 'function', 1, 10, '', '', ?, 1, 1, 1, 1, 1, 0, 0)",
                params![file_id, format!("helper_{i}"), body],
            )
            .unwrap();
        }
    }

    #[test]
    fn token_clones_finish_on_large_pub_bucket() {
        let repo = temp_repo("pub-bucket");
        let conn = connect(&repo);
        // Without bounds this is O(n²) Jaccard on long bodies and stalls for minutes.
        seed_pub_bucket(&conn, 400, 2_000);
        let started = Instant::now();
        let _ = find_clones(&conn);
        assert!(
            started.elapsed().as_secs() < 15,
            "find_clones took {:?}",
            started.elapsed()
        );
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn bucket_key_skips_pub_fn() {
        let toks = tokens("pub fn apply_purchase_wholesale(state: &mut GameState) { let x = 1; }");
        let key = bucket_key("apply_purchase_wholesale", &toks);
        assert_ne!(key, "pub");
        assert_ne!(key, "fn");
        assert!(key.starts_with("apply") || key == "apply_purcha");
    }
}
