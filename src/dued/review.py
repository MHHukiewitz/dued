from __future__ import annotations

from pathlib import Path

from dued import _native
from dued._codec import repo_of

__all__ = ["review_pack"]


def review_pack(conn: object, dest: Path, slice_query: str | None = None) -> Path:
    return Path(_native.review_pack(str(repo_of(conn)), str(dest), slice_query))
