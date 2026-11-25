# totem-overlay

On-screen layer overlay for a ZMK TOTEM. The keyboard reports its active layer over
Raw HID (USB or Bluetooth); this app shows that layer's keys in a click-through
overlay using wlr-layer-shell (MangoWM, Hyprland, Sway, KDE, …), and highlights keys as you press them.

This repo is two things:

- `totem_overlay/`: the Linux overlay app
- `firmware/` + `zephyr/module.yml`: a ZMK module that sends the active layer to the computer

## 1. Firmware

In your zmk-config, add this module and [zzeneg/zmk-raw-hid](https://github.com/zzeneg/zmk-raw-hid)
to `config/west.yml`:

```yaml
manifest:
  remotes:
    - name: zmkfirmware
      url-base: https://github.com/zmkfirmware
    - name: zzeneg
      url-base: https://github.com/zzeneg
    - name: amunds1-dev
      url-base: https://github.com/amunds1-dev
  projects:
    - name: zmk
      remote: zmkfirmware
      revision: main
      import: app/west.yml
    - name: zmk-raw-hid
      remote: zzeneg
      revision: main
    - name: zmk-layer-overlay
      remote: amunds1-dev
      revision: main
  self:
    path: config
```

and add the `raw_hid_adapter` shield to the half that connects to the computer (the central,
usually left) in `build.yaml`:

```yaml
  - board: xiao_ble//zmk
    shield: totem_left raw_hid_adapter
```

Build (GitHub Actions), then flash only that half: double-tap reset, copy the `.uf2`.
The module is on by default once Raw HID is enabled (`CONFIG_LAYER_OVERLAY_REPORT`). It also reports
pressed keys from both halves so the overlay can highlight them; set `CONFIG_LAYER_OVERLAY_KEY_REPORT=n`
in your `.conf` to turn that off (saves a little battery over Bluetooth).

## 2. Install (CachyOS / Arch)

```sh
sudo pacman -S --needed gtk4 gtk4-layer-shell python-gobject python-cairo python-hidapi python-yaml
uv tool install keymap-drawer            # or: pipx install keymap-drawer
sudo cp udev/70-totem-overlay.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

Unplug and replug the keyboard (or reconnect Bluetooth) after installing the udev rule.

Run from this folder with the system Python (it needs the distro's GTK bindings):

```sh
python -m totem_overlay --demo -k ~/zmk-config-totem/config/totem.keymap   # no keyboard needed
python -m totem_overlay --monitor                                          # test firmware: prints reports
python -m totem_overlay -k ~/zmk-config-totem/config/totem.keymap          # the real thing
```

Or install the command: `pip install --user --break-system-packages -e .` → `totem-overlay`.

## Options

| Flag | Default | |
|---|---|---|
| `-k, --keymap` | `~/zmk-config-totem/config/totem.keymap` | `.keymap` (parsed by keymap-drawer) or keymap-drawer YAML. Reloaded when the file changes. |
| `--show` | `layers` | `layers` = only while a non-base layer is active, `always` = stay visible |
| `--position` | `bottom` | `top`, `bottom-left`, `bottom-right`, `top-left`, `top-right`, `center` |
| `--margin`, `--scale`, `--opacity` | `40`, `1.0`, `0.88` | |
| `--product` | `TOTEM` | HID product name filter |

Autostart in MangoWM: add `exec-once=python -m totem_overlay -k …` (with the full path to this folder via `cd`/`PYTHONPATH`, or the installed `totem-overlay` command) to `~/.config/mango/config.conf`.

## How layers are matched

The firmware sends the highest active layer's index, id and name (see
`firmware/src/layer_report.c` for the protocol). The overlay picks the keymap layer with the same name,
falling back to the index. Conditional layers (e.g. `device` = `nav_num` + `sym_func`) are reported
by the keyboard itself, so they display correctly.

**Limitation:** keys are read from the keymap file. Bindings changed only in ZMK Studio won't show
until the file is updated. Reading the live keymap over ZMK Studio's protocol is a planned next step.

## Troubleshooting

- `--monitor` says no device: check `ls -l /dev/hidraw*` and that the new firmware is on the left half.
  Over Bluetooth, remove and re-pair the keyboard once after flashing, so the host reads the new HID services.
- Normal window instead of overlay: `gtk4-layer-shell` isn't installed or the compositor lacks layer-shell.
