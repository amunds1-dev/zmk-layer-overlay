from pathlib import Path

from totem_overlay import keymap, layout, protocol

HERE = Path(__file__).parent


def report(index, layer_id, mask, name=b""):
    data = bytearray(32)
    data[0:2] = bytes([protocol.MAGIC, protocol.TYPE_LAYER])
    data[2], data[3] = index, layer_id
    data[4:8] = mask.to_bytes(4, "little")
    data[8 : 8 + len(name)] = name
    return bytes(data)


def test_parse_report():
    s = protocol.parse_report(report(3, 3, 0b1111, b"device"))
    assert s == protocol.LayerState(index=3, layer_id=3, mask=0b1111, name="device")
    assert s.active_ids() == [0, 1, 2, 3]


def test_parse_report_ignores_foreign_reports():
    assert protocol.parse_report(bytes(32)) is None
    assert protocol.parse_report(b"\x4c") is None
    assert protocol.parse_report(bytes([protocol.MAGIC, 0x7F]) + bytes(30)) is None


def test_parse_key_report():
    data = bytearray(32)
    data[0:2] = bytes([protocol.MAGIC, protocol.TYPE_KEYS])
    data[2:10] = ((1 << 0) | (1 << 21) | (1 << 37)).to_bytes(8, "little")
    s = protocol.parse_report(bytes(data))
    assert isinstance(s, protocol.KeyState)
    assert s.pressed() == {0, 21, 37}


def test_query_report_has_report_id_and_full_length():
    q = protocol.query_report()
    assert q[:3] == bytes([0, protocol.MAGIC, protocol.QUERY]) and len(q) == 33


def test_vendor_keymap_matches_layout():
    layers = keymap.parse_yaml((HERE / "totem.yaml").read_text(encoding="utf-8"))
    assert [l.name for l in layers] == ["base", "nav_num", "sym_func", "device"]
    assert all(len(l.keys) == len(layout.totem()) == 38 for l in layers)
    nav = layers[1]
    assert nav.keys[0].tap == "⎇⌫"
    assert nav.keys[34].kind == "held"
    assert nav.keys[32].kind == "trans"
    assert layers[3].keys[5] == keymap.Key(tap="BT", hold="0")


def test_prettify():
    assert keymap.prettify("Gui+UP ARROW") == "◆↑"
    assert keymap.prettify("+") == "+"
    assert keymap.prettify("RIGHT SHIFT") == "Shift"


def test_layout_has_no_overlapping_keys():
    keys = layout.totem()
    for i, a in enumerate(keys):
        for b in keys[i + 1 :]:
            assert a.x + a.w <= b.x + 1e-9 or b.x + b.w <= a.x + 1e-9 or a.y + a.h <= b.y + 1e-9 or b.y + b.h <= a.y + 1e-9
