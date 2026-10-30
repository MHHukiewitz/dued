"""Helpers inside a Rust `#[cfg(test)]` module are test code, not core I/O."""

from __future__ import annotations

import json
import os
from pathlib import Path

from typer.testing import CliRunner

from dued.cli import app

runner = CliRunner()

PLANS_RS = """\
pub fn load_plan(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

pub fn plan_a() -> String { load_plan("a.json") }
pub fn plan_b() -> String { load_plan("b.json") }

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(tag);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn first() { let _ = load_plan(temp_dir("a").to_str().unwrap()); }

    #[test]
    fn second() { let _ = load_plan(temp_dir("b").to_str().unwrap()); }
}
"""


def test_cfg_test_module_helper_is_not_effect_in_core(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "mini"
    (repo / "src" / "game").mkdir(parents=True)
    (repo / "src" / "game" / "plans.rs").write_text(PLANS_RS, encoding="utf-8")
    scan = runner.invoke(app, ["--repo", str(repo), "--quiet", "--json", "scan", "--no-embed"])
    assert scan.exit_code == 0, scan.output
    result = runner.invoke(app, ["--repo", str(repo), "--json", "issues", "--all"])
    assert result.exit_code == 0, result.output
    rows = json.loads(result.stdout)
    hard = {r["name"] for r in rows if r["kind"] == "effect_in_core"}
    assert "temp_dir" not in hard, rows
    assert "load_plan" in hard, rows
