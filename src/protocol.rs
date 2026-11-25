//! Raw HID report protocol. `firmware/src/layer_report.c` is the source of truth.

pub const MAGIC: u8 = 0x4C;
pub const TYPE_LAYER: u8 = 0x01;
pub const TYPE_KEYS: u8 = 0x02;
pub const QUERY: u8 = 0x3F;
pub const REPORT_LEN: usize = 32;
const NAME_OFFSET: usize = 8;

/// Usage page / usage of zzeneg/zmk-raw-hid (QMK's raw HID defaults, so QMK boards match too).
pub const USAGE_PAGE: u16 = 0xFF60;
pub const USAGE: u16 = 0x61;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayerState {
    /// Position of the highest active layer in the keymap.
    pub index: u8,
    /// Stable id of that layer (differs from `index` after Studio reorders layers).
    pub layer_id: u8,
    /// Bitmask of all active layer ids.
    pub mask: u32,
    /// Layer display name, often empty.
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyState {
    /// Bitmask of pressed key positions 0..63, both halves.
    pub mask: u64,
}

impl KeyState {
    pub fn is_pressed(&self, position: usize) -> bool {
        position < 64 && self.mask >> position & 1 == 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Layer(LayerState),
    Keys(KeyState),
}

/// Decodes a keyboard -> host report, or `None` if it is not one of ours.
pub fn parse_report(data: &[u8]) -> Option<Report> {
    if data.len() < NAME_OFFSET + 2 || data[0] != MAGIC {
        return None;
    }
    match data[1] {
        TYPE_LAYER => {
            let raw = &data[NAME_OFFSET..data.len().min(REPORT_LEN)];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            Some(Report::Layer(LayerState {
                index: data[2],
                layer_id: data[3],
                mask: u32::from_le_bytes(data[4..8].try_into().ok()?),
                name: String::from_utf8_lossy(&raw[..end]).into_owned(),
            }))
        }
        TYPE_KEYS => {
            let bytes: [u8; 8] = data.get(2..10)?.try_into().ok()?;
            Some(Report::Keys(KeyState { mask: u64::from_le_bytes(bytes) }))
        }
        _ => None,
    }
}

/// Host -> keyboard request for the current state, with hidapi's leading report id (0).
pub fn query_report() -> [u8; REPORT_LEN + 1] {
    let mut r = [0u8; REPORT_LEN + 1];
    r[1] = MAGIC;
    r[2] = QUERY;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; REPORT_LEN];
        r[0] = MAGIC;
        r[1] = kind;
        r[2..2 + payload.len()].copy_from_slice(payload);
        r
    }

    #[test]
    fn parses_layer_report() {
        let mut payload = vec![2, 5, 0b100101, 0, 0, 0x80];
        payload.extend(b"NAV");
        let r = report(TYPE_LAYER, &payload);
        assert_eq!(
            parse_report(&r),
            Some(Report::Layer(LayerState {
                index: 2,
                layer_id: 5,
                mask: 0x8000_0025,
                name: "NAV".into()
            }))
        );
    }

    #[test]
    fn layer_name_fills_whole_report() {
        let mut payload = vec![1, 1, 2, 0, 0, 0];
        payload.extend([b'x'; 24]);
        let Some(Report::Layer(l)) = parse_report(&report(TYPE_LAYER, &payload)) else { panic!() };
        assert_eq!(l.name.len(), 24);
    }

    #[test]
    fn parses_key_report() {
        let mask: u64 = 1 | 1 << 20 | 1 << 37 | 1 << 63;
        let Some(Report::Keys(k)) = parse_report(&report(TYPE_KEYS, &mask.to_le_bytes())) else {
            panic!()
        };
        assert_eq!(k.mask, mask);
        assert!(k.is_pressed(0) && k.is_pressed(37) && k.is_pressed(63));
        assert!(!k.is_pressed(1) && !k.is_pressed(64));
    }

    #[test]
    fn ignores_foreign_reports() {
        assert_eq!(parse_report(&[]), None);
        assert_eq!(parse_report(&[0u8; 32]), None);
        assert_eq!(parse_report(&report(0x7F, &[])), None);
        assert_eq!(parse_report(&[MAGIC, TYPE_KEYS, 1, 2]), None);
    }

    #[test]
    fn query_has_report_id_and_full_length() {
        let q = query_report();
        assert_eq!(q.len(), 33);
        assert_eq!(&q[..3], &[0, MAGIC, QUERY]);
        assert!(q[3..].iter().all(|&b| b == 0));
    }
}
