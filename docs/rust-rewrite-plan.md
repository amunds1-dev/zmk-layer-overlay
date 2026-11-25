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
window_rule=is_floating:1,is_overlay:1,is_global:1,no_border:1,no_blur:1,no_shadow:1,unfocused_opacity:1.0,is_open_silent:1,app_id:^totem-overlay$
```

`no_blur:1` matters: with `blur=1` + `blur_optimized=1` Mango blurs a cached copy of the
wallpaper behind translucent windows, so lowering the opacity showed blurred wallpaper instead
of the windows underneath. `unfocused_opacity:1.0` stops Mango dimming the window on top of the
app's own opacity. (Overlay mode is a layer surface, so these window rules don't apply to it;
`blur_layer=0` already keeps it unblurred.)

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

## Implementation notes (milestones 1-5 done)

- Code is a Cargo project at the repo root (`src/`). `--render PATH W H` draws one frame to a PNG
  and `--demo` fakes keyboard events, handy for screenshots. `--config PATH` uses another
  settings file, so tests don't touch the user's real one.
- Single instance: GApplication id `io.github.amunds1_dev.TotemOverlay`, so a second
  `totem-overlay` (launcher, keybinding, `--settings`) opens the settings window in the running
  instance. The keyboard window's Wayland app id is reset to `totem-overlay` on map (GTK sets it
  from the GApplication id when the window is shown, so setting it on realize is too early); the settings window keeps the
  GApplication id so the `^totem-overlay$` window rule doesn't make it an overlay.
- Settings: right-click popover in window mode; settings window in overlay mode (opened
  automatically when switching to overlay). Command-line flags override the file for one run and
  are never saved: each settings edit is applied to both the effective and the file config.
- Overlay uses exclusive zone 0, so margins count from inside panels/bars.
- Studio client (`src/studio/`): hand-written prost messages matching zmk-studio-messages
  (no protoc), SOF/ESC/EOF framing per zmk-studio-ts-client. Ports are found via sysfs
  (`ttyACM*` with VID:PID 1d50:615e) and probed with GetDeviceInfo; the TOTEM exposes two, only
  one answers. Fetches the keymap, the active physical layout and details for the behaviors the
  keymap uses. Fetched on every connect; cached in `~/.cache/totem-overlay/keymap.json`.
- Labels key off behavior display names ("Key Press", "Momentary Layer", ...); unknown behaviors
  fall back to the parameter metadata. Key names are US (the user types US on the TOTEM).
  Transparent keys show what they fall through to, dimmed; the key holding the current layer stays
  blue.
- The user's keymap has no `display-name`s, so Studio reports empty layer names and the overlay
  shows "layer N". Renaming layers in ZMK Studio (or adding `display-name`) fixes that.
- `--fetch-keymap` prints and caches the keymap; `--render ... --layer N` draws a given layer.

## Open issues

- keymap-drawer "parse failure" (resolved 2026-11-25): `~/zmk-config-totem` was never cloned on the
  CachyOS machine, so `keymap parse -z` exits with argparse's "can't open ... No such file". The
  Python overlay showed only the first stderr line (the usage banner), which hid that. Clone the
  config repo (branch `layer-overlay`) before relying on the `.keymap` fallback. The Rust app
  should show the last stderr line (or all of it), not the first.
- Bluetooth path not yet tested by the user (needs re-pairing after the new firmware).
