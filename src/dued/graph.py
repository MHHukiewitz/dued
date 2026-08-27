from __future__ import annotations

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["choose_call_targets", "file_graph", "pagerank", "resolve_and_store_edges"]


def choose_call_targets(
    callee: str,
    targets: list[tuple[int, int]],
    src_file_id: int,
    lang_by_file: dict[int, str],
) -> list[tuple[int, int]]:
    return _native.choose_call_targets(callee, targets, src_file_id, lang_by_file)


def resolve_and_store_edges(conn: object, calls: object = None, imports: object = None) -> None:
    _native.resolve_and_store_edges(str(repo_of(conn)))


def file_graph(conn: object) -> tuple[set[int], list[tuple[int, int]]]:
    data = loads(_native.file_graph(str(repo_of(conn))))
    nodes = {int(n) for n in data["nodes"]}
    edges = [(int(a), int(b)) for a, b in data["edges"]]
    return nodes, edges


def pagerank(
    nodes: set[int],
    edges: list[tuple[int, int]],
    personalize: dict[int, float] | None = None,
    damping: float = 0.85,
    rounds: int = 40,
) -> dict[int, float]:
    return _native.pagerank(list(nodes), edges, personalize, damping, rounds)
