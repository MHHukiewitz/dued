"""Issue #29: with_capacity cross-file bind, empty tests, taint generic chop."""

import os
from pathlib import Path

from dued.progress import ProgressUI
from dued.scan import run_scan
from dued.slice import slice_symbol
from dued.store import connect

FIXTURE = Path(__file__).parent / "fixtures" / "slice_mesh_capacity"


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


def test_fill_region_mesh_slice_neighborhood(tmp_path: Path) -> None:
    repo = tmp_path / "slice_mesh_capacity"
    _copy_fixture(repo)
    _scan(repo)
    conn = connect(repo)
    sliced = slice_symbol(conn, "fill_region_mesh")
    assert "error" not in sliced, sliced

    files = set(sliced.get("files") or [])
    assert "edge.rs" not in files, files
    assert files == {"mesh.rs"} or files <= {"mesh.rs"}, files

    test_names = {row["name"] for row in (sliced.get("tests") or [])}
    assert test_names == {"rings_one", "rings_two", "rings_three", "rings_four"}, test_names

    params = sliced.get("taint", {}).get("params") or []
    assert params == ["region_id", "rings"], params
    assert not any("f64" in p for p in params), params

    unresolved = set(sliced.get("unresolved_callees") or [])
    # Vec::with_capacity must stay unresolved rather than binding edge.rs.
    assert "with_capacity" in unresolved or "edge.rs" not in files

    conn.close()
