from __future__ import annotations

from dued import _native
from dued._codec import repo_of

__all__ = ["apply_fingerprints", "fingerprint_overlap", "fingerprint_symbol"]


def fingerprint_symbol(
    name: str,
    effects: list[str],
    fan_in: int,
    fan_out: int,
    cyclomatic: int,
    cognitive: int,
    callees: list[str],
) -> str:
    return _native.fingerprint_symbol(name, effects, fan_in, fan_out, cyclomatic, cognitive, callees)


def fingerprint_overlap(a: str, b: str) -> float:
    return _native.fingerprint_overlap(a, b)


def apply_fingerprints(conn: object) -> None:
    _native.apply_fingerprints(str(repo_of(conn)))
