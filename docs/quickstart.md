# Quick start

## First pass

```bash
cd /path/to/repo
dued analyze
```

`dued analyze` prints a plan, then walks, parses, ranks, and writes reports. Progress goes to stderr.

The first Jina model load can take several minutes. Later runs reuse the local cache.

When the run ends, stdout is a short text summary. Pass `--json` for machine-readable output.

Open:

```text
dued-reports/latest/report.html
```

The HTML explorer includes the compact catalog: files, symbols, issues, names, clones, and reading order.

## After the first scan

These commands read `.dued/index.sqlite`. They do not walk the tree again.

```bash
dued report
dued rank
dued issues
dued dead
dued names
dued cluster
dued slice get_user
```

Use `--repo` when you are not in the target repository:

```bash
dued --repo /path/to/repo analyze
```

## Useful flags

```bash
dued analyze --no-git
dued analyze --no-embed
dued analyze --max-files 200
dued analyze --budget-seconds 60
dued --quiet --json analyze --no-embed --no-git
```

`--no-git` skips history mining. Use it when the tree has no useful git history.

`--no-embed` skips ONNX embeddings. Tests should pass this flag and set `DUED_STUB_EMBED=1`.

## Agent workflow

1. Run `dued analyze --quiet --json`.
2. Read `dued-reports/latest/agent.json`.
3. Read `brief.md` and `reading_order.md`.
4. Open `report.html` to search and sort.
5. Use `dued slice <symbol>` before you change behavior.

A Cursor playbook lives in [`.cursor/skills/dued/SKILL.md`](../.cursor/skills/dued/SKILL.md).
