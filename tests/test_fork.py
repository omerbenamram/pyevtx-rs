from __future__ import annotations

import faulthandler
import multiprocessing

import pytest

from evtx import PyEvtxParser

from test_threads import numbered_log


pytestmark = pytest.mark.skipif(
    "fork" not in multiprocessing.get_all_start_methods(), reason="no fork on this platform"
)


def count_records(path: str) -> int:
    return sum(1 for _ in PyEvtxParser(path).records())


def _child(connection, function, args) -> None:
    # If it hangs, say where, on stderr, while the parent still waits.
    faulthandler.enable()
    faulthandler.dump_traceback_later(30, exit=True)
    connection.send(function(*args))
    connection.close()


def run_in_fork(function, *args):
    """What `function(*args)` returns in a forked child, which gets 60 seconds."""
    context = multiprocessing.get_context("fork")
    receiver, sender = context.Pipe(duplex=False)
    child = context.Process(target=_child, args=(sender, function, args))
    child.start()
    sender.close()
    try:
        # A child that dies closes its end, which ends the wait.
        if receiver.poll(60):
            try:
                return receiver.recv()
            except EOFError:
                pass
        child.join(5)
        if child.exitcode is None:
            pytest.fail("the forked child hangs")
        pytest.fail(f"the forked child died with exit code {child.exitcode}")
    finally:
        if child.is_alive():
            child.kill()
        child.join()


def test_a_forked_child_parses_after_its_parent_did(small_sample: str) -> None:
    # The parent's parse starts the threads that chunks are parsed on, and a fork leaves them
    # behind: the child needs its own.
    expected = count_records(small_sample)
    assert expected > 0

    assert run_in_fork(count_records, small_sample) == expected
    # The parent's threads are still there.
    assert count_records(small_sample) == expected


# What a forked child inherits: arguments to it would be pickled.
inherited = None


def finish(iterator=None) -> tuple[int, list[str]]:
    iterator = inherited if iterator is None else iterator
    records, errors = 0, []
    while True:
        try:
            next(iterator)
            records += 1
        except StopIteration:
            return records, errors
        except RuntimeError as error:
            errors.append(str(error))


def test_an_iterator_started_before_the_fork_says_what_it_lost(tmp_path, small_sample) -> None:
    # Enough chunks that some are still to be read when the process forks.
    log = tmp_path / "many.evtx"
    log.write_bytes(numbered_log(small_sample, 64))
    total = count_records(str(log))
    assert total == 64 * 7

    iterator = PyEvtxParser(str(log), number_of_threads=2).records()
    next(iterator)
    # The chunks being parsed stay with the parent's threads; the child reads on after them.
    global inherited
    inherited = iterator
    try:
        records, errors = run_in_fork(finish)
    finally:
        inherited = None

    assert len(errors) == 1 and "forked" in errors[0], errors
    assert 0 < records < total - 1
    # The parent's iterator is unaffected.
    assert finish(iterator) == (total - 1, [])
