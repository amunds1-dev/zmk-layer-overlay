//! Key labels: HID usage names (ported from the Python prototype's pretty labels) and
//! turning a keymap binding into what is drawn on the key.

use crate::keymap::{Binding, Keymap, ParamKind};

/// What a key shows for the current layer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Face {
    /// Main label.
    pub tap: String,
    /// Small secondary label (hold action, or what kind of layer key this is).
    pub hold: String,
    pub style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    #[default]
    Normal,
    /// `&trans`: shows what it falls through to, dimmed.
    Transparent,
    /// `&none`, or no binding.
    Empty,
    /// Layer key whose layer is currently active.
    ActiveLayer,
}

// ZMK keeps implicit modifiers in the top byte of a `&kp` parameter (dt-bindings/zmk/modifiers.h).
const MOD_LCTL: u8 = 0x01;
const MOD_LSFT: u8 = 0x02;
const MOD_LALT: u8 = 0x04;
const MOD_LGUI: u8 = 0x08;
const MOD_RCTL: u8 = 0x10;
const MOD_RSFT: u8 = 0x20;
const MOD_RALT: u8 = 0x40;
const MOD_RGUI: u8 = 0x80;

const PAGE_KEYBOARD: u32 = 0x07;
const PAGE_CONSUMER: u32 = 0x0C;

/// Label for a HID usage as encoded in ZMK bindings: (mods << 24) | (page << 16) | id.
pub fn usage_label(encoded: u32) -> String {
    let mods = (encoded >> 24) as u8;
    let page = (encoded >> 16) & 0xFF;
    let id = encoded & 0xFFFF;
    let page = if page == 0 { PAGE_KEYBOARD } else { page };

    let shift_only = mods != 0 && mods & !(MOD_LSFT | MOD_RSFT) == 0;
    if page == PAGE_KEYBOARD
        && shift_only
        && let Some(s) = shifted_symbol(id)
    {
        return s.to_string();
    }
    let base = match page {
        PAGE_KEYBOARD => keyboard_label(id),
        PAGE_CONSUMER => consumer_label(id),
        _ => None,
    }
    .map(str::to_string)
    .unwrap_or_else(|| format!("{page:02X}:{id:02X}"));
    format!("{}{base}", mod_prefix(mods))
}

/// Compact modifier symbols, e.g. ⌃⇧ for Ctrl+Shift. AltGr gets its own mark.
fn mod_prefix(mods: u8) -> String {
    let mut s = String::new();
    if mods & (MOD_LCTL | MOD_RCTL) != 0 {
        s.push('⌃');
    }
    if mods & (MOD_LSFT | MOD_RSFT) != 0 {
        s.push('⇧');
    }
    if mods & MOD_LALT != 0 {
        s.push('⎇');
    }
    if mods & MOD_RALT != 0 {
        s.push_str("AltGr+");
    }
    if mods & (MOD_LGUI | MOD_RGUI) != 0 {
        s.push('◆');
    }
    s
}

fn shifted_symbol(id: u32) -> Option<&'static str> {
    Some(match id {
        0x1E => "!",
        0x1F => "@",
        0x20 => "#",
        0x21 => "$",
        0x22 => "%",
        0x23 => "^",
        0x24 => "&",
        0x25 => "*",
        0x26 => "(",
        0x27 => ")",
        0x2D => "_",
        0x2E => "+",
        0x2F => "{",
        0x30 => "}",
        0x31 => "|",
        0x33 => ":",
        0x34 => "\"",
        0x35 => "~",
        0x36 => "<",
        0x37 => ">",
        0x38 => "?",
        _ => return None,
    })
}

