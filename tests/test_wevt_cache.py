from __future__ import annotations

from pathlib import Path

import pytest

from evtx import PyEvtxParser, WevtCache


def test_wevt_cache_smoke(tmp_path: Path, small_sample: str):
    # An empty cache is valid; this is a smoke test that the API surface exists and
    # attaching it to the parser doesn't change baseline parsing behavior.
    cache_path = tmp_path / "cache.wevtcache"
    WevtCache().dump(cache_path, overwrite=True)

    cache = WevtCache.load(cache_path)
    parser = PyEvtxParser(small_sample, wevt_cache=cache)
    records = list(parser.records())
    assert len(records) == 7

    # Also allow passing the cache path directly (str or Path).
    parser2 = PyEvtxParser(small_sample, wevt_cache=cache_path)
    records2 = list(parser2.records())
    assert len(records2) == 7


def test_wevt_cache_resolve_template_guid(tmp_path: Path):
    cache_path = tmp_path / "cache.wevtcache"
    WevtCache().dump(cache_path, overwrite=True)
    cache = WevtCache.load(cache_path)

    with pytest.raises(KeyError):
        cache.resolve_template_guid("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", 1, 2)


def test_wevt_cache_render_template_xml_errors_without_template(tmp_path: Path):
    cache_path = tmp_path / "cache.wevtcache"
    WevtCache().dump(cache_path, overwrite=True)
    cache = WevtCache.load(cache_path)

    with pytest.raises(KeyError):
        cache.render_template_xml("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", [])


def test_wevt_cache_add_dll_is_strict(tmp_path: Path, repo_root: Path):
    pe_fixture = repo_root / "tests" / "fixtures" / "wevt_template_minimal_pe.bin"
    assert pe_fixture.exists()

    cache = WevtCache()
    with pytest.raises(RuntimeError):
        cache.add_dll(pe_fixture)

    # Dumping an (empty) cache should still create a valid .wevtcache file.
    out_file = tmp_path / "cache.wevtcache"
    cache.dump(out_file, overwrite=True)
    assert out_file.exists()

