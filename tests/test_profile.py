import json
from pathlib import Path

from dued.profile import ingest_profile
from dued.store import connect


def test_ingest_speedscope(tmp_path: Path) -> None:
    repo = tmp_path / "r"
    repo.mkdir()
    conn = connect(repo)
    conn.execute(
        "INSERT INTO files(relpath, language, digest, loc, size, is_test) VALUES (?,?,?,?,?,?)",
        ("app.py", "python", "x", 10, 10, 0),
    )
    fid = conn.execute("SELECT id FROM files").fetchone()["id"]
    conn.execute(
        """
        INSERT INTO symbols(file_id, name, kind, start_line, end_line, signature, docstring, body,
            cyclomatic, cognitive, nesting, nargs, is_public, is_entry, is_test)
        VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        """,
        (fid, "process", "function", 1, 5, "def process()", "", "def process():\n    return 1\n", 1, 0, 0, 0, 1, 0, 0),
    )
    profile = tmp_path / "cpu.json"
    profile.write_text(
        json.dumps(
            {
                "shared": {"frames": [{"name": "process"}, {"name": "main"}]},
                "profiles": [{"samples": [[0], [0], [1]], "weights": [4, 2, 1]}],
            }
        ),
        encoding="utf-8",
    )
    result = ingest_profile(conn, profile)
    row = conn.execute("SELECT profile_self, profile_total FROM files WHERE id = ?", (fid,)).fetchone()
    assert result["mapped"] >= 1
    assert row["profile_total"] == 6.0
    conn.close()
