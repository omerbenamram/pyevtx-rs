from __future__ import annotations

import io
import struct
import zlib
from pathlib import Path

import pytest

from evtx import ChunkHeader, FileHeader, PyEvtxParser

from test_threads import CHUNK, chunk_offset, numbered_log


def test_header_is_what_the_file_header_says(small_sample):
    data = Path(small_sample).read_bytes()
    header = PyEvtxParser(small_sample).header
    assert isinstance(header, FileHeader)
    first, last, next_id = struct.unpack_from("<QQQ", data, 8)
    assert (header.first_chunk_number, header.last_chunk_number, header.next_record_id) == (
        first,
        last,
        next_id,
    )
    assert (header.major_version, header.minor_version) == (3, 1)
    assert (header.header_size, header.header_block_size, header.chunk_count) == (128, 4096, 1)
    assert (header.flags, header.is_dirty, header.is_full) == (0, False, False)
    assert header.checksum == header.computed_checksum == zlib.crc32(data[:120])
    assert header.checksum_valid
    assert repr(header).startswith("FileHeader(version=3.1, chunks 0..0, next_record_id=8")


def test_a_damaged_header_is_reported_and_the_log_still_reads(small_sample):
    data = bytearray(Path(small_sample).read_bytes())
    data[24] ^= 1  # the next record ID, which the CRC32 covers
    data[120] = 0x3  # dirty and full; the flags come after the 120 bytes the CRC32 covers
    parser = PyEvtxParser(io.BytesIO(data))
    header = parser.header
    assert (header.next_record_id, header.flags, header.is_dirty, header.is_full) == (
        9,
        3,
        True,
        True,
    )
    assert header.computed_checksum == zlib.crc32(data[:120]) != header.checksum
    assert not header.checksum_valid
    assert len(list(parser.records())) == 7
    # The header outlives the parser's records.
    assert parser.header.next_record_id == 9


def test_chunks_say_what_their_headers_say(small_sample):
    log = numbered_log(small_sample, 3)
    parser = PyEvtxParser(io.BytesIO(log))
    chunks = parser.chunks()
    assert all(isinstance(chunk, ChunkHeader) for chunk in chunks)
    assert [(chunk.number, chunk.offset) for chunk in chunks] == [
        (n, chunk_offset(n)) for n in range(3)
    ]
    assert [
        (
            chunk.first_event_record_number,
            chunk.last_event_record_number,
            chunk.first_event_record_id,
            chunk.last_event_record_id,
        )
        for chunk in chunks
    ] == [(7 * n + 1, 7 * n + 7, 7 * n + 1, 7 * n + 7) for n in range(3)]
    first = chunks[0]
    (records_crc,) = struct.unpack_from("<I", log, chunk_offset(0) + 52)
    (header_crc,) = struct.unpack_from("<I", log, chunk_offset(0) + 124)
    assert (first.header_size, first.last_event_record_data_offset, first.free_space_offset) == (
        128,
        7680,
        8784,
    )
    assert (first.flags, first.is_dirty) == (1, True)
    assert (first.events_checksum, first.header_chunk_checksum) == (records_crc, header_crc)
    # numbered_log renumbers the copies' records without updating their CRC32s.
    assert [(c.header_checksum_valid, c.events_checksum_valid) for c in chunks] == [
        (True, True),
        (False, False),
        (False, False),
    ]
    assert repr(first) == (
        "ChunkHeader(number=0, record_ids=1..7, flags=0x1, header_checksum_valid=True, "
        "events_checksum_valid=True)"
    )
    # chunks() doesn't use up the parser.
    assert [record["event_record_id"] for record in parser.records()] == list(range(1, 22))


def test_checksums_are_checked_whatever_validate_checksums_says(small_sample):
    log = numbered_log(small_sample, 2)
    parser = PyEvtxParser(io.BytesIO(log), validate_checksums=True)
    assert [(c.header_checksum_valid, c.events_checksum_valid) for c in parser.chunks()] == [
        (True, True),
        (False, False),
    ]
    # The records iterators still check them.
    with pytest.raises(RuntimeError, match="chunk number 1"):
        list(parser.records())


def test_chunks_of_a_wrapped_log_come_in_record_order(small_sample):
    copies = numbered_log(small_sample, 4)
    chunk = lambda n: copies[chunk_offset(n) : chunk_offset(n + 1)]  # noqa: E731
    # The log has wrapped around: its oldest records are in its third chunk.
    log = copies[:4096] + chunk(2) + chunk(3) + chunk(0) + chunk(1)
    parser = PyEvtxParser(io.BytesIO(log))
    chunks = parser.chunks()
    assert [chunk.number for chunk in chunks] == [2, 3, 0, 1]
    assert [chunk.first_event_record_id for chunk in chunks] == [1, 8, 15, 22]
    assert [record["event_record_id"] for record in parser.records()] == list(range(1, 29))


def test_a_chunk_that_cant_be_read_raises_or_is_skipped(small_sample):
    log = numbered_log(small_sample, 4)
    log[chunk_offset(2) : chunk_offset(2) + 8] = b"BadMagic"
    with pytest.raises(RuntimeError, match="Failed to parse chunk number 2"):
        PyEvtxParser(io.BytesIO(log)).chunks()

    parser = PyEvtxParser(io.BytesIO(log), skip_errors=True)
    with pytest.warns(RuntimeWarning, match="chunk number 2.*Invalid EVTX chunk header magic"):
        chunks = parser.chunks()
    assert [chunk.number for chunk in chunks] == [0, 1, 3]


def test_empty_chunks_are_left_out(small_sample):
    log = numbered_log(small_sample, 3)
    log[chunk_offset(1) : chunk_offset(2)] = bytes(CHUNK)
    assert [chunk.number for chunk in PyEvtxParser(io.BytesIO(log)).chunks()] == [0, 2]


def test_chunks_need_an_unused_parser(small_sample):
    parser = PyEvtxParser(small_sample)
    list(parser.records())
    with pytest.raises(RuntimeError, match="can only be used once"):
        parser.chunks()


@pytest.mark.parametrize("method", ["records", "records_json"])
def test_record_errors_name_their_chunk(small_sample, method):
    log = numbered_log(small_sample, 2)
    # The magic of the first record of chunk 1, 512 bytes into it.
    log[chunk_offset(1) + 512 : chunk_offset(1) + 516] = b"\xff" * 4
    with pytest.warns(RuntimeWarning) as warned:
        records = list(getattr(PyEvtxParser(io.BytesIO(log), skip_errors=True), method)())
    # The walk resyncs on the next record.
    assert [record["event_record_id"] for record in records] == [*range(1, 8), *range(9, 15)]
    file_offset = chunk_offset(1) + 512
    assert [str(warning.message) for warning in warned] == [
        "Skipping EVTX data: RuntimeError: Failed to read the record at offset 0x200 of chunk 1 "
        f"(file offset {file_offset:#x}): An error occurred while trying to deserialize evtx "
        "stream.: Invalid EVTX record header magic, expected `2a2a0000`, found "
        "`[FF, FF, FF, FF]`"
    ]
