# Architecture

`dued` is one product with two layers.

```text
dued CLI (Python, Typer, Rich)
        │
        ▼
dued._native (PyO3)
        │
        ▼
dued-rs engine (Rust, SQLite, tree-sitter, ONNX)
```

The Python package is what you install from PyPI. Each `dued.*` module calls the Rust extension. The Rust crate also builds a native `dued` binary for local work. Both write `.dued/` and `dued-reports/`.

## Scan pipeline

`dued analyze` and `dued scan` run this sequence:

1. **Walk** — collect source files and skip vendor and build directories.
2. **Parse** — tree-sitter extractors for Python, TypeScript / JavaScript, and Rust.
3. **Metrics** — cyclomatic complexity, cognitive complexity, nesting, argument count.
4. **Graph** — imports and calls. Generic names such as `new` and `clone` do not fan out.
5. **Git** (optional) — churn, authors, bus factor, coupling.
6. **Rank** — PageRank-style reading order. Entry points rise. Generic helpers drop.
7. **Effects, risks, cost, fingerprints, hollow stubs**
8. **Issues** — god function, god module, effect in core, shotgun surgery.
9. **Names** — token health and homonyms.
10. **Clones** — token overlap and optional embed neighbors.
11. **Embed** (optional) — local ONNX vectors for signature, docstring, and body.
12. **Reports** — HTML explorer, JSON tables, brief, agent pack.

`dued report` and the query commands reuse the SQLite index. They do not walk again.

## Language support

| Language | Parser | Typical files |
| --- | --- | --- |
| Python | tree-sitter-python | `.py` |
| TypeScript / JavaScript | tree-sitter-typescript, tree-sitter-javascript | `.ts`, `.tsx`, `.js`, `.jsx` |
| Rust | tree-sitter-rust | `.rs` |

## Graph and rank

Call resolution prefers same-file and same-module targets. Overloaded names in one file stay unresolved instead of attributing fan-in to every match. Reading order skips generic non-entry names and dedupes by `relpath::name`.

## Python surface

`src/dued/` is the public Python API and the Typer CLI. Import the same functions from other Python code:

```python
from dued.walk import walk_repo
from dued.parse import parse_source
from dued.scan import run_scan
from dued.store import connect
```

Keep analysis logic in `dued-rs/`. Add a thin wrapper in `src/dued/` only when Python callers need a new function.

## Privacy

The tool does not call a third-party analysis API. Source stays on the machine that runs `dued`. The first embed run may download model weights into the local Hugging Face cache.
