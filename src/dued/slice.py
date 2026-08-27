from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["slice_symbol"]


def slice_symbol(conn: object, query: str, depth: int = 4) -> dict[str, object]:
    return loads(_native.slice_symbol(str(repo_of(conn)), query, depth))
