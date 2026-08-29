from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Optional

import typer

from dued import __version__
from dued.clones import find_clones, find_embed_clones, label_clusters
from dued.dead import dead_report
from dued.display import banner, emit as emit_human, print_analyze, print_brief
from dued.heatmap import write_heatmap
from dued.issues import list_issues
from dued.embed import DEFAULT_MODEL, export_label_csv, similar_lookup_error, similar_to
from dued.git_hist import analyze_history, history_report
from dued.names import analyze_names
from dued.paths import db_path, ensure_report_dir
from dued.profile import ingest_profile, launch_or_attach
from dued.progress import ProgressUI, progress_session
from dued.rank import compute_rank, reading_order
from dued.reports import refresh_report, write_report_dir
from dued.review import review_pack
from dued.scan import run_scan
from dued.slice import slice_symbol
from dued.store import connect

app = typer.Typer(
    help="Local due-diligence for Python, TypeScript, and Rust repos.",
    epilog="Typical first pass: dued analyze   then open the printed HTML path   then dued report",
    no_args_is_help=True,
)


def _repo(path: Path | None) -> Path:
    return (path or Path.cwd()).resolve()


def _emit(data: object, as_json: bool) -> None:
    emit_human(data, as_json)


def _version_callback(value: bool) -> None:
    if value:
        typer.echo(__version__)
        raise typer.Exit()


@app.callback()
def main(
    ctx: typer.Context,
    quiet: bool = typer.Option(False, "--quiet", help="Hide progress bars."),
    as_json: bool = typer.Option(False, "--json", help="Write machine-readable JSON to stdout."),
    repo: Optional[Path] = typer.Option(None, "--repo", help="Repository root. Default: current directory."),
    _version: bool = typer.Option(
        False,
        "--version",
        callback=_version_callback,
        is_eager=True,
        help="Print the dued version.",
    ),
) -> None:
    ctx.ensure_object(dict)
    ctx.obj["quiet"] = quiet or as_json
    ctx.obj["json"] = as_json
    ctx.obj["repo"] = _repo(repo)
    if ctx.obj["quiet"]:
        os.environ["DUED_QUIET"] = "1"


@app.command()
def version() -> None:
    """Print the dued version."""
    typer.echo(__version__)


@app.command()
def scan(
    ctx: typer.Context,
    max_files: Optional[int] = typer.Option(None, "--max-files"),
    budget_seconds: Optional[float] = typer.Option(None, "--budget-seconds"),
    model: str = typer.Option(DEFAULT_MODEL, "--model"),
    git: bool = typer.Option(False, "--git", help="Overlay git churn and coupling."),
    no_embed: bool = typer.Option(False, "--no-embed", help="Skip embeddings (tests only)."),
) -> None:
    """Walk, parse, measure, rank, and write dued/index.sqlite.

    This writes the index only. Use analyze for the HTML report pack.
    """
    repo = ctx.obj["repo"]
    if not ctx.obj["quiet"]:
        banner(repo, "scan", with_embed=not no_embed, with_git=git, model=model)
    with progress_session(True) as ui:
        summary = run_scan(
            repo,
            ui,
            max_files=max_files,
            budget_seconds=budget_seconds,
            model_name=model,
            with_git=git,
            with_embed=not no_embed,
        )
    _emit(summary, ctx.obj["json"])


@app.command()
def rank(ctx: typer.Context, limit: int = typer.Option(15, "--limit")) -> None:
    """Print the reading order from the current index. Does not scan again."""
    conn = connect(ctx.obj["repo"])
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("rank", 1)
        compute_rank(conn)
        order = reading_order(conn, limit=limit)
        ui.advance(task)
    conn.commit()
    conn.close()
    _emit(order, ctx.obj["json"])


@app.command()
def slice(
    ctx: typer.Context,
    symbol: str = typer.Argument(..., help="Name or path::name"),
    depth: int = typer.Option(4, "--depth"),
) -> None:
    """Show the behavior slice, effects, and blast radius for a symbol.

    Pass a name, or path::name when the name appears more than once.
    """
    conn = connect(ctx.obj["repo"])
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("slice", 1)
        data = slice_symbol(conn, symbol, depth=depth)
        dest = ensure_report_dir(ctx.obj["repo"])
        files = set(data.get("files") or [])
        if files:
            write_heatmap(conn, dest / "slice-heatmap.svg", slice_files=files)
        ui.advance(task)
    conn.close()
    _emit(data, ctx.obj["json"])


