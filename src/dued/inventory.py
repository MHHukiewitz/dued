from __future__ import annotations

from pathlib import Path

from dued import _native
from dued._codec import loads

__all__ = ["inventory", "package_map"]


def package_map(repo: Path) -> list[dict[str, object]]:
    return loads(_native.package_map(str(Path(repo))))


def inventory(conn: object, repo: Path) -> dict[str, object]:
    return loads(_native.inventory(str(Path(repo))))
