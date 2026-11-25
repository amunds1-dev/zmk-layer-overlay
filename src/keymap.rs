//! Keymap as read from the keyboard (ZMK Studio), cached in `~/.cache/totem-overlay/keymap.json`
//! so Bluetooth sessions work after one USB connection.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout;
use crate::protocol::LayerState;

/// Bumped when the cache format changes; older caches are ignored.
pub const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keymap {
    pub version: u32,
    /// Keyboard name from Studio's device info.
    pub device: String,
    /// In keymap order (index 0 = base layer).
    pub layers: Vec<Layer>,
    pub behaviors: Vec<Behavior>,
    /// The keyboard's active physical layout; empty if it reported none.
    pub layout: Vec<layout::Key>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Stable id (what Raw HID layer reports and layer parameters refer to).
    pub id: u32,
    pub name: String,
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub behavior: i32,
    pub param1: u32,
    pub param2: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Behavior {
    pub id: u32,
    /// Devicetree `display-name`, e.g. "Key Press", "Momentary Layer".
    pub name: String,
    /// Alternative parameter sets the behavior accepts.
    pub params: Vec<ParamSet>,
    /// For behaviors read from a `.keymap` file: the bindings it is built from (hold and tap
    /// of a hold-tap, the two sides of a mod-morph, the taps of a tap-dance).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParamSet {
    pub param1: Vec<ParamValue>,
    pub param2: Vec<ParamValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamValue {
    pub name: String,
    pub kind: ParamKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamKind {
    Nil,
    Constant(u32),
    Range { min: i32, max: i32 },
    HidUsage,
    LayerId,
}

impl ParamKind {
    pub fn matches(&self, v: u32) -> bool {
        match *self {
            ParamKind::Nil => v == 0,
            ParamKind::Constant(c) => v == c,
            ParamKind::Range { min, max } => i64::from(min) <= i64::from(v) && i64::from(v) <= i64::from(max),
            ParamKind::HidUsage | ParamKind::LayerId => true,
        }
    }
}

impl Keymap {
    pub fn behavior(&self, id: i32) -> Option<&Behavior> {
        u32::try_from(id).ok().and_then(|id| self.behaviors.iter().find(|b| b.id == id))
    }

    pub fn layer_by_id(&self, id: u32) -> Option<(usize, &Layer)> {
        self.layers.iter().enumerate().find(|(_, l)| l.id == id)
    }

    /// Display name of the layer with this id: its name, or "layer N" by position.
    pub fn layer_name(&self, id: u32) -> String {
        match self.layer_by_id(id) {
            Some((_, l)) if !l.name.is_empty() => l.name.clone(),
            Some((i, _)) => format!("layer {i}"),
            None => format!("layer #{id}"),
        }
    }
}

impl Keymap {
    /// Position of the layer a Raw HID layer report refers to: by id (stable when Studio
    /// reorders layers), else by the reported index.
    pub fn layer_index(&self, state: &LayerState) -> usize {
        self.layer_by_id(u32::from(state.layer_id))
            .map(|(i, _)| i)
            .unwrap_or(usize::from(state.index))
            .min(self.layers.len().saturating_sub(1))
    }

    /// Whether the layer with this id is on: the base layer always is; others per the report.
    pub fn layer_active(&self, state: Option<&LayerState>, id: u32) -> bool {
        if self.layers.first().is_some_and(|l| l.id == id) {
            return true;
        }
        state.is_some_and(|s| u32::from(s.layer_id) == id || (id < 32 && s.mask >> id & 1 == 1))
    }
}

pub fn cache_path() -> PathBuf {
    gtk::glib::user_cache_dir().join("totem-overlay").join("keymap.json")
}

/// Loads the cache; `None` if missing, unreadable or from another format version.
pub fn load_cache(path: &Path) -> Option<Keymap> {
    let text = fs::read_to_string(path).ok()?;
    let km: Keymap = serde_json::from_str(&text)
        .map_err(|e| eprintln!("ignoring keymap cache {}: {e}", path.display()))
        .ok()?;
    (km.version == CACHE_VERSION).then_some(km)
}

pub fn save_cache(path: &Path, km: &Keymap) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(km).map_err(io::Error::other)?)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const KP: u32 = 1;
    pub const MO: u32 = 2;
    pub const TRANS: u32 = 3;
    pub const NONE: u32 = 4;
    pub const LT: u32 = 5;
    pub const BT: u32 = 6;

    fn v(name: &str, kind: ParamKind) -> ParamValue {
        ParamValue { name: name.into(), kind }
    }

    /// A small keymap with ZMK's stock behavior names and metadata shapes.
    pub fn sample() -> Keymap {
        let set = |p1: Vec<ParamValue>, p2: Vec<ParamValue>| ParamSet { param1: p1, param2: p2 };
        let behaviors = vec![
            Behavior { id: KP, name: "Key Press".into(), params: vec![set(vec![v("Key", ParamKind::HidUsage)], vec![])], bindings: vec![] },
            Behavior { id: MO, name: "Momentary Layer".into(), params: vec![set(vec![v("Layer", ParamKind::LayerId)], vec![])], bindings: vec![] },
            Behavior { id: TRANS, name: "Transparent".into(), params: vec![], bindings: vec![] },
            Behavior { id: NONE, name: "None".into(), params: vec![], bindings: vec![] },
            Behavior {
                id: LT,
                name: "Layer-Tap".into(),
                params: vec![set(vec![v("Layer", ParamKind::LayerId)], vec![v("Tap", ParamKind::HidUsage)])],
                bindings: vec![],
            },
            Behavior {
                id: BT,
                name: "Bluetooth".into(),
                params: vec![
                    set(vec![v("Clear", ParamKind::Constant(0))], vec![]),
                    set(vec![v("Select Profile", ParamKind::Constant(3))], vec![v("Profile", ParamKind::Range { min: 0, max: 4 })]),
                ],
                bindings: vec![],
            },
        ];
        let b = |behavior: u32, param1: u32, param2: u32| Binding { behavior: behavior as i32, param1, param2 };
        let kp = |usage: u32| b(KP, 0x07_0000 | usage, 0);
        Keymap {
            version: CACHE_VERSION,
            device: "TOTEM".into(),
            layers: vec![
                Layer { id: 0, name: "Base".into(), bindings: vec![kp(0x04), b(MO, 1, 0), b(LT, 2, 0x07_002C), kp(0x05)] },
                Layer { id: 1, name: String::new(), bindings: vec![b(TRANS, 0, 0), b(TRANS, 0, 0), b(NONE, 0, 0), kp(0x52)] },
                Layer { id: 2, name: "Sys".into(), bindings: vec![b(BT, 3, 2), b(BT, 0, 0), b(TRANS, 0, 0), b(TRANS, 0, 0)] },
            ],
            behaviors,
            layout: vec![],
        }
    }

    #[test]
    fn cache_round_trip_and_version_check() {
        let dir = std::env::temp_dir().join(format!("totem-overlay-km-{}", std::process::id()));
        let path = dir.join("keymap.json");
        let mut km = sample();
        km.layout = layout::totem();
        save_cache(&path, &km).unwrap();
        assert_eq!(load_cache(&path), Some(km.clone()));
        km.version = CACHE_VERSION + 1;
        save_cache(&path, &km).unwrap();
        assert_eq!(load_cache(&path), None);
        fs::write(&path, "not json").unwrap();
        assert_eq!(load_cache(&path), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn layer_names_fall_back_to_position() {
        let km = sample();
        assert_eq!(km.layer_name(0), "Base");
        assert_eq!(km.layer_name(1), "layer 1");
        assert_eq!(km.layer_name(9), "layer #9");
    }

    #[test]
    fn reported_layer_is_found_by_id_then_index() {
        let mut km = sample();
        km.layers.swap(1, 2); // as after a Studio reorder: ids 0, 2, 1
        let state = |index, layer_id| LayerState { index, layer_id, mask: 0, name: String::new() };
        assert_eq!(km.layer_index(&state(1, 1)), 2);
        assert_eq!(km.layer_index(&state(1, 7)), 1);
        assert_eq!(km.layer_index(&state(9, 9)), 2); // clamped
    }

    #[test]
    fn active_layers() {
        let km = sample();
        let s = LayerState { index: 2, layer_id: 2, mask: 0b010, name: String::new() };
        assert!(km.layer_active(None, 0));
        assert!(!km.layer_active(None, 1));
        assert!(km.layer_active(Some(&s), 1) && km.layer_active(Some(&s), 2));
        assert!(!km.layer_active(Some(&LayerState { mask: 0, ..s.clone() }), 1));
    }

    #[test]
    fn param_kinds_match_values() {
        assert!(ParamKind::Range { min: 0, max: 4 }.matches(4));
        assert!(!ParamKind::Range { min: 0, max: 4 }.matches(5));
        assert!(ParamKind::Constant(3).matches(3) && !ParamKind::Constant(3).matches(0));
        assert!(ParamKind::Nil.matches(0) && !ParamKind::Nil.matches(1));
    }
}
