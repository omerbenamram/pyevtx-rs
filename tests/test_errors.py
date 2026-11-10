from __future__ import annotations

import io
import struct
import warnings
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
    assert f"Failed to parse chunk number 0 (record IDs {first_id}..{last_id})" in message
    assert "Failed to build string cache" in message
    assert "offset 65535" in message


def test_what_a_file_likes_seek_raises_is_what_the_caller_gets(small_sample: str):
    error = OSError("test seek failure")

    class BrokenSeek(io.BytesIO):
        fail = False

        def seek(self, offset, whence=0):
            if self.fail and offset == 4096:
                raise error
            return super().seek(offset, whence)

    stream = BrokenSeek(Path(small_sample).read_bytes())
    records = PyEvtxParser(stream).records()
    stream.fail = True
    # The exception itself, with its traceback through `seek`: it was a new OSError with the
    # message of this one in it.
    with pytest.raises(OSError) as raised:
        next(records)
    assert raised.value is error


class _RaisingReads(io.BytesIO):
    """A log whose `read()` raises `error` once `after` reads have been made."""

    def __init__(self, data, after, error):
        super().__init__(data)
        self.after = after
        self.error = error

    def read(self, *args):
        if self.after == 0:
            raise self.error
        self.after -= 1
        return super().read(*args)


@pytest.mark.parametrize("num_threads", [1, 4])
@pytest.mark.parametrize(
    "error",
    [KeyboardInterrupt(), ConnectionResetError("gone"), RuntimeError("mine"), ValueError("v")],
    ids=lambda error: type(error).__name__,
)
def test_what_a_file_likes_read_raises_is_what_the_caller_gets(
    small_sample, num_threads, error
):
    # It was a RuntimeError that said "Reached EOF", which skip_errors=True then skipped: a
    # Ctrl-C during a read went unseen, and the log just looked shorter.
    data = open(small_sample, "rb").read()

    # While the parser is created (it reads the file header).
    with pytest.raises(type(error)) as raised:
        PyEvtxParser(_RaisingReads(data, 0, error))
    assert raised.value is error

    # While records are read, with errors skipped or not.
    for skip_errors in (False, True):
        log = _RaisingReads(data, 3, error)
        parser = PyEvtxParser(
            log, number_of_threads=num_threads, skip_errors=skip_errors
        )
        with warnings.catch_warnings():
            warnings.simplefilter("error")
            with pytest.raises(type(error)) as raised:
                list(parser.records())
        assert raised.value is error

    # And while chunk headers are read.
    log = _RaisingReads(data, 3, error)
    with pytest.raises(type(error)) as raised:
        PyEvtxParser(log, skip_errors=True).chunks()
    assert raised.value is error
