<div align="center">
  <!-- Downloads -->
  <a href="https://pypi.org/project/evtx/">
    <img src="https://pepy.tech/badge/evtx"
      alt="Download" />
  </a>
</div>


# pyevtx-rs

Python bindings for `https://github.com/omerbenamram/evtx/`.

## Installation

Available on PyPi - https://pypi.org/project/evtx/.

To install from PyPi - `pip install evtx`

### Wheels

Wheels are currently automatically built for Python 3.10+ using the abi3 stable ABI tag (compatible with all versions from 3.10 onwards).

Supported platforms are:
  - Linux x86_64
  - macOS x86_64
  - macOS arm64 (m1)
  - Windows x86_64

### Installation from sources

Installation is possible for other platforms by installing from sources.

This requires a Rust compiler and a recent enough Setuptools and Pip.

Run `pip install -e .`

### Free-threaded Python 3.14

Free-threaded CPython 3.14 requires a version-specific `cp314-cp314t` wheel;
the ordinary `cp310-abi3` wheels are for GIL-enabled CPython. Releases ship both
variants for every supported target. Importing `evtx` does not re-enable the
GIL, and CI runs the test suite on the free-threaded wheels with the GIL
disabled, without a `PYTHON_GIL` override.

For a source build, use a free-threaded Python environment and Maturin 1.15 or
newer (the build-system requirement installs it automatically with pip):

```sh
python3.14t -m pip install .
```

Concurrent threads should use independent parsers and file handles. Do not call
`next()` concurrently on the same mutable iterator or modify a shared file-like
object while parsing it. The tests exercise parallel XML/JSON parsing from paths
and in-memory streams while Python garbage collection runs.

## Usage

### Parsing EVTX files

This will print each record as an XML string.

```python
from evtx import PyEvtxParser


def main():
    parser = PyEvtxParser("./samples/Security_short_selected.evtx")
    for record in parser.records():
        print(f'Event Record ID: {record["event_record_id"]}')
        print(f'Record Header Timestamp: {record["timestamp"]}')
        print(record['data'])
        print(f'------------------------------------------')
```


And this will print each record as a JSON string.

```python
from evtx import PyEvtxParser


def main():
    parser = PyEvtxParser("./samples/Security_short_selected.evtx")
    for record in parser.records_json():
        print(f'Event Record ID: {record["event_record_id"]}')
        print(f'Record Header Timestamp: {record["timestamp"]}')
        print(record['data'])
        print(f'------------------------------------------')
```

File-like objects are also supported.

```python
from evtx import PyEvtxParser


def main():
    a = open("./samples/Security_short_selected.evtx", 'rb')

    # io.BytesIO is also supported.
    parser = PyEvtxParser(a)
    for record in parser.records_json():
        print(f'Event Record ID: {record["event_record_id"]}')
        print(f'Record Header Timestamp: {record["timestamp"]}')
        print(record['data'])
        print(f'------------------------------------------')
```

### Threads

The records iterators parse `number_of_threads` chunks at a time, one per core
by default, while the records of the chunks before them are returned. Chunks are
parsed with the GIL released, so other Python threads go on meanwhile, and a
file-like object is only read from the thread that iterates. With
`number_of_threads=1`, chunks are parsed on that thread.

### Handling malformed records

By default, a malformed record or chunk raises an exception with its available
record/chunk context and underlying cause. Records are not silently dropped.
A normal `for` loop stops on that exception; the iterator can still be advanced
if you catch it and call `next()` again.

To keep processing recoverable parse failures automatically, opt in explicitly:

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("log.evtx", skip_errors=True)
for record in parser.records_json():
    print(record["event_record_id"])
