//! ZMK Studio RPC framing over a byte stream (serial / BLE), as in zmk-studio-ts-client
//! `src/framing.ts`: each message is SOF, payload with SOF/ESC/EOF bytes escaped by a
//! preceding ESC, then EOF.

pub const SOF: u8 = 0xAB;
pub const ESC: u8 = 0xAC;
pub const EOF: u8 = 0xAD;

pub fn encode(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(SOF);
    for &b in payload {
        if matches!(b, SOF | ESC | EOF) {
            out.push(ESC);
        }
        out.push(b);
    }
    out.push(EOF);
    out
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Idle,
    Data,
    Escaped,
}

/// Incremental decoder: feed bytes as they arrive, get complete payloads out.
#[derive(Debug, Default)]
pub struct Decoder {
    state: State,
    data: Vec<u8>,
}

impl Decoder {
    /// Feeds one byte; returns a payload when it completes a frame. Bytes outside a frame and
    /// a SOF in the middle of a frame (start of a new frame) resynchronise instead of failing,
    /// since the port may hold leftovers from an earlier session.
    pub fn push(&mut self, b: u8) -> Option<Vec<u8>> {
        match self.state {
            State::Idle => {
                if b == SOF {
                    self.state = State::Data;
                    self.data.clear();
                }
            }
            State::Data => match b {
                SOF => self.data.clear(),
                ESC => self.state = State::Escaped,
                EOF => {
                    self.state = State::Idle;
                    return Some(std::mem::take(&mut self.data));
                }
                _ => self.data.push(b),
            },
            State::Escaped => {
                self.data.push(b);
                self.state = State::Data;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut d = Decoder::default();
        bytes.iter().filter_map(|&b| d.push(b)).collect()
    }

    #[test]
    fn escapes_special_bytes() {
        assert_eq!(encode(&[1, SOF, 2, ESC, EOF]), vec![SOF, 1, ESC, SOF, 2, ESC, ESC, ESC, EOF, EOF]);
        assert_eq!(encode(&[]), vec![SOF, EOF]);
    }

    #[test]
    fn round_trips_every_byte_value() {
        let payload: Vec<u8> = (0..=255).collect();
        assert_eq!(decode_all(&encode(&payload)), vec![payload]);
    }

    #[test]
    fn decodes_back_to_back_frames_and_skips_garbage() {
        let mut stream = vec![0x00, 0x42];
        stream.extend(encode(&[1, 2]));
        stream.push(0x99);
        stream.extend(encode(&[EOF]));
        assert_eq!(decode_all(&stream), vec![vec![1, 2], vec![EOF]]);
    }

    #[test]
    fn sof_mid_frame_restarts() {
        let mut stream = vec![SOF, 7, 7];
        stream.extend(encode(&[3]));
        assert_eq!(decode_all(&stream), vec![vec![3]]);
    }
}
