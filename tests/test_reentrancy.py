"""A file-like object whose `read()` advances the iterator that is reading it."""

from __future__ import annotations

import io
import threading
from pathlib import Path

import pytest

import evtx
import evtx.wevt
from evtx import PyEvtxParser


class ReentrantFile:
    """Calls `next()` on the iterator that reads it, from its first `read()` after it has one."""

    def __init__(self, data: bytes, catch: bool) -> None:
        self.file = io.BytesIO(data)
        self.catch = catch
        self.iterator = None
        self.error: BaseException | None = None

    def read(self, size: int = -1) -> bytes:
        iterator, self.iterator = self.iterator, None
        if iterator is not None:
            try:
                next(iterator)
            except RuntimeError as error:
                self.error = error
                if not self.catch:
                    raise
        return self.file.read(size)

    def seek(self, offset: int, whence: int = 0) -> int:
        return self.file.seek(offset, whence)


def run_with_timeout(function):
    """The result of `function`, run on a thread: a deadlock fails the test instead of hanging it."""
    result = []

    def target():
        try:
            result.append((function(), None))
        except BaseException as error:
            result.append((None, error))

    thread = threading.Thread(target=target, daemon=True)
    thread.start()
    thread.join(timeout=30)
    assert not thread.is_alive(), "the iterator deadlocked on itself"
    value, error = result[0]
    if error is not None:
        raise error
    return value


@pytest.mark.parametrize("number_of_threads", [1, 0])
def test_reentering_from_read_raises_and_the_iterator_goes_on(
    small_sample: str, number_of_threads: int
) -> None:
    expected = len(list(PyEvtxParser(small_sample).records()))
    file = ReentrantFile(Path(small_sample).read_bytes(), catch=True)

    def iterate() -> int:
        records = PyEvtxParser(file, number_of_threads=number_of_threads).records()
        file.iterator = records
        return len(list(records))

    assert run_with_timeout(iterate) == expected
    assert isinstance(file.error, RuntimeError)
    assert "from inside its own file-like object's read()" in str(file.error)


@pytest.mark.parametrize("number_of_threads", [1, 0])
@pytest.mark.parametrize("skip_errors", [False, True])
def test_reentering_from_read_is_reported_when_read_lets_the_error_out(
    small_sample: str, number_of_threads: int, skip_errors: bool
) -> None:
    file = ReentrantFile(Path(small_sample).read_bytes(), catch=False)

    def iterate() -> None:
        records = PyEvtxParser(
            file, number_of_threads=number_of_threads, skip_errors=skip_errors
        ).records()
        file.iterator = records
        next(records)

    # Not the parser's account of a read that failed, and not a warning with skip_errors.
    with pytest.raises(RuntimeError, match="from inside its own file-like object's read()"):
        run_with_timeout(iterate)


def test_classes_say_which_module_they_are_in() -> None:
    for module, names in (
        (evtx, ["PyEvtxParser", "PyRecordsIterator", "FileHeader", "ChunkHeader", "WevtCache"]),
        (evtx.wevt, ["Manifest", "Provider", "Event", "Template", "TemplateItem"]),
    ):
        for name in names:
            cls = getattr(module, name)
            assert cls.__module__ == module.__name__
            assert repr(cls) == f"<class '{module.__name__}.{name}'>"
