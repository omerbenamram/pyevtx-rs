# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `PyEvtxParser(recover_records=True)` also yields the records left in
  unallocated chunk space, such as a cleared chunk's, as `evtx_dump
  --recover-records` does. Each record dict has a new `recovered` key, `True`
  for those records and `False` for live ones. Carved records that can't be
  parsed are left out, as in evtx.
- `PyEvtxParser(mta=path)` adds the localized strings of a LocaleMetaData
  `.MTA` file to each record as a `RenderingInfo` element. A file that can't
  be read raises `OSError` (`FileNotFoundError` when missing), and one that
  isn't an `.MTA` file raises `ValueError`.
- `PyEvtxParser.header` is what the log's file header says, as a
  `FileHeader`: its version, flags, chunk numbers and next record ID, and
  whether its CRC32 matches. `PyEvtxParser.chunks()` returns a `ChunkHeader`
  for each chunk, without parsing records: its number and offset in the file,
  its record numbers and IDs, its flags, and whether its header's and records'
  CRC32s match, much as `evtx_dump info` shows them.

### Changed

- Update the Rust parser to `evtx 0.15.1`. Records render as Windows renders
  them in many more ways, which changes the XML and JSON: timestamps have
  seven fractional digits, GUIDs are lowercase and in braces, elements that
  depend on a NULL value are left out, consecutive `Data` elements of one name
  are one JSON array, empty elements are `<X/>`, and a log that has wrapped
  around comes out oldest record first. evtx's CHANGELOG lists every change.
- An error about a record says where the record is: `Failed to read the record
  at offset 0x200 of chunk 1 (file offset 0x11200)`. A chunk that can't be
  parsed says `Failed to parse chunk number 1 (record IDs 8..14)`,
  where it said `Failed to parse chunk containing record IDs 8..14`.
- `number_of_threads` works: it had no effect, and records were parsed one
  chunk at a time on the calling thread, with the GIL held. The records
  iterators now parse that many chunks at a time on Rust threads, with the
  GIL released, while the records of the chunks before them are returned.
  The default, `0`, uses one thread per core: on 4 cores, iterating over a
  31 MB log is 3.1 times as fast in JSON and twice as fast in XML.
  `number_of_threads=1` parses on the calling thread, as before, but with the
  GIL released.
- Switched to upstream `pyo3-stub-gen` 0.23.2, which supports PyO3 0.30, and
  removed the vendored copy under `vendor/`. Generated stubs are unchanged.

### Fixed

- `PyEvtxParser`'s docstring listed 3 of its 8 arguments, with a signature
  that marked them positional-only. It now describes all of them, and
  `inspect.signature(PyEvtxParser)` gives the real one.
- The `WevtCache.resolve_template_guid` example called `from_system()` and
  `get_template()`, which don't exist. It now uses `load()` and
  `render_template_xml()`.

### Security

- Refreshed `Cargo.lock` (`crossbeam-epoch` 0.9.21, RUSTSEC-2026-0204) and
  `uv.lock` (development tools only: `jupyterlab`, `notebook`, `mistune`,
  `tornado`, `urllib3`, `soupsieve`; `setuptools` dropped) to clear
  Dependabot alerts.

## [0.14.1 - 2026-10-29]

### Changed

- Update the Rust parser to `evtx 0.14.0`. JSON output no longer repeats a key
  when an element holds two substitutions whose nested templates have elements
  with the same name: those names are numbered (`Name`, `Name_1`), as they are
  elsewhere (evtx #302).

## [0.14.0 - 2026-10-22]

### Changed

- Update the Rust parser to `evtx 0.13.0`.
- Update to PyO3 0.30.1 and pyo3-file 0.18.0.
- **Breaking:** `ansi_codec` now takes a WHATWG Encoding Standard label, since
  evtx decodes ANSI strings with `encoding_rs` instead of the unmaintained
  `encoding` crate (RUSTSEC-2021-0153). Common labels such as `windows-1252`,
  `windows-1251` and `utf-8` work as before, and `ascii` and `iso-8859-1`
  resolve to windows-1252. Code pages outside the standard (`big5-2003`, `hz`,
  `mac-cyrillic`, `mac-roman`, `pua-mapped-binary`) and non-ASCII-compatible
  encodings such as UTF-16 raise `ValueError`.

## [0.13.0 - 2026-09-22]

### Changed

- Update the Rust parser to `evtx 0.12.3`, including faster record rendering and
  malformed-input handling fixes.
- Update to PyO3 0.29.2, pyo3-file 0.17.0, and pyo3-stub-gen 0.23.0.
- Keep the existing Python API and Python 3.10+ stable-ABI wheel support.

## [0.11.1]

### Added

- Detect Windows import collisions between `pyevtx-rs` (`evtx`) and `python-evtx`
  (`Evtx`) and report actionable install guidance.
- Add repo-local Zed settings so Rust format-on-save uses the language server
  formatter.

### Changed

- Apply `rustfmt` to the Rust sources so editor format-on-save behavior starts
  from a clean baseline.

## [0.11.0]

### Added

- **`evtx.wevt` manifest introspection API** (pyfwevt-like):
  - `Manifest.parse(crim_blob)` to parse WEVT_TEMPLATE resources from PE files
  - `Provider`, `Event`, `Template`, `TemplateItem` classes for navigating manifest structure
  - `Template.to_xml()` for offline BinXML rendering without event data
- **`.wevtcache` file support**:
  - `WevtCache` class for building, saving, and loading `.wevtcache` files
  - `WevtCache.add_pe_file()` and `WevtCache.add_crim_blob()` for populating caches
  - `PyEvtxParser.with_wevt_cache()` for attaching cache to enable offline message rendering
- `py.typed` marker for PEP 561 compliance (better IDE type checking support)

### Changed

- Restructured as proper Python package with `evtx._native` extension module
- Update evtx library to 0.11.0
- Minimum Python version is now 3.10 (to match `abi3-py310` build target)

### Fixed

- `WevtCache.add_dll()` / `add_crim_blob()` are now transactional (all-or-nothing on error)
- CI modernization: updated to latest GitHub Actions runners and tooling

## [0.10.0]

- Update evtx library to 0.10.0 (latest).
- Expose `WevtCache` for working with offline WEVT template caches (feature `wevt_templates`).
- Ship generated Python type stubs (`evtx.pyi`) (via `pyo3-stub-gen`).

## [0.8.6]

- Update PyO3 Version to 0.22.2.
- Update evtx library to 0.8.3.

## [0.8.5]

- Update evtx library to 0.8.2.


## [0.8.4]

- Update PyO3 Version to 0.21.
- Update evtx library to 0.8.2.

## [0.8.3]

- Preserve record order during chunk iteration.

## [0.8.2]

- Update PyO3 version

## [0.8.1]

- Wheels are now published using abi3 tag, which means a single wheel can be used for all interpreters.
- Added profile guided optimization for the wheels - should yield a 10% improvement in performance.

## [0.8.0]

- Updated release to match `evtx 0.8.0`
- Added m1 builds

## [0.7.4]

- The parser's settings are now passed correctly to the iterator.

## [0.7.3] - 2022-02-13

- Publish python 3.10 wheels.

## [0.7.2] - 2020-07-13

- Bump to evtx library 0.7.2, and update PyO3
- Publish python 3.9 wheels.

## [0.6.11] - 2020-07-13

Bump to evtx library 0.6.8, and update PyO3.

## [0.6.10] - 2020-07-13

Updated wheels to rust stable, support python 3.8.

## [0.6.5] - 2020-01-14

Updated release to match `evtx 0.6.5` - also opts out of `evtx_dump`
dependencies.

## [0.6.3] - 2019-12-17

Updated release to match `evtx 0.6.3`

## [0.6.2] - 2019-12-17

Updated release to match `evtx 0.6.2`

## [0.4.0] - 2019-06-02

### Added

- PyEvtxParser now supports settings number of worker threads, and underlying
  ansi strings codec.

## [0.3.2] - 2019-05-20

### Added

- PyEvtxParser now supports file-like objects.