```

Each skipped error emits a `RuntimeWarning`, including the parser's diagnostic
message. If a chunk cannot be parsed, all its records are skipped. Python I/O
exceptions still propagate, and warnings configured as errors still stop the
iteration. Use Python's `warnings` module to capture these diagnostics; the
iterator always yields record dictionaries, never exception objects.

### Recovering deleted records

Records can outlive their chunk's live area: clearing a log, for one, resets its
chunks but leaves the old records in them. `recover_records=True` also yields
the records found in that unallocated space, with `record["recovered"]` set to
`True` (it is `False` for live records). Carved records that can't be parsed
are left out.

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("log.evtx", recover_records=True)
for record in parser.records_json():
    if record["recovered"]:
        print("recovered", record["event_record_id"])
```

### Localized strings from `.MTA` files

A log saved from Event Viewer with display information has a `LocaleMetaData`
folder next to it, with an `.MTA` file for each language. Pass one as `mta=`,
and each record gets a `RenderingInfo` element with its message and its level,
task, opcode and keyword names, so that a log from another machine renders
with its strings:

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("log.evtx", mta="LocaleMetaData/log_1033.MTA")
```

### File and chunk headers

`parser.header` gives what the log's file header says, and `parser.chunks()`
what each chunk's header says, without parsing any record, much as
`evtx_dump info` shows them. They help with triaging dirty, wrapped or damaged logs:

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("log.evtx")
header = parser.header
print(header.major_version, header.minor_version, header.is_dirty, header.next_record_id)
if not header.checksum_valid:
    print("the file header is damaged; its chunks are still read")

for chunk in parser.chunks():
    print(
        chunk.number,
        chunk.first_event_record_id,
        chunk.last_event_record_id,
        chunk.header_checksum_valid,
        chunk.events_checksum_valid,
    )

# chunks() leaves the parser to the records.
for record in parser.records():
    ...
```

The chunks come in the order `records()` reads them, from the one that holds
the oldest records, so a log that has wrapped around starts in the middle of
the file. `number` is a chunk's place in the file, and an error about a record
names its chunk by it. The checksums are checked whatever `validate_checksums`
says.

### Record timestamps and event timestamps

Each result contains two independently stored timestamps:

- `record["timestamp"]` is the **EVTX record-header timestamp**, decoded from the
  header's Windows FILETIME value and formatted in UTC.
