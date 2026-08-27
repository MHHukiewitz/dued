# Contributing to dued

Thank you for working on `dued`. This document explains how to set up a development tree, how to test, and how to open a change.

By taking part, you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## What this project is

`dued` is a local due-diligence CLI. It walks a repository, builds a SQLite index, and writes report files.

Keep these rules:

- Do not add an MCP server.
- Do not send source code to a third-party analysis API.
- Keep embeddings on the local machine.
- The public command is `dued`. Do not introduce a second product name.
- The Python package is the published CLI. Rust is the analysis engine.

## Prerequisites

- Python 3.11 or later
- A stable Rust toolchain (`rustc`, `cargo`)
- `pip`

Optional tools for profile commands:

- Python: `py-spy`
- TypeScript / JavaScript: Node
- Rust: `samply` or `cargo flamegraph`

## Development install

From the repository root:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

`pip install` compiles `dued._native` from `dued-rs/` through maturin.

## Tests

Do not download the embed model in tests or CI:

```bash
DUED_STUB_EMBED=1 pytest
```

Rust unit tests:

```bash
cargo test --manifest-path dued-rs/Cargo.toml
```

`--model stub` and `DUED_STUB_EMBED=1` use hash vectors. Use `--no-embed` when a test only needs the index structure.

## Layout

```text
src/dued/          Python CLI, thin wrappers, Typer entry
dued-rs/           Rust engine, PyO3 module, optional native binary
tests/             Pytest suite and fixtures
docs/              User and contributor documentation
scripts/           Compare and bench helpers
.cursor/skills/    Cursor playbook for this CLI
```

Python modules under `src/dued/` should stay thin. Put analysis logic in `dued-rs/`.

## Making a change

1. Open an issue first when the change is large or changes CLI behavior.
2. Create a branch from `main`.
3. Keep the change focused.
4. Add or update tests for the behavior you change.
5. Update docs when you change a command, a path, or a report file.
6. Run the test commands above.

## Pull requests

Use the pull request template. Include:

- Why the change is needed
- How you tested it
- Any CLI, report, or index format change

CI runs pytest with `DUED_STUB_EMBED=1` and `cargo test`.

## Style

- Match the style of the file you edit.
- Prefer small, named functions over large blocks.
- Do not add `try` / `except` in Python unless the caller must handle a known failure.
- In Rust, keep the current error style (`unwrap` / `expect` in this engine) unless a change needs a typed error.
- Do not reformat unrelated files.

## Issues

Use a bug report for a defect. Use a feature request for new behavior.

Security reports go through [SECURITY.md](SECURITY.md). Do not file a public issue for a security defect.

## License

By contributing, you license your work under the MIT License in [LICENSE](LICENSE).