/// US-layout names, like ZMK Studio and keymap-drawer show them.
fn keyboard_label(id: u32) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U",
        "V", "W", "X", "Y", "Z",
    ];
    const DIGITS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
    const F1_12: [&str; 12] = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"];
    const F13_24: [&str; 12] = ["F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24"];
    const KEYPAD: [&str; 16] =
        ["KP/", "KP*", "KP−", "KP+", "KP⏎", "KP1", "KP2", "KP3", "KP4", "KP5", "KP6", "KP7", "KP8", "KP9", "KP0", "KP."];
    let i = id as usize;
    Some(match id {
        0x04..=0x1D => LETTERS[i - 0x04],
        0x1E..=0x27 => DIGITS[i - 0x1E],
        0x28 => "⏎",
        0x29 => "Esc",
        0x2A => "⌫",
        0x2B => "⇥",
        0x2C => "␣",
        0x2D => "-",
        0x2E => "=",
        0x2F => "[",
        0x30 => "]",
        0x31 => "\\",
        0x32 => "#",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "Caps",
        0x3A..=0x45 => F1_12[i - 0x3A],
        0x46 => "PrtSc",
        0x47 => "ScrLk",
        0x48 => "Pause",
        0x49 => "Ins",
        0x4A => "Home",
        0x4B => "PgUp",
        0x4C => "Del",
        0x4D => "End",
        0x4E => "PgDn",
        0x4F => "→",
        0x50 => "←",
        0x51 => "↓",
        0x52 => "↑",
        0x53 => "NumLk",
        0x54..=0x63 => KEYPAD[i - 0x54],
        0x64 => "\\",
        0x65 => "Menu",
        0x68..=0x73 => F13_24[i - 0x68],
        0x7F => "🔇",
        0x80 => "🔊",
        0x81 => "🔉",
        0xE0 | 0xE4 => "Ctrl",
        0xE1 | 0xE5 => "Shift",
        0xE2 => "Alt",
        0xE6 => "AltGr",
        0xE3 | 0xE7 => "Super",
        _ => return None,
    })
}

fn consumer_label(id: u32) -> Option<&'static str> {
    Some(match id {
        0x6F => "☀+",
        0x70 => "☀−",
        0xB5 => "⏭",
        0xB6 => "⏮",
        0xB7 => "⏹",
        0xB8 => "⏏",
        0xCD => "⏯",
        0xE2 => "🔇",
        0xE9 => "🔊",
        0xEA => "🔉",
        _ => return None,
    })
}

/// `&bt` commands (dt-bindings/zmk/bt.h).
fn bluetooth_label(cmd: u32, profile: u32) -> String {
    match cmd {
        0 => "BT Clr".into(),
        1 => "BT →".into(),
        2 => "BT ←".into(),
        3 => format!("BT {profile}"),
        4 => "BT Clr All".into(),
        5 => format!("BT Disc {profile}"),
        _ => format!("BT {cmd}"),
    }
}

/// `&out` commands (dt-bindings/zmk/outputs.h).
fn output_label(cmd: u32) -> String {
    match cmd {
        0 => "USB/BLE".into(),
        1 => "USB".into(),
        2 => "BLE".into(),
        _ => format!("Out {cmd}"),
    }
}

/// The face of one binding. `layer_active(id)` says whether a layer is currently on, for
/// highlighting layer keys.
pub fn face(km: &Keymap, b: &Binding, layer_active: &dyn Fn(u32) -> bool) -> Face {
    let Some(behavior) = km.behavior(b.behavior) else {
        return Face { tap: "?".into(), ..Face::default() };
    };
    let layer = |id: u32| km.layer_name(id);
    let layer_key = |tap: String, hold: &str, id: u32| Face {
        tap,
        hold: hold.into(),
        style: if layer_active(id) { Style::ActiveLayer } else { Style::Normal },
    };
    let plain = |tap: String, hold: &str| Face { tap, hold: hold.into(), style: Style::Normal };
    match behavior.name.as_str() {
        "Key Press" => plain(usage_label(b.param1), ""),
        "Transparent" => Face { style: Style::Transparent, ..Face::default() },
        "None" => Face { style: Style::Empty, ..Face::default() },
        "Momentary Layer" => layer_key(layer(b.param1), "", b.param1),
        "Toggle Layer" => layer_key(layer(b.param1), "toggle", b.param1),
        "To Layer" => layer_key(layer(b.param1), "to", b.param1),
        "Sticky Layer" => layer_key(layer(b.param1), "sticky", b.param1),
        "Layer-Tap" => layer_key(usage_label(b.param2), &layer(b.param1), b.param1),
        "Mod-Tap" => plain(usage_label(b.param2), &usage_label(b.param1)),
        "Sticky Key" => plain(usage_label(b.param1), "sticky"),
        "Bluetooth" => plain(bluetooth_label(b.param1, b.param2), ""),
        "Output Selection" => plain(output_label(b.param1), ""),
        "Caps Word" => plain("Caps".into(), "word"),
        "Key Repeat" => plain("Repeat".into(), ""),
        "Bootloader" => plain("Boot".into(), ""),
        "Reset" => plain("Reset".into(), ""),
        "Studio Unlock" => plain("Studio".into(), "unlock"),
        "Key Toggle" => plain(usage_label(b.param1), "toggle"),
        "Grave/Escape" => plain("Esc".into(), "`"),
        // Composite behaviors from a .keymap file (Studio reports these by their own name).
        "Hold-Tap" | "Mod-Morph" | "Tap Dance" if !behavior.bindings.is_empty() => {
            composite_face(km, &behavior.name, &behavior.bindings, b, layer_active)
        }
        _ => generic_face(km, behavior, b),
    }
}

