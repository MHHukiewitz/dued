---
name: dued
description: Run the dued due-diligence CLI and read its report files for Python, TypeScript, and Rust repos.
---

# dued playbook

Use the `dued` CLI. Do not add an MCP server.

The published CLI is the Python package: `pip install dued`. It is a binding layer over the Rust engine. Import `dued.walk`, `dued.parse`, `dued.scan`, and the other modules from Python. The command writes `dued/` (index and reports).

## First pass

```bash
dued analyze --repo <path> --quiet --json
```

If git history is not useful:

```bash
dued analyze --repo <path> --no-git --quiet --json
```

Tests must not download models:

```bash
DUED_STUB_EMBED=1 dued analyze --repo <path> --no-embed --no-git --quiet --json
```

Read the newest `dued/<stamp>/agent.json` first. Open `report.html` in that folder to search and sort the full index. JSON tables are in `data/`. Use `dued report` to rebuild the HTML explorer from the SQLite index. It does not scan again.

## Next queries

```bash
dued slice <symbol> --quiet --json
dued dead --quiet --json
dued issues --quiet --json
dued names --quiet --json
dued cluster --similar-to <symbol> --quiet --json
```

## Profiles

```bash
dued ingest-profile path/to/profile.speedscope.json --quiet --json
```

Launch only when the user asks and the profiler is on PATH.

## Later labels

`dued label` writes a CSV for human scores. Do not train a model in this tool.
