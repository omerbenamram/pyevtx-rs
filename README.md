# pyevtx-rs

Python bindings for `https://github.com/omerbenamram/evtx/`.

## Installation

Available on PyPi - https://pypi.org/project/evtx/.

To install from PyPi - `pip install evtx`

### Wheels

Wheels are currently automatically built for Python 3.6, 3.7, 3.8, 3.9, 3.10 for all 64-bit platforms (Windows, macOS, and `manylinux`).

### Installation from sources

Installation is possible for other platforms by installing from sources.

This requires a Rust compiler and a recent enough Setuptools and Pip.

Run `pip install -e .`

## Usage

The API surface is currently fairly limited (only yields events as XML/JSON documents), but is planned to be expanded in the future.


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
from evtx.parser import PyEvtxParser


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
from evtx.parser import PyEvtxParser


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
