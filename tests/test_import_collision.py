from __future__ import annotations

import importlib.util
import shutil
import sys
from pathlib import Path

import pytest


def _execute_package_init(package_dir: Path, module_name: str) -> None:
    spec = importlib.util.spec_from_file_location(
        module_name,
        package_dir / "__init__.py",
        submodule_search_locations=[str(package_dir)],
    )
    assert spec is not None
    assert spec.loader is not None

    module = importlib.util.module_from_spec(spec)
    sys.modules.pop(module_name, None)
    try:
        spec.loader.exec_module(module)
    finally:
        sys.modules.pop(module_name, None)


def test_windows_import_errors_when_package_directory_case_collides(
    monkeypatch: pytest.MonkeyPatch,
    repo_root: Path,
    tmp_path: Path,
) -> None:
    package_dir = tmp_path / "site-packages" / "Evtx"
    package_dir.mkdir(parents=True)
    shutil.copyfile(
        repo_root / "python" / "evtx" / "__init__.py",
        package_dir / "__init__.py",
    )

    monkeypatch.setattr(sys, "platform", "win32")

    with pytest.raises(ImportError) as exc_info:
        _execute_package_init(package_dir, "evtx")

    message = str(exc_info.value)
    assert "pyevtx-rs is installed from PyPI as 'evtx'" in message
    assert "python-evtx is installed from PyPI as 'python-evtx'" in message
    assert "python-evtx" in message
    assert "Evtx" in message
    assert "uv pip uninstall python-evtx" in message
    assert "uv pip install --reinstall evtx" in message
    assert "python -m pip install --force-reinstall evtx" in message


def test_windows_import_errors_when_package_name_case_collides(
    monkeypatch: pytest.MonkeyPatch,
    repo_root: Path,
) -> None:
    monkeypatch.setattr(sys, "platform", "win32")

    with pytest.raises(ImportError) as exc_info:
        _execute_package_init(repo_root / "python" / "evtx", "Evtx")

    message = str(exc_info.value)
    assert "pyevtx-rs is installed from PyPI as 'evtx'" in message
    assert "python-evtx is installed from PyPI as 'python-evtx'" in message
    assert "python-evtx" in message
    assert "Evtx" in message
    assert "uv pip uninstall evtx" in message
    assert "python -m pip install --force-reinstall python-evtx" in message
    assert "separate virtual environments" in message
