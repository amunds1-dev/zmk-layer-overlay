//! ZMK Studio RPC client over USB serial (CDC ACM, `/dev/ttyACM*`): reads the keymap, layer
//! names, behaviors and physical layout. Request flow as in zmk-studio-ts-client.

pub mod frame;
pub mod proto;

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use prost::Message;

use crate::keymap::{self, Behavior, Binding, Keymap, Layer, ParamKind, ParamSet, ParamValue};
use crate::layout;
use proto::*;

/// ZMK's default USB VID:PID.
const VID: &str = "1d50";
const PID: &str = "615e";
const PROBE_TIMEOUT: Duration = Duration::from_millis(800);
const CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// Serial ports belonging to a ZMK keyboard, from sysfs. A keyboard may expose several CDC ACM
/// interfaces (Studio, logging); [`fetch`] probes each.
pub fn find_ports() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir("/sys/class/tty") else { return Vec::new() };
    let mut ports: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("ttyACM"))
        .filter(|e| {
            // .../ttyACMn/device is the USB interface; its parent is the USB device.
            let usb = e.path().join("device").join("..");
            let read = |f: &str| fs::read_to_string(usb.join(f)).map(|s| s.trim().to_lowercase());
            read("idVendor").is_ok_and(|v| v == VID) && read("idProduct").is_ok_and(|p| p == PID)
        })
        .map(|e| Path::new("/dev").join(e.file_name()))
        .collect();
    ports.sort();
    ports
}

#[derive(Debug)]
pub enum Error {
    /// No keyboard serial port present (e.g. connected over Bluetooth only).
    NoPort,
    Io(PathBuf, io::Error),
    /// The keyboard didn't answer Studio requests on any port.
    NoStudio,
    Locked,
    Protocol(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoPort => write!(f, "no ZMK serial port found"),
            Error::Io(p, e) if e.kind() == io::ErrorKind::PermissionDenied => write!(
                f,
                "no permission for {}: install the udev rule (see README) or join the uucp group",
                p.display()
            ),
            Error::Io(p, e) => write!(f, "{}: {e}", p.display()),
            Error::NoStudio => write!(f, "keyboard doesn't answer ZMK Studio requests (CONFIG_ZMK_STUDIO=y?)"),
            Error::Locked => write!(f, "keyboard is locked for ZMK Studio: press its &studio_unlock key"),
            Error::Protocol(m) => write!(f, "ZMK Studio: {m}"),
        }
    }
}

struct Client {
    path: PathBuf,
    port: Box<dyn serialport::SerialPort>,
    decoder: frame::Decoder,
    next_id: u32,
}

impl Client {
    fn open(path: &Path) -> Result<Self, Error> {
        let io_err = |e: serialport::Error| Error::Io(path.to_owned(), io::Error::from(e));
        // Baud rate is meaningless for CDC ACM; serialport puts the tty in raw mode.
        let port = serialport::new(path.to_string_lossy(), 115_200)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(io_err)?;
        let _ = port.clear(serialport::ClearBuffer::Input);
        Ok(Client { path: path.to_owned(), port, decoder: frame::Decoder::default(), next_id: 1 })
    }

