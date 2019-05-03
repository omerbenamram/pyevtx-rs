import pytest

from pathlib import Path
from evtx.evtx_parser import PyEvtxParser

SAMPLES = Path(__file__).parent.parent / 'samples'


@pytest.fixture
def small_sample() -> str:
    return str(SAMPLES / 'Security_short_selected.evtx')


def test_it_works():
    parser = PyEvtxParser("/Users/omerba/Workspace/evtx-rs/samples/security.evtx")
    records = list(parser)
    assert len(records) == 2261


def test_small_sample(small_sample):
    parser = PyEvtxParser(small_sample)
    records = list(parser)
    assert len(records) == 7


def test_it_returns_error_on_non_existing_path():
    with pytest.raises(RuntimeError):
        parser = PyEvtxParser("non_existing")
