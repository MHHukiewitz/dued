from __future__ import annotations

from pathlib import Path

from dued import _native
from dued._codec import loads, repo_of
from dued.progress import ProgressUI

__all__ = ["analyze_history", "history_report"]


def analyze_history(repo: Path, conn: object, ui: ProgressUI | None = None) -> dict[str, object]:
    task = ui.add("git history", 1) if ui is not None else None
    data = loads(_native.analyze_history(str(Path(repo))))
    if ui is not None:
        ui.advance(task)
    return data


def history_report(conn: object) -> dict[str, object]:
    return loads(_native.history_report(str(repo_of(conn))))
