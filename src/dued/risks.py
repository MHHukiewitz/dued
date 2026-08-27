from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["apply_risks", "tag_risks"]


def tag_risks(name: str, body: str, signature: str) -> list[str]:
    return _native.tag_risks(name, body, signature)


def apply_risks(conn: object) -> list[dict[str, object]]:
    return loads(_native.apply_risks(str(repo_of(conn))))
