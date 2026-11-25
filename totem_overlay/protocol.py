"""Raw HID layer-report protocol, mirrors firmware/src/layer_report.c."""

from __future__ import annotations

from dataclasses import dataclass

MAGIC = 0x4C
VERSION = 0x01
QUERY = 0x3F
REPORT_LEN = 32
NAME_OFFSET = 8

# Usage page / usage of zzeneg/zmk-raw-hid (same as QMK's raw HID defaults).
USAGE_PAGE = 0xFF60
USAGE = 0x61


@dataclass(frozen=True)
class LayerState:
    index: int  # position of the highest active layer in the keymap
    layer_id: int  # stable id of that layer (differs from index after Studio reorders)
    mask: int  # bitmask of all active layer ids
    name: str  # layer display name, may be empty

    def active_ids(self) -> list[int]:
        return [i for i in range(32) if self.mask >> i & 1]


def parse_report(data: bytes | list[int]) -> LayerState | None:
    """Decode a keyboard->host report, or return None if it is not ours."""
    data = bytes(data)
    if len(data) < NAME_OFFSET or data[0] != MAGIC or data[1] != VERSION:
        return None
    name = data[NAME_OFFSET:REPORT_LEN].split(b"\0", 1)[0].decode("utf-8", "replace")
    return LayerState(
        index=data[2],
        layer_id=data[3],
        mask=int.from_bytes(data[4:8], "little"),
        name=name,
    )


def query_report() -> bytes:
    """Host->keyboard request for the current state, with hidapi's leading report id."""
    return bytes([0x00, MAGIC, QUERY]) + bytes(REPORT_LEN - 2)
