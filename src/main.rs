mod draw;
mod hid;
mod layout;
mod overlay;
mod protocol;
mod state;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{cairo, gio, glib};

/// Wayland app id (xdg toplevel). Not a valid GApplication id (no dot), so it is set as the
/// program name, which GTK uses for the app id when the GtkApplication has none.
pub const APP_ID: &str = "totem-overlay";
/// Default window size: pixels per key unit.
const DEFAULT_UNIT_PX: f64 = 56.0;

const USAGE: &str = "usage: totem-overlay [options]

  --mode MODE         window (default): normal window the compositor manages;
                      overlay: always-on-top, click-through layer-shell surface
  --position POS      overlay anchor: top, bottom (default), left, right, top-left,
                      top-right, bottom-left, bottom-right, center
  --margin PX         overlay distance from the anchored edges (default: 40)
  --scale FACTOR      overlay size factor (default: 1.0)
  --product NAME      only use Raw HID devices whose product name contains NAME
                      (default: TOTEM; \"\" for any)
  --demo              fake layer changes and key presses instead of reading the keyboard
  -v, --verbose       print keyboard events
  --render PATH W H   draw one frame to a PNG and exit (development aid)";

#[derive(Debug, Clone)]
struct Options {
    mode: overlay::Mode,
    position: overlay::Position,
    margin: i32,
    scale: f64,
    product: Option<String>,
    demo: bool,
    verbose: bool,
    render: Option<(PathBuf, i32, i32)>,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut o = Options {
        mode: overlay::Mode::default(),
        position: overlay::Position::default(),
        margin: 40,
        scale: 1.0,
        product: Some("TOTEM".into()),
        demo: false,
        verbose: false,
        render: None,
    };
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--mode" => o.mode = value("--mode")?.parse()?,
            "--position" => o.position = value("--position")?.parse()?,
            "--margin" => {
                o.margin = value("--margin")?.parse().map_err(|_| "--margin needs a number of pixels")?
            }
            "--scale" => {
                o.scale = value("--scale")?
                    .parse()
                    .ok()
                    .filter(|s: &f64| *s > 0.0 && s.is_finite())
                    .ok_or("--scale needs a positive number")?
            }
            "--product" => {
                let p = value("--product")?;
                o.product = (!p.is_empty()).then_some(p);
            }
            "--demo" => o.demo = true,
            "-v" | "--verbose" => o.verbose = true,
            "--render" => {
                let path = args.next().map(PathBuf::from);
                let w = args.next().and_then(|v| v.parse().ok());
                let h = args.next().and_then(|v| v.parse().ok());
                match (path, w, h) {
                    (Some(p), Some(w), Some(h)) => o.render = Some((p, w, h)),
                    _ => return Err("--render needs PATH WIDTH HEIGHT".into()),
                }
            }
            "-h" | "--help" => return Ok(None),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Some(o))
}

struct State {
    keys: Vec<layout::Key>,
    labels: Vec<String>,
    keyboard: state::Keyboard,
    bg_alpha: f64,
    key_alpha: f64,
}

impl State {
    fn new() -> Self {
        let keys = layout::totem();
        // No keymap source yet (milestones 5/6): label keys with their position index.
        let labels = (0..keys.len()).map(|i| i.to_string()).collect();
        State { keys, labels, keyboard: state::Keyboard::default(), bg_alpha: 0.88, key_alpha: 1.0 }
    }

    fn draw(&self, cr: &cairo::Context, w: f64, h: f64) -> Result<(), cairo::Error> {
        let (title, subtitle) = self.keyboard.header();
        let scene = draw::Scene {
            keys: &self.keys,
            title: &title,
            subtitle: &subtitle,
            labels: &self.labels,
            pressed: self.keyboard.keys,
            bg_alpha: self.bg_alpha,
            key_alpha: self.key_alpha,
        };
        draw::draw(cr, w, h, &scene)
    }
}

fn main() -> glib::ExitCode {
    let opts = match parse_args(std::env::args().skip(1)) {
        Ok(Some(o)) => o,
        Ok(None) => {
            println!("{USAGE}");
            return glib::ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("{msg}\n{USAGE}");
            return glib::ExitCode::FAILURE;
        }
    };

    if let Some((path, w, h)) = &opts.render {
        return match render_png(&State::new(), path, *w, *h) {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("render failed: {e}");
                glib::ExitCode::FAILURE
            }
        };
    }

    glib::set_prgname(Some(APP_ID));
    let app = gtk::Application::builder().flags(gio::ApplicationFlags::NON_UNIQUE).build();
    app.connect_activate(move |app| build_window(app, &opts));
    // GTK must not see our arguments; they are parsed above.
    app.run_with_args::<&str>(&[])
}

