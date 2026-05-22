"""
Python package wrapper for the `evtx` native extension.

We ship the compiled Rust extension as `evtx._native` and re-export the public API
here to preserve backwards compatibility with older releases that exposed a
top-level `evtx` extension module.
"""

from __future__ import annotations

import sys
from pathlib import Path


def _filesystem_package_dir_name(package_file: str) -> str:
    package_dir = Path(package_file).resolve().parent

    try:
        for child in package_dir.parent.iterdir():
            if child.name.lower() == "evtx" and child.samefile(package_dir):
                return child.name
    except OSError:
        pass

    return package_dir.name


def _raise_on_windows_case_collision(module_name: str, package_file: str) -> None:
    if sys.platform != "win32":
        return

    imported_as = module_name.partition(".")[0]
    package_dir_name = _filesystem_package_dir_name(package_file)
    if imported_as == "evtx" and package_dir_name == "evtx":
        return

    raise ImportError(
        "Cannot import pyevtx-rs cleanly on Windows because two EVTX packages "
        "appear to share the same site-packages directory.\n\n"
        "Package names and import names:\n"
        "  - pyevtx-rs is installed from PyPI as 'evtx' and imported as 'evtx'.\n"
        "  - python-evtx is installed from PyPI as 'python-evtx' and imported "
        "as 'Evtx'.\n\n"
        "Windows treats 'evtx' and 'Evtx' as the same directory name, so these "
        "packages cannot be installed into the same Python environment.\n\n"
        f"This import found directory {package_dir_name!r} while importing "
        f"{imported_as!r}.\n\n"
        "To use pyevtx-rs ('evtx'), remove python-evtx and reinstall evtx:\n"
        "  uv pip uninstall python-evtx\n"
        "  uv pip install --reinstall evtx\n"
        "  # pip equivalent:\n"
        "  python -m pip uninstall python-evtx\n"
        "  python -m pip install --force-reinstall evtx\n\n"
        "To use python-evtx ('Evtx'), remove pyevtx-rs/evtx instead:\n"
        "  uv pip uninstall evtx\n"
        "  uv pip install --reinstall python-evtx\n"
        "  # pip equivalent:\n"
        "  python -m pip uninstall evtx\n"
        "  python -m pip install --force-reinstall python-evtx\n\n"
        "If you need both libraries, install them in separate virtual "
        "environments."
    )


_raise_on_windows_case_collision(__name__, __file__)

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
