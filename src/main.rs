mod config;
mod draw;
mod hid;
mod layout;
mod overlay;
mod protocol;
mod settings;
mod state;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{cairo, gdk, gio, glib};

use config::{Config, Show};
use overlay::{Mode, Position};

/// Wayland app id of the keyboard window, for compositor window rules.
pub const APP_ID: &str = "totem-overlay";
/// GApplication id: makes the app single-instance, so `totem-overlay --settings` reaches the
/// running instance. App ids without a dot are invalid here, hence the separate name.
const GAPP_ID: &str = "io.github.amunds1_dev.TotemOverlay";
/// Default window size: pixels per key unit.
const DEFAULT_UNIT_PX: f64 = 56.0;
const SAVE_DELAY: Duration = Duration::from_millis(500);
const FRAME: Duration = Duration::from_millis(16);

const USAGE: &str = "usage: totem-overlay [options]

  --settings          open the settings window (also reaches an already running instance)
  --mode MODE         window: normal window the compositor manages;
                      overlay: always-on-top, click-through layer-shell surface
  --show WHEN         always, or layers: only while a non-base layer is active
  --position POS      overlay anchor: top, bottom, left, right, top-left, top-right,
                      bottom-left, bottom-right, center
  --margin PX         overlay distance from the anchored edges
  --scale FACTOR      overlay size factor
  --product NAME      only use Raw HID devices whose product name contains NAME (\"\" for any)
  --config PATH       settings file (default: ~/.config/totem-overlay/config.toml)
  --demo              fake layer changes and key presses instead of reading the keyboard
  -v, --verbose       print keyboard events
  --render PATH W H   draw one frame to a PNG and exit (development aid)

--mode, --show, --position, --margin, --scale and --product override the settings file for
this run only.";

/// Command-line values that override the settings file for one run.
#[derive(Debug, Clone, Default, PartialEq)]
struct Overrides {
    mode: Option<Mode>,
    show: Option<Show>,
    position: Option<Position>,
    margin: Option<i32>,
    scale: Option<f64>,
    product: Option<String>,
}

impl Overrides {
    fn apply(&self, c: &mut Config) {
        if let Some(v) = self.mode {
            c.mode = v;
        }
        if let Some(v) = self.show {
            c.show = v;
        }
        if let Some(v) = self.position {
            c.position = v;
        }
        if let Some(v) = self.margin {
            c.margin = v;
        }
        if let Some(v) = self.scale {
            c.scale = v;
        }
        if let Some(v) = &self.product {
            c.product = v.clone();
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Options {
    overrides: Overrides,
    config: Option<PathBuf>,
    settings: bool,
    demo: bool,
    verbose: bool,
    render: Option<(PathBuf, i32, i32)>,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut o = Options::default();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        let ov = &mut o.overrides;
        match arg.as_str() {
            "--settings" => o.settings = true,
            "--mode" => ov.mode = Some(value("--mode")?.parse()?),
            "--show" => ov.show = Some(value("--show")?.parse()?),
            "--position" => ov.position = Some(value("--position")?.parse()?),
            "--margin" => {
                let m: i32 = value("--margin")?.parse().map_err(|_| "--margin needs a number of pixels")?;
                ov.margin = Some(m.clamp(config::MARGIN_RANGE.0, config::MARGIN_RANGE.1));
            }
            "--scale" => {
                let s = value("--scale")?
                    .parse()
                    .ok()
                    .filter(|s: &f64| *s > 0.0 && s.is_finite())
                    .ok_or("--scale needs a positive number")?;
                ov.scale = Some(s.clamp(config::SCALE_RANGE.0, config::SCALE_RANGE.1));
            }
            "--product" => ov.product = Some(value("--product")?),
            "--config" => o.config = Some(PathBuf::from(value("--config")?)),
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

/// What gets drawn.
struct View {
    keys: Vec<layout::Key>,
    labels: Vec<String>,
    keyboard: state::Keyboard,
    /// Show/hide fade, 0..1.
    alpha: f64,
    /// Problem with the settings file, shown until something is saved.
    notice: Option<String>,
}

impl View {
    fn new() -> Self {
        let keys = layout::totem();
        // No keymap source yet (milestones 5/6): label keys with their position index.
        let labels = (0..keys.len()).map(|i| i.to_string()).collect();
        View { keys, labels, keyboard: state::Keyboard::default(), alpha: 1.0, notice: None }
    }

    fn draw(&self, cr: &cairo::Context, w: f64, h: f64, config: &Config) -> Result<(), cairo::Error> {
        let (title, mut subtitle) = self.keyboard.header();
        if subtitle.is_empty()
            && let Some(n) = &self.notice
        {
            subtitle = n.clone();
        }
        let scene = draw::Scene {
            keys: &self.keys,
            title: &title,
            subtitle: &subtitle,
            labels: &self.labels,
            pressed: self.keyboard.keys,
            bg_alpha: config.background_opacity * self.alpha,
            key_alpha: config.key_opacity * self.alpha,
        };
        draw::draw(cr, w, h, &scene)
    }
}

struct App {
    gapp: gtk::Application,
    config_path: PathBuf,
    /// What is saved: the file contents plus edits from the settings UI.
    file_config: RefCell<Config>,
    /// What is used: `file_config` with command-line overrides on top.
    config: RefCell<Config>,
    view: RefCell<View>,
    window: RefCell<Option<(gtk::ApplicationWindow, gtk::DrawingArea)>>,
    settings_window: RefCell<Option<gtk::Window>>,
    fade: RefCell<Option<glib::SourceId>>,
    save: RefCell<Option<glib::SourceId>>,
    listener: RefCell<Option<hid::Listener>>,
    _hold: gio::ApplicationHoldGuard,
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let opts = match parse_args(args.iter().skip(1).cloned()) {
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
        let mut config = config::load(&opts.config.clone().unwrap_or_else(config::default_path)).0;
        opts.overrides.apply(&mut config);
        return match render_png(&View::new(), &config, path, *w, *h) {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("render failed: {e}");
                glib::ExitCode::FAILURE
            }
        };
    }

    glib::set_prgname(Some(APP_ID));
    let gapp = gtk::Application::builder()
        .application_id(GAPP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let running: Rc<RefCell<Option<Rc<App>>>> = Rc::default();
    gapp.connect_command_line(move |gapp, cmdline| {
        let args = cmdline.arguments().into_iter().skip(1).map(|a| a.to_string_lossy().into_owned());
        let opts = match parse_args(args) {
            Ok(Some(o)) => o,
            Ok(None) => Options::default(),
            Err(msg) => {
                cmdline.printerr_literal(&format!("{msg}\n"));
                return glib::ExitCode::FAILURE;
            }
        };
        if let Some(app) = running.borrow().as_ref() {
            // A second `totem-overlay`, e.g. from the launcher or a keybinding.
            if opts.overrides != Overrides::default() || opts.demo || opts.config.is_some() {
                cmdline.printerr_literal("totem-overlay is already running; opening its settings\n");
            }
            open_settings(app);
            return glib::ExitCode::SUCCESS;
        }
        let app = start(gapp, &opts);
        if opts.settings {
            open_settings(&app);
        }
        *running.borrow_mut() = Some(app);
        glib::ExitCode::SUCCESS
    });
    gapp.run_with_args(&args)
}

fn start(gapp: &gtk::Application, opts: &Options) -> Rc<App> {
    let config_path = opts.config.clone().unwrap_or_else(config::default_path);
    let (file_config, notice) = config::load(&config_path);
    if let Some(n) = &notice {
        eprintln!("{n}");
    }
    let mut config = file_config.clone();
    opts.overrides.apply(&mut config);

    let mut view = View::new();
    view.notice = notice;
    view.alpha = if view.keyboard.wants_visible(config.show) { 1.0 } else { 0.0 };

    let (tx, rx) = async_channel::unbounded::<hid::Event>();
    let listener = if opts.demo {
        start_demo(tx, view.keys.len());
        None
    } else {
        Some(hid::Listener::start(config.product_filter(), tx))
    };

    let app = Rc::new(App {
        gapp: gapp.clone(),
        config_path,
        file_config: RefCell::new(file_config),
        config: RefCell::new(config),
        view: RefCell::new(view),
        window: RefCell::default(),
        settings_window: RefCell::default(),
        fade: RefCell::default(),
        save: RefCell::default(),
        listener: RefCell::new(listener),
        _hold: gapp.hold(),
    });

    let a = app.clone();
    gapp.connect_shutdown(move |_| {
        if let Some(mut l) = a.listener.take() {
            l.stop();
        }
        flush_save(&a);
    });

    let verbose = opts.verbose;
    let a = app.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = rx.recv().await {
            if verbose {
                eprintln!("{event:?}");
            }
            let changed = a.view.borrow_mut().keyboard.apply(event);
            if changed {
                queue_draw(&a);
                update_visibility(&a);
            }
        }
    });

    build_window(&app);
    app
}

fn queue_draw(app: &App) {
    if let Some((_, area)) = app.window.borrow().as_ref() {
        area.queue_draw();
    }
}

/// Window size in pixels for the current config.
fn window_size(app: &App) -> (i32, i32) {
    let c = app.config.borrow();
    let (nat_w, nat_h) = draw::natural_size(&app.view.borrow().keys);
    let scale = if c.mode == Mode::Overlay { c.scale } else { 1.0 };
    let px = |v: f64| (v * DEFAULT_UNIT_PX * scale).round() as i32;
    (px(nat_w), px(nat_h))
}

/// (Re)creates the keyboard window for the current mode.
fn build_window(app: &Rc<App>) {
    let config = app.config.borrow().clone();
    let (width, height) = window_size(app);

    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    let a = app.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let config = a.config.borrow();
        if let Err(e) = a.view.borrow().draw(cr, f64::from(w), f64::from(h), &config) {
            eprintln!("draw failed: {e}");
        }
    });

    let win = gtk::ApplicationWindow::builder()
        .application(&app.gapp)
        .title("TOTEM layers")
        .decorated(false)
        .default_width(width)
        .default_height(height)
        .child(&area)
        .build();
    win.add_css_class("totem-overlay");
    // Let the drawing's own (translucent) background show instead of the theme's.
    let css = gtk::CssProvider::new();
    css.load_from_string("window.totem-overlay { background: transparent; }");
    gtk::style_context_add_provider_for_display(
        &gtk::prelude::WidgetExt::display(&win),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let overlay = config.mode == Mode::Overlay && overlay::setup(&win, config.position, config.margin);
    if overlay {
        // A layer surface takes its size from the content, not the default size.
        area.set_content_width(width);
        area.set_content_height(height);
    } else {
        if config.mode == Mode::Overlay {
            eprintln!("overlay mode needs wlr-layer-shell, which this compositor lacks; using a window");
        }
        // GTK sets the Wayland app id from the GApplication id while showing the window;
        // window rules expect `totem-overlay`.
        win.connect_map(|w| {
            if let Some(top) = w.surface().and_downcast::<gdk_wayland::WaylandToplevel>() {
                top.set_application_id(APP_ID);
            }
        });
        add_settings_popover(app, &area);
    }

    let a = app.clone();
    win.connect_close_request(move |_| {
        a.gapp.quit();
        glib::Propagation::Proceed
    });

    let old = app.window.replace(Some((win.clone(), area)));
    win.present();
    if let Some((old_win, _)) = old {
        // Don't let closing the old window quit the app.
        old_win.set_application(None::<&gtk::Application>);
        old_win.destroy();
    }
}

/// Right-click on the keyboard window opens the settings in a popover.
fn add_settings_popover(app: &Rc<App>, area: &gtk::DrawingArea) {
    let click = gtk::GestureClick::builder().button(gdk::BUTTON_SECONDARY).build();
    let a = app.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        let Some(area) = gesture.widget() else { return };
        let popover = gtk::Popover::new();
        popover.set_child(Some(&settings_widget(&a)));
        popover.set_parent(&area);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        popover.popup();
    });
    area.add_controller(click);
}

fn settings_widget(app: &Rc<App>) -> gtk::Box {
    let a = app.clone();
    let on_change: Rc<dyn Fn(settings::Change)> = Rc::new(move |change| apply_change(&a, change));
    let a = app.clone();
    settings::build(&app.config.borrow(), on_change, move || a.gapp.quit())
}

/// The settings window, for overlay mode (the overlay itself can't be clicked).
fn open_settings(app: &Rc<App>) {
    if let Some(w) = app.settings_window.borrow().as_ref() {
        w.present();
        return;
    }
    let win = gtk::Window::builder()
        .application(&app.gapp)
        .title("TOTEM overlay settings")
        .resizable(false)
        .child(&settings_widget(app))
        .build();
    let a = app.clone();
    win.connect_close_request(move |_| {
        a.settings_window.take();
        glib::Propagation::Proceed
    });
    *app.settings_window.borrow_mut() = Some(win.clone());
    win.present();
}

fn apply_change(app: &Rc<App>, change: settings::Change) {
    let old = app.config.borrow().clone();
    change(&mut app.config.borrow_mut());
    change(&mut app.file_config.borrow_mut());
    let new = app.config.borrow().clone();
    if new == old {
        return;
    }
    schedule_save(app);
    app.view.borrow_mut().notice = None;

    if new.mode != old.mode {
        // Rebuild outside the signal handler: the popover that sent this lives in the old window.
        let a = app.clone();
        glib::idle_add_local_once(move || {
            build_window(&a);
            if a.config.borrow().mode == Mode::Overlay {
                // The overlay can't be clicked; keep the settings reachable.
                open_settings(&a);
            }
        });
        return;
    }
    if new.mode == Mode::Overlay {
        let (w, h) = window_size(app);
        if let Some((win, area)) = app.window.borrow().as_ref() {
            overlay::place(win, new.position, new.margin);
            area.set_content_width(w);
            area.set_content_height(h);
        }
    }
    update_visibility(app);
    queue_draw(app);
}

/// Starts (or retargets) the show/hide fade for the current state and show policy.
fn update_visibility(app: &Rc<App>) {
    if app.fade.borrow().is_some() {
        return; // the running fade reads the target every frame
    }
    let a = app.clone();
    let target = move || {
        let show = a.config.borrow().show;
        if a.view.borrow().keyboard.wants_visible(show) { 1.0 } else { 0.0 }
    };
    if app.view.borrow().alpha == target() {
        return;
    }
    let a = app.clone();
    let id = glib::timeout_add_local(FRAME, move || {
        let t = target();
        let alpha = {
            let mut v = a.view.borrow_mut();
            v.alpha = state::fade_step(v.alpha, t);
            v.alpha
        };
        queue_draw(&a);
        if alpha == t {
            a.fade.take();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    *app.fade.borrow_mut() = Some(id);
}

fn schedule_save(app: &Rc<App>) {
    if let Some(id) = app.save.take() {
        id.remove();
    }
    let a = app.clone();
    let id = glib::timeout_add_local_once(SAVE_DELAY, move || {
        a.save.take();
        write_config(&a);
    });
    *app.save.borrow_mut() = Some(id);
}

/// Writes a pending save now (on quit).
fn flush_save(app: &App) {
    if let Some(id) = app.save.take() {
        id.remove();
        write_config(app);
    }
}

fn write_config(app: &App) {
    if let Err(e) = config::save(&app.config_path, &app.file_config.borrow()) {
        eprintln!("cannot save {}: {e}", app.config_path.display());
    }
}

/// Feeds fake events through the same path as the HID listener.
fn start_demo(tx: async_channel::Sender<hid::Event>, key_count: usize) {
    use protocol::{KeyState, LayerState, Report};
    let _ = tx.send_blocking(hid::Event::Connected("demo".into()));
    // Layers 0, 1, 2, 3, then a pause on the base layer.
    let step = Cell::new(0u8);
    let tx2 = tx.clone();
    glib::timeout_add_local(Duration::from_millis(1500), move || {
        let s = (step.get() + 1) % 5;
        step.set(s);
        let i = if s == 4 { 0 } else { s };
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

fn render_png(view: &View, config: &Config, path: &PathBuf, w: i32, h: i32) -> Result<(), Box<dyn std::error::Error>> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)?;
    let cr = cairo::Context::new(&surface)?;
    view.draw(&cr, f64::from(w), f64::from(h), config)?;
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
    fn no_args_overrides_nothing() {
        let o = parse(&[]).unwrap().unwrap();
        assert_eq!(o.overrides, Overrides::default());
        let mut c = Config::default();
        o.overrides.apply(&mut c);
        assert_eq!(c, Config::default());
    }

    #[test]
    fn overrides_apply_on_top_of_config() {
        let o = parse(&[
            "--mode", "overlay", "--position", "top-right", "--margin", "12", "--scale", "0.75", "--show",
            "layers", "--product", "",
        ])
        .unwrap()
        .unwrap();
        let mut c = Config { key_opacity: 0.5, ..Config::default() };
        o.overrides.apply(&mut c);
        assert_eq!(c.mode, Mode::Overlay);
        assert_eq!(c.position, Position::TopRight);
        assert_eq!((c.margin, c.scale, c.show), (12, 0.75, Show::Layers));
        assert_eq!(c.product_filter(), None);
        assert_eq!(c.key_opacity, 0.5);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let o = parse(&["--scale", "50", "--margin", "-3"]).unwrap().unwrap();
        assert_eq!(o.overrides.scale, Some(config::SCALE_RANGE.1));
        assert_eq!(o.overrides.margin, Some(0));
    }

    #[test]
    fn rejects_bad_args() {
        assert!(parse(&["--mode", "float"]).is_err());
        assert!(parse(&["--show", "sometimes"]).is_err());
        assert!(parse(&["--position"]).is_err());
        assert!(parse(&["--scale", "0"]).is_err());
        assert!(parse(&["--scale", "NaN"]).is_err());
        assert!(parse(&["--margin", "big"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["--render", "x.png", "10"]).is_err());
        assert!(parse(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn settings_flag() {
        assert!(parse(&["--settings"]).unwrap().unwrap().settings);
    }
}
