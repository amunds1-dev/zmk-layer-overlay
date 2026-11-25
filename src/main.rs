mod draw;
mod layout;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{cairo, gio, glib};

/// Wayland app id (xdg toplevel). Not a valid GApplication id (no dot), so it is set as the
/// program name, which GTK uses for the app id when the GtkApplication has none.
const APP_ID: &str = "totem-overlay";
/// Default window size: pixels per key unit.
const DEFAULT_UNIT_PX: f64 = 56.0;

struct State {
    keys: Vec<layout::Key>,
    labels: Vec<String>,
    title: String,
    subtitle: String,
    bg_alpha: f64,
    key_alpha: f64,
}

impl State {
    fn new() -> Self {
        let keys = layout::totem();
        // No keymap source yet (milestones 5/6): label keys with their position index.
        let labels = (0..keys.len()).map(|i| i.to_string()).collect();
        State {
            keys,
            labels,
            title: "TOTEM".into(),
            subtitle: "no keymap loaded".into(),
            bg_alpha: 0.88,
            key_alpha: 1.0,
        }
    }

    fn scene(&self) -> draw::Scene<'_> {
        draw::Scene {
            keys: &self.keys,
            title: &self.title,
            subtitle: &self.subtitle,
            labels: &self.labels,
            bg_alpha: self.bg_alpha,
            key_alpha: self.key_alpha,
        }
    }
}

fn main() -> glib::ExitCode {
    let mut args = std::env::args().skip(1);
    let mut render: Option<(PathBuf, i32, i32)> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            // Development aid: draw one frame to a PNG without opening a window.
            "--render" => {
                let path = args.next().map(PathBuf::from);
                let w = args.next().and_then(|v| v.parse().ok());
                let h = args.next().and_then(|v| v.parse().ok());
                match (path, w, h) {
                    (Some(p), Some(w), Some(h)) => render = Some((p, w, h)),
                    _ => return usage("--render needs PATH WIDTH HEIGHT"),
                }
            }
            "-h" | "--help" => {
                println!("usage: totem-overlay [--render PATH WIDTH HEIGHT]");
                return glib::ExitCode::SUCCESS;
            }
            other => return usage(&format!("unknown argument: {other}")),
        }
    }

    if let Some((path, w, h)) = render {
        return match render_png(&State::new(), &path, w, h) {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("render failed: {e}");
                glib::ExitCode::FAILURE
            }
        };
    }

    glib::set_prgname(Some(APP_ID));
    let app = gtk::Application::builder()
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(build_window);
    // GTK must not see our arguments; they are parsed above.
    app.run_with_args::<&str>(&[])
}

fn usage(msg: &str) -> glib::ExitCode {
    eprintln!("{msg}\nusage: totem-overlay [--render PATH WIDTH HEIGHT]");
    glib::ExitCode::FAILURE
}

fn build_window(app: &gtk::Application) {
    let state = Rc::new(RefCell::new(State::new()));
    let (nat_w, nat_h) = draw::natural_size(&state.borrow().keys);

    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    let st = state.clone();
    area.set_draw_func(move |_, cr, w, h| {
        if let Err(e) = draw::draw(cr, f64::from(w), f64::from(h), &st.borrow().scene()) {
            eprintln!("draw failed: {e}");
        }
    });

    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .title("TOTEM layers")
        .decorated(false)
        .default_width((nat_w * DEFAULT_UNIT_PX).round() as i32)
        .default_height((nat_h * DEFAULT_UNIT_PX).round() as i32)
        .child(&area)
        .build();
    // Let the drawing's own (translucent) background show instead of the theme's.
    let css = gtk::CssProvider::new();
    css.load_from_string("window.totem-overlay { background: transparent; }");
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&win),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    win.add_css_class("totem-overlay");
    win.present();
}

fn render_png(state: &State, path: &PathBuf, w: i32, h: i32) -> Result<(), Box<dyn std::error::Error>> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)?;
    let cr = cairo::Context::new(&surface)?;
    draw::draw(&cr, f64::from(w), f64::from(h), &state.scene())?;
    let mut file = std::fs::File::create(path)?;
    surface.write_to_png(&mut file)?;
    Ok(())
}