fn build_window(app: &gtk::Application, opts: &Options) {
    let state = Rc::new(RefCell::new(State::new()));
    let (nat_w, nat_h) = draw::natural_size(&state.borrow().keys);

    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    let st = state.clone();
    area.set_draw_func(move |_, cr, w, h| {
        if let Err(e) = st.borrow().draw(cr, f64::from(w), f64::from(h)) {
            eprintln!("draw failed: {e}");
        }
    });

    let scale = if opts.mode == overlay::Mode::Overlay { opts.scale } else { 1.0 };
    let width = (nat_w * DEFAULT_UNIT_PX * scale).round() as i32;
    let height = (nat_h * DEFAULT_UNIT_PX * scale).round() as i32;
    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .title("TOTEM layers")
        .decorated(false)
        .default_width(width)
        .default_height(height)
        .child(&area)
        .build();
    if opts.mode == overlay::Mode::Overlay {
        if overlay::setup(&win, opts.position, opts.margin) {
            // A layer surface takes its size from the content, not the default size.
            area.set_content_width(width);
            area.set_content_height(height);
        } else {
            eprintln!("overlay mode needs wlr-layer-shell, which this compositor lacks; using a window");
        }
    }
    // Let the drawing's own (translucent) background show instead of the theme's.
    let css = gtk::CssProvider::new();
    css.load_from_string("window.totem-overlay { background: transparent; }");
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&win),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    win.add_css_class("totem-overlay");

    let (tx, rx) = async_channel::unbounded::<hid::Event>();
    if opts.demo {
        start_demo(tx, state.borrow().keys.len());
    } else {
        let listener = RefCell::new(Some(hid::Listener::start(opts.product.clone(), tx)));
        app.connect_shutdown(move |_| {
            if let Some(mut l) = listener.take() {
                l.stop();
            }
        });
    }

    let verbose = opts.verbose;
    glib::spawn_future_local(async move {
        while let Ok(event) = rx.recv().await {
            if verbose {
                eprintln!("{event:?}");
            }
            if state.borrow_mut().keyboard.apply(event) {
                area.queue_draw();
            }
        }
    });

    win.present();
}

/// Feeds fake events through the same path as the HID listener.
fn start_demo(tx: async_channel::Sender<hid::Event>, key_count: usize) {
    use protocol::{KeyState, LayerState, Report};
    let _ = tx.send_blocking(hid::Event::Connected("demo".into()));
    let layer = Cell::new(0u8);
    let tx2 = tx.clone();
    glib::timeout_add_local(Duration::from_millis(1500), move || {
        let i = (layer.get() + 1) % 4;
        layer.set(i);
        let state = LayerState { index: i, layer_id: i, mask: 1 | 1 << i, name: String::new() };
        glib::ControlFlow::from(tx2.send_blocking(hid::Event::Report(Report::Layer(state))).is_ok())
    });
    let key = Cell::new(0usize);
    glib::timeout_add_local(Duration::from_millis(180), move || {
        let k = key.get();
        key.set((k + 1) % key_count);
        let keys = KeyState { mask: 1 << k };
        glib::ControlFlow::from(tx.send_blocking(hid::Event::Report(Report::Keys(keys))).is_ok())
    });
}

fn render_png(state: &State, path: &PathBuf, w: i32, h: i32) -> Result<(), Box<dyn std::error::Error>> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)?;
    let cr = cairo::Context::new(&surface)?;
    state.draw(&cr, f64::from(w), f64::from(h))?;
    let mut file = std::fs::File::create(path)?;
    surface.write_to_png(&mut file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn default_product_filter_is_totem() {
        assert_eq!(parse(&[]).unwrap().unwrap().product.as_deref(), Some("TOTEM"));
    }

    #[test]
    fn empty_product_matches_any() {
        assert_eq!(parse(&["--product", ""]).unwrap().unwrap().product, None);
    }

    #[test]
    fn parses_overlay_options() {
        let o = parse(&["--mode", "overlay", "--position", "top-right", "--margin", "12", "--scale", "0.75"])
            .unwrap()
            .unwrap();
        assert_eq!(o.mode, overlay::Mode::Overlay);
        assert_eq!(o.position, overlay::Position::TopRight);
        assert_eq!((o.margin, o.scale), (12, 0.75));
    }

    #[test]
    fn rejects_bad_overlay_options() {
        assert!(parse(&["--mode", "float"]).is_err());
        assert!(parse(&["--position"]).is_err());
        assert!(parse(&["--scale", "0"]).is_err());
        assert!(parse(&["--scale", "NaN"]).is_err());
        assert!(parse(&["--margin", "big"]).is_err());
    }

    #[test]
    fn rejects_unknown_and_incomplete_args() {
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["--render", "x.png", "10"]).is_err());
        assert!(parse(&["--help"]).unwrap().is_none());
    }
}
