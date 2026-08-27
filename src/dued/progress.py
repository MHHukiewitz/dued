from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager

from rich.progress import (
    BarColumn,
    MofNCompleteColumn,
    Progress,
    SpinnerColumn,
    TaskID,
    TextColumn,
    TimeElapsedColumn,
    TimeRemainingColumn,
)


class ProgressUI:
    """Progress bars with ETA. Quiet mode prints nothing."""

    def __init__(self, quiet: bool) -> None:
        self.quiet = quiet
        self._progress: Progress | None = None

    def start(self) -> None:
        if self.quiet:
            return
        self._progress = Progress(
            SpinnerColumn(),
            TextColumn("[bold]{task.description}"),
            BarColumn(),
            MofNCompleteColumn(),
            TimeElapsedColumn(),
            TimeRemainingColumn(),
        )
        self._progress.start()

    def stop(self) -> None:
        if self._progress is not None:
            self._progress.stop()
            self._progress = None

    def add(self, description: str, total: int | None) -> TaskID | None:
        if self._progress is None:
            return None
        return self._progress.add_task(description, total=total)

    def advance(self, task_id: TaskID | None, step: int = 1) -> None:
        if self._progress is None or task_id is None:
            return
        self._progress.advance(task_id, step)

    def update(self, task_id: TaskID | None, **kwargs: object) -> None:
        if self._progress is None or task_id is None:
            return
        self._progress.update(task_id, **kwargs)


@contextmanager
def progress_session(quiet: bool) -> Iterator[ProgressUI]:
    ui = ProgressUI(quiet)
    ui.start()
    yield ui
    ui.stop()
