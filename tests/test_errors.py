from __future__ import annotations

import io
import struct
from pathlib import Path

import pytest

from evtx import PyEvtxParser


@pytest.mark.parametrize("method", ["records", "records_json"])
def test_chunk_error_preserves_physical_index_and_cause(small_sample: str, method: str):
    source = Path(small_sample).read_bytes()
    chunk = source[4096 : 4096 + 65536]
    broken = bytearray(chunk)
    broken[:8] = b"BadMagic"
    # Include an empty chunk: counting yielded chunks would report the wrong index.
    stream = io.BytesIO(source[:4096] + chunk + bytes(65536) + broken)
    records = getattr(PyEvtxParser(stream, validate_checksums=False), method)()
    for _ in range(7):
        next(records)
    with pytest.raises(RuntimeError) as raised:
        next(records)
    message = str(raised.value)
    assert "Failed to parse chunk number 2" in message
    assert "Failed to parse chunk header" in message
    assert "Invalid EVTX chunk header magic" in message
    assert "42, 61, 64, 4D, 61, 67, 69, 63" in message


def test_string_cache_error_identifies_records_and_offset(small_sample: str):
    source = Path(small_sample).read_bytes()
    chunk = bytearray(source[4096 : 4096 + 65536])
    first_id, last_id = struct.unpack_from("<QQ", chunk, 24)
    struct.pack_into("<I", chunk, 128, 65535)
    records = PyEvtxParser(io.BytesIO(source[:4096] + chunk), validate_checksums=False).records()
    with pytest.raises(RuntimeError) as raised:
        next(records)
    message = str(raised.value)
    assert f"record IDs {first_id}..{last_id}" in message
    assert "Failed to build string cache" in message
    assert "offset 65535" in message


def test_io_error_preserves_exception_type_and_context(small_sample: str):
    class BrokenSeek(io.BytesIO):
        fail = False

        def seek(self, offset, whence=0):
            if self.fail and offset == 4096:
                raise OSError("test seek failure")
            return super().seek(offset, whence)

    stream = BrokenSeek(Path(small_sample).read_bytes())
    records = PyEvtxParser(stream).records()
    stream.fail = True
    with pytest.raises(OSError) as raised:
        next(records)
    assert "Failed to parse chunk number 0" in str(raised.value)
    assert "test seek failure" in str(raised.value)
