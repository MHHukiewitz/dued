use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};
use serde_json::{json, Value};

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

fn token_clone_score(a: &str, b: &str) -> f64 {
    let sa = shingles(&tokens(a), 5);
    let sb = shingles(&tokens(b), 5);
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    inter / union
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
    let mut buckets: HashMap<String, Vec<(i64, String, String, String)>> = HashMap::new();
    for row in rows {
        let toks = tokens(&row.2);
        let key = toks.first().cloned().unwrap_or_else(|| row.1.clone());
        buckets.entry(key.chars().take(12).collect()).or_default().push(row);
    }
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut bar = crate::progress::Bar::new("clones", buckets.len());
    for group in buckets.values() {
        for i in 0..group.len() {
            for b in group.iter().skip(i + 1) {
                let pair = (group[i].0.min(b.0), group[i].0.max(b.0));
                if seen.contains(&pair) {
                    continue;
                }
                let score = token_clone_score(&group[i].2, &b.2);
                if score < 0.55 {
                    continue;
                }
                seen.insert(pair);
                conn.execute(
                    "INSERT INTO clones(symbol_a, symbol_b, score, method) VALUES (?,?,?,?)",
                    params![group[i].0, b.0, score, "token"],
                )
                .ok();
                found.push(json!({
                    "a": format!("{}::{}", group[i].3, group[i].1),
                    "b": format!("{}::{}", b.3, b.1),
                    "score": score,
                    "method": "token",
                }));
            }
        }
        bar.tick("");
    }
    bar.finish();
    found
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
    let mut bar = crate::progress::Bar::new("embed clones", rows.len());
    for i in 0..rows.len() {
        bar.tick(&rows[i].1);
        for b in rows.iter().skip(i + 1) {
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
