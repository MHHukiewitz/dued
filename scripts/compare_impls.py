#!/usr/bin/env python3
"""Compare Python dued and the native dued binary on the same repo."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PY = ROOT / ".venv" / "bin" / "dued"
RS = ROOT / "dued-rs" / "target" / "release" / "dued"
FIXTURE = ROOT / "tests" / "fixtures" / "mini"


def run(cmd: list[str], cwd: Path) -> tuple[dict, float]:
    env = os.environ.copy()
    env["DUED_STUB_EMBED"] = "1"
    started = time.perf_counter()
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, env=env)
    elapsed = time.perf_counter() - started
    if proc.returncode != 0:
        raise SystemExit(f"{cmd[0]} failed:\n{proc.stderr}\n{proc.stdout}")
    return json.loads(proc.stdout), elapsed


def analyze(bin_path: Path, repo: Path, extra: list[str]) -> tuple[dict, float]:
    return run(
        [str(bin_path), "--repo", str(repo), "--quiet", "--json", "analyze", "--no-embed", "--no-git", *extra],
        repo,
    )


def query(bin_path: Path, repo: Path, *args: str) -> dict:
    data, _ = run([str(bin_path), "--repo", str(repo), "--quiet", "--json", *args], repo)
    return data


def symbol_set(repo: Path, index_dir: str) -> set[tuple[str, str, int, int, int]]:
    import sqlite3

    db = repo / index_dir / "index.sqlite"
    conn = sqlite3.connect(db)
    rows = conn.execute(
        """
        SELECT f.relpath, s.name, s.start_line, s.cyclomatic, s.cognitive
        FROM symbols s JOIN files f ON f.id = s.file_id
        ORDER BY f.relpath, s.start_line, s.name
        """
    ).fetchall()
    conn.close()
    return set(rows)


def compare_fixture() -> dict:
    work = Path("/tmp/dued-cmp-mini")
    if work.exists():
        shutil.rmtree(work)
    work.mkdir()
    for src in FIXTURE.iterdir():
        shutil.copy(src, work / src.name)
    py, py_t = analyze(PY, work, [])
    py_dead = {f"{x['relpath']}::{x['name']}" for x in query(PY, work, "dead")["symbols"]}
    py_rank = [f"{x['relpath']}::{x['name']}" for x in query(PY, work, "rank", "--limit", "10")]
    py_slice = query(PY, work, "slice", "get_user")
    py_syms = symbol_set(work, "dued")
    rs, rs_t = analyze(RS, work, [])
    rs_dead = {f"{x['relpath']}::{x['name']}" for x in query(RS, work, "dead")["symbols"]}
    rs_rank = [f"{x['relpath']}::{x['name']}" for x in query(RS, work, "rank", "--limit", "10")]
    rs_slice = query(RS, work, "slice", "get_user")
    rs_syms = symbol_set(work, "dued")
    return {
        "counts": {
            "python": {"files": py["files"], "symbols": py["symbols"], "edges": py["edges"], "hollow": py["hollow"]},
            "rust": {"files": rs["files"], "symbols": rs["symbols"], "edges": rs["edges"], "hollow": rs["hollow"]},
        },
        "symbol_match": py_syms == rs_syms,
        "python_only_symbols": sorted(list(py_syms - rs_syms))[:20],
        "rust_only_symbols": sorted(list(rs_syms - py_syms))[:20],
        "dead_match": py_dead == rs_dead,
        "dead_python": sorted(py_dead),
        "dead_rust": sorted(rs_dead),
        "rank_python": py_rank,
        "rank_rust": rs_rank,
        "slice_effects_python": py_slice.get("effects"),
        "slice_effects_rust": rs_slice.get("effects"),
        "slice_blast_python": py_slice.get("blast_radius"),
        "slice_blast_rust": rs_slice.get("blast_radius"),
        "fixture_seconds": {"python": round(py_t, 4), "rust": round(rs_t, 4)},
    }


def bench_repo() -> dict:
    py_times = []
    rs_times = []
    py_last: dict | None = None
    rs_last: dict | None = None
    for _ in range(3):
        shutil.rmtree(ROOT / "dued", ignore_errors=True)
        shutil.rmtree(ROOT / ".dued", ignore_errors=True)
        shutil.rmtree(ROOT / ".dued-rs", ignore_errors=True)
        py_last, pt = analyze(PY, ROOT, [])
        py_times.append(pt)
        shutil.rmtree(ROOT / "dued", ignore_errors=True)
        rs_last, rt = analyze(RS, ROOT, [])
        rs_times.append(rt)
    assert py_last is not None
    assert rs_last is not None
    return {
        "repo_seconds": {
            "python": [round(t, 4) for t in py_times],
            "rust": [round(t, 4) for t in rs_times],
            "python_mean": round(sum(py_times) / len(py_times), 4),
            "rust_mean": round(sum(rs_times) / len(rs_times), 4),
        },
        "repo_counts": {
            "python": {"files": py_last["files"], "symbols": py_last["symbols"], "edges": py_last["edges"]},
            "rust": {"files": rs_last["files"], "symbols": rs_last["symbols"], "edges": rs_last["edges"]},
        },
    }


def main() -> None:
    if not PY.is_file():
        raise SystemExit(f"missing {PY}")
    if not RS.is_file():
        raise SystemExit(f"missing {RS}; build with cargo build --release")
    report = {"fixture": compare_fixture(), "repo": bench_repo()}
    py_mean = report["repo"]["repo_seconds"]["python_mean"]
    rs_mean = report["repo"]["repo_seconds"]["rust_mean"]
    report["speedup"] = round(py_mean / rs_mean, 3) if rs_mean else None
    dest = ROOT / "dued" / "compare.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
