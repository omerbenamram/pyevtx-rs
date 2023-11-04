import pytest
import io

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

    assert records[0]['event_record_id']
    assert records[0]['timestamp'].endswith('UTC')
    assert '<EventID>' in records[0]['data']


def test_it_works_with_records(small_sample):
    parser = PyEvtxParser(small_sample)
    records = list(parser.records())
    assert len(records) == 7

    assert records[0]['event_record_id']
    assert records[0]['timestamp'].endswith('UTC')
    assert '<EventID>' in records[0]['data']


def test_it_works_with_json(small_sample):
    parser = PyEvtxParser(small_sample)
    records = list(parser.records_json())
    assert len(records) == 7

    assert records[0]['event_record_id']
    assert records[0]['timestamp'].endswith('UTC')
    assert json.loads(records[0]['data'])['Event']['System']['EventID']


def test_it_returns_error_when_iterating_twice(small_sample):
    parser = PyEvtxParser(small_sample)
    _ = list(parser.records())

    with pytest.raises(RuntimeError):
        parser.records()


def test_it_returns_error_on_non_existing_path():
    with pytest.raises(FileNotFoundError):
        parser = PyEvtxParser("non_existing")


def test_it_returns_error_when_using_next_on_parser(small_sample):
    parser = PyEvtxParser(small_sample)

    with pytest.raises(NotImplementedError):
        next(parser)


def test_it_works_on_io_object(small_sample):
    with open(small_sample, "rb") as o:
        r = o.read()

    parser = PyEvtxParser(io.BytesIO(r))
    records = list(parser.records())
    assert len(records) == 7

    assert records[0]['event_record_id']
    assert records[0]['timestamp'].endswith('UTC')
    assert '<EventID>' in records[0]['data']


def test_it_works_on_file_backed_object(small_sample):
    with open(small_sample, "rb") as o:
        parser = PyEvtxParser(o)

        records = list(parser.records())

    assert len(records) == 7

    assert records[0]['event_record_id']
    assert records[0]['timestamp'].endswith('UTC')
    assert '<EventID>' in records[0]['data']


def test_it_fails_on_file_opened_as_text(small_sample):
    with pytest.raises(OSError) as e:
        with open(small_sample, "rt") as o:
            parser = PyEvtxParser(o)

    assert "decode byte" in e.value.args[0]


def test_it_fails_nicely_on_close_files(small_sample):
    with open(small_sample, "rb") as o:
        parser = PyEvtxParser(o)

    with pytest.raises(OSError) as e:
        records = list(parser.records())

    assert "closed file" in e.value.args[0]


def test_it_fails_on_non_file_object():
    with pytest.raises(TypeError):
        parser = PyEvtxParser(3)


def test_it_supports_various_ascii_codecs(small_sample):
    with open(small_sample, "rb") as o:
        parser = PyEvtxParser(o, ansi_codec="ascii")

        records = list(parser.records())

        assert len(records) == 7

        assert records[0]['event_record_id']
        assert records[0]['timestamp'].endswith('UTC')
        assert '<EventID>' in records[0]['data']


def test_it_supports_various_num_threads(small_sample):
    with open(small_sample, "rb") as o:
        parser = PyEvtxParser(o, number_of_threads=1)

        records = list(parser.records())

        assert len(records) == 7

        assert records[0]['event_record_id'] == 1, "Expect records to be in order when using a single thread"
        assert records[0]['timestamp'].endswith('UTC')
        assert '<EventID>5152</EventID>' in records[0]['data']


