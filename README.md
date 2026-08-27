# dued

[![CI](https://github.com/MHHukiewitz/dued/actions/workflows/ci.yml/badge.svg)](https://github.com/MHHukiewitz/dued/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Local due-diligence CLI for Python, TypeScript, and Rust.

`dued` walks a repository, builds a SQLite index, ranks what to read first, traces behavior slices, and writes an HTML explorer. It does not send source code to a third-party analysis API. Embeddings run on this machine.

The published command is the Python package. That package wraps a Rust engine through `dued._native`.

## Install

```bash
pip install dued
```

Wheels include the compiled Rust extension. An sdist build needs a Rust toolchain.

From this repository:

```bash
git clone https://github.com/MHHukiewitz/dued.git
cd dued
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

```bash
dued --help
python -m dued --help
```

See [docs/install.md](docs/install.md) for build notes.

## First pass

```bash
cd /path/to/repo
dued analyze
```

`dued analyze` prints a plan, then walks, parses, ranks, and writes reports. Progress goes to stderr. The first Jina model load can take several minutes. Later runs reuse the local cache.

When the run ends, stdout prints the HTML path. Open that file in a browser. The HTML explorer is the main human report.

```text
dued/2026-08-27_11-20-00/report.html
```

Each analyze run writes a new dated pack. `dued report` rebuilds the newest pack from the index. Open `report.html` in that folder.

Query without a new scan:

```bash
dued report
dued rank
dued issues
dued dead
dued names
dued cluster
dued slice get_user
```

Use `--repo` when you are not in the target tree:

```bash
dued --repo /path/to/repo analyze
```

See [docs/quickstart.md](docs/quickstart.md) and [docs/cli.md](docs/cli.md).

## What it writes

Everything lands in one `dued/` folder in the target repository:

```text
dued/index.sqlite
dued/YYYY-MM-DD_HH-MM-SS/report.html
dued/YYYY-MM-DD_HH-MM-SS/data/
dued/YYYY-MM-DD_HH-MM-SS/agent.json
```

| Path | Role |
| --- | --- |
| `dued/index.sqlite` | Local index |
| `dued/<stamp>/report.html` | Searchable explorer (files, symbols, issues, names, clones) |
| `dued/<stamp>/data/` | Compact JSON tables for the explorer |
| `dued/<stamp>/agent.json` | Short pack for an agent or reviewer |
| `dued/<stamp>/index.json` | Summary counts and reading order |

`<stamp>` is local time, for example `2026-08-27_11-20-00`.

Do not commit `dued/` unless you mean to share an index.

See [docs/reports.md](docs/reports.md).

## Commands

```text
dued analyze              # scan, index, HTML pack
dued report               # rebuild the newest HTML pack from the index
dued scan                 # index only
dued rank                 # PageRank reading order
dued slice <symbol>       # behavior slice, effects, blast radius
dued dead                 # unused symbols, isolated files, hollow stubs
dued issues               # god functions, I/O in core, shotgun surgery
dued names                # name-health and homonyms
dued cluster              # token clones; --similar-to name
dued history              # git churn, coupling, bus factor
dued heatmap              # SVG treemap in the newest pack
dued profile --lang ...   # launch or attach a profiler, then ingest
dued ingest-profile <f>   # overlay an existing speedscope or CPU profile
dued review               # review JSON in the newest pack
dued label                # CSV of mismatch flags
dued --help
```

Global flags: `--repo`, `--quiet`, `--json`.

`analyze` uses git by default. Use `--no-git` when history is not useful. Use `--no-embed` in tests.

## Embeddings

Default model: `jinaai/jina-embeddings-v2-base-code` through ONNX Runtime.

The first run can download weights into the local Hugging Face cache. Tests and CI must not do that:

```bash
DUED_STUB_EMBED=1 dued analyze --no-embed --no-git
DUED_STUB_EMBED=1 pytest
```

`--model stub` also uses hash vectors.

See [docs/embeddings.md](docs/embeddings.md).

## Design

- Analysis runs locally.
- The Python package is the public CLI and the PyPI artifact.
- The Rust crate is the engine. Python modules call `dued._native`.
- Supported languages: Python, TypeScript / JavaScript, Rust.

See [docs/architecture.md](docs/architecture.md).

## Documentation

- [Install](docs/install.md)
- [Quick start](docs/quickstart.md)
- [CLI reference](docs/cli.md)
- [Reports](docs/reports.md)
- [Architecture](docs/architecture.md)
- [Embeddings](docs/embeddings.md)
- [Development](docs/development.md)
- [Publishing](docs/publishing.md)

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).

To report a security issue, use [SECURITY.md](SECURITY.md).

## License

MIT. See [LICENSE](LICENSE).
