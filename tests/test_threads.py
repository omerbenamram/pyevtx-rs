from __future__ import annotations

import gc
import io
import re
import struct
import sys
import threading
from pathlib import Path

import pytest

from evtx import PyEvtxParser


CHUNK = 65536
GIL_ENABLED = getattr(sys, "_is_gil_enabled", lambda: True)()


def numbered_log(sample: str, chunks: int) -> bytearray:
    """A log of `chunks` copies of the sample's chunk, with the records numbered on from one
    copy to the next: copy `n` has records 7n + 1 to 7n + 7."""
    source = Path(sample).read_bytes()
    header, chunk = source[:4096], source[4096 : 4096 + CHUNK]
    (free_space,) = struct.unpack_from("<I", chunk, 48)
    log = bytearray(header)
    for n in range(chunks):
        copy = bytearray(chunk)
        # The chunk header's first and last record numbers and IDs.
        for field in (8, 16, 24, 32):
            (value,) = struct.unpack_from("<Q", copy, field)
            struct.pack_into("<Q", copy, field, value + 7 * n)
        offset = 512
        while offset < free_space:
            size, record_id = struct.unpack_from("<IQ", copy, offset + 4)
            struct.pack_into("<Q", copy, offset + 8, record_id + 7 * n)
            offset += size
        log += copy
    return log


def chunk_offset(n: int) -> int:
    return 4096 + n * CHUNK


def records_and_errors(records) -> list:
    """The IDs of the records, and in their places the chunk or record each error names."""
    seen = []
    while True:
        try:
            record = next(records)
        except StopIteration:
            return seen
        except RuntimeError as error:
            seen.append(re.search(r"(chunk|record) number \d+", str(error))[0])
        else:
            seen.append(record["event_record_id"])


@pytest.mark.parametrize("method", ["records", "records_json"])
def test_records_are_the_same_with_any_number_of_threads(small_sample, method):
    log = numbered_log(small_sample, 10)
    one = list(getattr(PyEvtxParser(io.BytesIO(log), number_of_threads=1), method)())
    assert [record["event_record_id"] for record in one] == list(range(1, 71))
    for threads in (0, 2, 3, 64):
        parser = PyEvtxParser(io.BytesIO(log), number_of_threads=threads)
        assert list(getattr(parser, method)()) == one, threads


@pytest.mark.parametrize("threads", [1, 4])
def test_errors_keep_their_places_among_the_records(small_sample, threads):
    log = numbered_log(small_sample, 6)
    log[chunk_offset(2) : chunk_offset(2) + 8] = b"BadMagic"
    log[chunk_offset(4) + 512 + 24] = 0xFF  # the BinXML of record 29, the first of chunk 4
    records = PyEvtxParser(io.BytesIO(log), number_of_threads=threads).records()
    assert records_and_errors(records) == [
        *range(1, 15),
        "chunk number 2",
        *range(22, 29),
        "record number 29",
        *range(30, 43),
    ]


def test_file_like_objects_are_read_on_the_calling_thread(small_sample):
    class Reader(io.BytesIO):
        threads = set()

        def read(self, *args):
            self.threads.add(threading.get_ident())
            return super().read(*args)

    reader = Reader(bytes(numbered_log(small_sample, 10)))
    assert len(list(PyEvtxParser(reader, number_of_threads=4).records())) == 70
    assert Reader.threads == {threading.get_ident()}


def test_an_iterator_can_be_dropped_while_chunks_are_parsed(small_sample):
    parser = PyEvtxParser(io.BytesIO(numbered_log(small_sample, 10)), number_of_threads=4)
    records = parser.records_json()
    assert next(records)["event_record_id"] == 1
    del records, parser
    gc.collect()


@pytest.mark.parametrize("threads", [1, 4])
def test_threads_can_share_an_iterator(small_sample, tmp_path, threads):
    path = tmp_path / "numbered.evtx"
    path.write_bytes(numbered_log(small_sample, 20))
    records = PyEvtxParser(str(path), number_of_threads=threads).records_json()
    seen, errors = ([], []), []

    def take(ids):
        try:
            for record in records:
                ids.append(record["event_record_id"])
        except Exception as error:  # noqa: BLE001 - any error fails the test
            errors.append(error)

    workers = [threading.Thread(target=take, args=(ids,)) for ids in seen]
    for worker in workers:
        worker.start()
    for worker in workers:
        worker.join()
    assert errors == []
    # Each record went to one of the threads, and each thread got its records in order.
    assert sorted(seen[0] + seen[1]) == list(range(1, 141))
    assert all(ids == sorted(ids) for ids in seen)


@pytest.mark.skipif(not GIL_ENABLED, reason="the GIL is disabled")
def test_other_threads_run_while_chunks_are_parsed(small_sample, tmp_path):
    path = tmp_path / "numbered.evtx"
    path.write_bytes(numbered_log(small_sample, 50))
    go, ran = threading.Event(), threading.Event()

    def other():
        go.wait()
        ran.set()

    thread = threading.Thread(target=other)
    interval = sys.getswitchinterval()
    # With no forced switches, the other thread can only take the GIL when this one lets go of
    # it, which parsing the log in Rust does for each chunk.
    sys.setswitchinterval(1000)
    try:
        thread.start()
        go.set()
        for _ in range(20):
            for _ in PyEvtxParser(str(path), number_of_threads=1).records():
                pass
            if ran.is_set():
                break
        assert ran.is_set()
    finally:
        sys.setswitchinterval(interval)
        thread.join()
