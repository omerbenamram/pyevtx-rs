"""
Python package wrapper for the `evtx` native extension.

We ship the compiled Rust extension as `evtx._native` and re-export the public API
here to preserve backwards compatibility with older releases that exposed a
top-level `evtx` extension module.
"""

from __future__ import annotations

from ._native import PyEvtxParser, PyRecordsIterator

__all__ = [
    "PyEvtxParser",
    "PyRecordsIterator",
]

try:
    # Optional feature: offline WEVT template cache support.
    from ._native import WevtCache

    __all__.append("WevtCache")
except Exception:
    # Built without wevt_templates support (or extension not yet built).
    pass

