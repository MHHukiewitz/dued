from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["compute_rank", "reading_order"]


def compute_rank(conn: object) -> list[dict[str, object]]:
    return loads(_native.compute_rank(str(repo_of(conn))))


def reading_order(conn: object, limit: int = 15) -> list[dict[str, object]]:
    return loads(_native.reading_order(str(repo_of(conn)), limit))
