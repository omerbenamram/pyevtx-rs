# pyevtx-rs

[![PyPI](https://img.shields.io/pypi/v/evtx.svg)](https://pypi.org/project/evtx/)
[![Downloads](https://pepy.tech/badge/evtx)](https://pypi.org/project/evtx/)

Read Windows event logs (`.evtx`) from Python, on any platform. These are the Python bindings for the Rust [`evtx`](https://github.com/omerbenamram/evtx) parser.

```
pip install evtx
```

This example reads a log from the [`samples`](https://github.com/omerbenamram/pyevtx-rs/tree/master/samples) folder of this repository. Give it the path of your own `.evtx` file instead.

```python
import json
from evtx import PyEvtxParser

parser = PyEvtxParser("samples/Security_short_selected.evtx")
for record in parser.records_json():
    event = json.loads(record["data"])["Event"]
    print(record["event_record_id"], record["timestamp"], event["System"]["EventID"])
```

```
1 2016-06-29T15:24:34.346Z UTC 5152
2 2016-06-29T15:24:36.686Z UTC 4611
3 2016-06-29T15:24:36.686Z UTC 4776
...
```

## Usage

### Records

`records_json()` and `records()` yield one `dict` per record, with these keys:

| Key | Value |
| --- | --- |
| `event_record_id` | the record ID from the record's header |
| `timestamp` | the time in the record's header, as a string in UTC |
| `data` | the event, as a JSON string from `records_json()` or an XML string from `records()` |
| `recovered` | `True` for a record found in unallocated space, with `recover_records=True` (see below). `False` for every other record. |

`data` is a string, so a record you only pass on costs no decoding. Call `json.loads` on the ones you look into.

`PyEvtxParser` takes a path (`str`, `bytes` or `os.PathLike`) or a binary file-like object with `read` and `seek`, such as an open file or `io.BytesIO`:

```python
from evtx import PyEvtxParser

with open("log.evtx", "rb") as f:
    for record in PyEvtxParser(f).records():
        print(record["data"])  # XML
```

### Malformed records

By default, a malformed record or chunk raises an exception that says which record or chunk it was and why. Records are never dropped silently. A `for` loop stops on that exception, and the iterator can still be advanced if you catch it and call `next()` again.

To keep going past such records, say so:

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("log.evtx", skip_errors=True)
for record in parser.records_json():
    print(record["event_record_id"])
```

Each skipped error is then a `RuntimeWarning` with the parser's message. If a chunk cannot be parsed, all its records are skipped. Use Python's `warnings` module to log or count them. Exceptions from a file object's `read` or `seek` still reach you, and warnings configured as errors still stop the iteration. The iterator always yields record dictionaries, never exception objects.

### Deleted records

Clearing a log resets its chunks but leaves the old records in them, and records also stay in the slack after a chunk's last record. `recover_records=True` yields those too, with `record["recovered"]` set to `True`. The ones that can't be parsed are left out.

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("log.evtx", recover_records=True)
for record in parser.records_json():
    if record["recovered"]:
        print("recovered", record["event_record_id"])
```

### Message text from `.MTA` files

A record holds an event ID and its values, not the text Event Viewer shows for it. A log saved from Event Viewer with display information has a `LocaleMetaData` folder next to it, with an `.MTA` file for each language. Pass one as `mta=`, and each record gets a `RenderingInfo` element with its message and its level, task, opcode and keyword names:

```python
from evtx import PyEvtxParser

parser = PyEvtxParser("log.evtx", mta="LocaleMetaData/log_1033.MTA")
```

### File and chunk headers

`parser.header` gives what the log's file header says, and `parser.chunks()` what each chunk's header says, without parsing any record, much as `evtx_dump info` shows them. Start here with a log that is dirty, has wrapped around, or looks damaged:

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

The chunks come in the order `records()` reads them, from the one that holds the oldest records, so a log that has wrapped around starts in the middle of the file. `number` is a chunk's place in the file, and an error about a record names its chunk by it. The checksums are checked whatever `validate_checksums` says.

### Two timestamps

Each record has two timestamps, stored in different places:

- `record["timestamp"]` is from the record's header.
- `Event/System/TimeCreated/@SystemTime`, inside `record["data"]`, is the time the event was logged, as Windows [defines it](https://learn.microsoft.com/en-us/windows/win32/wes/eventschema-timecreated-systempropertiestype-element). It is the time Event Viewer shows, converted to local time.

```python
import json
from evtx import PyEvtxParser

record = next(PyEvtxParser("./samples/Security_short_selected.evtx").records_json())
event = json.loads(record["data"])
header_time = record["timestamp"]
event_time = event["Event"]["System"]["TimeCreated"]["#attributes"]["SystemTime"]
print(header_time, event_time)  # 2016-06-29T15:24:34.346Z UTC 2016-06-29T15:24:34.3460000Z
```

The parser reads each one as it is stored. It does not derive one from the other, so they can differ in value and in precision. When a difference matters to you, keep both. Nothing in the file says why two values differ.

### Threads

The records iterators parse `number_of_threads` chunks at a time, one per core by default, while the records of the chunks before them are returned. Chunks are parsed with the GIL released, so other Python threads go on meanwhile, and a file-like object is only read from a thread that iterates. With `number_of_threads=1`, chunks are parsed on that thread.

Threads can share an iterator: a call to `next()` waits for the one before it to return, and each record goes to one of the threads. To parse several logs in parallel, give each thread its own parser and its own file object. Do not modify a shared file-like object while it is being parsed.

### Options

| Argument | Default | What it does |
| --- | --- | --- |
| `number_of_threads` | one per core | how many chunks are parsed at the same time. `1` parses on the calling thread. A number above the size of the thread pool means the size of the pool, which is one thread per core unless `RAYON_NUM_THREADS` sets it. |
| `skip_errors` | `False` | turn a record or chunk that can't be parsed into a `RuntimeWarning` |
| `recover_records` | `False` | also yield records left in unallocated chunk space |
| `separate_json_attributes` | `False` | in JSON, put an element's attributes in `<name>_attributes` and not under `#attributes`, so that a field such as `EventID` keeps one type |
| `indent` | `True` | indent the XML. JSON is always compact. |
| `ansi_codec` | `windows-1252` | the encoding of ANSI strings in the log, such as `windows-1251` or `gbk` |
| `validate_checksums` | `False` | fail on a chunk whose CRC32 doesn't match. A log that was not closed cleanly can have one. |
| `mta` | `None` | an `.MTA` file to take message text from |
| `wevt_cache` | `None` | templates for records whose template is missing from the log (see below) |

## Records whose template is missing

A record refers to a template, which is normally stored in the same chunk. When that template is missing or damaged, the record can't be rendered. The same templates are in the `WEVT_TEMPLATE` resource of the provider's binary, and a `WevtCache` holds templates collected from such binaries for the parser to fall back to.

`notebooks/wevt_templates_e2e.ipynb` walks through all of it.

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

`add_dll` and `add_dir` read the `WEVT_TEMPLATE` resources of provider binaries into the cache, in memory:

```python
import os

from evtx import WevtCache

cache = WevtCache()

# Add a single provider binary (strict: raises on parse failures)
cache.add_dll(r"C:\Windows\System32\services.exe")

# Or scan directories
cache.add_dir(r"C:\Windows\System32", recursive=True, extensions="exe,dll,sys")
# A directory that isn't there raises FileNotFoundError.
if os.path.isdir(r"C:\Windows\SysWOW64"):
    cache.add_dir(r"C:\Windows\SysWOW64", recursive=True, extensions="exe,dll,sys")

# Optional: persist to disk for reuse by other tools (writes a single .wevtcache file)
cache.dump("wevt_cache_out.wevtcache", overwrite=True)

# Cache is ready to use:
print(cache.resolve_template_guid("555908D1-A6D7-4695-8E1E-26931D2012F4", 7000, 0))
```

### Render one record from the cache

With a cache file (from `WevtCache.dump`, or from `evtx_dump extract-wevt-templates`) and a record, which carries the values for its template, you can render that record's template yourself:

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

### Look inside a manifest (`evtx.wevt`)

The `evtx.wevt` module parses a raw `WEVT_TEMPLATE` resource (a CRIM blob) and lists its providers, events and templates, much as `pyfwevt` does:

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

## Installation details

`pip install evtx` gets a wheel, with nothing to compile, on:

- Linux x86_64 and arm64 (glibc and musl)
- macOS x86_64 and arm64
- Windows x86_64

One `abi3` wheel covers CPython 3.10 and later. Free-threaded CPython 3.14 has its own `cp314-cp314t` wheel for each of those platforms. Importing `evtx` there does not re-enable the GIL, and CI runs the test suite with the GIL disabled on the Linux x86_64 (glibc), macOS and Windows ones.

Releases after 0.15.0 also have a source distribution on PyPI, which pip builds on a platform or Python version without a wheel. That needs a stable Rust compiler. pip installs the build tool, Maturin, by itself. PyPy is not tested.

To build from a checkout, run `pip install .`, or `pip install -e .` to keep working on it. On free-threaded Python that is `python3.14t -m pip install .`, and it needs Maturin 1.15 or newer, which pip also installs by itself.
