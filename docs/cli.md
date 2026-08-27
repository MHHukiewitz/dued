# CLI reference

The entry point is `dued` from the Python package. `python -m dued` runs the same app.

## Global flags

These flags go before the subcommand:

```text
dued [GLOBAL] COMMAND
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `--repo PATH` | current directory | Repository root |
| `--quiet` | off | Hide progress on stderr |
| `--json` | off | Write JSON to stdout |

`--json` also hides progress.

## `dued version`

Print the package version.

## `dued analyze`

Full pack: scan, index, and HTML explorer.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--max-files N` | none | Stop after N source files |
| `--budget-seconds N` | none | Stop the scan after N seconds |
| `--model NAME` | Jina code model | Embed model, or `stub` |
| `--git` / `--no-git` | `--git` | Mine git churn and coupling |
| `--no-embed` | off | Skip embeddings |

## `dued scan`

Write `dued/index.sqlite` only. No report pack.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--max-files N` | none | Stop after N source files |
| `--budget-seconds N` | none | Stop after N seconds |
| `--model NAME` | Jina code model | Embed model, or `stub` |
| `--git` | off | Overlay git history |
| `--no-embed` | off | Skip embeddings |

`scan` does not enable git unless you pass `--git`. `analyze` enables git by default.

## `dued report`

Rebuild `report.html` and the compact JSON tables in the newest dated pack. Print a short summary. Does not scan again.

## `dued rank`

Print the PageRank reading order.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--limit N` | 15 | How many symbols to print |

## `dued slice <symbol>`

Show the behavior slice, effects, and blast radius.

Pass a unique name. When the bare name matches more than one symbol, the command
returns an ambiguity error with candidates. Qualify as `path::name`.

Unresolved call edges (generic or ambiguous callees such as `new` / `get`) stay
listed under `unresolved_callees`. They do not expand the blast radius.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--depth N` | 4 | Call-graph depth |

## `dued dead`

List unused symbols, isolated files, and hollow stubs.

## `dued issues`

List god functions, I/O in core, and shotgun-surgery flags.

## `dued names`

Report name-health flags and homonyms.

## `dued cluster`

Token clones and embed clusters.

| Flag | Meaning |
| --- | --- |
| `--similar-to NAME` | Nearest neighbors for one symbol |

## `dued history`

Mine git churn, coupling, and bus factor. Refines rank.

## `dued heatmap`

Write SVG and HTML treemap heatmaps under the newest dated pack in `dued/`.

## `dued review`

Write `review.json` into the newest dated pack.

| Flag | Meaning |
| --- | --- |
| `--slice NAME` | Focus the pack on one symbol |

## `dued profile`

Launch or attach a profiler, then ingest the result.

| Flag | Meaning |
| --- | --- |
| `--lang python\|ts\|rust` | Profiler family |
| `--pid N` | Attach to a process |
| `--duration N` | Attach window in seconds |
| command after `--` | Process to launch |

Examples:

```text
dued profile --lang python -- python app.py
dued profile --pid 12345 --lang python
dued profile --lang ts -- node server.js
dued profile --lang rust -- cargo run
```

Python needs `py-spy`. TypeScript uses Node `--cpu-prof`. Rust uses `samply` or `cargo flamegraph`.

## `dued ingest-profile <file>`

Overlay an existing speedscope or CPU profile on the index.

## `dued label`

Export mismatch flags as CSV.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--out PATH` | newest `dued/<stamp>/labels.csv` | Output file |

## `dued index-path`

Print the SQLite path for `--repo`.
