from __future__ import annotations

import json
import sqlite3
from pathlib import Path

from dued import _native
from dued._codec import repo_of
from dued.paths import db_path

__all__ = ["Index", "connect", "delete_file_row", "get_meta", "repo_of", "reset_scan_tables", "set_meta"]


class Index:
    """SQLite index for one repo. The Rust engine owns the schema."""

    def __init__(self, repo: Path) -> None:
        self.repo = Path(repo).resolve()
        _native.init_index(str(self.repo))
        self._conn = sqlite3.connect(str(db_path(self.repo)))
        self._conn.row_factory = sqlite3.Row
        self._conn.isolation_level = None

    def execute(self, sql: str, parameters: object = ()) -> sqlite3.Cursor:
        return self._conn.execute(sql, parameters)

    def commit(self) -> None:
        self._conn.commit()

    def close(self) -> None:
        self._conn.close()


def connect(repo: Path) -> Index:
    return Index(Path(repo))


def set_meta(conn: Index, key: str, value: object) -> None:
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        (key, json.dumps(value)),
    )


def get_meta(conn: Index, key: str, default: object = None) -> object:
    row = conn.execute("SELECT value FROM meta WHERE key = ?", (key,)).fetchone()
    if row is None:
        return default
    return json.loads(row["value"])


def reset_scan_tables(conn: Index) -> None:
    conn.execute("DELETE FROM issues")
    conn.execute("DELETE FROM name_flags")
    conn.execute("DELETE FROM clones")
    conn.execute("DELETE FROM edges")
    conn.execute("DELETE FROM call_facts")
    conn.execute("DELETE FROM import_facts")
    conn.execute("DELETE FROM symbols")
    conn.execute("DELETE FROM files")
    conn.execute("DELETE FROM git_coupling")


def delete_file_row(conn: Index, relpath: str) -> None:
    row = conn.execute("SELECT id FROM files WHERE relpath = ?", (relpath,)).fetchone()
    if row is None:
        return
    fid = row["id"]
    conn.execute(
        "DELETE FROM name_flags WHERE symbol_id IN (SELECT id FROM symbols WHERE file_id = ?)",
        (fid,),
    )
    conn.execute(
        "DELETE FROM clones WHERE symbol_a IN (SELECT id FROM symbols WHERE file_id = ?) "
        "OR symbol_b IN (SELECT id FROM symbols WHERE file_id = ?)",
        (fid, fid),
    )
    conn.execute(
        "DELETE FROM issues WHERE file_id = ? OR symbol_id IN (SELECT id FROM symbols WHERE file_id = ?)",
        (fid, fid),
    )
    conn.execute("DELETE FROM call_facts WHERE src_file_id = ?", (fid,))
    conn.execute("DELETE FROM import_facts WHERE src_file_id = ?", (fid,))
    conn.execute("DELETE FROM edges WHERE src_file_id = ? OR dst_file_id = ?", (fid, fid))
    conn.execute("DELETE FROM symbols WHERE file_id = ?", (fid,))
    conn.execute("DELETE FROM files WHERE id = ?", (fid,))
