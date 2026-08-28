"""Large Rust sources must enter files and survive scan clones (#11)."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

from dued.progress import ProgressUI
from dued.scan import run_scan
from dued.slice import slice_symbol
from dued.store import connect

ACCESS_REL = "src/game/access_network.rs"
STATE_REL = "src/game/state.rs"
ACCESS_MIN = 130_805
STATE_MIN = 300_868


def _pad_comments(base: str, target_bytes: int) -> str:
    out = base
    i = 0
    while len(out.encode("utf-8")) < target_bytes:
        out += f"// padding line {i} keep large Rust sources in the index under scan\n"
        i += 1
    return out


def _write_large_game(repo: Path) -> None:
    access = (
        "pub struct WholesaleContract {\n"
        "    pub id: u64,\n"
        "    pub qty: i64,\n"
        "}\n\n"
        "impl WholesaleContract {\n"
        "    pub fn apply(&self) -> u64 { self.id }\n"
        "}\n\n"
    )
    for i in range(40):
        pad = "x" * 120
        access += (
            f"pub fn access_rule_{i}(state: &mut i32) -> i32 {{\n"
            f"    let mut acc = *state;\n"
            f"    // {pad}\n"
            f"    acc = acc.wrapping_add({i});\n"
            f"    *state = acc;\n"
            f"    acc\n"
            f"}}\n"
        )
    access = _pad_comments(access, ACCESS_MIN)

    state = (
        "pub struct GameState {\n"
        "    pub tick: u64,\n"
        "}\n\n"
        "impl GameState {\n"
        "    pub fn step(&mut self) { self.tick += 1; }\n"
        "}\n\n"
    )
    for i in range(40):
        pad = "y" * 120
        state += (
            f"pub fn state_op_{i}(tick: &mut u64) {{\n"
            f"    let mut acc = *tick;\n"
            f"    // {pad}\n"
            f"    acc = acc.wrapping_add({i});\n"
            f"    *tick = acc;\n"
            f"}}\n"
        )
    state = _pad_comments(state, STATE_MIN)

    (repo / "src" / "game").mkdir(parents=True)
    (repo / ACCESS_REL).write_text(access, encoding="utf-8")
    (repo / STATE_REL).write_text(state, encoding="utf-8")
    assert (repo / ACCESS_REL).stat().st_size >= ACCESS_MIN
    assert (repo / STATE_REL).stat().st_size >= STATE_MIN


def _git_init(repo: Path) -> None:
    subprocess.run(["git", "init"], cwd=repo, check=True, capture_output=True)
    subprocess.run(
        ["git", "config", "user.email", "test@example.com"],
        cwd=repo,
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["git", "config", "user.name", "test"],
        cwd=repo,
        check=True,
        capture_output=True,
    )
    subprocess.run(["git", "add", "."], cwd=repo, check=True, capture_output=True)
    subprocess.run(
        ["git", "commit", "-m", "init"],
        cwd=repo,
        check=True,
        capture_output=True,
    )


def test_large_rust_files_indexed_and_slice_resolves(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "large_game"
    repo.mkdir()
    _write_large_game(repo)
    _git_init(repo)

    ui = ProgressUI(quiet=True)
    summary = run_scan(
        repo,
        ui,
        max_files=None,
        budget_seconds=None,
        model_name="stub",
        with_git=True,
        with_embed=False,
    )
    assert summary["files"] >= 2
    assert summary["parsed"] >= 2

    conn = connect(repo)
    relpaths = {row[0] for row in conn.execute("SELECT relpath FROM files")}
    assert any(p.endswith("access_network.rs") for p in relpaths), relpaths
    assert any(p.endswith("state.rs") for p in relpaths), relpaths
    max_size = conn.execute("SELECT MAX(size) FROM files").fetchone()[0]
    assert max_size >= STATE_MIN, max_size

    sliced = slice_symbol(conn, "WholesaleContract")
    assert "error" not in sliced, sliced
    names = {row["name"] for row in sliced["symbols"]}
    assert "WholesaleContract" in names
    conn.close()
