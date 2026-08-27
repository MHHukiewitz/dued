from __future__ import annotations

from pathlib import Path

from dued import _native
from dued._codec import loads, repo_of
from dued.progress import ProgressUI

__all__ = ["ingest_profile", "launch_or_attach"]


def ingest_profile(conn: object, profile_path: Path) -> dict[str, object]:
    return loads(_native.ingest_profile(str(repo_of(conn)), str(profile_path)))


def launch_or_attach(
    repo: Path,
    lang: str,
    pid: int | None,
    command: list[str],
    dest: Path,
    ui: ProgressUI | None = None,
    duration: int = 15,
) -> Path:
    task = ui.add("profile", 1) if ui is not None else None
    path = Path(_native.launch_or_attach(str(Path(repo)), lang, pid, command, str(dest), duration))
    if ui is not None:
        ui.advance(task)
    return path
