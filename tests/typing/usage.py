"""Typed use of the public API. It is type-checked by tests/test_typing.py, and never run."""

from __future__ import annotations

from pathlib import Path

from typing_extensions import assert_type

from evtx import PyEvtxParser, PyRecordsIterator, Record


def record_ids(path: Path) -> list[int]:
    ids = []
    for record in PyEvtxParser(path).records_json():
        assert_type(record, Record)
        assert_type(record["event_record_id"], int)
        assert_type(record["timestamp"], str)
        assert_type(record["data"], str)
        assert_type(record["recovered"], bool)
        ids.append(record["event_record_id"])
    return ids


def first_xml(path: str) -> str:
    records = PyEvtxParser(path).records()
    assert_type(records, PyRecordsIterator)
    assert_type(iter(records), PyRecordsIterator)
    return next(records)["data"].upper()


def iterate_parser(path: str) -> list[str]:
    # Iterating the parser itself gives the XML records.
    return [record["data"] for record in PyEvtxParser(path)]
