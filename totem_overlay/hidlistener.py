"""Background thread that reads layer and key reports from the keyboard's Raw HID interface."""

from __future__ import annotations

import logging
import threading
from typing import Callable

from . import protocol

log = logging.getLogger(__name__)

try:
    import hid  # cython-hidapi: `pacman -S python-hidapi` or `pip install hidapi`
except ImportError:  # pragma: no cover - depends on the system
    hid = None


def _require_hidapi() -> None:
    if hid is None or not hasattr(hid, "device"):
        raise RuntimeError(
            "hidapi Python bindings not found. Install `python-hidapi` (pacman) or "
            "`pip install hidapi`. Note: the unrelated `hid` package from PyPI does not work."
        )


def find_devices(product: str | None = None) -> list[dict]:
    """Raw HID interfaces matching the layer-report usage page, optionally filtered by product."""
    _require_hidapi()
    found = []
    for info in hid.enumerate():
        if info.get("usage_page") != protocol.USAGE_PAGE or info.get("usage") != protocol.USAGE:
            continue
        name = info.get("product_string") or ""
        if product and product.lower() not in name.lower():
            continue
        found.append(info)
    # Prefer USB over Bluetooth when both are present (USB paths don't mention uhid).
    found.sort(key=lambda i: b"uhid" in (i.get("path") or b""))
    return found


class HidListener(threading.Thread):
    """Calls on_report(LayerState | KeyState) for every report and on_connection(bool, str)
    on (dis)connect.

    Callbacks run on this thread; GUI code must marshal them (e.g. GLib.idle_add).
    """

    def __init__(
        self,
        on_report: Callable[[protocol.LayerState | protocol.KeyState], None],
        on_connection: Callable[[bool, str], None],
        product: str | None = "TOTEM",
        retry_seconds: float = 2.0,
    ) -> None:
        super().__init__(daemon=True, name="hid-listener")
        _require_hidapi()
        self._on_report = on_report
        self._on_connection = on_connection
        self._product = product
        self._retry = retry_seconds
        self._stop = threading.Event()

    def stop(self) -> None:
        self._stop.set()

    def run(self) -> None:
        while not self._stop.is_set():
            devices = find_devices(self._product)
            if not devices:
                self._stop.wait(self._retry)
                continue
            info = devices[0]
            try:
                self._listen(info)
            except (OSError, ValueError) as exc:
                log.info("lost keyboard: %s", exc)
            self._on_connection(False, "")
            self._stop.wait(self._retry)

    def _listen(self, info: dict) -> None:
        dev = hid.device()
        try:
            dev.open_path(info["path"])
        except OSError as exc:
            log.warning(
                "cannot open %s (%s). Is the udev rule installed? See README.",
                info["path"].decode(errors="replace"),
                exc,
            )
            raise
        try:
            label = info.get("product_string") or "keyboard"
            self._on_connection(True, label)
            dev.write(protocol.query_report())
            while not self._stop.is_set():
                data = dev.read(64, 500)
                if not data:
                    continue
                report = protocol.parse_report(data)
                if report is not None:
                    self._on_report(report)
        finally:
            dev.close()
