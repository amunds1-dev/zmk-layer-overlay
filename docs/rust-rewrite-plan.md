# Rust rewrite plan

Handoff from the first session (built on Windows, firmware + Python prototype). Status at handoff:
firmware flashed on the left half and verified working (layer + key highlighting work in the Python
overlay over USB). Development now continues on the user's CachyOS machine, where the app can be
built, run and screenshotted directly.

## Decisions (agreed with the user)

1. **Rust + gtk4-rs**, plus the `gtk4-layer-shell` crate. One binary, a `.desktop` launcher entry,
   and a `PKGBUILD` so it installs with `makepkg -si` and appears in the app launcher.
2. **Two display modes**, switchable in settings and with a CLI flag:
   - **window** (default): normal xdg toplevel, app id `totem-overlay`. The user floats/moves/
     maximizes it with MangoWM. The drawing scales to the window size, keeping aspect ratio.
   - **overlay**: wlr-layer-shell, OVERLAY layer, keyboard interactivity none, empty input region
     (click-through), anchored by a position setting. For use over games while learning the layout.
3. **Separate opacity** for the window background and for the keys (fill + labels), adjustable in a
   small settings UI (right-click menu or gear icon -> popover with sliders) and persisted to
   `~/.config/totem-overlay/config.toml`. Other settings to persist: mode, overlay position/margin,
   show policy (always / only on non-base layers), scale for overlay mode.
4. **Keymap source: ZMK Studio RPC over USB** (the firmware has Studio enabled with
   `CONFIG_ZMK_STUDIO_LOCKING=n`, transport `studio-rpc-usb-uart` = CDC ACM serial, `/dev/ttyACM*`).
   Read layers, bindings, layer names and the physical layout. Cache to
   `~/.cache/totem-overlay/keymap.json` so Bluetooth sessions work after one USB connection.
   Refresh when the keyboard is (re)connected over USB.
   **Fallback:** parse a ZMK `.keymap` file (path in config) for the common behaviors
   (`&kp`, `&mo`, `&lt`, `&mt`, `&to`, `&tog`, `&sl`, `&bt`, `&trans`, `&none`, modifier functions like
   `LA()`/`LG()`); anything else is shown by its behavior name.
5. **Physical layout from the keyboard** (Studio's physical layouts, includes rotation). Built-in
   fallback: the TOTEM data below. The user will review how closely it matches the real board.
6. Layer/key state stays on the existing Raw HID protocol (see `CLAUDE.md`), read with the `hidapi`
   crate on a background thread.

## Studio RPC notes (verify against upstream before relying on them)

- Protobuf definitions: https://github.com/zmkfirmware/zmk-studio-messages (use `prost`).
- Reference clients: https://github.com/zmkfirmware/zmk-studio-ts-client and the ZMK Studio app
  (https://github.com/zmkfirmware/zmk-studio). Check their framing (start/escape/end bytes) and the
  request flow: core get device info / lock state -> keymap get keymap -> get physical layouts ->
  behaviors list + details (to turn behavior ids into names/param types).
- `&kp` params are HID usages (page << 16 | id) with implicit modifier bits in the top byte. A
  HID-usage -> label table is needed (port `totem_overlay/keymap.py`'s pretty labels: arrows, ⌫, ⏎, …).
- Serial access on Arch needs the user in the `uucp` group, or a udev rule for the ZMK VID:PID
  `1d50:615e` tty. Document it, and add it to the shipped udev rule.

## TOTEM physical layout (from zmk-config-totem `totem-layouts.dtsi`)

Units: x, y, w, h in centi-key-units (100 = 1u); rot in centi-degrees, around (rx, ry).
Order = keymap order (0..37).

```
 w   h    x    y    rot    rx    ry
100 100   73  152 -1000   123   202
100 100  186   60  -400   236   110
100 100  300    0     0     0     0
100 100  400   50     0     0     0
100 100  500   66     0     0     0
100 100  985   66     0     0     0
100 100 1085   50     0     0     0
100 100 1185    0     0     0     0
100 100 1300   60   400  1350   110
100 100 1412  152  1000  1462   202
100 100   90  250 -1000   140   300
100 100  192  160  -400   242   210
100 100  300  100     0     0     0
100 100  400  150     0     0     0
100 100  500  166     0     0     0
100 100  985  166     0     0     0
100 100 1085  150     0     0     0
100 100 1185  100     0     0     0
100 100 1293  160   400  1343   210
100 100 1395  250  1000  1445   300
100 100    0  323 -1000    50   373
100 100  106  349 -1000   156   399
100 100  199  260  -400   249   310
100 100  300  200     0     0     0
100 100  400  250     0     0     0
100 100  500  266     0     0     0
100 100  985  266     0     0     0
100 100 1085  250     0     0     0
100 100 1185  200     0     0     0
100 100 1287  260   400  1337   310
100 100 1379  349  1000  1429   399
100 100 1485  323  1000  1535   373
100 100  372  371     0     0     0
100 100  483  388  1500   533   438
100 100  585  433  3000   635   483
100 100  900  433 -3000   950   483
100 100 1002  388 -1500  1052   438
100 100 1113  371     0     0     0
```

## MangoWM integration (from the mango wiki, github.com/mangowm/mango/wiki/rules)

Config: `~/.config/mango/config.conf`. Autostart is `exec_once=…` (underscore; the Python README
says `exec-once`, which is wrong; fix it). Suggested rule for window mode, ship it in the README:

```
window_rule=is_floating:1,is_overlay:1,is_global:1,no_border:1,is_open_silent:1,app_id:^totem-overlay$
```

`is_overlay` = always on top, `is_global` = on every tag, `is_open_silent` = don't take focus on open.
Floating size/position: `width`, `height`, `offset_x`, `offset_y` keys.

## Milestones

1. Cargo project (`app/` or repo root, keep `firmware/` untouched), GTK window drawing the built-in
   TOTEM layout with rotation (cairo), window mode, scales with window size.
2. HID listener thread -> layer + pressed-key state (port `hidlistener.py`/`protocol.py`, add tests).
3. Overlay mode (layer-shell, click-through, anchors, margin).
4. Settings popover + `config.toml` (background opacity, key opacity, mode, show policy, position).
5. Studio RPC client: fetch keymap + physical layout over `/dev/ttyACM*`, cache, label table.
6. `.keymap` fallback parser.
7. Packaging: `PKGBUILD`, `.desktop`, icon, udev rules (hidraw + tty), README rewrite. Remove the
   Python prototype.

## Open issues

- On the user's machine, keymap-drawer fails to parse `~/zmk-config-totem/config/totem.keymap` (the
  Python overlay only shows the first line of the error). Run
  `keymap parse -z ~/zmk-config-totem/config/totem.keymap` to see why. Becomes less important once
  Studio is the keymap source, but it may point to something odd in the file (check `git status` /
  line endings in that clone).
- Bluetooth path not yet tested by the user (needs re-pairing after the new firmware).
