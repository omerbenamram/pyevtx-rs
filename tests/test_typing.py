from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
from pathlib import Path

import pytest

import evtx


USAGE = Path(__file__).parent / "typing_usage" / "usage.py"


def test_typed_usage_passes_mypy(tmp_path: Path) -> None:
    """The stubs type a record as a dict, so `record["data"]` passes a type checker."""
    # Not imported: mypy runs in its own process, and its compiled modules needn't load here.
    if importlib.util.find_spec("mypy") is None:
        pytest.skip("mypy is not installed")
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "mypy",
            "--strict",
            # No configuration of the user's or the directory's.
            "--config-file",
            os.devnull,
            "--cache-dir",
            str(tmp_path),
            str(USAGE),
        ],
        capture_output=True,
        text=True,
        timeout=300,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_a_record_is_a_dict_of_four_keys(small_sample: str) -> None:
    types = {"event_record_id": int, "timestamp": str, "data": str, "recovered": bool}
    for records in (
        evtx.PyEvtxParser(small_sample).records(),
        evtx.PyEvtxParser(small_sample).records_json(),
        iter(evtx.PyEvtxParser(small_sample)),
    ):
        record = next(records)
        assert type(record) is dict
        assert {key: type(value) for key, value in record.items()} == types

