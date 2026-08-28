from __future__ import annotations

import json
from pathlib import Path

from rich.console import Console
from rich.table import Table

from dued import __version__
from dued.paths import db_path, report_root

console = Console()


def banner(repo: Path, command: str, with_embed: bool, with_git: bool, model: str) -> None:
    console.print(f"[bold]dued[/bold] {__version__}")
    console.print(f"repo     {repo}")
    console.print(f"index    {db_path(repo)}")
    console.print(f"reports  {report_root(repo)}")
    console.print(f"command  {command}")
    steps = ["walk source files", "parse + metrics", "build call graph"]
    if with_git:
        steps.append("git history")
    steps.append("rank + name health")
    if with_embed:
        if model == "stub":
            steps.append("embed symbols (stub vectors)")
        else:
            steps.append("embed symbols (Jina, local ONNX)")
    else:
        steps.append("skip embeddings")
    steps.append("write reports")
    console.print("plan     " + " → ".join(steps))
    if with_embed and model != "stub":
        console.print("note     first Jina load downloads ONNX weights (can take several minutes)")
        console.print("note     later runs reuse the Hugging Face cache")
    console.print("note     use --json for machine output; [bold]dued report[/bold] to re-read results")
    console.print()


def emit(data: object, as_json: bool) -> None:
    if as_json:
        # Plain stdout: Rich soft-wrap inserts raw newlines inside JSON strings.
        print(json.dumps(data, indent=2, default=str), flush=True)
        return
    if isinstance(data, dict):
        if "report" in data and "files" in data:
            print_analyze(data)
            return
        if "reading_order" in data and "issues" in data:
            print_brief(data, None)
            return
        if "symbols" in data and "hollow" in data and "edges" not in data:
            print_dead(data)
            return
        if "blast_radius" in data or ("effects" in data and "symbols" in data and "files" in data):
            print_slice(data)
            return
        if "clones" in data and "clusters" in data:
            print_cluster(data)
            return
    if isinstance(data, list):
        print_list(data)
        return
    console.print(data)


def print_analyze(data: dict) -> None:
    console.print("[bold]Scan complete[/bold]\n")
    for key, label in (
        ("files", "files"),
        ("symbols", "symbols"),
        ("edges", "edges"),
        ("parsed", "parsed this run"),
        ("reused", "reused files"),
        ("issues", "issues"),
        ("hollow", "hollow stubs"),
        ("clones", "clones"),
        ("mismatches", "mismatches"),
        ("model", "model"),
        ("elapsed_seconds", "elapsed seconds"),
    ):
        if key in data:
            console.print(f"  {label:<18} {data[key]}")
    inv = data.get("inventory") or {}
    langs = inv.get("languages") or []
    if langs:
        text = ", ".join(f"{row.get('language')} ({row.get('n')} files)" for row in langs)
        console.print(f"  {'languages':<18} {text}")
    git = data.get("git") or {}
    if git.get("enabled"):
        console.print(f"  {'git':<18} {git.get('commits')} commits")
    report = data.get("report")
    if report:
        dest = Path(str(report))
        console.print()
        console.print("[bold]Reports[/bold]")
        console.print(f"  HTML     {dest / 'report.html'}")
        console.print(f"  folder   {dest}")
        console.print()
        console.print("[bold]Explore[/bold]")
        console.print("  open the HTML file in a browser")
        console.print("  dued report              re-print this summary from disk")
        console.print("  dued rank                reading order")
        console.print("  dued issues              flagged problems")
        console.print("  dued dead                unused symbols and files")
        console.print("  dued names               name-health flags")
        console.print("  dued cluster             clones")
        console.print("  dued slice <symbol>      behavior slice")