/// A behavior built from others: a hold-tap passes its two parameters to its hold and tap
/// behaviors; a mod-morph shows its plain side with the modified side below; a tap-dance its
/// first tap with the second below.
fn composite_face(
    km: &Keymap,
    kind: &str,
    inner: &[Binding],
    b: &Binding,
    layer_active: &dyn Fn(u32) -> bool,
) -> Face {
    let sub = |i: usize, param: Option<u32>| {
        inner.get(i).map_or_else(Face::default, |ib| {
            let ib = match param {
                Some(p) => Binding { param1: p, param2: 0, ..*ib },
                None => *ib,
            };
            face(km, &ib, layer_active)
        })
    };
    let (main, below) = match kind {
        "Hold-Tap" => (sub(1, Some(b.param2)), sub(0, Some(b.param1))),
        _ => (sub(0, None), sub(1, None)),
    };
    let style = if below.style == Style::ActiveLayer { Style::ActiveLayer } else { Style::Normal };
    Face { tap: main.tap, hold: below.tap, style }
}

/// Unknown behavior (custom hold-tap, macro, ...): describe it from its parameter metadata.
fn generic_face(km: &Keymap, behavior: &crate::keymap::Behavior, b: &Binding) -> Face {
    let describe = |values: &[crate::keymap::ParamValue], v: u32| -> Option<String> {
        let value = values.iter().find(|p| p.kind.matches(v))?;
        Some(match value.kind {
            ParamKind::Nil => return None,
            ParamKind::Constant(_) => value.name.clone(),
            ParamKind::Range { .. } => v.to_string(),
            ParamKind::HidUsage => usage_label(v),
            ParamKind::LayerId => km.layer_name(v),
        })
    };
    let set = behavior
        .params
        .iter()
        .find(|s| {
            (s.param1.is_empty() || s.param1.iter().any(|p| p.kind.matches(b.param1)))
                && (s.param2.is_empty() || s.param2.iter().any(|p| p.kind.matches(b.param2)))
        });
    let (p1, p2) = match set {
        Some(s) => (describe(&s.param1, b.param1), describe(&s.param2, b.param2)),
        None => (None, None),
    };
    // Hold-tap style (two parameters): tap on top, hold below, like Mod-Tap.
    match (p1, p2) {
        (Some(hold), Some(tap)) => Face { tap, hold, style: Style::Normal },
        (Some(p), None) | (None, Some(p)) => Face { tap: p, hold: behavior.name.clone(), style: Style::Normal },
        (None, None) => Face { tap: behavior.name.clone(), ..Face::default() },
    }
}

