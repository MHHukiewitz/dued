from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["dead_files", "dead_report", "dead_symbols"]


def dead_symbols(conn: object) -> list[dict[str, object]]:
    return loads(_native.dead_symbols(str(repo_of(conn))))


def dead_files(conn: object) -> list[dict[str, object]]:
    return loads(_native.dead_files(str(repo_of(conn))))


def dead_report(conn: object) -> dict[str, object]:
    return loads(_native.dead_report(str(repo_of(conn))))
