use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection};

fn effect_patterns() -> &'static [(&'static str, Regex)] {
    static PATS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATS.get_or_init(|| {
        [
            ("filesystem", r"\b(open|read_file|write_file|Path\(|fs\.|fs::|std::fs|tokio::fs|File::)\b"),
            ("network", r"\b(requests\.|httpx|fetch\(|axios|ureq|reqwest|hyper::|websocket)\b"),
            ("db", r"(?i)\b(execute\(|query\(|sqlite3|sqlalchemy|prisma|diesel|sqlx)\b"),
            ("process", r"\b(subprocess|os\.system|child_process|Command::|std::process)\b"),
            ("global_mutate", r"\bglobal\b|static mut\b"),
            ("unsafe", r"\bunsafe\b|\bany\b|\beval\("),
            ("panic", r"\b(unwrap\(|expect\(|panic!|raise |throw )\b"),
        ]
        .into_iter()
        .map(|(n, p)| (n, Regex::new(p).unwrap()))
        .collect()
    })
}

pub fn tag_effects(body: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for (name, pat) in effect_patterns() {
        if pat.is_match(body) {
            tags.push((*name).to_string());
        }
    }
    tags
}

pub fn apply_effects(conn: &Connection) {
    let mut stmt = conn.prepare("SELECT id, body FROM symbols").unwrap();
    let rows: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().flatten().collect();
    drop(stmt);
    let mut update = conn.prepare("UPDATE symbols SET effects = ? WHERE id = ?").unwrap();
    let mut bar = crate::progress::Bar::new("effects", rows.len());
    for (id, body) in rows {
        let tags = serde_json::to_string(&tag_effects(&body)).unwrap();
        update.execute(params![tags, id]).ok();
        bar.tick("");
    }
    bar.finish();
}
