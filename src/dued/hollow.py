from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["apply_hollow", "hollow_symbols", "is_hollow"]


def is_hollow(body: str, docstring: str) -> str:
    return _native.is_hollow(body, docstring)


def hollow_symbols(conn: object) -> list[dict[str, object]]:
    return loads(_native.hollow_symbols(str(repo_of(conn))))


def apply_hollow(conn: object) -> list[dict[str, object]]:
    return loads(_native.apply_hollow(str(repo_of(conn))))
