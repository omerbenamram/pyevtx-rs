from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

import evtx


USAGE = Path(__file__).parent / "typing" / "usage.py"


def test_typed_usage_passes_mypy(tmp_path: Path) -> None:
    """The stubs type a record as a `Record`, so `record["data"]` is a `str` to a type checker."""
    pytest.importorskip("mypy")
    result = subprocess.run(
        [sys.executable, "-m", "mypy", "--strict", "--cache-dir", str(tmp_path), str(USAGE)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_record_has_the_keys_of_its_type(small_sample: str) -> None:
    types = {"event_record_id": int, "timestamp": str, "data": str, "recovered": bool}
    assert evtx.Record.__annotations__.keys() == types.keys()
    for records in (
        evtx.PyEvtxParser(small_sample).records(),
        evtx.PyEvtxParser(small_sample).records_json(),
        iter(evtx.PyEvtxParser(small_sample)),
    ):
        record = next(records)
        assert type(record) is dict
        assert {key: type(value) for key, value in record.items()} == types

