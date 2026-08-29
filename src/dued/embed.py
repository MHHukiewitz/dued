from __future__ import annotations

from pathlib import Path

from dued import _native
from dued._codec import loads, repo_of
from dued.progress import ProgressUI

DEFAULT_MODEL = _native.DEFAULT_MODEL

__all__ = [
    "DEFAULT_MODEL",
    "embed_symbols",
    "export_label_csv",
    "mismatch_flags",
    "similar_lookup_error",
    "similar_to",
    "use_stub",
]


def use_stub(model_name: str = "stub") -> bool:
    return _native.use_stub(model_name)


def embed_symbols(conn: object, model_name: str, ui: ProgressUI | None = None, only_missing: bool = False) -> None:
    task = ui.add("embed symbols", 1) if ui is not None else None
    _native.embed_symbols(str(repo_of(conn)), model_name, only_missing)
    if ui is not None:
        ui.advance(task)


def mismatch_flags(conn: object) -> list[dict[str, object]]:
    return loads(_native.mismatch_flags(str(repo_of(conn))))


def similar_to(conn: object, query: str, limit: int = 10) -> list[dict[str, object]]:
    return loads(_native.similar_to(str(repo_of(conn)), query))[:limit]


def similar_lookup_error(conn: object, query: str) -> str | None:
    return _native.similar_lookup_error(str(repo_of(conn)), query)


def export_label_csv(conn: object, dest: Path) -> int:
    return _native.export_label_csv(str(repo_of(conn)), str(dest))
