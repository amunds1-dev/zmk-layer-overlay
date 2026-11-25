"""GTK4 overlay window, placed with wlr-layer-shell when available (wlroots, KDE, ...)."""

from __future__ import annotations

import logging
import math
from dataclasses import dataclass
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("PangoCairo", "1.0")
gi.require_foreign("cairo")
from gi.repository import Gio, GLib, Gtk, Pango, PangoCairo  # noqa: E402

import cairo  # noqa: E402

try:
    gi.require_version("Gtk4LayerShell", "1.0")
    from gi.repository import Gtk4LayerShell as LayerShell  # noqa: E402
except (ValueError, ImportError):
    LayerShell = None

from . import keymap as km  # noqa: E402
from . import layout  # noqa: E402
from .protocol import LayerState  # noqa: E402

log = logging.getLogger(__name__)

POSITIONS = ("bottom", "top", "bottom-left", "bottom-right", "top-left", "top-right", "center")


@dataclass
class Options:
    keymap_path: Path
    show: str = "layers"  # "layers": only while a non-base layer is active; "always"
    position: str = "bottom"
    margin: int = 40
    scale: float = 1.0
    opacity: float = 0.88
    product: str | None = "TOTEM"
    demo: bool = False


# Colours (r, g, b) in 0..1
BG = (0.09, 0.09, 0.11)
KEY = (0.20, 0.20, 0.24)
KEY_EMPTY = (0.14, 0.14, 0.17)
KEY_HELD = (0.36, 0.55, 0.95)
TEXT = (0.95, 0.95, 0.97)
TEXT_DIM = (0.55, 0.55, 0.62)
ACCENT = (0.45, 0.65, 1.0)


