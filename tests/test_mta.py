"""`mta=` adds the strings of a LocaleMetaData `.MTA` file to each record."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from evtx import PyEvtxParser

SAMPLES = Path(__file__).parent.parent / "samples" / "mta"
LOG = str(SAMPLES / "classic.evtx")
STRINGS = SAMPLES / "LocaleMetaData" / "classic_1033.MTA"


def rendering_info(**kwargs):
    return [
        json.loads(record["data"])["Event"].get("RenderingInfo")
        for record in PyEvtxParser(LOG, **kwargs).records_json()
    ]


def test_mta_adds_each_records_strings():
    assert rendering_info() == [None] * 4

    # What Windows' EvtFormatMessage gives for each event from this archive (see
    # samples/mta/README.md): the last event has no message.
    infos = rendering_info(mta=STRINGS)
    assert [info.get("Message") for info in infos] == [
        "Classic information message.",
        "Classic warning with\r\ntwo lines.",
        "Classic error <&>.",
        None,
    ]
    assert [info["Level"] for info in infos] == ["Information", "Warning", "Error", "Information"]


def test_mta_takes_a_path_as_str_and_renders_in_xml():
    record = next(PyEvtxParser(LOG, mta=str(STRINGS)).records())
    assert "<Message>Classic information message.</Message>" in record["data"]


def test_mta_errors(tmp_path):
    with pytest.raises(FileNotFoundError, match="missing.MTA"):
        PyEvtxParser(LOG, mta=tmp_path / "missing.MTA")
    not_mta = tmp_path / "not.MTA"
    not_mta.write_bytes(b"not an MTA file at all")
    with pytest.raises(ValueError, match="not an MTA file"):
        PyEvtxParser(LOG, mta=not_mta)
    with pytest.raises(TypeError, match="mta must be a path"):
        PyEvtxParser(LOG, mta=42)