- `Event/System/TimeCreated/@SystemTime` is part of the **event payload** in
  `record["data"]`. Windows defines it as the time the event was logged; see the
  [Windows event schema](https://learn.microsoft.com/en-us/windows/win32/wes/eventschema-timecreated-systempropertiestype-element).

Use `TimeCreated` when comparing the event time shown by Event Viewer. Its UI may
display local time, so normalize time zones before comparing. In the default JSON
format, read both values like this:

```python
import json
from evtx import PyEvtxParser

record = next(PyEvtxParser("./samples/Security_short_selected.evtx").records_json())
event = json.loads(record["data"])
header_time = record["timestamp"]
event_time = event["Event"]["System"]["TimeCreated"]["#attributes"]["SystemTime"]
print(header_time, event_time)
```

The parser does not derive one timestamp from the other or apply a fixed delay.
They can differ in value and fractional-second precision because they come from
different fields. A gap such as 30 seconds cannot be attributed to buffering or
another producer behavior without examining the original event. Keep both values
when that distinction matters; do not overwrite one to make them agree.

### WEVT template cache (offline rendering fallback)

When EVTX embedded templates are missing/corrupted, the Rust `evtx` crate can optionally fall back
to an offline `WEVT_TEMPLATE` cache (provider resources). This Python extension exposes that cache
as `WevtCache`.

For an end-to-end walkthrough (including a synthetic PE fixture patched to contain a valid CRIM),
see `notebooks/wevt_templates_e2e.ipynb`.

```python
from evtx import PyEvtxParser, WevtCache

cache = WevtCache.load("/path/to/wevt_cache.wevtcache")
parser = PyEvtxParser("/path/to/log.evtx", wevt_cache=cache)

for record in parser.records_json():
    print(record["event_record_id"], record["timestamp"])
```

You can also pass the `.wevtcache` path directly:

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("/path/to/log.evtx", wevt_cache="/path/to/wevt_cache.wevtcache")
```

### Build a cache from provider binaries (EXE/DLL/SYS)

You can generate the cache directly from Python by scanning provider binaries and extracting their
`WEVT_TEMPLATE` resources into an **in-memory** cache:

```python
from evtx import WevtCache

cache = WevtCache()

# Add a single provider binary (strict: raises on parse failures)
cache.add_dll(r"C:\Windows\System32\services.exe")

# Or scan directories
cache.add_dir(r"C:\Windows\System32", recursive=True, extensions="exe,dll,sys")
cache.add_dir(r"C:\Windows\SysWOW64", recursive=True, extensions="exe,dll,sys")

# Optional: persist to disk for reuse by other tools (writes a single .wevtcache file)
cache.dump("wevt_cache_out.wevtcache", overwrite=True)

# Cache is ready to use:
print(cache.resolve_template_guid("555908D1-A6D7-4695-8E1E-26931D2012F4", 7000, 0))
```

### End-to-end offline rendering (TemplateInstance + cache)

If you have:

- An offline cache file (`.wevtcache`) (from `evtx_dump extract-wevt-templates`)
- An EVTX record that contains a `TemplateInstance` (substitution values)

…you can render the record’s template offline:

```python
from evtx import WevtCache

cache = WevtCache.load("/path/to/wevt_cache.wevtcache")

xml = cache.render_record_xml(
    "/path/to/log.evtx",
    record_id=12345,
    template_instance_index=0,
    # Provide one of:
    template_guid="bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
    # OR:
    # provider_guid="aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
    # event_id=7000,
    # version=0,
)

print(xml)
```

If you know `(provider_guid, event_id, version)` and want to look up the `template_guid` first:

```python
from evtx import WevtCache

cache = WevtCache.load("/path/to/wevt_cache.wevtcache")
template_guid = cache.resolve_template_guid(
    "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
    7000,
    0,
)
print(template_guid)
```

You can also render a template directly from a Python substitutions list:

```python
from evtx import WevtCache

cache = WevtCache.load("/path/to/wevt_cache.wevtcache")
xml = cache.render_template_xml(
    "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
    ["foo", 123, None, True],
)
print(xml)
```

### WEVT manifest introspection (`evtx.wevt`)

For low-level exploration of `WEVT_TEMPLATE` resources (similar to `pyfwevt`), use the `evtx.wevt` module
to parse raw CRIM blobs and inspect providers, events, and templates:

```python
from evtx.wevt import Manifest

# Parse a CRIM blob (extracted from a PE's WEVT_TEMPLATE resource)
with open("wevt_template.bin", "rb") as f:
    crim_blob = f.read()

manifest = Manifest.parse(crim_blob)

for provider in manifest.providers:
    print(f"Provider: {provider.identifier}")
    print(f"  Events: {len(provider.events)}")
    print(f"  Templates: {len(provider.templates)}")

    for event in provider.events[:3]:
        print(f"    Event {event.identifier} v{event.version}")
        if event.template_offset:
            tpl = provider.get_template_by_offset(event.template_offset)
            if tpl:
                print(f"      Template: {tpl.identifier}")
                for item in tpl.items:
                    print(f"        - {item.name}: in={item.input_data_type} out={item.output_data_type}")
```

Render a template's BinXML structure (with placeholder substitutions for debugging):

```python
from evtx.wevt import Manifest

manifest = Manifest.parse(crim_blob)
template = manifest.providers[0].templates[0]

# Renders BinXML with {sub:N} placeholders
xml = template.to_xml()
print(xml)
```

#### `evtx.wevt` API

| Class | Description |
|-------|-------------|
| `Manifest` | Parsed CRIM blob containing providers |
| `Manifest.parse(bytes)` | Parse a raw CRIM blob |
| `Provider` | Event provider with GUID, events, templates |
| `Provider.get_template_by_offset(int)` | Lookup template by offset |
| `Event` | Event definition (id, version, template_offset) |
| `Template` | Template with GUID and items |
| `Template.to_xml(ansi_codec=None)` | Render BinXML to XML string |
| `TemplateItem` | Substitution slot (name, input/output types) |
