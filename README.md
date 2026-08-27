# dued

[![CI](https://github.com/MHHukiewitz/dued/actions/workflows/ci.yml/badge.svg)](https://github.com/MHHukiewitz/dued/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Local due-diligence CLI for Python, TypeScript, and Rust.

`dued` walks a repository, builds a local SQLite index, ranks what to read first, traces behavior slices, and writes an HTML report pack. It does not send source code to a third-party analysis API. Embeddings run on this machine.

The published command is the Python package. That package wraps a Rust engine through `dued._native`.

## Install

```bash
pip install dued
```

Wheels include the compiled Rust extension. An sdist build needs a Rust toolchain.

```bash
dued --help
python -m dued --help
```

See [docs/install.md](docs/install.md) for development installs and build notes.

## Quick start

```bash
cd /path/to/repo
dued analyze
```

Open the HTML explorer:

```text
dued-reports/latest/report.html
```

Query the index without a new scan:

```bash
dued report
dued rank
dued issues
dued dead
dued names
dued cluster
dued slice get_user
```

See [docs/quickstart.md](docs/quickstart.md) and [docs/cli.md](docs/cli.md).

## What it writes

| Path | Role |
| --- | --- |
| `.dued/index.sqlite` | Local index in the target repo |
| `dued-reports/<timestamp>/` | One report pack |
| `dued-reports/latest/` | Latest pack (HTML, JSON, brief) |

`dued report` rebuilds the explorer from SQLite. It does not scan again.

See [docs/reports.md](docs/reports.md).

## Design

- Analysis runs locally.
- The Python package is the public CLI and the PyPI artifact.
- The Rust crate is the engine. Python modules call `dued._native`.
- Supported languages: Python, TypeScript / JavaScript, Rust.
- Default embed model: `jinaai/jina-embeddings-v2-base-code` through ONNX Runtime.

See [docs/architecture.md](docs/architecture.md) and [docs/embeddings.md](docs/embeddings.md).

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

This is an open-source project. Read [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request.

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).

To report a security issue, use [SECURITY.md](SECURITY.md).

## License

MIT. See [LICENSE](LICENSE).
