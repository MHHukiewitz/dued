from __future__ import annotations

from dued import _native
from dued._codec import repo_of

__all__ = ["apply_effects", "tag_effects"]


def tag_effects(body: str) -> list[str]:
    return _native.tag_effects(body)


def apply_effects(conn: object) -> None:
    _native.apply_effects(str(repo_of(conn)))
