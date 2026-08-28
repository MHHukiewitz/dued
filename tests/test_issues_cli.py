"""CLI `issues` must surface more than god_function under a crowded score list."""

from __future__ import annotations

import json
import os
from pathlib import Path

from typer.testing import CliRunner

from dued.cli import app
from dued.store import connect

FIXTURE = Path(__file__).parent / "fixtures" / "issues_kinds"
runner = CliRunner()


def _copy_fixture(repo: Path) -> None:
    for src in FIXTURE.rglob("*"):
        if not src.is_file():
            continue
        dest = repo / src.relative_to(FIXTURE)
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(src.read_bytes())


def _seed_crowded_issues(repo: Path) -> None:
    """Index contains many high-score gods plus low-score effect/shotgun rows."""
    conn = connect(repo)
    conn.execute("DELETE FROM issues")
    file_id = conn.execute(
        "SELECT id FROM files WHERE relpath LIKE '%engine.py' LIMIT 1"
    ).fetchone()
    fid = int(file_id[0]) if file_id else None
    for i in range(50):
        conn.execute(
            "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (?,?,?,?,?)",
            (None, fid, "god_function", f"god {i}", 10000.0 - i),
        )
    conn.execute(
        "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (?,?,?,?,?)",
        (None, fid, "god_module", "god module symbols=20 cognitive=50", 50.0),
    )
    conn.execute(
        "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (?,?,?,?,?)",
        (None, fid, "effect_in_core", "I/O mixed into core (fan_in=3)", 20.0),
    )
    conn.execute(
        "INSERT INTO issues(symbol_id, file_id, kind, detail, score) VALUES (?,?,?,?,?)",
        (None, fid, "shotgun_surgery", "core/engine.py <-> ui/view.py shared=4", 0.8),
    )
    conn.commit()
    conn.close()


def test_json_issues_includes_effect_and_shotgun(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "issues_kinds"
    repo.mkdir()
    _copy_fixture(repo)

    analyze = runner.invoke(
        app,
        ["--repo", str(repo), "--quiet", "--json", "analyze", "--no-embed", "--no-git"],
    )
    assert analyze.exit_code == 0, analyze.output
    payload = json.loads(analyze.stdout)
    dest = Path(payload["report"])
    assert (dest / "data" / "issues.json").is_file() or (dest / "agent.json").is_file()

    _seed_crowded_issues(repo)

    result = runner.invoke(app, ["--repo", str(repo), "--quiet", "--json", "issues"])
    assert result.exit_code == 0, result.output
    rows = json.loads(result.stdout)
    kinds = {row["kind"] for row in rows}
    assert "god_function" in kinds, kinds
    assert "god_module" in kinds, kinds
    assert "effect_in_core" in kinds, kinds
    assert "shotgun_surgery" in kinds, kinds
    assert sum(1 for row in rows if row["kind"] == "god_function") == 40
    assert sum(1 for row in rows if row["kind"] == "shotgun_surgery") == 1

    human = runner.invoke(app, ["--repo", str(repo), "--quiet", "issues"])
    assert human.exit_code == 0, human.output
    assert "effect_in_core" in human.stdout
    assert "shotgun_surgery" in human.stdout
    assert "god_module" in human.stdout
