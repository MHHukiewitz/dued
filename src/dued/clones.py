from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["find_clones", "find_embed_clones", "label_clusters"]


def find_clones(conn: object, min_score: float = 0.55) -> list[dict[str, object]]:
    return loads(_native.find_clones(str(repo_of(conn))))


def find_embed_clones(conn: object, min_score: float = 0.92) -> list[dict[str, object]]:
    return loads(_native.find_embed_clones(str(repo_of(conn))))


def label_clusters(conn: object, k: int | None = None) -> list[dict[str, object]]:
    return loads(_native.label_clusters(str(repo_of(conn))))
