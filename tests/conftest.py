from __future__ import annotations

from pathlib import Path

import pytest


SAMPLES = Path(__file__).parent.parent / "samples"


@pytest.fixture
def repo_root() -> Path:
    # .../external/pyevtx-rs/tests -> repo root
    return Path(__file__).resolve().parents[3]


@pytest.fixture
def small_sample() -> str:
    return str(SAMPLES / "Security_short_selected.evtx")

