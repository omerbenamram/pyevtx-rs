from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

import pytest

from evtx import PyEvtxParser, WevtCache


class BytesPath:
    """An `os.PathLike` whose path is `bytes`."""

    def __init__(self, path: bytes) -> None:
        self.path = path

    def __fspath__(self) -> bytes:
        return self.path


def record_count(path: object, **kwargs: object) -> int:
    return len(list(PyEvtxParser(path, **kwargs).records()))


@pytest.fixture
def non_utf8_dir(tmp_path: Path) -> bytes:
    """A directory whose name is not valid UTF-8, on a file system that takes such names."""
    if sys.platform != "linux":
        pytest.skip("file names are arbitrary bytes on Linux only")
    directory = os.path.join(os.fsencode(tmp_path), b"caf\xe9")
    try:
        os.mkdir(directory)
    except OSError:
        pytest.skip("the file system refuses a name that is not UTF-8")
    return directory


def test_bytes_path_opens(small_sample: str) -> None:
    expected = record_count(small_sample)
    assert expected > 0
    assert record_count(os.fsencode(small_sample)) == expected
    assert record_count(BytesPath(os.fsencode(small_sample))) == expected


def test_non_utf8_file_name_opens(small_sample: str, non_utf8_dir: bytes) -> None:
    expected = record_count(small_sample)
    log = os.path.join(non_utf8_dir, b"log\xff.evtx")
    shutil.copyfile(small_sample, log)

    # As Python gives the name: bytes, or a str with the bytes as surrogate escapes.
    assert record_count(log) == expected
    assert record_count(os.fsdecode(log)) == expected
    assert record_count(Path(os.fsdecode(log))) == expected
    assert record_count(BytesPath(log)) == expected
    # And as os.listdir gives it.
    (name,) = os.listdir(os.fsdecode(non_utf8_dir))
    assert record_count(os.path.join(os.fsdecode(non_utf8_dir), name)) == expected


def test_non_utf8_paths_of_the_mta_and_the_wevt_cache(
    small_sample: str, non_utf8_dir: bytes, repo_root: Path
) -> None:
    samples = repo_root / "samples" / "mta"
    mta = os.path.join(non_utf8_dir, b"classic_1033.MTA")
    shutil.copyfile(samples / "LocaleMetaData" / "classic_1033.MTA", mta)
    for path in (mta, os.fsdecode(mta)):
        record = next(PyEvtxParser(str(samples / "classic.evtx"), mta=path).records())
        assert "Classic information message." in record["data"]

    cache = os.path.join(non_utf8_dir, b"cache.wevtcache")
    for path in (cache, os.fsdecode(cache)):
        WevtCache().dump(path, overwrite=True)
        assert os.path.exists(cache)
        WevtCache.load(path)
        assert record_count(small_sample, wevt_cache=path) > 0
        os.remove(cache)

    assert WevtCache().add_dir(non_utf8_dir) == 0


def test_a_path_that_is_not_there_is_not_found(tmp_path: Path) -> None:
    missing = tmp_path / "missing.evtx"
    for path in (str(missing), os.fsencode(missing), missing):
        with pytest.raises(FileNotFoundError):
            PyEvtxParser(path)


def test_fspath_of_another_type_is_a_type_error() -> None:
    class NotAPath:
        def __fspath__(self) -> int:
            return 1

    with pytest.raises(TypeError, match="__fspath__"):
        PyEvtxParser(NotAPath())


def test_str_that_no_file_name_encodes_to_is_refused() -> None:
    if sys.platform == "win32":
        pytest.skip("Windows file names are UTF-16, lone surrogates included")
    # A lone surrogate that isn't an escaped byte. It used to be opened as U+FFFD.
    with pytest.raises(UnicodeEncodeError):
        PyEvtxParser("\ud800.evtx")
