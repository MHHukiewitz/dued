from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["analyze_names", "name_complexity", "stem_family", "tokenize_name"]


def tokenize_name(name: str) -> list[str]:
    return _native.tokenize_name(name)


def stem_family(tokens: list[str]) -> str:
    if not tokens:
        return ""
    if len(tokens) == 1:
        return tokens[0]
    if tokens[-1] in {"model", "dto", "service", "handler", "repo", "view"}:
        return tokens[-1]
    return tokens[0]


def name_complexity(name: str) -> int:
    return len(tokenize_name(name))


def analyze_names(conn: object) -> list[dict[str, object]]:
    return loads(_native.analyze_names(str(repo_of(conn))))
