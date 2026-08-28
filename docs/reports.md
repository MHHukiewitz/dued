# Reports

`dued analyze` writes the index and a timestamped pack in one folder:

```text
dued/index.sqlite
dued/YYYY-MM-DD_HH-MM-SS/
```

The pack folder uses local time, for example `2026-08-27_11-20-00`. Open `report.html` in that folder. `dued analyze` prints the path.

`dued report` rebuilds the newest dated pack from `dued/index.sqlite`. It does not walk the tree again.

## HTML explorer

Open `dued/<stamp>/report.html`.

The page is self-contained. Catalog tables are embedded as JSON. Use the search box, language filter, and kind tabs to sort files, symbols, issues, names, clones, and dead code.

## JSON tables

`dued/<stamp>/data/` holds compact tables. These files omit bodies and embeddings:

- `files.json`
- `symbols.json`
- `issues.json`
- plus other list tables used by the explorer

## Other pack files

| File | Role |
| --- | --- |
| `report.html` | Human explorer |
| `index.json` | Summary counts and reading order |
| `agent.json` | Compact pack for an agent or reviewer |
| `rank.json` | Reading order |
| `review.json` | Structured review pack |
| `heatmap.svg` | Treemap |
| `labels.csv` | Optional mismatch export from `dued label` |

## Index

The index is `dued/index.sqlite` in the target repository.

Schema highlights:

- `files` — path, language, size, test flag, git and profile overlays
- `symbols` — name, kind, span, metrics, effects, risks, embeddings
- `edges` / `call_facts` / `import_facts` — graph inputs
- `meta` — schema version, parser version, and scan metadata

When the stored parser version does not match the engine, the next scan re-parses every walked file even if digests match. You do not need `--force` or a deleted index after a parse-rule upgrade.
