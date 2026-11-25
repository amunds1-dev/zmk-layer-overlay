//! Background thread that reads layer and key reports from the keyboard's Raw HID interface.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use hidapi::{DeviceInfo, HidApi};

use crate::protocol::{self, Report};

const RETRY: Duration = Duration::from_secs(2);
const READ_TIMEOUT_MS: i32 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Connected(String),
    Disconnected,
    Report(Report),
    /// A problem worth showing to the user (no hidapi, no permission, ...).
    Error(String),
}

pub struct Listener {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Listener {
    /// Starts the listener thread. Events arrive on `tx`; the thread ends when the receiver is
    /// dropped or the listener is stopped. `product` filters devices by product-name substring.
    pub fn start(product: Option<String>, tx: async_channel::Sender<Event>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = thread::Builder::new()
            .name("hid-listener".into())
            .spawn(move || run(product.as_deref(), &tx, &flag))
            .expect("spawn hid thread");
        Listener { stop, handle: Some(handle) }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop();
    }
}

fn sleep_unless_stopped(stop: &AtomicBool, d: Duration) {
    let step = Duration::from_millis(100);
    let mut left = d;
    while !stop.load(Ordering::Relaxed) && !left.is_zero() {
        let s = left.min(step);
        thread::sleep(s);
        left -= s;
    }
}

fn run(product: Option<&str>, tx: &async_channel::Sender<Event>, stop: &AtomicBool) {
    let send = |e: Event| tx.send_blocking(e).is_ok();
    let mut api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            send(Event::Error(format!("hidapi: {e}")));
            return;
        }
    };
    let mut last_error = String::new();
    while !stop.load(Ordering::Relaxed) && !tx.is_closed() {
        if let Err(e) = api.refresh_devices() {
            eprintln!("hid: enumerate failed: {e}");
        }
        let Some(info) = find_device(&api, product) else {
            sleep_unless_stopped(stop, RETRY);
            continue;
        };
        match listen(&api, &info, &send, stop) {
            Ok(()) => last_error.clear(),
            Err(msg) => {
                // Report each distinct failure once, not every retry.
                if msg != last_error {
                    eprintln!("hid: {msg}");
                    send(Event::Error(msg.clone()));
                    last_error = msg;
                }
            }
        }
        send(Event::Disconnected);
        sleep_unless_stopped(stop, RETRY);
    }
}

/// The Raw HID interface to use: matching usage page/usage and product name, USB before Bluetooth.
fn find_device(api: &HidApi, product: Option<&str>) -> Option<DeviceInfo> {
    let product = product.map(str::to_lowercase);
    let mut found: Vec<&DeviceInfo> = api
        .device_list()
        .filter(|d| d.usage_page() == protocol::USAGE_PAGE && d.usage() == protocol::USAGE)
        .filter(|d| match &product {
            Some(p) => d.product_string().unwrap_or("").to_lowercase().contains(p),
            None => true,
        })
        .collect();
    found.sort_by_key(|d| matches!(d.bus_type(), hidapi::BusType::Bluetooth));
    found.first().map(|d| (*d).clone())
}

fn listen(
    api: &HidApi,
    info: &DeviceInfo,
    send: &dyn Fn(Event) -> bool,
    stop: &AtomicBool,
) -> Result<(), String> {
    let path = info.path().to_string_lossy().into_owned();
    let dev = info
        .open_device(api)
        .map_err(|e| format!("cannot open {path} ({e}); is the udev rule installed? See README"))?;
    let label = info.product_string().unwrap_or("keyboard").to_string();
    if !send(Event::Connected(label)) {
        return Ok(());
    }
    dev.write(&protocol::query_report()).map_err(|e| format!("query failed: {e}"))?;
    let mut buf = [0u8; 64];
    while !stop.load(Ordering::Relaxed) {
        let n = match dev.read_timeout(&mut buf, READ_TIMEOUT_MS) {
            Ok(n) => n,
            Err(e) => {
                // Unplugged or out of Bluetooth range: a disconnect, not an error to show.
                eprintln!("hid: lost keyboard: {e}");
                return Ok(());
            }
        };
        if n == 0 {
            continue;
        }
        if let Some(report) = protocol::parse_report(&buf[..n])
            && !send(Event::Report(report))
        {
            return Ok(());
        }
    }
    Ok(())
}
