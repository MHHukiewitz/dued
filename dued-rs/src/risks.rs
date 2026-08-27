use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn risk_patterns() -> &'static [(&'static str, Regex)] {
    static PATS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATS.get_or_init(|| {
        [
            ("auth", r"(?i)(password|passwd|oauth|jwt|session|login|authn|secret)"),
            ("crypto", r"(?i)\b(encrypt|decrypt|hashlib|hmac|aes|rsa|fernet|bcrypt|sha256)\b"),
            ("parser", r"(?i)\b(parse|parser|grammar|ast|yaml\.load|pickle\.loads)\b"),
            ("money", r"(?i)\b(price|invoice|payment|currency|ledger|balance)\b"),
            ("migration", r"\b(alembic|migrate|schema_version|ALTER TABLE)\b"),
            ("unsafe", r"\bunsafe\b|\beval\(|\bexec\(|pickle\.loads"),
            ("any_type", r":\s*Any\b|\bas any\b|\bany\b"),
        ]
        .into_iter()
        .map(|(n, p)| (n, Regex::new(p).unwrap()))
        .collect()
    })
}

pub fn tag_risks(name: &str, body: &str, signature: &str) -> Vec<String> {
    let text = format!("{name}\n{signature}\n{body}");
    let mut tags = Vec::new();
    for (label, pat) in risk_patterns() {
        if pat.is_match(&text) {
            tags.push((*label).to_string());
        }
    }
    tags
}

pub fn apply_risks(conn: &Connection) -> Vec<Value> {
    let mut found = Vec::new();
    let mut stmt = conn.prepare("SELECT id, name, body, signature FROM symbols").unwrap();
    let rows: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .flatten()
        .collect();
    drop(stmt);
    let mut update = conn.prepare("UPDATE symbols SET risks = ? WHERE id = ?").unwrap();
    let mut bar = crate::progress::Bar::new("risks", rows.len());
    for (id, name, body, signature) in rows {
        let tags = tag_risks(&name, &body, &signature);
        update
            .execute(params![serde_json::to_string(&tags).unwrap(), id])
            .ok();
        if !tags.is_empty() {
            found.push(json!({"id": id, "name": name, "risks": tags}));
        }
        bar.tick(&name);
    }
    bar.finish();
    found
}
