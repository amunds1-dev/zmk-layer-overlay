//! Fallback keymap source: a ZMK `.keymap` file, for keyboards without ZMK Studio (or before
//! the first USB connection). Produces the same [`Keymap`] as Studio, so labels work the same;
//! the physical layout is not in the file, so the built-in TOTEM layout is used.

mod dts;
mod expr;
mod pp;

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::keymap::{Behavior, Binding, CACHE_VERSION, Keymap, Layer, ParamKind, ParamSet, ParamValue};
use dts::{Cell, Node};

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, std::io::Error),
    Parse(PathBuf, String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(p, e) => write!(f, "cannot read {}: {e}", p.display()),
            Error::Parse(p, e) => write!(f, "{}: {e}", p.display()),
        }
    }
}

/// `~/...` paths from the config, made absolute.
pub fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => gtk::glib::home_dir().join(rest),
        None => PathBuf::from(path),
    }
}

/// Reads and parses a `.keymap` file. Warnings (e.g. includes that couldn't be found) are
/// returned alongside the keymap.
pub fn load(path: &Path) -> Result<(Keymap, Vec<String>), Error> {
    let mut pp = pp::Preprocessor::with_zmk();
    let toks = pp.file(path).map_err(|e| Error::Io(path.into(), e))?;
    let text = pp::join(&toks);
    let device = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let km = from_dts(&text, device).map_err(|e| Error::Parse(path.into(), e))?;
    Ok((km, std::mem::take(&mut pp.warnings)))
}

/// Parses preprocessed devicetree text.
fn from_dts(text: &str, device: String) -> Result<Keymap, String> {
    let root = dts::parse(text)?;
    let mut keymap_node = None;
    let mut labelled: HashMap<&str, &Node> = HashMap::new();
    root.walk(&mut |n| {
        if keymap_node.is_none() && n.string("compatible") == Some("zmk,keymap") {
            keymap_node = Some(n);
        }
        for l in &n.labels {
            labelled.insert(l, n);
        }
    });
    let keymap_node = keymap_node.ok_or("no node with compatible = \"zmk,keymap\"")?;

    let mut behaviors = Behaviors { labelled, list: Vec::new(), ids: HashMap::new() };
    let mut layers = Vec::new();
    for node in &keymap_node.children {
        if matches!(node.string("status"), Some("reserved" | "disabled")) || node.prop("bindings").is_none() {
            continue;
        }
        let name = node.string("display-name").or_else(|| node.string("label")).unwrap_or(&node.name);
        let bindings = behaviors.bindings(&node.cells("bindings"))?;
        layers.push(Layer { id: layers.len() as u32, name: name.to_string(), bindings });
    }
    if layers.is_empty() {
        return Err("the keymap has no layers".into());
    }
    Ok(Keymap { version: CACHE_VERSION, device, layers, behaviors: behaviors.list, layout: Vec::new() })
}

/// ZMK's built-in behaviors: label, Studio display name, binding cells, parameter kind(s).
const BUILTIN: &[(&str, &str, usize, &[ParamKind])] = &[
    ("kp", "Key Press", 1, &[ParamKind::HidUsage]),
    ("mo", "Momentary Layer", 1, &[ParamKind::LayerId]),
    ("tog", "Toggle Layer", 1, &[ParamKind::LayerId]),
    ("to", "To Layer", 1, &[ParamKind::LayerId]),
    ("sl", "Sticky Layer", 1, &[ParamKind::LayerId]),
    ("lt", "Layer-Tap", 2, &[ParamKind::LayerId, ParamKind::HidUsage]),
    ("mt", "Mod-Tap", 2, &[ParamKind::HidUsage, ParamKind::HidUsage]),
    ("sk", "Sticky Key", 1, &[ParamKind::HidUsage]),
    ("kt", "Key Toggle", 1, &[ParamKind::HidUsage]),
    ("bt", "Bluetooth", 2, &[]),
    ("out", "Output Selection", 1, &[]),
    ("caps_word", "Caps Word", 0, &[]),
    ("key_repeat", "Key Repeat", 0, &[]),
    ("gresc", "Grave/Escape", 0, &[]),
    ("bootloader", "Bootloader", 0, &[]),
    ("sys_reset", "Reset", 0, &[]),
    ("studio_unlock", "Studio Unlock", 0, &[]),
    ("soft_off", "Soft Off", 0, &[]),
    ("trans", "Transparent", 0, &[]),
    ("none", "None", 0, &[]),
    ("rgb_ug", "Underglow", 2, &[]),
    ("bl", "Backlight", 2, &[]),
    ("ext_power", "External Power", 1, &[]),
    ("mkp", "Mouse Key Press", 1, &[]),
    ("mmv", "Mouse Move", 1, &[]),
    ("msc", "Mouse Scroll", 1, &[]),
];

