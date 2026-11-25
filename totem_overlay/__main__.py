"""Command line entry point: `totem-overlay` or `python -m totem_overlay`."""

from __future__ import annotations

import argparse
import ctypes.util
import logging
import os
import sys
from pathlib import Path

DEFAULT_KEYMAP = "~/zmk-config-totem/config/totem.keymap"


def _ensure_layer_shell_preload() -> None:
    """gtk4-layer-shell must be loaded before libwayland-client, which from Python
    means LD_PRELOAD. Re-exec ourselves once with it set."""
    if not os.environ.get("WAYLAND_DISPLAY") or os.environ.get("TOTEM_OVERLAY_REEXEC"):
        return
    if "gtk4-layer-shell" in os.environ.get("LD_PRELOAD", ""):
        return
    lib = ctypes.util.find_library("gtk4-layer-shell")
    if lib is None:
        return  # not installed; overlay.py falls back to a normal window
    env = dict(os.environ, TOTEM_OVERLAY_REEXEC="1")
    env["LD_PRELOAD"] = ":".join(filter(None, [lib, env.get("LD_PRELOAD")]))
    os.execve(sys.executable, [sys.executable, *sys.orig_argv[1:]], env)


def _monitor(product: str | None) -> int:
    """Print layer reports to the terminal; useful to test the firmware."""
    import threading

    from .hidlistener import HidListener, find_devices
    from .protocol import LayerState

    devices = find_devices(product)
    if not devices:
        print("No Raw HID layer-report device found (is the new firmware flashed, udev rule installed?)")
    for d in devices:
        print(f"found: {d.get('product_string')!r} at {d['path'].decode(errors='replace')}")

    def on_report(r):
        if isinstance(r, LayerState):
            print(f"layer index={r.index} id={r.layer_id} name={r.name!r} active_ids={r.active_ids()}", flush=True)
        else:
            print(f"keys pressed={sorted(r.pressed())}", flush=True)

    def on_conn(c, label):
        print(("connected to " + label) if c else "disconnected", flush=True)

    listener = HidListener(on_report, on_conn, product=product)
    listener.start()
    try:
        threading.Event().wait()
    except KeyboardInterrupt:
        pass
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(prog="totem-overlay", description=__doc__)
    p.add_argument(
        "-k", "--keymap", default=os.environ.get("TOTEM_OVERLAY_KEYMAP", DEFAULT_KEYMAP),
        help="ZMK .keymap file (parsed with keymap-drawer) or keymap-drawer YAML (default: %(default)s)",
    )
    p.add_argument("--show", choices=["layers", "always"], default="layers",
                   help="'layers': only while a non-base layer is active (default); 'always': stay on screen")
    p.add_argument("--position", default="bottom",
                   choices=["bottom", "top", "bottom-left", "bottom-right", "top-left", "top-right", "center"])
    p.add_argument("--margin", type=int, default=40, help="distance from the screen edge in px")
    p.add_argument("--scale", type=float, default=1.0)
    p.add_argument("--opacity", type=float, default=0.88)
    p.add_argument("--product", default="TOTEM", help="HID product name to match ('' for any raw HID device)")
    p.add_argument("--demo", action="store_true", help="cycle through layers without a keyboard")
    p.add_argument("--monitor", action="store_true", help="print layer reports to the terminal, no GUI")
    p.add_argument("-v", "--verbose", action="store_true")
    args = p.parse_args(argv)

    logging.basicConfig(level=logging.DEBUG if args.verbose else logging.INFO, format="%(levelname)s %(message)s")
    product = args.product or None

    if args.monitor:
        return _monitor(product)

    _ensure_layer_shell_preload()
    from .overlay import Options, OverlayApp

    opts = Options(
        keymap_path=Path(args.keymap).expanduser(),
        show=args.show,
        position=args.position,
        margin=args.margin,
        scale=args.scale,
        opacity=args.opacity,
        product=product,
        demo=args.demo,
    )
    return OverlayApp(opts).run([sys.argv[0]])


if __name__ == "__main__":
    sys.exit(main())
