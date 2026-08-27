# Publishing

The PyPI package is `dued`. That package is the public CLI.

## Local build

```bash
pip install maturin
maturin build --release
```

Wheels land in `target/wheels/` or `dist/`, depending on the maturin version.

Publish:

```bash
maturin publish
```

Do this only when you intend to release. You need a PyPI account and a trusted publisher or an API token.

## GitHub Actions

`.github/workflows/publish.yml` builds wheels for Linux, macOS, and Windows, plus an sdist. It runs on a `v*` tag.

The publish job uses trusted publishing (`id-token: write`) and the `pypi` GitHub Environment. Configure that environment on the repository before the first tag.

## Version

Keep these in sync when you cut a release:

- `pyproject.toml` `version`
- `dued-rs/Cargo.toml` `version`
- `src/dued/__init__.py` `__version__`

Tag the commit as `v0.1.0` (or the matching version).
