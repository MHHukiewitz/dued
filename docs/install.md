# Install

## From PyPI

```bash
pip install dued
```

This installs the `dued` command. After install:

```bash
dued --help
dued version
python -m dued --help
```

Published wheels include the compiled Rust extension `dued._native`. You do not need Rust to install a wheel.

An sdist build compiles the extension on your machine. That path needs:

- Python 3.11 or later
- A stable Rust toolchain
- A C compiler for some crates

## From this repository

```bash
git clone https://github.com/MHHukiewitz/dued.git
cd dued
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

`pip install` uses maturin. It builds `dued-rs/` with the `extension-module` feature. That default path does not compile `ort`. Use stub embeddings or `--no-embed` unless you add the Cargo `jina` feature.

## Optional native binary

A local Cargo build also produces a `dued` binary. It uses the same index and report paths as the Python CLI.

```bash
cargo build --release --manifest-path dued-rs/Cargo.toml --bin dued
./dued-rs/target/release/dued --help
```

`pip install dued` is the published install path.

## Supported languages in the target repo

`dued` reads:

- Python (`.py`)
- TypeScript and JavaScript (`.ts`, `.tsx`, `.js`, `.jsx`)
- Rust (`.rs`)

It skips common vendor and build directories such as `.git`, `.venv`, `node_modules`, and `target`.

## Environment

| Variable | Effect |
| --- | --- |
| `DUED_STUB_EMBED=1` | Use hash vectors. Required in tests and CI. |
| `DUED_QUIET=1` | Hide progress. The `--quiet` and `--json` flags set this. |
