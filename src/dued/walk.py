from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from dued import _native
from dued._codec import loads

__all__ = ["SourceFile", "walk_repo"]


@dataclass(frozen=True)
class SourceFile:
    path: Path
    relpath: str
    language: str
    size: int
    digest: str
    is_test: bool
    loc: int
    tokens: int


def walk_repo(repo: Path, max_files: int | None = None) -> list[SourceFile]:
    rows = loads(_native.walk_repo(str(Path(repo)), max_files))
    return [
        SourceFile(
            path=Path(row["path"]),
            relpath=row["relpath"],
            language=row["language"],
            size=row["size"],
            digest=row["digest"],
            is_test=row["is_test"],
            loc=row["loc"],
            tokens=row["tokens"],
        )
        for row in rows
    ]
