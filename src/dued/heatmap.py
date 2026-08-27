from __future__ import annotations

from pathlib import Path

from dued import _native
from dued._codec import loads, repo_of

__all__ = ["write_heatmap"]


def write_heatmap(conn: object, dest: Path, slice_files: set[str] | None = None) -> dict[str, object]:
    files = list(slice_files) if slice_files is not None else None
    return loads(_native.write_heatmap(str(repo_of(conn)), str(dest), files))
