# zmk-layer-overlay

On-screen layer overlay for ZMK keyboards (primarily a GEIST TOTEM, 38 keys, split,
Seeed XIAO BLE / nRF52840, left half = central). Target: Linux, Wayland, MangoWM on CachyOS.

The app is Rust + gtk4-rs (rewrite from a Python prototype is done; design notes and history in
`docs/rust-rewrite-plan.md`).

## Layout of this repo

- `firmware/` + `zephyr/module.yml`: ZMK module (C). Sends layer and pressed-key reports
  over Raw HID. Pulled into the user's zmk-config by west; do not move these paths.
- `src/`: the app. `hid.rs`/`protocol.rs` Raw HID, `studio/` ZMK Studio RPC client (keymap +
  physical layout over `/dev/ttyACM*`), `keymap_file/` `.keymap` fallback parser, `labels.rs`
  key labels, `draw.rs` cairo drawing, `overlay.rs` layer-shell, `settings.rs`/`config.rs`.
- `data/`: `.desktop` + icon. `packaging/arch/PKGBUILD`: builds the checkout's HEAD
  (`makepkg -si` from that folder; not the repo root, makepkg's `src/` would clash).
- `udev/`: hidraw + Studio tty access rule. `tools/gen-zmk-defines.py`: regenerates
  `src/keymap_file/zmk-defines.h` from ZMK headers.
- Checks: `cargo test`, `cargo clippy --all-targets`. `--demo`, `--render`, `--monitor`,
  `--fetch-keymap`, `--parse-keymap` help with testing without/with the keyboard.

## Related repo

`amunds1-dev/zmk-config-totem` (fork of Keycoon/zmk-config-totem), cloned at `~/zmk-config-totem`.
Its `main` has `zmk-raw-hid` + this repo in `config/west.yml`, and `raw_hid_adapter` plus the
`studio-rpc-usb-uart` snippet on the left half in `build.yaml` (the `layer-overlay` branch was
merged and deleted 2026-11-25). Only push to the fork (`origin`), never to Keycoon's upstream.
Firmware builds run on GitHub Actions on push, or:
`gh workflow run build.yml -R amunds1-dev/zmk-config-totem --ref main`.
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
