"""Load a keymap (keymap-drawer YAML, or a ZMK .keymap parsed through keymap-drawer)."""

from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

import yaml


@dataclass(frozen=True)
class Key:
    tap: str = ""
    hold: str = ""
    shifted: str = ""
    kind: str = ""  # "", "trans", "held", "ghost"


@dataclass(frozen=True)
class Layer:
    name: str
    keys: list[Key]


# Whole-label replacements, applied after modifier prefixes are stripped.
_LABELS = {
    "BACKSPACE": "⌫",
    "DELETE": "Del",
    "ENTER": "⏎",
    "RETURN": "⏎",
    "SPACE": "␣",
    "TAB": "⇥",
    "ESC": "Esc",
    "ESCAPE": "Esc",
    "LEFT": "←",
    "RIGHT": "→",
    "UP": "↑",
    "DOWN": "↓",
    "LEFT ARROW": "←",
    "RIGHT ARROW": "→",
    "UP ARROW": "↑",
    "DOWN ARROW": "↓",
    "HOME": "Home",
    "END": "End",
    "PAGE UP": "PgUp",
    "PAGE DOWN": "PgDn",
    "LCTRL": "Ctrl",
    "RCTRL": "Ctrl",
    "LEFT CONTROL": "Ctrl",
    "RIGHT CONTROL": "Ctrl",
    "LSHIFT": "Shift",
    "RSHIFT": "Shift",
    "LEFT SHIFT": "Shift",
    "RIGHT SHIFT": "Shift",
    "LALT": "Alt",
    "RALT": "AltGr",
    "LEFT ALT": "Alt",
    "RIGHT ALT": "AltGr",
    "LGUI": "Super",
    "RGUI": "Super",
    "LEFT GUI": "Super",
    "RIGHT GUI": "Super",
    "CAPS": "Caps",
    "CAPSLOCK": "Caps",
    "MUTE": "🔇",
    "VOLUME UP": "🔊",
    "VOLUME DOWN": "🔉",
    "PLAY PAUSE": "⏯",
    "NEXT": "⏭",
    "PREV": "⏮",
    "BRIGHTNESS INC": "☀+",
    "BRIGHTNESS DEC": "☀−",
    "BT CLR": "BT Clr",
    "PRINTSCREEN": "PrtSc",
}

_MODS = {"Alt": "⎇", "Gui": "◆", "Ctrl": "⌃", "Shift": "⇧", "AltGr": "⎇"}


def prettify(label: str) -> str:
    """Shorten keymap-drawer's raw labels (e.g. 'Alt+LEFT ARROW' -> '⎇←')."""
    label = label.strip()
    if not label:
        return ""
    prefix = ""
    while "+" in label[:-1]:
        mod, rest = label.split("+", 1)
        if mod not in _MODS:
            break
        prefix += _MODS[mod]
        label = rest
    return prefix + _LABELS.get(label, label)


def _key(raw) -> Key:
    if raw is None:
        return Key()
    if isinstance(raw, dict):
        return Key(
            tap=prettify(str(raw.get("t", raw.get("tap", "")) or "")),
            hold=prettify(str(raw.get("h", raw.get("hold", "")) or "")),
            shifted=prettify(str(raw.get("s", raw.get("shifted", "")) or "")),
            kind=str(raw.get("type", "") or ""),
        )
    return Key(tap=prettify(str(raw)))


def _flatten(rows) -> list:
    keys: list = []
    for item in rows:
        if isinstance(item, list):
            keys.extend(_flatten(item))
        else:
            keys.append(item)
    return keys


def parse_yaml(text: str) -> list[Layer]:
    data = yaml.safe_load(text) or {}
    layers = data.get("layers") or {}
    return [Layer(name=str(name), keys=[_key(k) for k in _flatten(keys or [])]) for name, keys in layers.items()]


def _run_keymap_drawer(path: Path) -> str:
    exe = shutil.which("keymap")
    if exe is None:
        raise RuntimeError(
            "'keymap' (keymap-drawer) is not installed; install it with "
            "`uv tool install keymap-drawer` or `pipx install keymap-drawer`, "
            "or pass a pre-generated YAML file instead"
        )
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    result = subprocess.run(
        [exe, "parse", "-z", str(path)], capture_output=True, text=True, encoding="utf-8", env=env
    )
    if result.returncode != 0:
        raise RuntimeError(f"keymap-drawer failed to parse {path}:\n{result.stderr.strip()}")
    return result.stdout


def load(path: str | Path) -> list[Layer]:
    """Load layers from a keymap-drawer YAML file or a ZMK .keymap file."""
    path = Path(path).expanduser()
    if path.suffix == ".keymap":
        text = _run_keymap_drawer(path)
    else:
        text = path.read_text(encoding="utf-8")
    layers = parse_yaml(text)
    if not layers:
        raise RuntimeError(f"no layers found in {path}")
    return layers
