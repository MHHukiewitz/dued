# Development

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the contribution process.

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

This compiles `dued._native` from `dued-rs/Cargo.toml` with `extension-module` only. Real Jina ONNX needs `--features jina` (see [embeddings.md](embeddings.md)).

## Test

```bash
DUED_STUB_EMBED=1 pytest
cargo test --manifest-path dued-rs/Cargo.toml
```

The pytest suite lives in `tests/`. The mini fixture is `tests/fixtures/mini/`.

## Native binary

```bash
cargo build --release --manifest-path dued-rs/Cargo.toml --bin dued
```

Compare the Python CLI and that binary:

```bash
DUED_STUB_EMBED=1 python3 scripts/compare_impls.py
```

Speed helper:

```bash
DUED_STUB_EMBED=1 python3 scripts/bench_bindings.py
```

Those scripts wipe `dued/` on the target they measure. Do not point them at an index you need unless the script stashes it first.

## Project map

| Path | Role |
| --- | --- |
| `src/dued/cli.py` | Typer commands |
| `src/dued/*.py` | Thin wrappers over `_native` |
| `dued-rs/src/python.rs` | PyO3 exports |
| `dued-rs/src/scan.rs` | Scan pipeline |
| `dued-rs/src/parse.rs` | tree-sitter extractors |
| `dued-rs/src/graph.rs` | Call and import resolution |
| `dued-rs/src/rank.rs` | Reading order |
| `dued-rs/src/explorer.rs` | HTML pack |
| `dued-rs/src/store.rs` | SQLite schema |
| `.github/workflows/ci.yml` | Install, pytest, cargo test |
| `.github/workflows/publish.yml` | Wheel and sdist publish on `v*` tags |

## CI

Push and pull request runs install the package, then:

```text
DUED_STUB_EMBED=1 pytest
cargo test --manifest-path dued-rs/Cargo.toml
```
