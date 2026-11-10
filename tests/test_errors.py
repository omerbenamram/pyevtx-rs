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
    """A log whose `read()` raises `error` from the offset `at` on."""

    def __init__(self, data, at, error):
        super().__init__(data)
        self.at = at
        self.error = error

    def read(self, *args):
        if self.tell() >= self.at:
            raise self.error
        return super().read(*args)


_HEADER, _CHUNK = 4096, 65536


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
    source = Path(small_sample).read_bytes()
    chunk = source[_HEADER : _HEADER + _CHUNK]
    data = source[:_HEADER] + chunk * 6
    per_chunk = len(list(PyEvtxParser(io.BytesIO(source)).records()))

    def parser(at, **kwargs):
        return PyEvtxParser(
            _RaisingReads(data, at, error), number_of_threads=num_threads, **kwargs
        )

    # While the parser is created (it reads the file header).
    with pytest.raises(type(error)) as raised:
        parser(0)
    assert raised.value is error

    # While the iterator is made (it reads the chunk headers, to start at the oldest).
    with pytest.raises(type(error)) as raised:
        parser(_HEADER + _CHUNK).records()
    assert raised.value is error

    # While records are read, with errors skipped or not. Records read before come first,
    # and the exception ends the log: the parser may be anywhere in the file by then.
    for skip_errors in (False, True):
        log = _RaisingReads(data, len(data), error)
        records = PyEvtxParser(
            log, number_of_threads=num_threads, skip_errors=skip_errors
        ).records()
        first = next(records)
        log.at = _HEADER + 3 * _CHUNK
        got = [first]
        with warnings.catch_warnings():
            warnings.simplefilter("error")
            with pytest.raises(type(error)) as raised:
                got.extend(records)
        assert raised.value is error
        assert len(got) <= 3 * per_chunk
        if num_threads == 1:
            assert len(got) == 3 * per_chunk
        log.at = len(data)
        assert list(records) == []

    # And while chunk headers are read.
    log = _RaisingReads(data, len(data), error)
    headers = PyEvtxParser(log, skip_errors=True)
    log.at = _HEADER + 3 * _CHUNK
    with pytest.raises(type(error)) as raised:
        headers.chunks()
    assert raised.value is error


def test_an_interrupted_read_is_tried_again(small_sample):
    # `InterruptedError` is the one exception a read is tried again after.
    class Interrupted(io.BytesIO):
        interrupts = 3

        def read(self, *args):
            if self.interrupts and self.tell() >= _HEADER:
                self.interrupts -= 1
                raise InterruptedError()
            return super().read(*args)

    source = Path(small_sample).read_bytes()
    log = Interrupted(source)
    assert len(list(PyEvtxParser(log).records())) == len(
        list(PyEvtxParser(io.BytesIO(source)).records())
    )
    assert log.interrupts == 0
