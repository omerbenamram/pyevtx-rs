import pytest
from pyevtx_rs.evtx_parser import PyEvtxParser


def test_it_works():
    parser = PyEvtxParser("/Users/omerba/Workspace/evtx-rs/samples/security.evtx")
    for record in parser:
        print(record)


def test_it_returns_error_on_non_existing_path():
    with pytest.raises(RuntimeError):
        parser = PyEvtxParser("non_existing")


