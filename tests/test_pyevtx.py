import pytest

from pathlib import Path
from evtx import PyEvtxParser
import json

SAMPLES = Path(__file__).parent.parent / 'samples'


@pytest.fixture
def small_sample() -> str:
    return str(SAMPLES / 'Security_short_selected.evtx')


def test_it_works(small_sample):
    parser = PyEvtxParser(small_sample)
    records = list(parser)

    assert len(records) == 7

    assert records[0]['event_record_id'] == 7
    assert records[0]['timestamp'].endswith('UTC')
    assert '<EventID>4673</EventID>' in records[0]['data']


def test_it_works_with_records(small_sample):
    parser = PyEvtxParser(small_sample)
    records = list(parser.records())
    assert len(records) == 7

    assert records[0]['event_record_id'] == 7
    assert records[0]['timestamp'].endswith('UTC')
    assert '<EventID>4673</EventID>' in records[0]['data']


def test_it_works_with_json(small_sample):
    parser = PyEvtxParser(small_sample)
    records = list(parser.records_json())
    assert len(records) == 7

    assert records[0]['event_record_id'] == 7
    assert records[0]['timestamp'].endswith('UTC')
    assert json.loads(records[0]['data'])['Event']['System']['EventID'] == 4673


def test_it_returns_error_when_iterating_twice(small_sample):
    parser = PyEvtxParser(small_sample)
    _ = list(parser.records())

    with pytest.raises(RuntimeError):
        parser.records()


def test_it_returns_error_on_non_existing_path():
    with pytest.raises(RuntimeError):
        parser = PyEvtxParser("non_existing")


def test_it_returns_error_when_using_next_on_parser(small_sample):
    parser = PyEvtxParser(small_sample)

    with pytest.raises(NotImplementedError):
        next(parser)
