use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection};

fn effect_patterns() -> &'static [(&'static str, Regex)] {
    static PATS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATS.get_or_init(|| {
        [
            // Bare English "open" is not filesystem; require open( / File:: / OpenOptions / fs paths.
            ("filesystem", r"\b(open\(|read_file|write_file|Path\(|fs\.|fs::|std::fs|tokio::fs|File::|OpenOptions)\b"),
            ("network", r"\b(requests\.|httpx|fetch\(|axios|ureq|reqwest|hyper::|websocket)\b"),
            ("db", r"(?i)\b(execute\(|query\(|sqlite3|sqlalchemy|prisma|diesel|sqlx)\b"),
            ("process", r"\b(subprocess|os\.system|child_process|Command::|std::process)\b"),
            // Python `global` statement only; comment text like "global customer" must not match.
            ("global_mutate", r"(?m)^\s*global\b|static mut\b"),
            ("unsafe", r"\bunsafe\b|\beval\("),
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

#[cfg(test)]
mod tests {
    use super::tag_effects;

    #[test]
    fn iterator_any_is_not_unsafe() {
        let body = "fn sitting_isp_kit_backhaul_floor(xs: &[u8]) -> bool { xs.iter().any(|x| *x > 0) }";
        let tags = tag_effects(body);
        assert!(!tags.iter().any(|t| t == "unsafe"), "{tags:?}");
    }

    #[test]
    fn real_unsafe_block_is_tagged() {
        let body = "fn poke(p: *const u8) -> u8 { unsafe { *p } }";
        let tags = tag_effects(body);
        assert!(tags.iter().any(|t| t == "unsafe"), "{tags:?}");
    }

    #[test]
    fn english_open_in_format_is_not_filesystem() {
        let body = r#"fn dispatch_company_command(technology: &str) { format!("flipped {technology} open"); }"#;
        let tags = tag_effects(body);
        assert!(!tags.iter().any(|t| t == "filesystem"), "{tags:?}");
    }

    #[test]
    fn file_open_and_open_options_are_filesystem() {
        let file_open = "fn load() { let _ = std::fs::File::open(\"x\"); }";
        let opts = "fn load() { let _ = std::fs::OpenOptions::new().read(true).open(\"x\"); }";
        assert!(tag_effects(file_open).iter().any(|t| t == "filesystem"), "{:?}", tag_effects(file_open));
        assert!(tag_effects(opts).iter().any(|t| t == "filesystem"), "{:?}", tag_effects(opts));
    }

    #[test]
    fn comment_global_is_not_global_mutate() {
        let body = "fn update_customer_growth() {\n    // Calculate global customer counts\n    let n = 1;\n}";
        let tags = tag_effects(body);
        assert!(!tags.iter().any(|t| t == "global_mutate"), "{tags:?}");
    }

    #[test]
    fn python_global_statement_is_global_mutate() {
        let body = "def f():\n    global x\n    x = 1\n";
        let tags = tag_effects(body);
        assert!(tags.iter().any(|t| t == "global_mutate"), "{tags:?}");
    }
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