@app.command()
def dead(ctx: typer.Context) -> None:
    """List unused symbols and isolated files."""
    conn = connect(ctx.obj["repo"])
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("dead code", 1)
        data = dead_report(conn)
        ui.advance(task)
    conn.close()
    _emit(data, ctx.obj["json"])


@app.command()
def issues(ctx: typer.Context) -> None:
    """List god functions, effect-in-core, and shotgun-surgery flags."""
    conn = connect(ctx.obj["repo"])
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("issues", 1)
        data = list_issues(conn)
        ui.advance(task)
    conn.close()
    _emit(data, ctx.obj["json"])


@app.command()
def names(ctx: typer.Context) -> None:
    """Report symbol name-health flags."""
    conn = connect(ctx.obj["repo"])
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("name health", 1)
        flags = analyze_names(conn)
        conn.commit()
        ui.advance(task)
    conn.close()
    _emit(flags, ctx.obj["json"])


@app.command()
def cluster(
    ctx: typer.Context,
    similar: Optional[str] = typer.Option(None, "--similar-to"),
) -> None:
    """Token clones and optional similar-to query."""
    conn = connect(ctx.obj["repo"])
    if similar:
        err = similar_lookup_error(conn, similar)
        if err:
            conn.close()
            _emit({"error": err, "clones": [], "clusters": [], "similar": []}, ctx.obj["json"])
            raise typer.Exit(code=1)
        with progress_session(ctx.obj["quiet"]) as ui:
            task = ui.add("similar", 1)
            near = similar_to(conn, similar)
            conn.commit()
            ui.advance(task)
        conn.close()
        _emit({"clones": [], "clusters": [], "similar": near}, ctx.obj["json"])
        return
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("cluster", 1)
        clones = find_clones(conn)
        clones.extend(find_embed_clones(conn))
        clusters = label_clusters(conn)
        conn.commit()
        ui.advance(task)
    conn.close()
    _emit({"clones": clones, "clusters": clusters, "similar": []}, ctx.obj["json"])


@app.command()
def history(ctx: typer.Context) -> None:
    """Git churn, coupling, and bus factor. Refines rank."""
    repo = ctx.obj["repo"]
    conn = connect(repo)
    with progress_session(ctx.obj["quiet"]) as ui:
        info = analyze_history(repo, conn, ui)
        compute_rank(conn)
        report = history_report(conn)
        conn.commit()
    conn.close()
    _emit({"summary": info, **report}, ctx.obj["json"])


@app.command()
def heatmap(ctx: typer.Context) -> None:
    """Write SVG and HTML treemap heatmaps."""
    repo = ctx.obj["repo"]
    dest = ensure_report_dir(repo)
    conn = connect(repo)
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("heatmap", 1)
        data = write_heatmap(conn, dest / "heatmap.svg")
        ui.advance(task)
    conn.close()
    _emit(data, ctx.obj["json"])


@app.command("ingest-profile")
def ingest_profile_cmd(
    ctx: typer.Context,
    profile: Path = typer.Argument(..., exists=True, readable=True),
) -> None:
    """Overlay an existing speedscope or CPU profile on the index."""
    conn = connect(ctx.obj["repo"])
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("ingest profile", 1)
        data = ingest_profile(conn, profile)
        compute_rank(conn)
        conn.commit()
        ui.advance(task)
    conn.close()
    _emit(data, ctx.obj["json"])


