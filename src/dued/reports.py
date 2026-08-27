from __future__ import annotations

import json
from pathlib import Path

from dued import _native

__all__ = ["write_report_dir", "refresh_report"]


def write_report_dir(repo: Path, conn: object, extra: dict[str, object] | None = None) -> Path:
    return Path(_native.write_report_dir(str(Path(repo)), json.dumps(extra or {})))


def refresh_report(repo: Path, conn: object) -> Path:
    return Path(_native.refresh_report(str(Path(repo))))
