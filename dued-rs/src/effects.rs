use std::sync::OnceLock;

use regex::Regex;
use rusqlite::{params, Connection};

fn effect_patterns() -> &'static [(&'static str, Regex)] {
    static PATS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATS.get_or_init(|| {
        [
            // Bare English "open" is not filesystem; require File:: / OpenOptions / fs paths. Calls to
            // `open(` are checked separately in `open_call_is_filesystem` so a local binding named
            // `open` (closure, variable or parameter) is not read as a file open.
            // `fs.` alone also matches a local variable named `fs` (e.g. fs.remaining_days); require a
            // known Node fs API name after `fs.` so plain field/method access on an `fs` binding is skipped.
            ("filesystem", r"\b(read_file|write_file|Path\(|fs\.(promises|createReadStream|createWriteStream|watch|watchFile|(readFile|writeFile|appendFile|exists|mkdir|rmdir|rm|unlink|readdir|stat|lstat|copyFile|rename|open|close|chmod|chown|truncate|access|realpath|symlink|readlink|link)(Sync)?)|fs::|std::fs|tokio::fs|File::|OpenOptions)\b"),
            ("network", r"\b(requests\.|httpx|fetch\(|axios|ureq|reqwest|hyper::|websocket)\b"),
            // Bare `.query(` is not db (e.g. SpatialIndex); require DB APIs / sqlx `query!`.
            // No trailing \b after execute(/query!( — next char is often "(" or a quote.
            ("db", r"(?i)\b(sqlite3|sqlalchemy|prisma|diesel|sqlx)\b|\bexecute\(|query!\("),
            // Bare `Command::` enum arms and `std::process::id()` are not process spawn.
            ("process", r"\b(subprocess|os\.system|child_process|std::process::Command|Command::new)\b"),
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

/// A call written `open(`. Group 1 is the character before `open` so `.open(` and `::open(`
/// (method and path calls) can be told apart from a bare call. The trailing `\b` keeps the
/// behavior of the old `\b(open\(|...)\b` alternation.
fn open_call_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(^|[^\w])open\(\b").unwrap())
}

/// The body binds a local name `open`: a Rust/JS variable, a Python assignment, a closure
/// parameter, a fn/def/function parameter, or a JS arrow parameter.
fn open_local_binding_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"\b(?:let|const|var)\s+(?:mut\s+)?open\b",
            r"|(?m:^\s*open\s*(?::[^=\n]*)?=[^=])",
            r"|\|(?:[^|\n]*,)?\s*(?:mut\s+)?open\s*(?:[:,][^|\n]*)?\|",
            r"|\b(?:fn|def|function)\s*\w*\s*(?:<[^>\n]*>)?\s*\((?:[^)]*,)?\s*(?:mut\s+)?open\s*[:,)=]",
            r"|\(\s*(?:[^()\n]*,\s*)?open\s*(?:,[^()\n]*)?\)\s*=>",
        ))
        .unwrap()
    })
}

/// `open(` is a file open unless it is a bare call to a local binding named `open`.
/// `.open(` and `::open(` stay file opens (e.g. `OpenOptions::new().open`, `fs::open`).
fn open_call_is_filesystem(body: &str) -> bool {
    let mut bare = false;
    for c in open_call_re().captures_iter(body) {
        match c.get(1).map(|m| m.as_str()) {
            Some(".") | Some(":") => return true,
            _ => bare = true,
        }
    }
    bare && !open_local_binding_re().is_match(body)
}

