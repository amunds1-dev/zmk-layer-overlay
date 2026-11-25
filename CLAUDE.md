# zmk-layer-overlay

On-screen layer overlay for ZMK keyboards (primarily a GEIST TOTEM, 38 keys, split,
Seeed XIAO BLE / nRF52840, left half = central). Target: Linux, Wayland, MangoWM on CachyOS.

**Active work: rewriting the app in Rust. Read `docs/rust-rewrite-plan.md` first.**

## Layout of this repo

- `firmware/` + `zephyr/module.yml`: ZMK module (C). Sends layer and pressed-key reports
  over Raw HID. Pulled into the user's zmk-config by west; do not move these paths.
- `totem_overlay/`: current Python prototype (GTK4 + layer-shell). Working reference for the
  Rust app; delete once the Rust app reaches parity.
- `udev/`: hidraw access rule. `tests/`: Python tests (`python -m pytest tests`).

## Related repo

`amunds1-dev/zmk-config-totem` (fork of Keycoon/zmk-config-totem), cloned at `~/zmk-config-totem`.
Branch `layer-overlay` adds `zmk-raw-hid` + this repo to `config/west.yml` and
`raw_hid_adapter` to the left half in `build.yaml`. Not merged to `main` yet (the keymap editor
commits to `main`; merge once the user is happy). Firmware builds run on GitHub Actions:
`gh workflow run build.yml -R amunds1-dev/zmk-config-totem --ref layer-overlay`.
Firmware builds pull this repo's `main`, so firmware changes here need a push + rebuild.

## Raw HID protocol (firmware/src/layer_report.c is the source of truth)

Usage page 0xFF60, usage 0x61, 32-byte reports, byte 0 = 0x4C magic, byte 1 = type.
- type 0x01 layer: [2] highest active layer index, [3] its id, [4..7] active-layer bitmask by id (LE),
  [8..31] layer name (NUL padded, often empty).
- type 0x02 keys: [2..9] pressed key positions bitmask 0..63 (LE), both halves.
- host -> kbd: [0x4C, 0x3F] = query; keyboard replies with both reports.
Changing the protocol means changing both sides and rebuilding firmware.

## Conventions

- Commits: author `amunds1-dev <amunds1-dev@users.noreply.github.com>` (repo-local git config).
  Never put the user's personal email in commits.
- Keep README user-facing; design notes go in `docs/`.
