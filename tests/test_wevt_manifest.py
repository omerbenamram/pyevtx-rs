from __future__ import annotations

from pathlib import Path

import pytest

from evtx.wevt import Manifest


FIXTURES = Path(__file__).parent / "fixtures"


@pytest.fixture
def services_crim_blob() -> bytes:
    """CRIM blob extracted from Windows services.exe WEVT_TEMPLATE resource."""
    return (FIXTURES / "services_wevt_template.bin").read_bytes()


def test_wevt_manifest_parse_real_crim(services_crim_blob: bytes):
    """Parse a real CRIM blob from services.exe and verify structure."""
    manifest = Manifest.parse(services_crim_blob)
    providers = manifest.providers

    # services.exe has multiple providers
    assert len(providers) >= 1

    # Find the Service Control Manager provider
    scm_provider = None
    for p in providers:
        if "0063715b" in p.identifier.lower():  # SCM provider GUID prefix
            scm_provider = p
            break

    assert scm_provider is not None, f"SCM provider not found in {[p.identifier for p in providers]}"

    # SCM provider should have events and templates
    assert len(scm_provider.events) > 0
    assert len(scm_provider.templates) > 0

    # Check first event has expected fields
    event = scm_provider.events[0]
    assert event.identifier >= 0
    assert event.version >= 0

    # Check templates can be retrieved by offset
    for evt in scm_provider.events:
        if evt.template_offset is not None:
            tpl = scm_provider.get_template_by_offset(evt.template_offset)
            assert tpl is not None, f"Template not found for offset {evt.template_offset}"
            break


def test_wevt_manifest_template_items(services_crim_blob: bytes):
    """Verify template items have expected properties."""
    manifest = Manifest.parse(services_crim_blob)

    # Find a template with items
    for provider in manifest.providers:
        for template in provider.templates:
            if len(template.items) > 0:
                item = template.items[0]
                # Basic sanity checks on item properties
                assert item.input_data_type >= 0
                assert item.output_data_type >= 0
                assert item.number_of_values >= 0
                assert item.value_data_size >= 0
                # name can be None or a string
                assert item.name is None or isinstance(item.name, str)
                return

    pytest.skip("No templates with items found")


def test_wevt_manifest_template_to_xml(services_crim_blob: bytes):
    """Verify Template.to_xml() produces valid XML output."""
    manifest = Manifest.parse(services_crim_blob)

    for provider in manifest.providers:
        for template in provider.templates:
            xml = template.to_xml()
            assert isinstance(xml, str)
            assert len(xml) > 0
            # Should contain XML-like content
            assert "<" in xml and ">" in xml
            return

    pytest.skip("No templates found")


def test_wevt_manifest_provider_guid_format(services_crim_blob: bytes):
    """Verify provider GUIDs are properly formatted."""
    manifest = Manifest.parse(services_crim_blob)

    for provider in manifest.providers:
        guid = provider.identifier
        # GUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        assert len(guid) == 36
        parts = guid.split("-")
        assert len(parts) == 5
        assert [len(p) for p in parts] == [8, 4, 4, 4, 12]