struct Behaviors<'a> {
    labelled: HashMap<&'a str, &'a Node>,
    list: Vec<Behavior>,
    /// Label -> (behavior id, binding cells).
    ids: HashMap<String, (i32, usize)>,
}

impl Behaviors<'_> {
    /// Splits a flat cell list (`&kp 4 &mo 1 ...`) into bindings.
    fn bindings(&mut self, cells: &[Cell]) -> Result<Vec<Binding>, String> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < cells.len() {
            let Cell::Ref(label) = &cells[i] else {
                return Err(format!("binding {} starts with a number instead of &behavior", out.len()));
            };
            let (behavior, n) = self.resolve(label, 0)?;
            let params: Vec<u32> = cells[i + 1..]
                .iter()
                .take(n)
                .map_while(|c| match c {
                    Cell::Num(v) => Some(*v as u32),
                    Cell::Ref(_) => None,
                })
                .collect();
            i += 1 + params.len();
            out.push(Binding {
                behavior,
                param1: params.first().copied().unwrap_or(0),
                param2: params.get(1).copied().unwrap_or(0),
            });
        }
        Ok(out)
    }

    /// Behavior id and cell count for `&label`, registering the behavior on first use.
    fn resolve(&mut self, label: &str, depth: usize) -> Result<(i32, usize), String> {
        if let Some(&r) = self.ids.get(label) {
            return Ok(r);
        }
        if depth > 8 {
            return Err(format!("behavior &{label} refers to itself"));
        }
        let (behavior, cells) = match self.labelled.get(label).copied() {
            Some(node) if node.string("compatible").is_some_and(|c| c.starts_with("zmk,behavior-")) => {
                self.custom(label, node, depth)?
            }
            _ => match BUILTIN.iter().find(|b| b.0 == label) {
                Some(&(_, name, cells, kinds)) => {
                    let value = |k: &ParamKind| ParamValue { name: String::new(), kind: *k };
                    let params = if kinds.is_empty() {
                        Vec::new()
                    } else {
                        vec![ParamSet {
                            param1: kinds.first().map(value).into_iter().collect(),
                            param2: kinds.get(1).map(value).into_iter().collect(),
                        }]
                    };
                    (Behavior { id: 0, name: name.into(), params, bindings: Vec::new() }, cells)
                }
                // Unknown (from a module, or defined in a file we couldn't include): show its
                // name, and assume parameters are whatever numbers follow.
                None => (Behavior { id: 0, name: label.into(), params: Vec::new(), bindings: Vec::new() }, 2),
            },
        };
        let id = self.list.len() as i32 + 1;
        self.list.push(Behavior { id: id as u32, ..behavior });
        self.ids.insert(label.into(), (id, cells));
        Ok((id, cells))
    }

    /// A behavior defined in the keymap: hold-taps, mod-morphs, tap-dances, macros, ...
    fn custom(&mut self, label: &str, node: &Node, depth: usize) -> Result<(Behavior, usize), String> {
        let compatible = node.string("compatible").unwrap_or_default();
        let cells = match node.cells("#binding-cells").first() {
            Some(Cell::Num(n)) => *n as usize,
            _ => 0,
        };
        let display = node.string("display-name").unwrap_or(label).to_string();
        // The behaviors it is built from, e.g. `bindings = <&kp>, <&kp>` for a hold-tap.
        let mut inner = Vec::new();
        let node_cells = node.cells("bindings");
        let mut i = 0;
        while i < node_cells.len() {
            if let Cell::Ref(l) = &node_cells[i] {
                let (id, n) = self.resolve(l, depth + 1)?;
                let params: Vec<u32> = node_cells[i + 1..]
                    .iter()
                    .take(n)
                    .map_while(|c| if let Cell::Num(v) = c { Some(*v as u32) } else { None })
                    .collect();
                i += params.len();
                inner.push(Binding {
                    behavior: id,
                    param1: params.first().copied().unwrap_or(0),
                    param2: params.get(1).copied().unwrap_or(0),
                });
            }
            i += 1;
        }
        let name = match compatible {
            "zmk,behavior-hold-tap" if inner.len() == 2 => "Hold-Tap",
            "zmk,behavior-mod-morph" if inner.len() == 2 => "Mod-Morph",
            "zmk,behavior-tap-dance" if !inner.is_empty() => "Tap Dance",
            _ => {
                inner.clear();
                &display
            }
        };
        let behavior = Behavior { id: 0, name: name.to_string(), params: Vec::new(), bindings: inner };
        Ok((behavior, cells))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::{self, Style};

    fn parse(src: &str) -> Keymap {
        let mut pp = pp::Preprocessor::with_zmk();
        let text = pp::join(&pp.text(src));
        from_dts(&text, "test".into()).unwrap()
    }

    fn tap_labels(km: &Keymap, layer: usize) -> Vec<String> {
        labels::layer_faces(km, layer, &|_| false).into_iter().map(|f| f.tap).collect()
    }

    const KEYMAP: &str = r#"
        #include <behaviors.dtsi>
        #include <dt-bindings/zmk/keys.h>
        #include <dt-bindings/zmk/bt.h>
        #define NAV 1
        / {
            behaviors {
                hm: homerow_mods {
                    compatible = "zmk,behavior-hold-tap";
                    #binding-cells = <2>;
                    bindings = <&kp>, <&kp>;
                };
                cs: comma_semi {
                    compatible = "zmk,behavior-mod-morph";
                    #binding-cells = <0>;
                    bindings = <&kp COMMA>, <&kp SEMI>;
                    mods = <(MOD_LSFT|MOD_RSFT)>;
                };
            };
            keymap {
                compatible = "zmk,keymap";
                base {
                    bindings = <&kp Q &hm LSHFT A &lt NAV SPACE &cs &mo NAV>;
                };
                nav_layer {
                    display-name = "Nav";
                    bindings = <&kp LA(BACKSPACE) &bt BT_SEL 2 &bt BT_CLR &trans &none>;
                };
                extra { status = "reserved"; };
            };
        };
    "#;

    #[test]
    fn layers_names_and_labels() {
        let km = parse(KEYMAP);
        assert_eq!(km.layers.len(), 2);
        assert_eq!(km.layer_name(0), "base");
        assert_eq!(km.layer_name(1), "Nav");
        assert_eq!(tap_labels(&km, 0), ["Q", "A", "␣", ",", "Nav"]);
        assert_eq!(tap_labels(&km, 1), ["⎇⌫", "BT 2", "BT Clr", ",", ""]);
        let faces = labels::layer_faces(&km, 0, &|id| id == 1);
        assert_eq!(faces[1].hold, "Shift");
        assert_eq!(faces[2].hold, "Nav");
        assert_eq!(faces[2].style, Style::ActiveLayer);
        assert_eq!(faces[3].hold, ";");
        assert_eq!(faces[4].style, Style::ActiveLayer);
    }

    #[test]
    fn parameter_counts_follow_the_behavior() {
        let km = parse(KEYMAP);
        let bt = &km.layers[1].bindings;
        assert_eq!((bt[1].param1, bt[1].param2), (3, 2));
        assert_eq!((bt[2].param1, bt[2].param2), (0, 0));
        // &hm LSHFT A: hold modifier, tap key.
        let hm = km.layers[0].bindings[1];
        assert_eq!((hm.param1, hm.param2), (0x07_00E1, 0x07_0004));
    }

    #[test]
    fn unknown_behaviors_show_their_name() {
        let km = parse("/ { keymap { compatible = \"zmk,keymap\"; l { bindings = <&magic 1 &kp A>; }; }; };");
        assert_eq!(tap_labels(&km, 0), ["magic", "A"]);
    }

    #[test]
    fn missing_keymap_is_an_error() {
        let mut pp = pp::Preprocessor::with_zmk();
        let text = pp::join(&pp.text("/ { foo { }; };"));
        assert!(from_dts(&text, String::new()).is_err());
    }
}
