from __future__ import annotations

from pathlib import Path

from dued import _native
from dued._codec import loads
from dued.embed import DEFAULT_MODEL
from dued.progress import ProgressUI

__all__ = ["run_scan"]


def run_scan(
    repo: Path,
    ui: ProgressUI,
    max_files: int | None,
    budget_seconds: float | None,
    model_name: str = DEFAULT_MODEL,
    with_git: bool = False,
    with_embed: bool = True,
) -> dict[str, object]:
    _ = ui
    return loads(
        _native.run_scan(str(Path(repo)), max_files, budget_seconds, with_git, with_embed, model_name)
    )
