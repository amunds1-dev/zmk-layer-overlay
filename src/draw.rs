//! Cairo rendering of the keyboard. Independent of GTK widgets so it can also render to a PNG.

use std::f64::consts::PI;

use gtk::cairo::{Context, Operator};
use gtk::pango;

use crate::layout::{self, Key};

type Rgb = (f64, f64, f64);

const BG: Rgb = (0.09, 0.09, 0.11);
const KEY: Rgb = (0.20, 0.20, 0.24);
const TEXT: Rgb = (0.95, 0.95, 0.97);
const TEXT_DIM: Rgb = (0.55, 0.55, 0.62);
const ACCENT: Rgb = (0.45, 0.65, 1.0);

/// Sizes relative to one key unit, so everything scales with the window.
const PAD: f64 = 0.3;
const HEADER: f64 = 0.55;
const GAP: f64 = 0.08;
const KEY_RADIUS: f64 = 0.12;
const LABEL_SIZE: f64 = 0.27;

pub struct Scene<'a> {
    pub keys: &'a [Key],
    pub title: &'a str,
    pub subtitle: &'a str,
    /// Labels in keymap order; missing entries are drawn blank.
    pub labels: &'a [String],
    /// Opacity of the window background.
    pub bg_alpha: f64,
    /// Opacity of the keys (fill and labels).
    pub key_alpha: f64,
}

/// Layout size in key units including padding and header, for picking a default window size.
pub fn natural_size(keys: &[Key]) -> (f64, f64) {
    let b = layout::bounds(keys);
    (b.width() + 2.0 * PAD, b.height() + 2.0 * PAD + HEADER)
}

/// Draws the scene into a `width` x `height` area, fitting the layout and keeping its aspect ratio.
pub fn draw(cr: &Context, width: f64, height: f64, scene: &Scene) -> Result<(), gtk::cairo::Error> {
    cr.set_operator(Operator::Clear);
    cr.paint()?;
    cr.set_operator(Operator::Over);

    let (nat_w, nat_h) = natural_size(scene.keys);
    let unit = (width / nat_w).min(height / nat_h);
    if unit <= 0.0 {
        return Ok(());
    }
    // Centre the content; the background fills the whole area.
    let ox = (width - nat_w * unit) / 2.0;
    let oy = (height - nat_h * unit) / 2.0;

    rounded(cr, 0.0, 0.0, width, height, 0.3 * unit);
    set_rgba(cr, BG, scene.bg_alpha);
    cr.fill()?;

    let title_w = text(cr, scene.title, ox + PAD * unit, oy + PAD * unit * 0.6, 0.27 * unit, ACCENT, scene.key_alpha, true);
    if !scene.subtitle.is_empty() {
        let x = ox + PAD * unit + title_w + 0.25 * unit;
        text(cr, scene.subtitle, x, oy + PAD * unit * 0.6 + 0.05 * unit, 0.21 * unit, TEXT_DIM, scene.key_alpha, false);
    }

    let b = layout::bounds(scene.keys);
    let kx = ox + PAD * unit - b.min_x * unit;
    let ky = oy + (PAD + HEADER) * unit - b.min_y * unit;
    for (i, key) in scene.keys.iter().enumerate() {
        cr.save()?;
        cr.translate(kx, ky);
        cr.scale(unit, unit);
        cr.translate(key.rx, key.ry);
        cr.rotate(key.rot.to_radians());
        cr.translate(key.x - key.rx + GAP / 2.0, key.y - key.ry + GAP / 2.0);
        let (w, h) = (key.w - GAP, key.h - GAP);
        rounded(cr, 0.0, 0.0, w, h, KEY_RADIUS);
        set_rgba(cr, KEY, scene.key_alpha);
        cr.fill()?;
        if let Some(label) = scene.labels.get(i).filter(|l| !l.is_empty()) {
            centered(cr, label, w, h, unit, TEXT, scene.key_alpha);
        }
        cr.restore()?;
    }
    Ok(())
}

fn set_rgba(cr: &Context, (r, g, b): Rgb, a: f64) {
    cr.set_source_rgba(r, g, b, a);
}

fn rounded(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

fn make_layout(cr: &Context, s: &str, px: f64, bold: bool) -> pango::Layout {
    let lay = pangocairo::functions::create_layout(cr);
    let mut font = pango::FontDescription::from_string("Sans");
    if bold {
        font.set_weight(pango::Weight::Bold);
    }
    font.set_absolute_size(px * f64::from(pango::SCALE));
    lay.set_font_description(Some(&font));
    lay.set_text(s);
    lay
}

/// Draws text with its top-left at (x, y) in device units; returns its width.
#[allow(clippy::too_many_arguments)]
fn text(cr: &Context, s: &str, x: f64, y: f64, px: f64, rgb: Rgb, a: f64, bold: bool) -> f64 {
    let lay = make_layout(cr, s, px, bold);
    cr.move_to(x, y);
    set_rgba(cr, rgb, a);
    pangocairo::functions::show_layout(cr, &lay);
    f64::from(lay.pixel_size().0)
}

/// Draws a label centred in a key whose user space is scaled to key units (`unit` px per unit).
/// Text is laid out in device-pixel sizes so hinting stays sharp, then scaled back.
fn centered(cr: &Context, s: &str, w: f64, h: f64, unit: f64, rgb: Rgb, a: f64) {
    let mut px = LABEL_SIZE * unit;
    let max_w = (w - 0.1) * unit;
    let mut lay = make_layout(cr, s, px, false);
    while f64::from(lay.pixel_size().0) > max_w && px > 6.0 {
        px *= 0.9;
        lay = make_layout(cr, s, px, false);
    }
    let (tw, th) = lay.pixel_size();
    cr.save().ok();
    cr.scale(1.0 / unit, 1.0 / unit);
    cr.move_to((w * unit - f64::from(tw)) / 2.0, (h * unit - f64::from(th)) / 2.0);
    set_rgba(cr, rgb, a);
    pangocairo::functions::update_layout(cr, &lay);
    pangocairo::functions::show_layout(cr, &lay);
    cr.restore().ok();
}
