"""--json must be strict JSON on stdout (no Rich soft-wrap)."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

from dued.display import emit

FIXTURE = Path(__file__).parent / "fixtures" / "mini"


def test_emit_json_strict_loads_long_strings(capsys) -> None:
    # Lines longer than Rich's default width used to get raw newlines inside strings.
    payload = {
        "kind": "io_at_core",
        "detail": "x" * 240,
        "relpath": "src/" + ("nested/" * 20) + "module.rs",
        "name": "sitting_isp_kit_backhaul_floor",
        "rows": [{"detail": "y" * 180, "path": "a" * 100}],
    }
    emit(payload, as_json=True)
    out = capsys.readouterr().out
    parsed = json.loads(out)
    assert parsed["detail"] == payload["detail"]
    assert parsed["rows"][0]["detail"] == payload["rows"][0]["detail"]


def test_cli_json_issues_strict_loads(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "mini"
    repo.mkdir()
    for src in FIXTURE.iterdir():
        if src.is_file():
            (repo / src.name).write_bytes(src.read_bytes())
    env = {**os.environ, "DUED_STUB_EMBED": "1", "COLUMNS": "40"}
    analyze = subprocess.run(
        [sys.executable, "-m", "dued", "--repo", str(repo), "--quiet", "--json", "analyze", "--no-embed", "--no-git"],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    json.loads(analyze.stdout)
    issues = subprocess.run(
        [sys.executable, "-m", "dued", "--repo", str(repo), "--quiet", "--json", "issues"],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    json.loads(issues.stdout)
