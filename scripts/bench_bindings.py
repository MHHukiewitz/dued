#!/usr/bin/env python3
"""Wall-clock compare: Python CLI (Rust bindings) vs native dued."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PY = ROOT / ".venv" / "bin" / "dued"
RS = ROOT / "dued-rs" / "target" / "release" / "dued"
FIXTURE = ROOT / "tests" / "fixtures" / "mini"
OUT = ROOT / "dued-reports" / "bindings-bench.json"
RUNS = 3
MAINNET = Path("/Users/mikehenry/Workspace/Fun/Mainnet")
STASH_NAMES = (".dued", ".dued-rs")


def run(cmd: list[str], cwd: Path) -> tuple[dict, float]:
    env = os.environ.copy()
    env["DUED_STUB_EMBED"] = "1"
    started = time.perf_counter()
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, env=env)
    elapsed = time.perf_counter() - started
    if proc.returncode != 0:
        raise SystemExit(f"{cmd[0]} failed:\n{proc.stderr}\n{proc.stdout}")
    return json.loads(proc.stdout, strict=False), elapsed


def wipe(repo: Path) -> None:
    shutil.rmtree(repo / ".dued", ignore_errors=True)
    shutil.rmtree(repo / ".dued-rs", ignore_errors=True)


def stash_indexes(repo: Path) -> None:
    for name in STASH_NAMES:
        src = repo / name
        bak = repo / f"{name}.bench-stash"
        if bak.exists():
            shutil.rmtree(bak)
        if src.exists():
            shutil.move(str(src), str(bak))
            print(f"  stashed {src} -> {bak}", flush=True)


def restore_indexes(repo: Path) -> None:
    for name in STASH_NAMES:
        dest = repo / name
        bak = repo / f"{name}.bench-stash"
        if dest.exists():
            shutil.rmtree(dest)
        if bak.exists():
            shutil.move(str(bak), str(dest))
            print(f"  restored {dest}", flush=True)


def invoke(bin_path: Path, repo: Path, command: str) -> tuple[dict, float]:
    extra = ["--no-embed"]
    if command == "analyze":
        extra.append("--no-git")
    return run(
        [str(bin_path), "--repo", str(repo), "--quiet", "--json", command, *extra],
        repo,
    )


def mean(values: list[float]) -> float:
    return sum(values) / len(values)


def bench_target(label: str, repo: Path, command: str) -> dict:
    py_times: list[float] = []
    rs_times: list[float] = []
    py_last: dict = {}
    rs_last: dict = {}
    for i in range(RUNS):
        wipe(repo)
        py_last, pt = invoke(PY, repo, command)
        print(f"  {label} python {command} {i + 1}/{RUNS} {pt:.3f}s", flush=True)
        wipe(repo)
        rs_last, rt = invoke(RS, repo, command)
        print(f"  {label} native {command} {i + 1}/{RUNS} {rt:.3f}s", flush=True)
        py_times.append(pt)
        rs_times.append(rt)
    py_mean = mean(py_times)
    rs_mean = mean(rs_times)
    return {
        "command": command,
        "python_bindings": {
            "seconds": [round(t, 4) for t in py_times],
            "mean": round(py_mean, 4),
            "files": py_last.get("files"),
            "symbols": py_last.get("symbols"),
            "edges": py_last.get("edges"),
        },
        "native_dued_rs": {
            "seconds": [round(t, 4) for t in rs_times],
            "mean": round(rs_mean, 4),
            "files": rs_last.get("files"),
            "symbols": rs_last.get("symbols"),
            "edges": rs_last.get("edges"),
        },
        "native_over_python": round(rs_mean / py_mean, 3) if py_mean else None,
        "python_over_native": round(py_mean / rs_mean, 3) if rs_mean else None,
    }


def copy_mini() -> Path:
    work = Path("/tmp/dued-bench-mini")
    if work.exists():
        shutil.rmtree(work)
    work.mkdir()
    for src in FIXTURE.iterdir():
        if src.is_file():
            shutil.copy(src, work / src.name)
    return work


def load_report() -> dict:
    if OUT.is_file():
        return json.loads(OUT.read_text(encoding="utf-8"), strict=False)
    return {
        "note": "Python CLI uses dued._native (Rust). Native CLI is cargo-built dued. Both use --no-embed --no-git --quiet --json. Indexes are wiped before each run.",
        "python_cli": str(PY),
        "native_cli": str(RS),
        "runs": RUNS,
    }


def save_report(report: dict) -> None:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report, indent=2))


def run_core(report: dict) -> None:
    mini = copy_mini()
    print("== mini fixture ==", flush=True)
    report["mini"] = {
        "scan": bench_target("mini", mini, "scan"),
        "analyze": bench_target("mini", mini, "analyze"),
    }
    print("== Code-Analyzer ==", flush=True)
    report["code_analyzer"] = {
        "scan": bench_target("repo", ROOT, "scan"),
        "analyze": bench_target("repo", ROOT, "analyze"),
    }


def run_mainnet(report: dict) -> None:
    if not MAINNET.is_dir():
        raise SystemExit(f"missing {MAINNET}")
    print("== Mainnet ==", flush=True)
    stash_indexes(MAINNET)
    report["mainnet"] = {
        "path": str(MAINNET),
        "scan": bench_target("mainnet", MAINNET, "scan"),
        "analyze": bench_target("mainnet", MAINNET, "analyze"),
    }
    restore_indexes(MAINNET)


def main() -> None:
    if not PY.is_file():
        raise SystemExit(f"missing {PY}; install with pip install -e .")
    if not RS.is_file():
        raise SystemExit(f"missing {RS}; build with cargo build --release --bin dued")
    only = ""
    if len(sys.argv) > 1:
        only = sys.argv[1]
    report = load_report()
    report["python_cli"] = str(PY)
    report["native_cli"] = str(RS)
    report["runs"] = RUNS
    if only == "--mainnet":
        run_mainnet(report)
    elif only == "--core":
        run_core(report)
    else:
        run_core(report)
        run_mainnet(report)
    save_report(report)


if __name__ == "__main__":
    main()