    fn call(&mut self, subsystem: RequestSubsystem, timeout: Duration) -> Result<ResponseSubsystem, Error> {
        let id = self.next_id;
        self.next_id += 1;
        let req = Request { request_id: id, subsystem: Some(subsystem) };
        let io_err = |p: &Path, e| Error::Io(p.to_owned(), e);
        self.port.write_all(&frame::encode(&req.encode_to_vec())).map_err(|e| io_err(&self.path, e))?;
        self.port.flush().map_err(|e| io_err(&self.path, e))?;

        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 256];
        while Instant::now() < deadline {
            let n = match self.port.read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) => return Err(io_err(&self.path, e)),
            };
            for &b in &buf[..n] {
                let Some(payload) = self.decoder.push(b) else { continue };
                let Ok(resp) = Response::decode(payload.as_slice()) else { continue };
                // Notifications and answers to earlier (timed out) requests are skipped.
                if let Some(ResponseType::RequestResponse(rr)) = resp.r#type
                    && rr.request_id == id
                {
                    return match rr.subsystem {
                        Some(ResponseSubsystem::Meta(MetaResponse {
                            response_type: Some(MetaResponseType::SimpleError(1)),
                        })) => Err(Error::Locked),
                        Some(ResponseSubsystem::Meta(m)) => Err(Error::Protocol(format!("error {m:?}"))),
                        Some(s) => Ok(s),
                        None => Err(Error::Protocol("empty response".into())),
                    };
                }
            }
        }
        Err(Error::Protocol("request timed out".into()))
    }

    fn core(&mut self, r: CoreRequestType, t: Duration) -> Result<CoreResponseType, Error> {
        match self.call(RequestSubsystem::Core(CoreRequest { request_type: Some(r) }), t)? {
            ResponseSubsystem::Core(CoreResponse { response_type: Some(r) }) => Ok(r),
            other => Err(unexpected(other)),
        }
    }

    fn keymap(&mut self, r: KeymapRequestType) -> Result<KeymapResponseType, Error> {
        match self.call(RequestSubsystem::Keymap(KeymapRequest { request_type: Some(r) }), CALL_TIMEOUT)? {
            ResponseSubsystem::Keymap(KeymapResponse { response_type: Some(r) }) => Ok(r),
            other => Err(unexpected(other)),
        }
    }

    fn behaviors(&mut self, r: BehaviorsRequestType) -> Result<BehaviorsResponseType, Error> {
        match self.call(RequestSubsystem::Behaviors(BehaviorsRequest { request_type: Some(r) }), CALL_TIMEOUT)? {
            ResponseSubsystem::Behaviors(BehaviorsResponse { response_type: Some(r) }) => Ok(r),
            other => Err(unexpected(other)),
        }
    }
}

fn unexpected(r: ResponseSubsystem) -> Error {
    Error::Protocol(format!("unexpected response {r:?}"))
}

/// Opens the first port that answers a Studio request; returns it with the device name.
fn connect() -> Result<(Client, String), Error> {
    let ports = find_ports();
    if ports.is_empty() {
        return Err(Error::NoPort);
    }
    let mut first_err = None;
    for path in ports {
        let mut client = match Client::open(&path) {
            Ok(c) => c,
            Err(e) => {
                first_err.get_or_insert(e);
                continue;
            }
        };
        match client.core(CoreRequestType::GetDeviceInfo(true), PROBE_TIMEOUT) {
            Ok(CoreResponseType::GetDeviceInfo(info)) => return Ok((client, info.name)),
            Ok(_) => continue,
            Err(Error::Locked) => return Err(Error::Locked),
            Err(e @ Error::Io(..)) => {
                first_err.get_or_insert(e);
            }
            Err(_) => {} // not the Studio port
        }
    }
    Err(first_err.unwrap_or(Error::NoStudio))
}

