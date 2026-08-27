import os
from pathlib import Path

from dued.progress import ProgressUI
from dued.scan import run_scan
from dued.slice import slice_symbol
from dued.store import connect

FIXTURE = Path(__file__).parent / "fixtures" / "slice_collision"
COLLISION_NAMES = {"new", "default", "get", "as_str", "is_empty"}
DECOY_FILES = {"noise_fs.rs", "noise_net.rs", "noise_proc.rs"}
BAD_EFFECTS = {"filesystem", "network", "process", "unsafe", "global_mutate"}


def _copy_fixture(repo: Path) -> None:
    repo.mkdir()
    for src in FIXTURE.iterdir():
        if src.is_file():
            (repo / src.name).write_bytes(src.read_bytes())


def _scan(repo: Path, *, min_files: int = 1) -> dict:
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
    assert summary["files"] >= min_files
    return summary


def test_unique_name_slice_not_polluted_by_common_methods(tmp_path: Path) -> None:
    repo = tmp_path / "slice_collision"
    _copy_fixture(repo)
    _scan(repo, min_files=4)
    conn = connect(repo)

    for query in ("sync_graph_access_layers", "apply_op"):
        sliced = slice_symbol(conn, query)
        assert "error" not in sliced, sliced
        names = {row["name"] for row in sliced["symbols"]}
        assert query in names
        assert "ensure_graph_world" in names

        files = set(sliced["files"])
        assert not (files & DECOY_FILES), f"{query} pulled decoy files: {files}"
        assert sliced["blast_radius"] < len(DECOY_FILES) + 1

        # Same-file generics may resolve (choose_call_targets); decoy files must not.
        for row in sliced["symbols"]:
            if row["name"] in COLLISION_NAMES:
                assert row["relpath"] == "bridge.rs", row

        assert not (set(sliced.get("effects") or []) & BAD_EFFECTS), sliced.get("effects")

    conn.close()


def test_ambiguous_common_name_requires_path_qualification(tmp_path: Path) -> None:
    repo = tmp_path / "slice_collision_ambiguous"
    _copy_fixture(repo)
    _scan(repo, min_files=4)
    conn = connect(repo)
    sliced = slice_symbol(conn, "new")
    assert sliced.get("error") == "ambiguous symbol name; qualify as path::name"
    assert sliced.get("blast_radius") == 0
    candidates = sliced.get("candidates") or []
    assert len(candidates) >= 2
    relpaths = {row["relpath"] for row in candidates}
    assert len(relpaths) >= 2

    qualified = slice_symbol(conn, "noise_fs.rs::new")
    assert "error" not in qualified, qualified
    assert qualified["root"]["relpath"] == "noise_fs.rs"
    assert qualified["root"]["name"] == "new"
    conn.close()


def test_ambiguous_non_generic_does_not_union_targets(tmp_path: Path) -> None:
    """Ambiguous non-generic callees stay unresolved and must not union both defs."""
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "slice_ambiguous_process"
    repo.mkdir()
    (repo / "caller.rs").write_text(
        "pub fn unique_entry() {\n    let _ = process();\n}\n\npub fn process() -> i32 { 1 }\n",
        encoding="utf-8",
    )
    (repo / "other.rs").write_text(
        "pub fn process() -> i32 {\n    let _ = std::fs::read_to_string(\"/tmp/x\");\n    2\n}\n",
        encoding="utf-8",
    )
    _scan(repo, min_files=2)
    conn = connect(repo)
    sliced = slice_symbol(conn, "unique_entry")
    assert "error" not in sliced, sliced
    names = {row["name"] for row in sliced["symbols"]}
    # Same-file process may resolve; other.rs::process must not.
    for row in sliced["symbols"]:
        if row["name"] == "process":
            assert row["relpath"] == "caller.rs", row
    assert "other.rs" not in set(sliced["files"])
    assert "filesystem" not in (sliced.get("effects") or [])
    conn.close()
