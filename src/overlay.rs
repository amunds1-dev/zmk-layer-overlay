//! Overlay mode: a click-through wlr-layer-shell surface on the OVERLAY layer.

use std::fmt;
use std::str::FromStr;

use gtk::prelude::*;
use gtk::cairo;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal xdg toplevel the compositor manages.
    #[default]
    Window,
    /// Layer-shell overlay, click-through, anchored by [`Position`].
    Overlay,
}

impl FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "window" => Ok(Mode::Window),
            "overlay" => Ok(Mode::Overlay),
            _ => Err(format!("unknown mode {s:?} (window, overlay)")),
        }
    }
}

/// Where the overlay sits on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl Position {
    pub const ALL: [Position; 9] = [
        Position::Top,
        Position::Bottom,
        Position::Left,
        Position::Right,
        Position::TopLeft,
        Position::TopRight,
        Position::BottomLeft,
        Position::BottomRight,
        Position::Center,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Position::Top => "top",
            Position::Bottom => "bottom",
            Position::Left => "left",
            Position::Right => "right",
            Position::TopLeft => "top-left",
            Position::TopRight => "top-right",
            Position::BottomLeft => "bottom-left",
            Position::BottomRight => "bottom-right",
            Position::Center => "center",
        }
    }

    /// Screen edges the surface is anchored to. An unanchored axis is centred by the compositor.
    pub fn edges(self) -> &'static [Edge] {
        match self {
            Position::Top => &[Edge::Top],
            Position::Bottom => &[Edge::Bottom],
            Position::Left => &[Edge::Left],
            Position::Right => &[Edge::Right],
            Position::TopLeft => &[Edge::Top, Edge::Left],
            Position::TopRight => &[Edge::Top, Edge::Right],
            Position::BottomLeft => &[Edge::Bottom, Edge::Left],
            Position::BottomRight => &[Edge::Bottom, Edge::Right],
            Position::Center => &[],
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Position {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Position::ALL.into_iter().find(|p| p.name() == s).ok_or_else(|| {
            let names: Vec<_> = Position::ALL.iter().map(|p| p.name()).collect();
            format!("unknown position {s:?} ({})", names.join(", "))
        })
    }
}

/// Turns `win` (not yet realized) into a click-through overlay. Returns false, leaving the window
/// untouched, when the compositor has no layer-shell support.
pub fn setup(win: &gtk::ApplicationWindow, position: Position, margin: i32) -> bool {
    if !gtk4_layer_shell::is_supported() {
        return false;
    }
    win.init_layer_shell();
    win.set_namespace(Some(crate::APP_ID));
    win.set_layer(Layer::Overlay);
    win.set_keyboard_mode(KeyboardMode::None);
    win.set_exclusive_zone(-1); // don't move other surfaces (panels/bars) out of the way
    for &edge in position.edges() {
        win.set_anchor(edge, true);
        win.set_margin(edge, margin);
    }
    win.set_resizable(false);
    // Empty input region: pointer events go to whatever is underneath.
    win.connect_map(|w| {
        if let Some(surface) = w.surface() {
            surface.set_input_region(Some(&cairo::Region::create()));
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip() {
        for p in Position::ALL {
            assert_eq!(p.name().parse::<Position>(), Ok(p));
            assert_eq!(p.to_string(), p.name());
        }
        assert!("middle".parse::<Position>().is_err());
    }

    #[test]
    fn corner_positions_anchor_two_edges() {
        assert_eq!(Position::BottomRight.edges(), &[Edge::Bottom, Edge::Right]);
        assert_eq!(Position::Top.edges(), &[Edge::Top]);
        assert!(Position::Center.edges().is_empty());
    }

    #[test]
    fn modes_parse() {
        assert_eq!("overlay".parse(), Ok(Mode::Overlay));
        assert_eq!("window".parse(), Ok(Mode::Window));
        assert!("float".parse::<Mode>().is_err());
    }
}
