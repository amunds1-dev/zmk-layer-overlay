//! Persisted settings: `~/.config/totem-overlay/config.toml`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::overlay::{Mode, Position};

/// When the drawing is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Show {
    #[default]
    Always,
    /// Only while a layer other than the base layer is active.
    Layers,
}

impl FromStr for Show {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "always" => Ok(Show::Always),
            "layers" => Ok(Show::Layers),
            _ => Err(format!("unknown show policy {s:?} (always, layers)")),
        }
    }
}

impl fmt::Display for Show {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Show::Always => "always",
            Show::Layers => "layers",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mode: Mode,
    pub show: Show,
    /// Opacity of the background, 0..1.
    pub background_opacity: f64,
    /// Opacity of the keys (fill and labels), 0..1.
    pub key_opacity: f64,
    /// Overlay mode: anchor, distance from the anchored edges in pixels, size factor.
    pub position: Position,
    pub margin: i32,
    pub scale: f64,
    /// Only use Raw HID devices whose product name contains this (empty: any).
    pub product: String,
    /// ZMK `.keymap` file used when the keyboard's keymap can't be read over USB (ZMK Studio).
    /// `~/` is allowed; empty for none.
    pub keymap_file: String,
}

pub const SCALE_RANGE: (f64, f64) = (0.3, 3.0);
pub const MARGIN_RANGE: (i32, i32) = (0, 2000);

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::Window,
            show: Show::Always,
            background_opacity: 0.88,
            key_opacity: 1.0,
            position: Position::Bottom,
            margin: 40,
            scale: 1.0,
            product: "TOTEM".into(),
            keymap_file: String::new(),
        }
    }
}

impl Config {
    /// Clamps values a hand-edited file could put out of range.
    pub fn sanitized(mut self) -> Self {
        let unit = |v: f64, default: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { default };
        let d = Config::default();
        self.background_opacity = unit(self.background_opacity, d.background_opacity);
        self.key_opacity = unit(self.key_opacity, d.key_opacity);
        self.scale = if self.scale.is_finite() { self.scale.clamp(SCALE_RANGE.0, SCALE_RANGE.1) } else { d.scale };
        self.margin = self.margin.clamp(MARGIN_RANGE.0, MARGIN_RANGE.1);
        self
    }

    pub fn product_filter(&self) -> Option<String> {
        (!self.product.is_empty()).then(|| self.product.clone())
    }
}

pub fn default_path() -> PathBuf {
    gtk::glib::user_config_dir().join("totem-overlay").join("config.toml")
}

/// Loads the config. A missing file gives the defaults. An unreadable file is moved aside to
/// `config.toml.bak` (so saving from the settings UI can't silently destroy it) and the defaults
/// are used; the returned message says so.
pub fn load(path: &Path) -> (Config, Option<String>) {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (Config::default(), None),
        Err(e) => return (Config::default(), Some(format!("cannot read {}: {e}", path.display()))),
    };
    match toml::from_str::<Config>(&text) {
        Ok(c) => (c.sanitized(), None),
        Err(e) => {
            let backup = path.with_extension("toml.bak");
            let moved = fs::rename(path, &backup).is_ok();
            let first = e.to_string().lines().next().unwrap_or_default().to_string();
            let msg = if moved {
                format!("invalid {} ({first}); moved to {}, using defaults", path.display(), backup.display())
            } else {
                format!("invalid {} ({first}); using defaults", path.display())
            };
            (Config::default(), Some(msg))
        }
    }
}

pub fn save(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = toml::to_string_pretty(config).map_err(io::Error::other)?;
    // Write then rename, so a crash never leaves a half-written file.
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, format!("# totem-overlay settings (also editable from the app)\n{text}"))?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("totem-overlay-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir("round-trip");
        let path = dir.join("sub").join("config.toml");
        let c = Config {
            mode: Mode::Overlay,
            show: Show::Layers,
            background_opacity: 0.5,
            key_opacity: 0.75,
            position: Position::TopRight,
            margin: 12,
            scale: 1.5,
            product: String::new(),
            keymap_file: "~/zmk-config/config/board.keymap".into(),
        };
        save(&path, &c).unwrap();
        assert_eq!(load(&path), (c, None));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("position = \"top-right\""), "{text}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = temp_dir("missing");
        assert_eq!(load(&dir.join("nope.toml")), (Config::default(), None));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_file_fills_in_defaults_and_clamps() {
        let dir = temp_dir("partial");
        let path = dir.join("config.toml");
        fs::write(&path, "show = \"layers\"\nkey_opacity = 7\nscale = 0.01\nmargin = -5\n").unwrap();
        let (c, err) = load(&path);
        assert_eq!(err, None);
        assert_eq!(c.show, Show::Layers);
        assert_eq!((c.key_opacity, c.scale, c.margin), (1.0, SCALE_RANGE.0, 0));
        assert_eq!(c.mode, Mode::Window);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_file_is_moved_aside() {
        let dir = temp_dir("invalid");
        let path = dir.join("config.toml");
        fs::write(&path, "mode = \"sideways\"\n").unwrap();
        let (c, err) = load(&path);
        assert_eq!(c, Config::default());
        assert!(err.unwrap().contains("config.toml.bak"));
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(dir.join("config.toml.bak")).unwrap(), "mode = \"sideways\"\n");
        fs::remove_dir_all(dir).unwrap();
    }
}
