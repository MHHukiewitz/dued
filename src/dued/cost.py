from __future__ import annotations

from dued import _native
from dued._codec import repo_of

__all__ = ["apply_cost_hints", "cost_hint"]


def cost_hint(body: str) -> int:
    return _native.cost_hint(body)


def apply_cost_hints(conn: object) -> None:
    _native.apply_cost_hints(str(repo_of(conn)))
