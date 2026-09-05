from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["apply_issues", "list_issues"]


def apply_issues(conn: object) -> list[dict[str, object]]:
    return loads(_native.apply_issues(str(repo_of(conn))))


def list_issues(conn: object, limit: int = 40) -> list[dict[str, object]]:
    """Return issues ordered by score.

    ``limit`` is a per-kind cap. Pass a negative value to return every row.
    """
    return loads(_native.list_issues(str(repo_of(conn)), limit))
