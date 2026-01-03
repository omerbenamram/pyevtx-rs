"""
WEVT_TEMPLATE (Windows Event manifest) introspection.

This module exposes a pyfwevt-like view of CRIM/WEVT provider manifests for offline
template exploration and debugging.
"""

from __future__ import annotations

# These will be provided by the native extension.
from .._native import Event, Manifest, Provider, Template, TemplateItem

__all__ = [
    "Event",
    "Manifest",
    "Provider",
    "Template",
    "TemplateItem",
]

