use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection};

fn cost_regexes() -> &'static (Regex, Regex, Regex) {
    static RE: OnceLock<(Regex, Regex, Regex)> = OnceLock::new();
    RE.get_or_init(|| {
        (
            Regex::new(r"\b(for|while|loop)\b").unwrap(),
            Regex::new(r"\b(clone\(|\.clone\(|to_vec\(|to_owned\(|Vec::|malloc|alloc)\b").unwrap(),
            Regex::new(r"\b(open\(|fetch\(|read_to_string|execute\(|json\.loads)\b").unwrap(),
        )
    })
}

pub fn cost_hint(body: &str) -> i64 {
    let (loops, alloc, io) = cost_regexes();
    loops.find_iter(body).count() as i64 + alloc.find_iter(body).count() as i64 + 2 * io.find_iter(body).count() as i64
}

pub fn apply_cost_hints(conn: &Connection) {
    let mut stmt = conn.prepare("SELECT id, body FROM symbols").unwrap();
    let rows: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().flatten().collect();
    drop(stmt);
    let mut update = conn.prepare("UPDATE symbols SET cost_hint = ? WHERE id = ?").unwrap();
    let mut bar = crate::progress::Bar::new("cost", rows.len());
    for (id, body) in rows {
        update.execute(params![cost_hint(&body), id]).ok();
        bar.tick("");
    }
    bar.finish();
}
