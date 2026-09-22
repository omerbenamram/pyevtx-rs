from __future__ import annotations

import io
import warnings
from pathlib import Path

import pytest

from evtx import PyEvtxParser


@pytest.mark.parametrize("method", ["records", "records_json"])
def test_bad_records_raise_by_default_and_iterator_can_resume(small_sample, method):
    source = bytearray(Path(small_sample).read_bytes())
    source[4096 + 512 + 24] = 0xFF  # first record's BinXML token
    records = getattr(PyEvtxParser(io.BytesIO(source), validate_checksums=False), method)()
    with pytest.raises(RuntimeError, match="record number 1.*invalid byte `0xff`"):
        next(records)
    assert [record["event_record_id"] for record in records] == list(range(2, 8))


@pytest.mark.parametrize("method", ["records", "records_json"])
def test_skip_errors_warns_and_preserves_remaining_records(small_sample, method):
    source = bytearray(Path(small_sample).read_bytes())
    expected = list(getattr(PyEvtxParser(io.BytesIO(source)), method)())[1:]
    source[4096 + 512 + 24] = 0xFF
    parser = PyEvtxParser(io.BytesIO(source), validate_checksums=False, skip_errors=True)
    with pytest.warns(RuntimeWarning, match="record number 1.*invalid byte `0xff`") as caught:
        records = list(getattr(parser, method)())
    assert len(caught) == 1
    assert records == expected


def test_skip_errors_recovers_from_a_bad_chunk(small_sample):
    source = Path(small_sample).read_bytes()
    chunk = source[4096 : 4096 + 65536]
    broken = bytearray(chunk)
    broken[:8] = b"BadMagic"
    stream = io.BytesIO(source[:4096] + chunk + broken + chunk)
    parser = PyEvtxParser(stream, validate_checksums=False, skip_errors=True)
    with pytest.warns(RuntimeWarning, match="chunk number 1.*Invalid EVTX chunk header magic"):
        records = list(parser.records())
    assert [r["event_record_id"] for r in records] == list(range(1, 8)) * 2


def test_skip_errors_respects_warnings_as_errors(small_sample):
    source = bytearray(Path(small_sample).read_bytes())
    source[4096 + 512 + 24] = 0xFF
    records = PyEvtxParser(io.BytesIO(source), validate_checksums=False, skip_errors=True).records()
    with warnings.catch_warnings():
        warnings.simplefilter("error", RuntimeWarning)
        with pytest.raises(RuntimeWarning, match="record number 1"):
            next(records)
    assert next(records)["event_record_id"] == 2


def test_skip_errors_does_not_swallow_io_exceptions(small_sample):
    stream = io.BytesIO(Path(small_sample).read_bytes())
    records = PyEvtxParser(stream, skip_errors=True).records()
    stream.close()
    with pytest.raises(OSError, match="closed file"):
        next(records)
