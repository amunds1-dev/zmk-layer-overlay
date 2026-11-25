"""Physical key positions of the TOTEM, in key units (1u = one key pitch).

Keys are listed in keymap order: 3 rows of 10 alpha keys (the bottom row also
has one extra outer pinky key per side, 12 in total), then 6 thumb keys.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class KeyPos:
    x: float
    y: float
    w: float = 1.0
    h: float = 1.0


# Column stagger for the left half, pinky -> inner index column.
_STAGGER = [0.6, 0.2, 0.0, 0.2, 0.35]
_HALF_WIDTH = 6.0  # outer pinky key + 5 columns
_GAP = 1.0  # space between the halves


def _mirror(p: KeyPos) -> KeyPos:
    return KeyPos(2 * _HALF_WIDTH + _GAP - p.x - p.w, p.y, p.w, p.h)


def _left(col: int, row: int) -> KeyPos:
    return KeyPos(1.0 + col, row + _STAGGER[col])


def totem() -> list[KeyPos]:
    keys: list[KeyPos] = []
    for row in range(3):
        if row == 2:
            keys.append(KeyPos(0.0, 2.85))  # left outer pinky key
        keys.extend(_left(col, row) for col in range(5))
        keys.extend(_mirror(_left(col, row)) for col in reversed(range(5)))
        if row == 2:
            keys.append(_mirror(KeyPos(0.0, 2.85)))  # right outer pinky key
    left_thumbs = [KeyPos(3.0, 3.35), KeyPos(4.0, 3.45), KeyPos(5.0, 3.6)]
    keys.extend(left_thumbs)
    keys.extend(_mirror(p) for p in reversed(left_thumbs))
    return keys


def bounds(keys: list[KeyPos]) -> tuple[float, float]:
    """Width and height of the layout in key units."""
    return max(k.x + k.w for k in keys), max(k.y + k.h for k in keys)
