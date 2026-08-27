from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def loads(raw: str) -> Any:
    return json.loads(raw)


def repo_of(conn: object) -> Path:
    repo = getattr(conn, "repo", None)
    if repo is not None:
        return Path(repo)
    row = conn.execute("PRAGMA database_list").fetchone()
    return Path(row[2]).resolve().parent.parent
