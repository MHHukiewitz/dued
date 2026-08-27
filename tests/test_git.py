import subprocess
from pathlib import Path

from dued.git_hist import analyze_history
from dued.progress import ProgressUI
from dued.store import connect


def _git(repo: Path, args: list[str]) -> None:
    result = subprocess.run(["git", *args], cwd=repo, capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(result.stderr)


def test_history_churn(tmp_path: Path) -> None:
    repo = tmp_path / "g"
    repo.mkdir()
    _git(repo, ["init"])
    _git(repo, ["config", "user.email", "dev@example.com"])
    _git(repo, ["config", "user.name", "Dev"])
    (repo / "a.py").write_text("x = 1\n", encoding="utf-8")
    (repo / "b.py").write_text("y = 1\n", encoding="utf-8")
    _git(repo, ["add", "."])
    _git(repo, ["commit", "-m", "one"])
    (repo / "a.py").write_text("x = 2\n", encoding="utf-8")
    (repo / "b.py").write_text("y = 2\n", encoding="utf-8")
    _git(repo, ["add", "."])
    _git(repo, ["commit", "-m", "two"])
    (repo / "a.py").write_text("x = 3\n", encoding="utf-8")
    (repo / "b.py").write_text("y = 3\n", encoding="utf-8")
    _git(repo, ["add", "."])
    _git(repo, ["commit", "-m", "three"])
    conn = connect(repo)
    conn.execute(
        "INSERT INTO files(relpath, language, digest, loc, size, is_test) VALUES (?,?,?,?,?,?)",
        ("a.py", "python", "x", 1, 1, 0),
    )
    conn.execute(
        "INSERT INTO files(relpath, language, digest, loc, size, is_test) VALUES (?,?,?,?,?,?)",
        ("b.py", "python", "y", 1, 1, 0),
    )
    info = analyze_history(repo, conn, ProgressUI(quiet=True))
    assert info["enabled"] is True
    assert info["commits"] >= 3
    a = conn.execute("SELECT churn, authors FROM files WHERE relpath='a.py'").fetchone()
    assert a["churn"] > 0
    assert a["authors"] >= 1
    conn.close()
