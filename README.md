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

## Usage

### Parsing EVTX files

This will print each record as an XML string.

```python
from evtx import PyEvtxParser


def main():
    parser = PyEvtxParser("./samples/Security_short_selected.evtx")
    for record in parser.records():
        print(f'Event Record ID: {record["event_record_id"]}')
        print(f'Event Timestamp: {record["timestamp"]}')
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
        print(f'Event Timestamp: {record["timestamp"]}')
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
        print(f'Event Timestamp: {record["timestamp"]}')
        print(record['data'])
        print(f'------------------------------------------')
```

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