class OverlayApp(Gtk.Application):
    def __init__(self, opts: Options) -> None:
        super().__init__(application_id="dev.totem.LayerOverlay", flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.opts = opts
        self.positions = layout.totem()
        self.layers: list[km.Layer] = []
        self.state: LayerState | None = None
        self.connected = False
        self.device_label = ""
        self.error = ""
        self.alpha = 0.0
        self._fade_id = 0
        self._reload_id = 0
        self._listener = None
        self.connect("activate", self._on_activate)
        self.connect("shutdown", self._on_shutdown)

    # ---- setup -------------------------------------------------------------

    def _on_activate(self, _app) -> None:
        self._load_keymap()
        self._watch_keymap()

        unit = 48 * self.opts.scale
        self.unit = unit
        self.pad = round(14 * self.opts.scale)
        self.header = round(26 * self.opts.scale)
        w, h = layout.bounds(self.positions)
        width = round(w * unit + 2 * self.pad)
        height = round(h * unit + 2 * self.pad + self.header)

        win = Gtk.ApplicationWindow(application=self, title="TOTEM layers")
        win.set_decorated(False)
        win.set_resizable(False)
        win.set_default_size(width, height)
        css = Gtk.CssProvider()
        rule = "window { background: transparent; }"
        if hasattr(css, "load_from_string"):  # GTK >= 4.12
            css.load_from_string(rule)
        else:
            css.load_from_data(rule, -1)
        Gtk.StyleContext.add_provider_for_display(
            win.get_display(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        area = Gtk.DrawingArea()
        area.set_content_width(width)
        area.set_content_height(height)
        area.set_draw_func(self._draw)
        win.set_child(area)
        self.area = area
        self.win = win

        self._setup_layer_shell(win)
        win.connect("map", self._make_click_through)
        win.present()

        if self.opts.demo:
            self._start_demo()
        else:
            self._start_listener()
        self._update_visibility()

    def _setup_layer_shell(self, win: Gtk.Window) -> None:
        if LayerShell is None or not LayerShell.is_supported():
            log.warning(
                "wlr-layer-shell not available; using a normal window "
                "(install gtk4-layer-shell, and use a compositor that supports it)"
            )
            return
        LayerShell.init_for_window(win)
        LayerShell.set_namespace(win, "totem-overlay")
        LayerShell.set_layer(win, LayerShell.Layer.OVERLAY)
        LayerShell.set_keyboard_mode(win, LayerShell.KeyboardMode.NONE)
        edges = {
            "top": LayerShell.Edge.TOP,
            "bottom": LayerShell.Edge.BOTTOM,
            "left": LayerShell.Edge.LEFT,
            "right": LayerShell.Edge.RIGHT,
        }
        pos = self.opts.position
        for word, edge in edges.items():
            if word in pos.split("-"):
                LayerShell.set_anchor(win, edge, True)
                LayerShell.set_margin(win, edge, self.opts.margin)

    def _make_click_through(self, win: Gtk.Window) -> None:
        surface = win.get_surface()
        if surface is not None:
            surface.set_input_region(cairo.Region())

    def _on_shutdown(self, _app) -> None:
        if self._listener is not None:
            self._listener.stop()

    # ---- keymap ------------------------------------------------------------

    def _load_keymap(self) -> None:
        try:
            self.layers = km.load(self.opts.keymap_path)
            self.error = ""
            n = len(self.layers[0].keys)
            if n != len(self.positions):
                log.warning("keymap has %d keys per layer, TOTEM layout has %d", n, len(self.positions))
            log.info("loaded layers: %s", ", ".join(layer.name for layer in self.layers))
        except Exception as exc:  # show the problem in the overlay instead of crashing
            log.error("%s", exc)
            self.error = str(exc).splitlines()[0]

    def _watch_keymap(self) -> None:
        gfile = Gio.File.new_for_path(str(self.opts.keymap_path))
        self._monitor = gfile.monitor_file(Gio.FileMonitorFlags.NONE, None)
        self._monitor.connect("changed", self._on_keymap_changed)

    def _on_keymap_changed(self, *_args) -> None:
        if self._reload_id:
            GLib.source_remove(self._reload_id)
        self._reload_id = GLib.timeout_add(300, self._reload_keymap)

    def _reload_keymap(self) -> bool:
        self._reload_id = 0
        self._load_keymap()
        self.area.queue_draw()
        return False

    # ---- input sources -----------------------------------------------------

    def _start_listener(self) -> None:
        from .hidlistener import HidListener

        try:
            self._listener = HidListener(
                on_state=lambda s: GLib.idle_add(self._set_state, s),
                on_connection=lambda c, label: GLib.idle_add(self._set_connected, c, label),
                product=self.opts.product,
            )
        except RuntimeError as exc:
            self.error = str(exc).splitlines()[0]
            log.error("%s", exc)
            return
        self._listener.start()

    def _start_demo(self) -> None:
        self._set_connected(True, "demo")
        self._demo_step = 0

        def tick() -> bool:
            n = max(len(self.layers), 1)
            idx = self._demo_step % (n + 1)  # include a pause on the base layer
            idx = 0 if idx == n else idx
            self._demo_step += 1
            name = self.layers[idx].name if idx < len(self.layers) else ""
            self._set_state(LayerState(index=idx, layer_id=idx, mask=1 | 1 << idx, name=name))
            return True

        tick()
        GLib.timeout_add(1500, tick)

    def _set_state(self, state: LayerState) -> bool:
        self.state = state
        self._update_visibility()
        self.area.queue_draw()
        return False

    def _set_connected(self, connected: bool, label: str) -> bool:
        self.connected = connected
        self.device_label = label
        if not connected:
            self.state = None
        log.info("keyboard %s %s", "connected" if connected else "disconnected", label)
        self._update_visibility()
        self.area.queue_draw()
        return False

    # ---- visibility --------------------------------------------------------

    def current_layer(self) -> tuple[int, km.Layer | None]:
        """Map the reported state to a layer from the keymap file."""
        if not self.layers:
            return 0, None
        if self.state is None:
            return 0, self.layers[0]
        if self.state.name:
            for i, layer in enumerate(self.layers):
                if layer.name == self.state.name:
                    return i, layer
        if self.state.index < len(self.layers):
            return self.state.index, self.layers[self.state.index]
        return self.state.index, None

    def _wanted_visible(self) -> bool:
        if self.opts.show == "always" or self.error:
            return True
        return self.connected and self.state is not None and self.state.index != 0

    def _update_visibility(self) -> None:
        target = 1.0 if self._wanted_visible() else 0.0
        if self._fade_id:
            GLib.source_remove(self._fade_id)
            self._fade_id = 0
        # Fade in fast, fade out a little slower so quick layer taps don't flicker.
        step = 0.25 if target > self.alpha else 0.12

        def fade() -> bool:
            self.alpha += math.copysign(min(step, abs(target - self.alpha)), target - self.alpha)
            self.area.queue_draw()
            if abs(self.alpha - target) < 1e-3:
                self.alpha = target
                self._fade_id = 0
                return False
            return True

        self._fade_id = GLib.timeout_add(16, fade)

    # ---- drawing -----------------------------------------------------------

    def _draw(self, _area, cr: cairo.Context, width: int, height: int) -> None:
        cr.set_operator(cairo.OPERATOR_CLEAR)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)
        if self.alpha <= 0:
            return
        a = self.alpha * self.opts.opacity

        _rounded(cr, 0, 0, width, height, 14 * self.opts.scale)
        cr.set_source_rgba(*BG, a)
        cr.fill()

        index, layer = self.current_layer()
        if self.error:
            title, sub = "error", self.error
        elif layer is None:
            title, sub = f"layer {index}", "not in keymap file"
        else:
            title = layer.name
            sub = "" if self.connected else "keyboard not connected"
        title_w = self._text(cr, title, self.pad, self.pad * 0.6, 13, ACCENT, a, bold=True)
        if sub:
            self._text(cr, sub, self.pad + title_w + 12 * self.opts.scale, self.pad * 0.6 + 2 * self.opts.scale, 10, TEXT_DIM, a)

        keys = layer.keys if layer else []
        u, gap = self.unit, 4 * self.opts.scale
        for i, pos in enumerate(self.positions):
            key = keys[i] if i < len(keys) else km.Key()
            x = self.pad + pos.x * u + gap / 2
            y = self.pad + self.header + pos.y * u + gap / 2
            w, h = pos.w * u - gap, pos.h * u - gap
            self._draw_key(cr, key, x, y, w, h, a)

    def _draw_key(self, cr, key: km.Key, x, y, w, h, a) -> None:
        empty = not (key.tap or key.hold or key.shifted)
        if key.kind == "held":
            color, fg = KEY_HELD, TEXT
        elif key.kind in ("trans", "ghost") or empty:
            color, fg = KEY_EMPTY, TEXT_DIM
        else:
            color, fg = KEY, TEXT
        _rounded(cr, x, y, w, h, 6 * self.opts.scale)
        cr.set_source_rgba(*color, a)
        cr.fill()

        if key.kind == "trans":
            return  # transparent keys fall through; keep them visually quiet
        if key.tap:
            self._centered(cr, key.tap, x, y + (-h * 0.08 if key.hold else 0), w, h, 13, fg, a)
        if key.hold:
            self._centered(cr, key.hold, x, y + h * 0.30, w, h * 0.4, 8.5, TEXT_DIM, a)
        if key.shifted:
            self._centered(cr, key.shifted, x, y - h * 0.30, w, h * 0.4, 8.5, TEXT_DIM, a)

    def _layout(self, cr, text: str, size: float, bold=False) -> Pango.Layout:
        lay = PangoCairo.create_layout(cr)
        font = Pango.FontDescription.from_string(f"Sans {'Bold ' if bold else ''}{size * self.opts.scale:.1f}")
        lay.set_font_description(font)
        lay.set_text(text, -1)
        return lay

    def _text(self, cr, text, x, y, size, rgb, a, bold=False) -> float:
        lay = self._layout(cr, text, size, bold)
        cr.move_to(x, y)
        cr.set_source_rgba(*rgb, a)
        PangoCairo.show_layout(cr, lay)
        return lay.get_size()[0] / Pango.SCALE

    def _centered(self, cr, text, x, y, w, h, size, rgb, a) -> None:
        # Shrink long labels until they fit inside the key.
        lay = self._layout(cr, text, size)
        tw, th = (v / Pango.SCALE for v in lay.get_size())
        while tw > w - 6 and size > 6:
            size -= 0.5
            lay = self._layout(cr, text, size)
            tw, th = (v / Pango.SCALE for v in lay.get_size())
        cr.move_to(x + (w - tw) / 2, y + (h - th) / 2)
        cr.set_source_rgba(*rgb, a)
        PangoCairo.show_layout(cr, lay)


def _rounded(cr, x, y, w, h, r) -> None:
    cr.new_sub_path()
    cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
    cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
    cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
    cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
    cr.close_path()
