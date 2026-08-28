# Embeddings

`scan` and `analyze` can embed each symbol signature, docstring, and body.

Default model: `jinaai/jina-embeddings-v2-base-code`

Runtime: ONNX Runtime (`ort`), behind the Cargo `jina` feature. Default `pip install -e .` and maturin `extension-module` builds omit `ort`. Stub vectors and `--no-embed` work without that feature.

To compile real Jina support:

```bash
cargo build --manifest-path dued-rs/Cargo.toml --features jina
```

Or pass `jina` to maturin when you build the extension. The pinned `ort` version is `=2.0.0-rc.10` (compatible with rustc 1.85).

The first run with `jina` can download model files into the Hugging Face cache. Later runs reuse that cache.

## When to skip

Tests and CI must not download the model:

```bash
DUED_STUB_EMBED=1 dued analyze --no-embed --no-git --quiet --json
```

`--model stub` also uses hash vectors. Only one model is loaded at a time.

Use `--no-embed` when you only need structure, rank, and issues.

## What embeddings are for

- Similar-symbol search: `dued cluster --similar-to <name>`
- Embed-based clone clusters
- Mismatch flags that `dued label` exports as CSV

Bodies and vectors are not copied into the HTML explorer JSON. The explorer uses compact tables.

## Hardware

ONNX Runtime uses the local CPU by default. Large repositories take longer on the embed stage than on parse. Progress bars on stderr show that stage when the count is known.
