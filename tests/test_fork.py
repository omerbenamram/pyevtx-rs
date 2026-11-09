from __future__ import annotations

import multiprocessing
import os

import pytest

from evtx import PyEvtxParser


pytestmark = pytest.mark.skipif(
    "fork" not in multiprocessing.get_all_start_methods(), reason="no fork on this platform"
)


def count_records(path: str) -> int:
    return sum(1 for _ in PyEvtxParser(path).records())


def run_in_fork(function, *args):
    """What `function(*args)` returns in a forked child, which gets 60 seconds."""
    with multiprocessing.get_context("fork").Pool(1) as pool:
        result = pool.apply_async(function, args)
        try:
            return result.get(timeout=60)
        except multiprocessing.TimeoutError:
            pytest.fail("the forked child hangs")


def test_a_forked_child_parses_after_its_parent_did(small_sample: str) -> None:
    # The parent's parse starts the threads that chunks are parsed on, and a fork leaves them
    # behind: the child needs its own.
    expected = count_records(small_sample)
    assert expected > 0

    assert run_in_fork(count_records, small_sample) == expected
    # The parent's threads are still there.
    assert count_records(small_sample) == expected


def test_one_thread_needs_no_pool(small_sample: str) -> None:
    before = len(os.listdir("/proc/self/task")) if os.path.isdir("/proc/self/task") else None
    records = sum(1 for _ in PyEvtxParser(small_sample, number_of_threads=1).records())
    assert records > 0
    if before is not None:
        assert len(os.listdir("/proc/self/task")) == before