def print_brief(data: dict, html: Path | None) -> None:
    console.print("[bold]dued report[/bold]\n")
    console.print(f"  {'repo':<18} {data.get('repo', '')}")
    console.print(f"  {'files':<18} {data.get('files', '')}")
    console.print(f"  {'symbols':<18} {data.get('symbols', '')}")
    langs = data.get("languages") or []
    if langs:
        text = ", ".join(f"{row.get('language')}={row.get('n')}" for row in langs)
        console.print(f"  {'languages':<18} {text}")
    console.print()
    table = Table(title="Reading order")
    table.add_column("#", style="dim")
    table.add_column("symbol")
    table.add_column("why")
    for i, item in enumerate(data.get("reading_order") or [], start=1):
        table.add_row(str(i), f"{item.get('relpath')}::{item.get('name')}", str(item.get("why") or ""))
    console.print(table)
    issues = data.get("issues") or []
    if issues:
        console.print()
        itable = Table(title=f"Issues ({len(issues)})")
        itable.add_column("kind")
        itable.add_column("where")
        itable.add_column("detail")
        for item in issues[:15]:
            name = item.get("name") or ""
            loc = f"{item.get('relpath') or ''}::{name}" if name else str(item.get("relpath") or "")
            itable.add_row(str(item.get("kind") or ""), loc, str(item.get("detail") or ""))
        console.print(itable)
    if html is not None:
        console.print()
        console.print("[bold]HTML report[/bold]")
        console.print(f"  {html}")
        console.print("  open that file in a browser to search and sort the full index")
        console.print("  JSON tables are in the data/ folder next to the HTML file")


def print_dead(data: dict) -> None:
    symbols = data.get("symbols") or []
    files = data.get("files") or []
    hollow = data.get("hollow") or []
    console.print("[bold]Dead code[/bold]")
    console.print(f"  unused symbols  {len(symbols)}")
    console.print(f"  isolated files  {len(files)}")
    console.print(f"  hollow stubs    {len(hollow)}")
    console.print()
    for item in symbols[:25]:
        console.print(f"  {item.get('relpath')}::{item.get('name')}  {item.get('signature') or ''}")


def print_slice(data: dict) -> None:
    console.print("[bold]Behavior slice[/bold]\n")
    console.print(f"  {'query':<18} {data.get('query', '')}")
    if data.get("error"):
        console.print(f"  {'error':<18} {data.get('error')}")
        candidates = data.get("candidates") or []
        if candidates:
            console.print()
            console.print("Candidates (use path::name)")
            for item in candidates[:20]:
                console.print(f"  {item.get('relpath')}::{item.get('name')}")
        return
    console.print(f"  {'blast radius':<18} {data.get('blast_radius', '')}")
    effects = data.get("effects") or []
    if effects:
        tags = ", ".join(str(x) for x in effects)
        console.print(f"  {'effects':<18} {tags}")
    unresolved = data.get("unresolved_callees") or []
    if unresolved:
        tags = ", ".join(str(x) for x in unresolved[:12])
        console.print(f"  {'unresolved':<18} {tags}")


def print_cluster(data: dict) -> None:
    clones = data.get("clones") or []
    console.print("[bold]Clusters[/bold]")
    console.print(f"  clone pairs  {len(clones)}")


def print_list(items: list) -> None:
    if not items:
        console.print("(no rows)")
        return
    first = items[0] if isinstance(items[0], dict) else {}
    if first.get("why") and first.get("name"):
        table = Table(title="Reading order")
        table.add_column("#", style="dim")
        table.add_column("symbol")
        table.add_column("why")
        for i, item in enumerate(items, start=1):
            table.add_row(str(i), f"{item.get('relpath')}::{item.get('name')}", str(item.get("why") or ""))
        console.print(table)
        return
    if first.get("kind") and first.get("detail"):
        table = Table(title=f"Flags ({len(items)})")
        table.add_column("kind")
        table.add_column("where")
        table.add_column("detail")
        for item in items:
            name = item.get("name") or ""
            loc = f"{item.get('relpath') or ''}::{name}" if name else str(item.get("relpath") or "")
            table.add_row(str(item.get("kind") or ""), loc, str(item.get("detail") or ""))
        console.print(table)
        return
    for item in items:
        if isinstance(item, dict) and item.get("relpath") and item.get("name"):
            console.print(f"  {item['relpath']}::{item['name']}")
        else:
            console.print(f"  {item}")
