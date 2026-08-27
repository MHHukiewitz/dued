from __future__ import annotations

from pathlib import Path

INDEX_DIR_NAME = ".dued"
REPORT_DIR_NAME = "dued-reports"
DB_NAME = "index.sqlite"


def repo_root(path: Path) -> Path:
    return path.resolve()


def index_dir(repo: Path) -> Path:
    return repo_root(repo) / INDEX_DIR_NAME


def db_path(repo: Path) -> Path:
    return index_dir(repo) / DB_NAME


def report_root(repo: Path) -> Path:
    return repo_root(repo) / REPORT_DIR_NAME
