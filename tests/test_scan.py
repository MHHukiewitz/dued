import os
from pathlib import Path

from dued.dead import dead_symbols
from dued.issues import list_issues
from dued.progress import ProgressUI
from dued.scan import run_scan
from dued.slice import slice_symbol
from dued.store import connect

FIXTURE = Path(__file__).parent / "fixtures" / "mini"


def test_scan_rank_dead_slice(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "mini"
    repo.mkdir()
    for src in FIXTURE.iterdir():
        if src.is_file():
            (repo / src.name).write_bytes(src.read_bytes())
    ui = ProgressUI(quiet=True)
    summary = run_scan(repo, ui, max_files=None, budget_seconds=None, model_name="stub", with_git=False, with_embed=True)
    assert summary["files"] == 3
    assert summary["symbols"] >= 8
    conn = connect(repo)
    dead = dead_symbols(conn)
    dead_names = {row["name"] for row in dead}
    assert "unused_helper" in dead_names
    sliced = slice_symbol(conn, "lib.rs::get_user")
    assert "error" not in sliced, sliced
    assert sliced["blast_radius"] >= 1
    assert "filesystem" in sliced["effects"] or sliced["symbols"]
    ambiguous = slice_symbol(conn, "get_user")
    assert ambiguous.get("error") == "ambiguous symbol name; qualify as path::name"
    conn.close()
    again = run_scan(repo, ui, max_files=None, budget_seconds=None, model_name="stub", with_git=False, with_embed=True)
    assert again["reused"] == 3
    assert again["parsed"] == 0


def test_dirty_rescan_git_no_embed_keeps_issue_paths(tmp_path: Path) -> None:
    """Incremental scan must not panic on UNIQUE files.relpath; issues keep paths."""
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "dirty_git"
    repo.mkdir()
    core = repo / "core.py"
    branches = "\n".join(f"    if x == {i}:\n        return {i}" for i in range(20))
    core.write_text(f"def messy(x):\n{branches}\n    return x\n", encoding="utf-8")
    (repo / "ok.py").write_text("def fine():\n    return 1\n", encoding="utf-8")
    ui = ProgressUI(quiet=True)
    first = run_scan(
        repo, ui, max_files=None, budget_seconds=None, model_name="stub", with_git=True, with_embed=False
    )
    assert first["parsed"] == 2
    core.write_text(
        f"def messy(x):\n{branches}\n    return x + 1\n",
        encoding="utf-8",
    )
    second = run_scan(
        repo, ui, max_files=None, budget_seconds=None, model_name="stub", with_git=True, with_embed=False
    )
    assert second["parsed"] == 1
    assert second["reused"] == 1
    conn = connect(repo)
    issues = list_issues(conn, limit=40)
    gods = [row for row in issues if row.get("kind") == "god_function"]
    assert gods, issues
    for row in gods:
        assert row.get("relpath"), row
        assert row.get("name"), row
    conn.close()
