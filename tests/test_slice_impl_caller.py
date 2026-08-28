"""Slice callers must include Game impl methods that call a unique crate fn."""

from __future__ import annotations

import os
from pathlib import Path

from dued.progress import ProgressUI
from dued.scan import run_scan
from dued.slice import slice_symbol
from dued.store import connect

FIXTURE = Path(__file__).parent / "fixtures" / "slice_impl_caller"


def _copy_fixture(repo: Path) -> None:
    repo.mkdir()
    for src in FIXTURE.iterdir():
        if src.is_file():
            (repo / src.name).write_bytes(src.read_bytes())


def _scan(repo: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    ui = ProgressUI(quiet=True)
    summary = run_scan(
        repo,
        ui,
        max_files=None,
        budget_seconds=None,
        model_name="stub",
        with_git=False,
        with_embed=False,
    )
    assert summary["files"] >= 2


def test_slice_allocate_customers_includes_impl_caller(tmp_path: Path) -> None:
    repo = tmp_path / "slice_impl_caller"
    _copy_fixture(repo)
    _scan(repo)
    conn = connect(repo)

    sliced = slice_symbol(conn, "allocate_customers")
    assert "error" not in sliced, sliced

    callers = sliced.get("callers") or []
    names = {row["name"] for row in callers}
    assert "apply_competitive_alloc_jobs" in names, callers
    bridge = [row for row in callers if row["name"] == "apply_competitive_alloc_jobs"]
    assert bridge, callers
    assert bridge[0]["relpath"] == "graph_bridge.rs"

    # Unique-name blast stays on the define file (issue #1).
    assert sliced["blast_radius"] == 1
    assert set(sliced.get("files") or []) == {"allocate.rs"}

    conn.close()
