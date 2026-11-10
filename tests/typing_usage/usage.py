"""Typed use of the public API. It is type-checked by tests/test_typing.py, and never run."""

from __future__ import annotations

from pathlib import Path

from typing import Any

from evtx import PyEvtxParser, PyRecordsIterator


def record_ids(path: Path) -> list[int]:
    ids: list[int] = []
    for record in PyEvtxParser(path).records_json():
        # A record is a plain dict, so it goes wherever one does and can be added to.
        row: dict[str, Any] = record
        row["parsed"] = len(record["data"])
        ids.append(record["event_record_id"])
    return ids


def all_records(path: Path) -> list[dict[str, Any]]:
    return list(PyEvtxParser(path).records())


def first_xml(path: str) -> str:
    records = PyEvtxParser(path).records()
    again: PyRecordsIterator = iter(records)
    data: str = next(again)["data"]
    return data.upper()


def iterate_parser(path: str) -> list[str]:
    # Iterating the parser itself gives the XML records.
    return [str(record["data"]) for record in PyEvtxParser(path)]
