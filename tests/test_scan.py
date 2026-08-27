import os
from pathlib import Path

from dued.dead import dead_symbols
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
    sliced = slice_symbol(conn, "get_user")
    assert sliced["blast_radius"] >= 1
    assert "filesystem" in sliced["effects"] or sliced["symbols"]
    conn.close()
    again = run_scan(repo, ui, max_files=None, budget_seconds=None, model_name="stub", with_git=False, with_embed=True)
    assert again["reused"] == 3
    assert again["parsed"] == 0