@app.command()
def profile(
    ctx: typer.Context,
    lang: str = typer.Option(..., "--lang", help="python | ts | rust"),
    pid: Optional[int] = typer.Option(None, "--pid"),
    duration: int = typer.Option(15, "--duration"),
    command: Optional[list[str]] = typer.Argument(None),
) -> None:
    """Launch or attach a profiler, then ingest the result."""
    repo = ctx.obj["repo"]
    dest = ensure_report_dir(repo)
    out = dest / "profile.speedscope.json"
    conn = connect(repo)
    with progress_session(ctx.obj["quiet"]) as ui:
        path = launch_or_attach(repo, lang, pid, command or [], out, ui, duration=duration)
        data = ingest_profile(conn, path)
        compute_rank(conn)
        conn.commit()
    conn.close()
    _emit(data, ctx.obj["json"])


@app.command()
def analyze(
    ctx: typer.Context,
    max_files: Optional[int] = typer.Option(None, "--max-files"),
    budget_seconds: Optional[float] = typer.Option(None, "--budget-seconds"),
    model: str = typer.Option(DEFAULT_MODEL, "--model"),
    git: bool = typer.Option(True, "--git/--no-git"),
    no_embed: bool = typer.Option(False, "--no-embed"),
) -> None:
    """Full due-diligence pack: scan, reports, HTML, and review brief.

    Progress prints on stderr. The first Jina load can take several minutes.
    After it finishes, open the printed report.html path to search the full index.
    """
    repo = ctx.obj["repo"]
    if not ctx.obj["quiet"]:
        banner(repo, "analyze", with_embed=not no_embed, with_git=git, model=model)
    ui = ProgressUI(quiet=True)
    summary = run_scan(
        repo,
        ui,
        max_files=max_files,
        budget_seconds=budget_seconds,
        model_name=model,
        with_git=git,
        with_embed=not no_embed,
    )
    conn = connect(repo)
    dest = write_report_dir(repo, conn, extra={"scan": summary})
    review_pack(conn, dest)
    conn.close()
    payload = {"report": str(dest), **summary}
    if ctx.obj["json"]:
        typer.echo(json.dumps(payload, indent=2, default=str))
    else:
        print_analyze(payload)


@app.command()
def report(ctx: typer.Context) -> None:
    """Rebuild the HTML explorer from the index and print a short brief.

    Does not scan again. Open report.html in a browser to search the full pack.
    """
    repo = ctx.obj["repo"]
    index_file = db_path(repo)
    if not index_file.is_file():
        typer.echo("no index yet. run: dued analyze", err=True)
        typer.echo(f"expected {index_file}", err=True)
        raise typer.Exit(code=1)
    conn = connect(repo)
    n = conn.execute("SELECT COUNT(*) AS n FROM files").fetchone()["n"]
    if n == 0:
        conn.close()
        typer.echo("empty index. run: dued analyze", err=True)
        raise typer.Exit(code=1)
    dest = refresh_report(repo, conn)
    conn.close()
    html = dest / "report.html"
    data = json.loads((dest / "index.json").read_text(encoding="utf-8"))
    if ctx.obj["json"]:
        typer.echo(json.dumps(data, indent=2, default=str))
    else:
        print_brief(data, html)


@app.command()
def review(
    ctx: typer.Context,
    symbol: Optional[str] = typer.Option(None, "--slice"),
) -> None:
    """Write a human review pack from the current index."""
    repo = ctx.obj["repo"]
    dest = ensure_report_dir(repo)
    conn = connect(repo)
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("review pack", 1)
        review_pack(conn, dest, slice_query=symbol)
        ui.advance(task)
    conn.close()
    _emit({"report": str(dest)}, ctx.obj["json"])


@app.command()
def label(
    ctx: typer.Context,
    dest: Optional[Path] = typer.Option(None, "--out"),
) -> None:
    """Export mismatch flags as a CSV for later human scoring / LoRA."""
    repo = ctx.obj["repo"]
    out = dest or (ensure_report_dir(repo) / "labels.csv")
    conn = connect(repo)
    with progress_session(ctx.obj["quiet"]) as ui:
        task = ui.add("export labels", 1)
        count = export_label_csv(conn, out)
        ui.advance(task)
    conn.close()
    _emit({"rows": count, "path": str(out)}, ctx.obj["json"])


@app.command()
def index_path(ctx: typer.Context) -> None:
    """Print the SQLite index path."""
    typer.echo(str(db_path(ctx.obj["repo"])))


if __name__ == "__main__":
    app()
