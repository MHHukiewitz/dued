import json
import os
from pathlib import Path

from typer.testing import CliRunner

from dued.cli import app

runner = CliRunner()


def test_similar_to_missing_name_is_fast_error(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "mini"
    repo.mkdir()
    (repo / "lib.rs").write_text("pub fn helper() {}\n", encoding="utf-8")
    scan = runner.invoke(app, ["--repo", str(repo), "--quiet", "--json", "scan", "--no-embed"])
    assert scan.exit_code == 0, scan.output
    result = runner.invoke(
        app,
        ["--repo", str(repo), "--quiet", "--json", "cluster", "--similar-to", "GameState"],
    )
    assert result.exit_code == 1, result.output
    payload = json.loads(result.stdout)
    assert payload["error"] == "symbol not found"
    assert payload["similar"] == []
    assert payload["clones"] == []


def test_similar_to_unembedded_name_is_fast_error(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "mini"
    repo.mkdir()
    (repo / "state.rs").write_text("pub struct GameState { pub tick: u64 }\n", encoding="utf-8")
    scan = runner.invoke(app, ["--repo", str(repo), "--quiet", "--json", "scan", "--no-embed"])
    assert scan.exit_code == 0, scan.output
    result = runner.invoke(
        app,
        ["--repo", str(repo), "--quiet", "--json", "cluster", "--similar-to", "GameState"],
    )
    assert result.exit_code == 1, result.output
    payload = json.loads(result.stdout)
    assert "no embedding" in payload["error"]
    assert payload["similar"] == []
    assert payload["clones"] == []