/// Faces for every key of layer `index`. Transparent keys show, dimmed, what they fall
/// through to on the next active layer below.
pub fn layer_faces(km: &Keymap, index: usize, layer_active: &dyn Fn(u32) -> bool) -> Vec<Face> {
    let Some(layer) = km.layers.get(index) else { return Vec::new() };
    let is_trans = |b: &Binding| km.behavior(b.behavior).is_some_and(|bh| bh.name == "Transparent");
    layer
        .bindings
        .iter()
        .enumerate()
        .map(|(pos, b)| {
            if !is_trans(b) {
                return face(km, b, layer_active);
            }
            let below = km.layers[..index]
                .iter()
                .enumerate()
                .rev()
                .filter(|(i, l)| *i == 0 || layer_active(l.id))
                .filter_map(|(_, l)| l.bindings.get(pos))
                .find(|b| !is_trans(b));
            match below.map(|b| face(km, b, layer_active)) {
                // The key holding this layer stays highlighted when it falls through.
                Some(f) if f.style == Style::ActiveLayer => f,
                Some(f) => Face { style: Style::Transparent, ..f },
                None => Face { style: Style::Transparent, ..Face::default() },
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unusual_byte_groupings)] // usages written as mods_page_id
mod tests {
    use super::*;
    use crate::keymap::tests::sample;

    const KB: u32 = 0x07_0000;

    #[test]
    fn plain_keys() {
        assert_eq!(usage_label(KB | 0x04), "A");
        assert_eq!(usage_label(KB | 0x27), "0");
        assert_eq!(usage_label(KB | 0x2A), "⌫");
        assert_eq!(usage_label(KB | 0x33), ";");
        assert_eq!(usage_label(KB | 0x45), "F12");
        assert_eq!(usage_label(KB | 0x6B), "F16");
        assert_eq!(usage_label(KB | 0xE5), "Shift");
        assert_eq!(usage_label(KB | 0xE6), "AltGr");
    }

    #[test]
    fn shifted_keys_show_their_symbol() {
        // ZMK's PLUS = LS(EQUAL), STAR = LS(N8), LEFT_PARENTHESIS = LS(N9)
        assert_eq!(usage_label(0x02_07_002E), "+");
        assert_eq!(usage_label(0x02_07_0025), "*");
        assert_eq!(usage_label(0x20_07_0026), "(");
        // Shift with a key that has no shifted symbol keeps the prefix.
        assert_eq!(usage_label(0x02_07_0050), "⇧←");
    }

    #[test]
    fn modifier_functions() {
        // LA(BACKSPACE), LG(UP_ARROW), LC(LS(Z))
        assert_eq!(usage_label(0x04_07_002A), "⎇⌫");
        assert_eq!(usage_label(0x08_07_0052), "◆↑");
        assert_eq!(usage_label(0x03_07_001D), "⌃⇧Z");
    }

    #[test]
    fn consumer_and_unknown_usages() {
        assert_eq!(usage_label(0x0C_00E9), "🔊");
        assert_eq!(usage_label(0x0C_006F), "☀+");
        assert_eq!(usage_label(0x0C_0123), "0C:123");
        assert_eq!(usage_label(0x00_0004), "A"); // page 0 means keyboard
    }

    #[test]
    fn binding_faces() {
        let km = sample();
        let none_active = |_: u32| false;
        let base = layer_faces(&km, 0, &none_active);
        assert_eq!(base[0], Face { tap: "A".into(), ..Face::default() });
        assert_eq!(base[1], Face { tap: "layer 1".into(), ..Face::default() });
        assert_eq!(base[2], Face { tap: "␣".into(), hold: "Sys".into(), style: Style::Normal });

        let sys = layer_faces(&km, 2, &none_active);
        assert_eq!(sys[0].tap, "BT 2");
        assert_eq!(sys[1].tap, "BT Clr");
    }

    #[test]
    fn active_layer_keys_are_highlighted() {
        let km = sample();
        let one_active = |id: u32| id == 1;
        let base = layer_faces(&km, 0, &one_active);
        assert_eq!(base[1].style, Style::ActiveLayer);
        assert_eq!(base[2].style, Style::Normal);
    }

    #[test]
    fn transparent_keys_show_what_they_fall_through_to() {
        let km = sample();
        // Layer 2 active on top of layer 1: key 3 is trans on 2, ↑ on 1.
        let active = |id: u32| id == 1 || id == 2;
        let sys = layer_faces(&km, 2, &active);
        assert_eq!(sys[3], Face { tap: "↑".into(), hold: String::new(), style: Style::Transparent });
        // Key 2 is trans on 2; layer 1 has &none there, which is not transparent.
        assert_eq!(sys[2].style, Style::Transparent);
        assert_eq!(sys[2].tap, "");
        // Layer 1 inactive: falls through to the base layer instead.
        let only_two = |id: u32| id == 2;
        assert_eq!(layer_faces(&km, 2, &only_two)[3].tap, "B");
        // Key 0 on layer 1 falls through to base A.
        assert_eq!(layer_faces(&km, 1, &active)[0].tap, "A");
        // Key 1 on layer 1 falls through to the &mo 1 holding it, which stays highlighted.
        assert_eq!(layer_faces(&km, 1, &active)[1].style, Style::ActiveLayer);
    }
}
