from __future__ import annotations

import os
import subprocess
import sys
import sysconfig

import pytest


pytestmark = pytest.mark.skipif(
    not sysconfig.get_config_var("Py_GIL_DISABLED"),
    reason="requires free-threaded CPython",
)


def test_import_and_parallel_parsers_keep_gil_disabled(small_sample):
    # Use a fresh interpreter so earlier imports cannot hide an extension that
    # enables the GIL. A process timeout also bounds a potential deadlock.
    script = """
import concurrent.futures
import gc
import io
from pathlib import Path
import sys
import threading

assert not sys._is_gil_enabled()
from evtx import PyEvtxParser
assert not sys._is_gil_enabled(), "importing evtx enabled the GIL"

path = sys.argv[1]
data = Path(path).read_bytes()
start = threading.Barrier(4)
def parse(worker):
    start.wait(timeout=10)
    method = "records_json" if worker % 2 else "records"
    expected = list(getattr(PyEvtxParser(path), method)())
    for _ in range(30):
        source = io.BytesIO(data) if worker < 2 else path
        actual = list(getattr(PyEvtxParser(source), method)())
        assert actual == expected
    return len(expected)

with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    futures = [pool.submit(parse, worker) for worker in range(4)]
    for _ in range(10):
        gc.collect()
    assert [future.result(timeout=30) for future in futures] == [7] * 4
assert not sys._is_gil_enabled()
"""
    env = os.environ.copy()
    env.pop("PYTHON_GIL", None)
    result = subprocess.run(
        [sys.executable, "-c", script, small_sample],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert result.returncode == 0, result.stdout + result.stderr
