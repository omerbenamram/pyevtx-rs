"""Checks the records PyEvtxParser yields against evtx's own record iterators, for every log
under the folder that `EVTX_SAMPLES` names (CI uses evtx's `samples/`), with the default settings
and with `recover_records`. The Rust side is `examples/records.rs`, built against the same evtx
as the bindings, so a difference is in how the bindings walk the chunks and records: their
order, which records are left out, and where the errors are. Skipped without `EVTX_SAMPLES`."""

from __future__ import annotations

import json
import os
import subprocess
import warnings
from pathlib import Path

import pytest

from evtx import PyEvtxParser


REPO = Path(__file__).resolve().parents[1]
SAMPLES = os.environ.get("EVTX_SAMPLES")

pytestmark = pytest.mark.skipif(not SAMPLES, reason="EVTX_SAMPLES names no folder of logs")


def samples() -> list[str]:
    if not SAMPLES:
        return []
    return sorted(path.relative_to(SAMPLES).as_posix() for path in Path(SAMPLES).rglob("*.evtx"))


@pytest.fixture(scope="session")
def rust_records():
    """Runs `examples/records.rs`, built once with the features of the bindings, on a log."""
    build = subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "--locked",
            "--example",
            "records",
            "--no-default-features",
            "--features",
            "wevt_templates",
            "--message-format=json",
        ],
        cwd=REPO,
        check=True,
        capture_output=True,
        text=True,
    )
    executable = next(
        message["executable"]
        for message in map(json.loads, build.stdout.splitlines())
        if message.get("reason") == "compiler-artifact" and message["target"]["name"] == "records"
    )

    def run(path: str, output_format: str, recover: bool) -> list[dict]:
        args = [executable, path, output_format] + (["recover"] if recover else [])
        lines = subprocess.run(args, check=True, capture_output=True).stdout.splitlines()
        return [json.loads(line) for line in lines]

    return run


@pytest.mark.parametrize("recover", [False, True], ids=["live", "recover"])
@pytest.mark.parametrize("method", ["records", "records_json"])
@pytest.mark.parametrize("sample", samples())
def test_records_match_evtx(rust_records, sample, method, recover):
    path = str(Path(SAMPLES) / sample)
    lines = rust_records(path, "xml" if method == "records" else "json", recover)
    errors = [line["error"] for line in lines if "error" in line]
    expected = [line for line in lines if "error" not in line]

    parser = PyEvtxParser(path, skip_errors=True, recover_records=recover)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        records = list(getattr(parser, method)())
    skipped = [str(w.message) for w in caught if str(w.message).startswith("Skipping EVTX data")]

    for index, (record, line) in enumerate(zip(records, expected)):
        assert record == {**line, "timestamp": line["timestamp"] + " UTC"}, f"record {index}"
    assert len(records) == len(expected)
    assert len(skipped) == len(errors), (skipped, errors)
