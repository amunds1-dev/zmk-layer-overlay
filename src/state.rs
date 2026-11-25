//! Keyboard state as seen by the UI, updated from HID listener events.

use crate::config::Show;
use crate::hid::Event;
use crate::protocol::{KeyState, LayerState, Report};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Keyboard {
    /// Product name while connected.
    pub device: Option<String>,
    /// Last layer report; `None` until the first one after connecting.
    pub layer: Option<LayerState>,
    pub keys: KeyState,
    /// Last problem reported by the listener, cleared on connect.
    pub error: Option<String>,
}

impl Keyboard {
    /// Applies an event; returns whether anything visible changed.
    pub fn apply(&mut self, event: Event) -> bool {
        let before = self.clone();
        match event {
            Event::Connected(name) => {
                self.device = Some(name);
                self.error = None;
            }
            Event::Disconnected => {
                self.device = None;
                self.layer = None;
                self.keys = KeyState::default();
            }
            Event::Report(Report::Layer(l)) => self.layer = Some(l),
            Event::Report(Report::Keys(k)) => self.keys = k,
            Event::Error(msg) => self.error = Some(msg),
        }
        *self != before
    }

    pub fn connected(&self) -> bool {
        self.device.is_some()
    }

    /// Whether the drawing should be visible under the given show policy. Errors always show.
    pub fn wants_visible(&self, show: Show) -> bool {
        match show {
            Show::Always => true,
            Show::Layers => {
                self.error.is_some() || (self.connected() && self.layer.as_ref().is_some_and(|l| l.index != 0))
            }
        }
    }

    /// Header text: (title, subtitle).
    pub fn header(&self) -> (String, String) {
        let title = match &self.layer {
            Some(l) if !l.name.is_empty() => l.name.clone(),
            Some(l) => format!("layer {}", l.index),
            None => "TOTEM".into(),
        };
        let subtitle = if let Some(e) = &self.error {
            e.clone()
        } else if !self.connected() {
            "keyboard not connected".into()
        } else {
            String::new()
        };
        (title, subtitle)
    }
}

/// One animation frame (~16 ms) of the show/hide fade. Fades in fast and out a little slower,
/// so quick layer taps don't flicker.
pub fn fade_step(alpha: f64, target: f64) -> f64 {
    let step = if target > alpha { 0.25 } else { 0.12 };
    if (target - alpha).abs() <= step { target } else { alpha + step.copysign(target - alpha) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(index: u8, name: &str) -> Event {
        Event::Report(Report::Layer(LayerState {
            index,
            layer_id: index,
            mask: 1 | 1 << index,
            name: name.into(),
        }))
    }

    #[test]
    fn starts_disconnected() {
        let kb = Keyboard::default();
        assert!(!kb.connected());
        assert_eq!(kb.header(), ("TOTEM".into(), "keyboard not connected".into()));
    }

    #[test]
    fn tracks_layer_and_keys() {
        let mut kb = Keyboard::default();
        assert!(kb.apply(Event::Connected("TOTEM".into())));
        assert!(kb.apply(layer(2, "")));
        assert_eq!(kb.header(), ("layer 2".into(), String::new()));
        assert!(kb.apply(layer(3, "NAV")));
        assert_eq!(kb.header().0, "NAV");
        assert!(kb.apply(Event::Report(Report::Keys(KeyState { mask: 1 << 5 }))));
        assert!(kb.keys.is_pressed(5));
    }

    #[test]
    fn repeated_report_is_not_a_change() {
        let mut kb = Keyboard::default();
        kb.apply(Event::Connected("TOTEM".into()));
        assert!(kb.apply(layer(1, "")));
        assert!(!kb.apply(layer(1, "")));
    }

    #[test]
    fn disconnect_clears_layer_and_keys() {
        let mut kb = Keyboard::default();
        kb.apply(Event::Connected("TOTEM".into()));
        kb.apply(layer(1, ""));
        kb.apply(Event::Report(Report::Keys(KeyState { mask: 3 })));
        assert!(kb.apply(Event::Disconnected));
        assert_eq!(kb.layer, None);
        assert_eq!(kb.keys, KeyState::default());
    }

    #[test]
    fn error_shows_until_connected() {
        let mut kb = Keyboard::default();
        kb.apply(Event::Error("cannot open /dev/hidraw3".into()));
        assert_eq!(kb.header().1, "cannot open /dev/hidraw3");
        kb.apply(Event::Disconnected);
        assert_eq!(kb.header().1, "cannot open /dev/hidraw3");
        kb.apply(Event::Connected("TOTEM".into()));
        assert_eq!(kb.error, None);
    }

    #[test]
    fn layers_policy_shows_only_non_base_layers() {
        let mut kb = Keyboard::default();
        assert!(kb.wants_visible(Show::Always));
        assert!(!kb.wants_visible(Show::Layers));
        kb.apply(Event::Connected("TOTEM".into()));
        kb.apply(layer(0, ""));
        assert!(!kb.wants_visible(Show::Layers));
        kb.apply(layer(2, ""));
        assert!(kb.wants_visible(Show::Layers));
        kb.apply(Event::Disconnected);
        assert!(!kb.wants_visible(Show::Layers));
        kb.apply(Event::Error("cannot open".into()));
        assert!(kb.wants_visible(Show::Layers));
    }

    #[test]
    fn fade_reaches_target_exactly() {
        let mut a = 0.0;
        let mut frames = 0;
        while a != 1.0 {
            a = fade_step(a, 1.0);
            frames += 1;
        }
        assert_eq!(frames, 4);
        while a != 0.0 {
            a = fade_step(a, 0.0);
            frames += 1;
        }
        assert_eq!(frames, 4 + 9);
        assert_eq!(fade_step(0.5, 0.5), 0.5);
    }
}