/// Reads the whole keymap from the keyboard.
pub fn fetch() -> Result<Keymap, Error> {
    let (mut client, device) = connect()?;

    let KeymapResponseType::GetKeymap(km) = client.keymap(KeymapRequestType::GetKeymap(true))? else {
        return Err(Error::Protocol("bad keymap response".into()));
    };
    let layout = match client.keymap(KeymapRequestType::GetPhysicalLayouts(true)) {
        Ok(KeymapResponseType::GetPhysicalLayouts(pl)) => convert_layout(&pl),
        Ok(_) | Err(Error::Protocol(_)) => Vec::new(),
        Err(e) => return Err(e),
    };

    // Details only for the behaviors the keymap uses.
    let used: BTreeSet<i32> = km.layers.iter().flat_map(|l| l.bindings.iter().map(|b| b.behavior_id)).collect();
    let mut behaviors = Vec::new();
    for id in used.into_iter().filter_map(|id| u32::try_from(id).ok()) {
        let req = BehaviorsRequestType::GetBehaviorDetails(GetBehaviorDetailsRequest { behavior_id: id });
        if let BehaviorsResponseType::GetBehaviorDetails(d) = client.behaviors(req)? {
            behaviors.push(convert_behavior(d));
        }
    }

    Ok(Keymap {
        version: keymap::CACHE_VERSION,
        device,
        layers: km
            .layers
            .into_iter()
            .map(|l| Layer {
                id: l.id,
                name: l.name,
                bindings: l
                    .bindings
                    .into_iter()
                    .map(|b| Binding { behavior: b.behavior_id, param1: b.param1, param2: b.param2 })
                    .collect(),
            })
            .collect(),
        behaviors,
        layout,
    })
}

fn convert_layout(pl: &PhysicalLayouts) -> Vec<layout::Key> {
    let Some(active) = pl.layouts.get(pl.active_layout_index as usize) else { return Vec::new() };
    active.keys.iter().map(|k| layout::Key::from_zmk(k.width, k.height, k.x, k.y, k.r, k.rx, k.ry)).collect()
}

fn convert_behavior(d: GetBehaviorDetailsResponse) -> Behavior {
    let values = |v: Vec<BehaviorParameterValueDescription>| -> Vec<ParamValue> {
        v.into_iter()
            .map(|p| ParamValue {
                kind: match p.value_type {
                    None | Some(ValueType::Nil(_)) => ParamKind::Nil,
                    Some(ValueType::Constant(c)) => ParamKind::Constant(c),
                    Some(ValueType::Range(r)) => ParamKind::Range { min: r.min, max: r.max },
                    Some(ValueType::HidUsage(_)) => ParamKind::HidUsage,
                    Some(ValueType::LayerId(_)) => ParamKind::LayerId,
                },
                name: p.name,
            })
            .collect()
    };
    Behavior {
        id: d.id,
        name: d.display_name,
        params: d.metadata.into_iter().map(|s| ParamSet { param1: values(s.param1), param2: values(s.param2) }).collect(),
        bindings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_uses_the_active_physical_layout() {
        let key = |x| KeyPhysicalAttrs { width: 100, height: 100, x, y: 0, r: -1000, rx: 50, ry: 50 };
        let pl = PhysicalLayouts {
            active_layout_index: 1,
            layouts: vec![
                PhysicalLayout { name: "a".into(), keys: vec![key(0)] },
                PhysicalLayout { name: "b".into(), keys: vec![key(0), key(100)] },
            ],
        };
        let keys = convert_layout(&pl);
        assert_eq!(keys.len(), 2);
        assert_eq!((keys[1].x, keys[1].rot, keys[1].rx), (1.0, -10.0, 0.5));
        assert!(convert_layout(&PhysicalLayouts { active_layout_index: 5, layouts: vec![] }).is_empty());
    }

    #[test]
    fn behavior_metadata_is_converted() {
        let d = GetBehaviorDetailsResponse {
            id: 7,
            display_name: "Bluetooth".into(),
            metadata: vec![BehaviorBindingParametersSet {
                param1: vec![BehaviorParameterValueDescription {
                    name: "Select".into(),
                    value_type: Some(ValueType::Constant(3)),
                }],
                param2: vec![BehaviorParameterValueDescription {
                    name: "Profile".into(),
                    value_type: Some(ValueType::Range(ValueRange { min: 0, max: 4 })),
                }],
            }],
        };
        let b = convert_behavior(d);
        assert_eq!(b.name, "Bluetooth");
        assert_eq!(b.params[0].param1[0].kind, ParamKind::Constant(3));
        assert_eq!(b.params[0].param2[0].kind, ParamKind::Range { min: 0, max: 4 });
    }
}
