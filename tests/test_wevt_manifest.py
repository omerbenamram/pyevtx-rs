from __future__ import annotations

import struct

from evtx.wevt import Manifest


def _name_hash_utf16_u16(name: str) -> int:
    h = 0
    b = name.encode("utf-16le")
    for i in range(0, len(b), 2):
        (cu,) = struct.unpack_from("<H", b, i)
        h = (h * 65599 + cu) & 0xFFFFFFFF
    return h & 0xFFFF


def _push_inline_name(buf: bytearray, name: str) -> None:
    h = _name_hash_utf16_u16(name)
    u16_len = len(name.encode("utf-16le")) // 2
    buf += struct.pack("<HH", h, u16_len)
    buf += name.encode("utf-16le")
    buf += b"\x00\x00"  # NUL terminator (u16)


def _build_minimal_crim_blob_with_named_item() -> tuple[bytes, int]:
    # Ported from `tests/test_wevt_templates.rs` (Rust) "minimal CRIM with TTBL/TEMP + BinXML".
    provider_data_off = 16 + 20
    wevt_size = 28  # WEVT header (20) + 1 descriptor (8)
    ttbl_off = provider_data_off + wevt_size
    temp_off = ttbl_off + 12

    # BinXML fragment: <EventData><Data>{sub:0}</Data></EventData>
    binxml = bytearray()
    binxml += bytes([0x0F, 0x01, 0x01, 0x00])  # StartOfStream + fragment header

    # <EventData>
    binxml += bytes([0x01])  # OpenStartElement
    binxml += struct.pack("<H", 0xFFFF)  # dependency id
    binxml += struct.pack("<I", 0)  # data size (not enforced)
    _push_inline_name(binxml, "EventData")
    binxml += bytes([0x02])  # CloseStartElement

    # <Data>
    binxml += bytes([0x01])  # OpenStartElement
    binxml += struct.pack("<H", 0xFFFF)  # dependency id
    binxml += struct.pack("<I", 0)  # data size
    _push_inline_name(binxml, "Data")
    binxml += bytes([0x02])  # CloseStartElement

    # {sub:0} substitution, type=StringType (0x01)
    binxml += bytes([0x0D])
    binxml += struct.pack("<H", 0)
    binxml += bytes([0x01])

    # </Data></EventData> + EndOfStream
    binxml += bytes([0x04, 0x04, 0x00])

    item_name = "Foo"
    item_name_u16_count = len(item_name.encode("utf-16le")) // 2
    item_name_struct_size = 4 + item_name_u16_count * 2 + 2  # size + utf16 + NUL

    descriptor_count = 1
    name_count = 1
    template_items_offset = temp_off + 40 + len(binxml)
    name_offset = template_items_offset + 20  # right after 1 descriptor

    temp_size = 40 + len(binxml) + 20 * descriptor_count + item_name_struct_size
    ttbl_size = 12 + temp_size

    ttbl = bytearray()
    ttbl += b"TTBL"
    ttbl += struct.pack("<I", ttbl_size)
    ttbl += struct.pack("<I", 1)  # template count

    # TEMP header
    ttbl += b"TEMP"
    ttbl += struct.pack("<I", temp_size)
    ttbl += struct.pack("<I", descriptor_count)
    ttbl += struct.pack("<I", name_count)
    ttbl += struct.pack("<I", template_items_offset)
    ttbl += struct.pack("<I", 1)  # event_type
    ttbl += b"\x11" * 16  # template GUID

    # BinXML fragment
    ttbl += binxml

    # Template item descriptor (20 bytes)
    ttbl += struct.pack("<I", 0)  # unknown1
    ttbl += bytes([0x01])  # inType (UnicodeString)
    ttbl += bytes([0x01])  # outType (xs:string)
    ttbl += struct.pack("<H", 0)  # unknown3
    ttbl += struct.pack("<I", 0)  # unknown4
    ttbl += struct.pack("<H", 1)  # count
    ttbl += struct.pack("<H", 0)  # length
    ttbl += struct.pack("<I", name_offset)

    # Template item name (size-prefixed utf16 + NUL)
    ttbl += struct.pack("<I", item_name_struct_size)
    ttbl += item_name.encode("utf-16le")
    ttbl += b"\x00\x00"

    assert len(ttbl) == ttbl_size

    total_size = ttbl_off + len(ttbl)
    blob = bytearray()

    # CRIM header
    blob += b"CRIM"
    blob += struct.pack("<I", total_size)
    blob += struct.pack("<H", 3)  # major
    blob += struct.pack("<H", 1)  # minor
    blob += struct.pack("<I", 1)  # provider_count

    # provider descriptor
    blob += b"\x22" * 16  # provider GUID
    blob += struct.pack("<I", provider_data_off)

    # WEVT header + 1 descriptor (TTBL)
    blob += b"WEVT"
    blob += struct.pack("<I", wevt_size)
    blob += struct.pack("<I", 0xFFFFFFFF)  # message_identifier
    blob += struct.pack("<I", 1)  # descriptor count
    blob += struct.pack("<I", 0)  # unknown2 count
    blob += struct.pack("<I", ttbl_off)  # TTBL offset
    blob += struct.pack("<I", 0)  # unknown

    # TTBL
    blob += ttbl

    assert len(blob) == total_size
    return bytes(blob), temp_off


def test_wevt_manifest_introspection_smoke():
    blob, temp_off = _build_minimal_crim_blob_with_named_item()

    manifest = Manifest.parse(blob)
    providers = manifest.providers
    assert len(providers) == 1

    provider = providers[0]
    assert provider.identifier == "22222222-2222-2222-2222-222222222222"
    assert provider.events == []

    templates = provider.templates
    assert len(templates) == 1

    tpl = templates[0]
    assert tpl.identifier == "11111111-1111-1111-1111-111111111111"

    by_off = provider.get_template_by_offset(temp_off)
    assert by_off is not None
    assert by_off.identifier == tpl.identifier

    items = tpl.items
    assert len(items) == 1
    item = items[0]
    assert item.name == "Foo"
    assert item.input_data_type == 1
    assert item.output_data_type == 1
    assert item.number_of_values == 1
    assert item.value_data_size == 0

    xml = tpl.to_xml()
    assert "{sub:0}" in xml
    assert "EventData" in xml
    assert "Data" in xml

