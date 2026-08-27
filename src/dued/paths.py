from __future__ import annotations

import re
from datetime import datetime
from pathlib import Path

WORK_DIR_NAME = "dued"
DB_NAME = "index.sqlite"
_STAMP = re.compile(r"^\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}(_\d+)?$")


def repo_root(path: Path) -> Path:
    return path.resolve()


def work_dir(repo: Path) -> Path:
    return repo_root(repo) / WORK_DIR_NAME


def index_dir(repo: Path) -> Path:
    return work_dir(repo)


def db_path(repo: Path) -> Path:
    return work_dir(repo) / DB_NAME


def report_root(repo: Path) -> Path:
    return work_dir(repo)


def is_report_stamp(name: str) -> bool:
    return bool(_STAMP.fullmatch(name))


def newest_report_dir(repo: Path) -> Path | None:
    root = report_root(repo)
    if not root.is_dir():
        return None
    names = sorted(p for p in root.iterdir() if p.is_dir() and is_report_stamp(p.name))
    if not names:
        return None
    return names[-1]


def new_report_dir(repo: Path) -> Path:
    stamp = datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
    root = report_root(repo)
    dest = root / stamp
    n = 2
    while dest.exists():
        dest = root / f"{stamp}_{n}"
        n += 1
    dest.mkdir(parents=True, exist_ok=True)
    return dest


def ensure_report_dir(repo: Path) -> Path:
    found = newest_report_dir(repo)
    if found is not None:
        return found
    return new_report_dir(repo)
