//! Settings controls, shown in a popover (window mode, right-click) or a settings window
//! (`totem-overlay --settings`, needed in overlay mode because the overlay is click-through).

use std::rc::Rc;

use gtk::prelude::*;

use crate::config::{Config, MARGIN_RANGE, SCALE_RANGE, Show};
use crate::overlay::{Mode, Position};

/// Applies one change to a config. The app runs it on both the effective config and the one
/// saved to disk, so command-line overrides of other fields are never written to the file.
pub type Change = Box<dyn Fn(&mut Config)>;

const SHOW_OPTIONS: [(Show, &str); 2] = [(Show::Always, "Always"), (Show::Layers, "Only on non-base layers")];
const MODE_OPTIONS: [(Mode, &str); 2] = [(Mode::Window, "Window"), (Mode::Overlay, "Overlay (click-through)")];

/// Builds the settings controls for `config`. `on_change` gets every edit; `on_quit` is the
/// Quit button.
pub fn build(config: &Config, on_change: Rc<dyn Fn(Change)>, on_quit: impl Fn() + 'static) -> gtk::Box {
    let grid = gtk::Grid::builder().row_spacing(8).column_spacing(12).build();
    let mut row = 0;
    let mut add = |label: &str, widget: &gtk::Widget| {
        let l = gtk::Label::builder().label(label).xalign(0.0).build();
        grid.attach(&l, 0, row, 1, 1);
        grid.attach(widget, 1, row, 1, 1);
        row += 1;
    };

    let bg = opacity_slider(config.background_opacity, on_change.clone(), |c, v| c.background_opacity = v);
    add("Background opacity", bg.upcast_ref());
    let keys = opacity_slider(config.key_opacity, on_change.clone(), |c, v| c.key_opacity = v);
    add("Key opacity", keys.upcast_ref());

    let show = dropdown(&SHOW_OPTIONS, config.show, on_change.clone(), |c, v| c.show = v);
    add("Show", show.upcast_ref());

    let mode = dropdown(&MODE_OPTIONS, config.mode, on_change.clone(), |c, v| c.mode = v);
    add("Mode", mode.upcast_ref());

    let positions: Vec<(Position, &str)> = Position::ALL.iter().map(|&p| (p, p.name())).collect();
    let position = dropdown(&positions, config.position, on_change.clone(), |c, v| c.position = v);
    add("Overlay position", position.upcast_ref());

    let margin = gtk::SpinButton::with_range(f64::from(MARGIN_RANGE.0), f64::from(MARGIN_RANGE.1), 5.0);
    margin.set_value(f64::from(config.margin));
    let cb = on_change.clone();
    margin.connect_value_changed(move |s| {
        let v = s.value_as_int();
        cb(Box::new(move |c| c.margin = v));
    });
    add("Overlay margin (px)", margin.upcast_ref());

    let scale = gtk::SpinButton::with_range(SCALE_RANGE.0, SCALE_RANGE.1, 0.05);
    scale.set_digits(2);
    scale.set_value(config.scale);
    let cb = on_change.clone();
    scale.connect_value_changed(move |s| {
        let v = s.value();
        cb(Box::new(move |c| c.scale = v));
    });
    add("Overlay scale", scale.upcast_ref());

    // Overlay-only settings are greyed out in window mode.
    let overlay_widgets: [gtk::Widget; 3] = [position.upcast(), margin.upcast(), scale.upcast()];
    let set_sensitive = move |m: Mode| overlay_widgets.iter().for_each(|w| w.set_sensitive(m == Mode::Overlay));
    set_sensitive(config.mode);
    mode.connect_selected_notify(move |d| set_sensitive(MODE_OPTIONS[d.selected() as usize].0));

    let quit = gtk::Button::with_label("Quit");
    quit.set_halign(gtk::Align::End);
    quit.connect_clicked(move |_| on_quit());

    let hint = gtk::Label::builder()
        .label("Run <tt>totem-overlay --settings</tt> to open this in overlay mode.")
        .use_markup(true)
        .wrap(true)
        .xalign(0.0)
        .max_width_chars(40)
        .build();
    hint.add_css_class("dim-label");

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
    vbox.add_css_class("totem-settings");
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    vbox.append(&grid);
    vbox.append(&hint);
    vbox.append(&quit);
    vbox
}

fn opacity_slider(value: f64, on_change: Rc<dyn Fn(Change)>, set: fn(&mut Config, f64)) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
    s.set_value(value);
    s.set_width_request(200);
    s.set_draw_value(true);
    s.set_digits(2);
    s.connect_value_changed(move |s| {
        let v = s.value();
        on_change(Box::new(move |c| set(c, v)));
    });
    s
}

fn dropdown<T: Copy + PartialEq + 'static>(
    options: &[(T, &str)],
    current: T,
    on_change: Rc<dyn Fn(Change)>,
    set: fn(&mut Config, T),
) -> gtk::DropDown {
    let labels: Vec<&str> = options.iter().map(|o| o.1).collect();
    let d = gtk::DropDown::from_strings(&labels);
    d.set_selected(options.iter().position(|o| o.0 == current).unwrap_or(0) as u32);
    let values: Vec<T> = options.iter().map(|o| o.0).collect();
    d.connect_selected_notify(move |d| {
        if let Some(&v) = values.get(d.selected() as usize) {
            on_change(Box::new(move |c| set(c, v)));
        }
    });
    d
}
