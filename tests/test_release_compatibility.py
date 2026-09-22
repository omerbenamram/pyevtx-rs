"""Keep 0.13.x error handling compatible while backporting wheel support."""

import io
from pathlib import Path

import pytest

from evtx import PyEvtxParser


@pytest.mark.parametrize("method", ["records", "records_json"])
def test_malformed_record_keeps_0130_behavior(small_sample, method):
    source = bytearray(Path(small_sample).read_bytes())
    expected = list(getattr(PyEvtxParser(io.BytesIO(source)), method)())[1:]
    source[4096 + 512 + 24] = 0xFF  # invalid BinXML token in the first record
    parser = PyEvtxParser(io.BytesIO(source), validate_checksums=False)
    assert list(getattr(parser, method)()) == expected
