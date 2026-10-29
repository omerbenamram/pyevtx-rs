# Releasing

This project is published to PyPI as `evtx`. Its version is independent of the `evtx` crate's
(PyPI 0.14.0 wraps crate 0.13.0), and tags are bare `X.Y.Z`, without a `v`.

## One-time setup

- A PyPI token in the `PYPI_API_TOKEN` Actions secret.

## Cut a release

1. If the release needs unreleased `evtx` or `pyo3-file` changes, release those crates first and
   point `Cargo.toml` at the new crates.io versions.
2. Pick the version. While on 0.x, a breaking change or a new feature bumps the minor version
   and fixes alone bump the patch version.
3. Open a `chore(release): X.Y.Z` PR that bumps `version` in `Cargo.toml`, commits the refreshed
   `Cargo.lock` (CI builds with `--locked`), and adds a `## [X.Y.Z - YYYY-MM-DD]` section to
   `CHANGELOG.md`.
4. Merge it.

The merge runs `.github/workflows/auto-tag.yml`. It sees a `Cargo.toml` version with no tag,
pushes the `X.Y.Z` tag on the merge commit, and calls `.github/workflows/ci.yml` for it. Once
every wheel job passes, its `release` job publishes them to PyPI with `uv publish`.

Pushing an `X.Y.Z` tag by hand still runs `ci.yml` directly; auto-tag then skips that version
because the tag exists. To retry a failed release, re-run the failed jobs of its run.
