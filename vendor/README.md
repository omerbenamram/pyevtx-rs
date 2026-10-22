# Vendored crates

## pyo3-stub-gen

Unmodified copy of [pyo3-stub-gen](https://github.com/Jij-Inc/pyo3-stub-gen)
0.23.1 from crates.io (MIT OR Apache-2.0), except that the `pyo3` and `numpy`
requirements in `Cargo.toml` are relaxed from `< 0.30.0` to `< 0.31.0` so it
resolves against pyo3 0.30. Remove this directory and the `[patch.crates-io]`
entry in the root `Cargo.toml` once an upstream release supports pyo3 0.30.
