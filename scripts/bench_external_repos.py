#!/usr/bin/env python3
"""Run Python dued and the native dued binary on larger local repos."""

from __future__ import annotations

import json
import os
import shutil
import sqlite3
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PY = ROOT / ".venv" / "bin" / "dued"
RS = ROOT / "dued-rs" / "target" / "release" / "dued"
OUT = ROOT / "dued" / "big-repos.json"

REPOS = [
    ("Mainnet", Path("/Users/mikehenry/Workspace/Fun/Mainnet")),
    ("everlast-notebookllm", Path("/Users/mikehenry/Workspace/Everlast-NotebookLLM")),
    ("superteam-talent-monorepo", Path("/Users/mikehenry/Workspace/supertalent24")),
]


def run(cmd: list[str], cwd: Path) -> tuple[dict, float]:
    env = os.environ.copy()
    env["DUED_STUB_EMBED"] = "1"
    started = time.perf_counter()
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, env=env)
    elapsed = time.perf_counter() - started
    if proc.returncode != 0:
        raise SystemExit(f"{cmd[0]} failed in {cwd}:\n{proc.stderr}\n{proc.stdout}")
    return json.loads(proc.stdout), elapsed


def analyze(bin_path: Path, repo: Path) -> tuple[dict, float]:
    return run(
        [
            str(bin_path),
            "--repo",
            str(repo),
            "--quiet",
            "--json",
            "analyze",
            "--no-embed",
            "--no-git",
        ],
        repo,
    )


def query(bin_path: Path, repo: Path, *args: str) -> dict:
    data, _ = run([str(bin_path), "--repo", str(repo), "--quiet", "--json", *args], repo)
    return data


def symbol_keys(repo: Path, index_dir: str) -> set[tuple[str, str, int, int, int]]:
    db = repo / index_dir / "index.sqlite"
    conn = sqlite3.connect(db)
    rows = conn.execute(
        """
        SELECT f.relpath, s.name, s.start_line, s.cyclomatic, s.cognitive
        FROM symbols s JOIN files f ON f.id = s.file_id
        """
    ).fetchall()
    conn.close()
    return set(rows)


def compare_one(name: str, repo: Path) -> dict:
    print(f"== {name} ==", flush=True)
    shutil.rmtree(repo / "dued", ignore_errors=True)
    shutil.rmtree(repo / ".dued", ignore_errors=True)
    shutil.rmtree(repo / ".dued-rs", ignore_errors=True)
    py, py_t = analyze(PY, repo)
    print(f"  python {py_t:.2f}s files={py.get('files')} symbols={py.get('symbols')} edges={py.get('edges')}", flush=True)
    py_dead = {f"{x['relpath']}::{x['name']}" for x in query(PY, repo, "dead")["symbols"]}
    py_rank = [f"{x['relpath']}::{x['name']}" for x in query(PY, repo, "rank", "--limit", "10")]
    py_syms = symbol_keys(repo, "dued")
    rs, rs_t = analyze(RS, repo)
    print(f"  rust   {rs_t:.2f}s files={rs.get('files')} symbols={rs.get('symbols')} edges={rs.get('edges')}", flush=True)
    rs_dead = {f"{x['relpath']}::{x['name']}" for x in query(RS, repo, "dead")["symbols"]}
    rs_rank = [f"{x['relpath']}::{x['name']}" for x in query(RS, repo, "rank", "--limit", "10")]
    rs_syms = symbol_keys(repo, "dued")
    only_py = sorted(list(py_syms - rs_syms))[:15]
    only_rs = sorted(list(rs_syms - py_syms))[:15]
    return {
        "path": str(repo),
        "seconds": {"python": round(py_t, 4), "rust": round(rs_t, 4)},
        "speedup": round(py_t / rs_t, 3) if rs_t else None,
        "counts": {
            "python": {"files": py["files"], "symbols": py["symbols"], "edges": py["edges"], "hollow": py.get("hollow")},
            "rust": {"files": rs["files"], "symbols": rs["symbols"], "edges": rs["edges"], "hollow": rs.get("hollow")},
        },
        "symbol_match": py_syms == rs_syms,
        "python_only_symbols": only_py,
        "rust_only_symbols": only_rs,
        "dead_match": py_dead == rs_dead,
        "dead_python_n": len(py_dead),
        "dead_rust_n": len(rs_dead),
        "rank_python": py_rank,
        "rank_rust": rs_rank,
        "rank_match": py_rank == rs_rank,
    }


def main() -> None:
    if not PY.is_file():
        raise SystemExit(f"missing {PY}")
    if not RS.is_file():
        raise SystemExit(f"missing {RS}")
    report = {}
    for name, repo in REPOS:
        if not repo.is_dir():
            raise SystemExit(f"missing repo {repo}")
        report[name] = compare_one(name, repo)
        dest = OUT
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
