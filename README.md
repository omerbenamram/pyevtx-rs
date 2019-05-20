[![Build Status](https://dev.azure.com/benamram/evtx/_apis/build/status/omerbenamram.pyevtx-rs?branchName=master)](https://dev.azure.com/benamram/evtx/_build/latest?definitionId=2&branchName=master)

# pyevtx-rs

Python bindings for `https://github.com/omerbenamram/evtx/`.

## Installation

Available on PyPi - https://pypi.org/project/evtx/.

To install from PyPi - `pip install evtx` 

### Wheels
Wheels are currently automatically built for python3.6 python3.7 for all 64-bit platforms (Windows, macOS, and `manylinux`).

### Installation from sources
Installation is possible for other platforms by installing from sources, this requires a nightly rust compiler and `setuptools-rust`.

Run `python setup.py install`

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