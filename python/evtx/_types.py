"""Types of the values the native module returns."""

from __future__ import annotations

from typing import TypedDict


class Record(TypedDict):
    """A record, as the iterators of `records()` and `records_json()` return it."""

    event_record_id: int
    #: When the record was written, in UTC, as text.
    timestamp: str
    #: The record as XML or JSON text.
    data: str
    #: Whether the record was carved from unallocated chunk space (`recover_records=True`).
    recovered: bool
