"""Effect tags must not treat SpatialIndex.query or Command enums as db/process."""

from __future__ import annotations

from dued.effects import tag_effects


def test_spatial_index_query_not_db() -> None:
    body = "fn find_region_at(&self, x: f64, y: f64) -> Option<RegionId> { self.index.query(x, y) }"
    tags = tag_effects(body)
    assert "db" not in tags, tags


def test_real_db_apis_are_db() -> None:
    assert "db" in tag_effects('fn load() { let _ = sqlite3.connect("x"); }')
    assert "db" in tag_effects('fn run(conn) { conn.execute("SELECT 1"); }')
    assert "db" in tag_effects('async fn load(pool: &PgPool) { let _ = query!("SELECT 1"); }')
    assert "db" in tag_effects('async fn load(pool: &PgPool) { let _ = sqlx::query("SELECT 1"); }')


def test_command_enum_and_process_id_not_process() -> None:
    bodies = [
        "fn parse_command(s: &str) -> Command { Command::Parse }",
        "fn run_command(c: Command) { match c { Command::Run => {} } }",
        'fn format_exit_report() -> String { format!("pid {}", std::process::id()) }',
        'fn campaign_io_path() -> PathBuf { PathBuf::from(format!("{}", std::process::id())) }',
    ]
    for body in bodies:
        tags = tag_effects(body)
        assert "process" not in tags, (body, tags)


def test_process_spawn_apis_are_process() -> None:
    assert "process" in tag_effects('fn run() { let _ = Command::new("true"); }')
    assert "process" in tag_effects('fn run() { let _ = std::process::Command::new("true"); }')
    assert "process" in tag_effects("def run():\n    subprocess.run([\"true\"])\n")
    assert "process" in tag_effects("def run():\n    os.system(\"true\")\n")
    assert "process" in tag_effects('const { spawn } = require("child_process");')