pub fn tag_effects(body: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for (name, pat) in effect_patterns() {
        if pat.is_match(body) || (*name == "filesystem" && open_call_is_filesystem(body)) {
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
    fn fs_local_variable_is_not_filesystem() {
        let rust = "fn tick(fs: &mut Fleet) { fs.remaining_days = fs.remaining_days.saturating_sub(1); }";
        let py = "def tick(fs):\n    fs.remaining_days = fs.free_sub(1)\n";
        let js = "function tick(fs) { fs.remaining_days = fs.len(); }";
        for body in [rust, py, js] {
            let tags = tag_effects(body);
            assert!(!tags.iter().any(|t| t == "filesystem"), "{body} -> {tags:?}");
        }
    }

    #[test]
    fn fs_paths_and_node_apis_are_filesystem() {
        let std_fs = "fn load() { let _ = std::fs::read(\"x\"); }";
        let fs_write = "fn save() { let _ = fs::write(\"x\", b\"y\"); }";
        let use_fs = "use std::fs;\nfn load() { let _ = fs::read_to_string(\"x\"); }";
        let tokio_fs = "async fn load() { let _ = tokio::fs::read(\"x\").await; }";
        let node_fs = "function load() { fs.readFileSync(\"x\"); }";
        let node_async = "async function save() { await fs.promises.writeFile(\"x\", \"y\"); fs.writeFile(\"z\", \"y\", cb); }";
        for body in [std_fs, fs_write, use_fs, tokio_fs, node_fs, node_async] {
            let tags = tag_effects(body);
            assert!(tags.iter().any(|t| t == "filesystem"), "{body} -> {tags:?}");
        }
    }

    #[test]
    fn open_local_binding_is_not_filesystem() {
        let closure = "fn unreachable_cells(&self) -> Vec<Cell> { let open = |x: i32, y: i32| -> bool { self.is_inside(x, y) }; if open(n.0, n.1) { out.push(n); } out }";
        let closure_mut = "fn f() { let mut open = |c| c > 0; open(c) }";
        let param = "fn walk(open: impl Fn(i32) -> bool) -> bool { open(x) }";
        let closure_param = "fn walk(xs: &[i32]) { xs.iter().for_each(|open| { open(x); }) }";
        let py_param = "def walk(cells, open):\n    return [c for c in cells if open(c)]\n";
        let py_assign = "def walk(cells):\n    open = cells.get\n    return open(cells)\n";
        let js = "function walk(cells) { const open = (c) => c > 0; return open(cells); }";
        let js_arrow = "const walk = (cells, open) => open(cells);";
        for body in [closure, closure_mut, param, closure_param, py_param, py_assign, js, js_arrow] {
            let tags = tag_effects(body);
            assert!(!tags.iter().any(|t| t == "filesystem"), "{body} -> {tags:?}");
        }
    }

    #[test]
    fn real_open_calls_are_filesystem() {
        let py_with = "def load(path):\n    with open(path) as f:\n        return f.read()\n";
        let py_bare = "def load(path):\n    f = open(path)\n    return f\n";
        let file_open = "fn load(p: &Path) { let _ = File::open(p); }";
        let opts = "fn load(p: &Path) { let _ = OpenOptions::new().read(true).open(p); }";
        let fs_open = "fn load(p: &Path) { let _ = fs::open(p); }";
        let os_open = "def load(path):\n    return os.open(path, 0)\n";
        let either_or = "function f(x) { return x || open(p) || y; }";
        for body in [py_with, py_bare, file_open, opts, fs_open, os_open, either_or] {
            let tags = tag_effects(body);
            assert!(tags.iter().any(|t| t == "filesystem"), "{body} -> {tags:?}");
        }
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

    #[test]
    fn spatial_index_query_is_not_db() {
        let body = "fn find_region_at(&self, x: f64, y: f64) -> Option<RegionId> { self.index.query(x, y) }";
        let tags = tag_effects(body);
        assert!(!tags.iter().any(|t| t == "db"), "{tags:?}");
    }

    #[test]
    fn real_db_apis_are_db() {
        let sqlite = "fn load() { let _ = sqlite3.connect(\"x\"); }";
        let exec = "fn run(conn) { conn.execute(\"SELECT 1\"); }";
        let sqlx_macro = "async fn load(pool: &PgPool) { let _ = query!(\"SELECT 1\"); }";
        let sqlx_path = "async fn load(pool: &PgPool) { let _ = sqlx::query(\"SELECT 1\"); }";
        assert!(tag_effects(sqlite).iter().any(|t| t == "db"), "{:?}", tag_effects(sqlite));
        assert!(tag_effects(exec).iter().any(|t| t == "db"), "{:?}", tag_effects(exec));
        assert!(tag_effects(sqlx_macro).iter().any(|t| t == "db"), "{:?}", tag_effects(sqlx_macro));
        assert!(tag_effects(sqlx_path).iter().any(|t| t == "db"), "{:?}", tag_effects(sqlx_path));
    }

    #[test]
    fn command_enum_and_process_id_are_not_process() {
        let parse = "fn parse_command(s: &str) -> Command { Command::Parse }";
        let run = "fn run_command(c: Command) { match c { Command::Run => {} } }";
        let report = "fn format_exit_report() -> String { format!(\"pid {}\", std::process::id()) }";
        let path = "fn campaign_io_path() -> PathBuf { PathBuf::from(format!(\"{}\", std::process::id())) }";
        for body in [parse, run, report, path] {
            let tags = tag_effects(body);
            assert!(!tags.iter().any(|t| t == "process"), "{body} -> {tags:?}");
        }
    }

    #[test]
    fn process_spawn_apis_are_process() {
        let cmd_new = "fn run() { let _ = Command::new(\"true\"); }";
        let std_cmd = "fn run() { let _ = std::process::Command::new(\"true\"); }";
        let sub = "def run():\n    subprocess.run([\"true\"])\n";
        let system = "def run():\n    os.system(\"true\")\n";
        let child = "const { spawn } = require(\"child_process\");";
        assert!(tag_effects(cmd_new).iter().any(|t| t == "process"), "{:?}", tag_effects(cmd_new));
        assert!(tag_effects(std_cmd).iter().any(|t| t == "process"), "{:?}", tag_effects(std_cmd));
        assert!(tag_effects(sub).iter().any(|t| t == "process"), "{:?}", tag_effects(sub));
        assert!(tag_effects(system).iter().any(|t| t == "process"), "{:?}", tag_effects(system));
        assert!(tag_effects(child).iter().any(|t| t == "process"), "{:?}", tag_effects(child));
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
