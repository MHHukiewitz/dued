import json
import os
from pathlib import Path

from typer.testing import CliRunner

from dued.cli import app

FIXTURE = Path(__file__).parent / "fixtures" / "mini"
runner = CliRunner()


def test_version() -> None:
    result = runner.invoke(app, ["version"])
    assert result.exit_code == 0, result.output
    assert "0.1.0" in result.stdout


def test_analyze_writes_reports(tmp_path: Path) -> None:
    os.environ["DUED_STUB_EMBED"] = "1"
    repo = tmp_path / "mini"
    repo.mkdir()
    for src in FIXTURE.iterdir():
        if src.is_file():
            (repo / src.name).write_bytes(src.read_bytes())
    result = runner.invoke(
        app,
        ["--repo", str(repo), "--quiet", "--json", "analyze", "--no-embed", "--no-git"],
    )
    assert result.exit_code == 0, result.output
    payload = json.loads(result.stdout)
    dest = Path(payload["report"])
    assert (dest / "brief.md").is_file()
    assert (dest / "agent.json").is_file()
    assert (dest / "reading_order.md").is_file()
    assert (dest / "questions.md").is_file()
    assert (dest / "heatmap.svg").is_file()
    assert (dest / "report.html").is_file()
    html = (dest / "report.html").read_text(encoding="utf-8")
    assert "Reading order" in html
    assert "Explore from the CLI" in html
    assert "dued-data" in html
    assert "Search the index" in html
    assert "data-tab-sec" in html
    assert "kindTabs" in html
    assert (dest / "data" / "symbols.json").is_file()
    assert (dest / "data" / "files.json").is_file()
    symbols = json.loads((dest / "data" / "symbols.json").read_text(encoding="utf-8"))
    assert isinstance(symbols, list)
    assert len(symbols) >= 1
    shown = runner.invoke(app, ["--repo", str(repo), "--quiet", "report"])
    assert shown.exit_code == 0, shown.output
    assert "dued report" in shown.output
    assert "HTML report" in shown.output
    latest = dest.parent / "latest"
    assert latest.is_symlink() or latest.is_dir()
    label = runner.invoke(app, ["--repo", str(repo), "--quiet", "--json", "label"])
    assert label.exit_code == 0, label.output
