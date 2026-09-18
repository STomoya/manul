# AGENTS.md

## Layout

- `crates/*` — Rust workspace. Each crate's own unit/integration tests live
  alongside it (`src/**/*.rs` `#[cfg(test)]` modules, `crates/<name>/tests/`).
- `src/manul` — the Python package; thin wrappers around the compiled
  extension plus `.pyi` stubs.
- `tests/` — the Python test suite (pytest), one file per `src/manul` area.

## Rust

Run everything from the repo root.

```sh
cargo test
```

Any crate that links against libpython (a pyo3 binding crate, e.g. `manul_pyo3`)
needs `LD_LIBRARY_PATH` pointed at it first, or its tests fail to link:

```sh
export LD_LIBRARY_PATH="$(uv run python -c "import sysconfig; print(sysconfig.get_config_var('LIBDIR'))")${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

Format and lint before committing (pre-commit also runs these):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Coverage (`cargo-llvm-cov`, excludes `lib.rs` re-export files):

```sh
make rscoverage
```

Target: **95%+ line coverage** for plain Rust crates. A crate that exposes
pyo3 bindings will always sit lower — its `#[pyclass]`/`#[pymethods]`
attribute lines are proc-macro-generated Python-registration boilerplate that
only executes when the compiled extension is actually `import`ed from Python,
not from `cargo test`. Don't chase that gap; verify pyo3-facing behavior via
`tests/` integration tests and the Python test suite instead.

If a code path can never fail given its current implementation (e.g. a
`Result`-returning function with no fallible call in its body), don't leave it
wrapped just to satisfy a signature — the dead branch is also untestable.

## Python

```sh
uv run pytest
```

Runs with coverage (`--cov=src`) by default per `pyproject.toml`. Target:
**100%** — `src/manul` is a thin binding layer over the Rust core, so there's
no excuse for gaps here.

Lint, format, and type-check before committing (pre-commit also runs these):

```sh
uv run ruff check --fix .
uv run ruff format .
uvx ty check .
```

## Commits

Conventional-commit-style prefixes: `feat`, `fix`, `refactor`, `test`,
`chore`. Branches are `<type>/<short-description>` (e.g. `refactor/pyo3-isolation`,
`test/improve-rust-coverage`), merged to `main` via PR.
