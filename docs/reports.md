# Reports

`dued analyze` writes a timestamped directory and a `latest` pointer:

```text
dued-reports/<YYYYMMDD-HHMMSS>/
dued-reports/latest/
```

`dued report` rebuilds the explorer from `.dued/index.sqlite`. It does not walk the tree again.

## HTML explorer

Open `dued-reports/latest/report.html`.

The page is self-contained. Catalog tables are embedded as JSON. Use the search box, language filter, and kind tabs to sort files, symbols, issues, names, clones, and dead code.

## JSON tables

`dued-reports/latest/data/` holds compact tables. These files omit bodies and embeddings:

- `files.json`
- `symbols.json`
- `issues.json`
- plus other list tables used by the explorer

## Other pack files

| File | Role |
| --- | --- |
| `brief.md` | Short human brief |
| `index.json` | Summary counts and reading order |
| `agent.json` | Compact pack for an agent or reviewer |
| `rank.json` | Reading order |
| `reading_order.md` | Reading order as Markdown |
| `questions.md` | Review questions |
| `review.json` | Structured review pack |
| `heatmap.svg` | Treemap |
| `labels.csv` | Optional mismatch export from `dued label` |

## Index

The index is `.dued/index.sqlite` in the target repository.

Schema highlights:

- `files` — path, language, size, test flag, git and profile overlays
- `symbols` — name, kind, span, metrics, effects, risks, embeddings
- `edges` / `call_facts` / `import_facts` — graph inputs
- `meta` — schema version and scan metadata

Do not commit `.dued/` or `dued-reports/` unless you have a reason to share an index.
