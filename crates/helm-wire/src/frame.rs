use thiserror::Error;

use crate::crc::crc16_ccitt_false;

pub const SYNC_BYTE: u8 = 0xA5;
pub const MAX_BODY_LEN: usize = 64;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WireError {
    #[error("buffer too short")]
    BufferTooShort,
    #[error("invalid sync")]
    InvalidSync,
    #[error("body length {0} exceeds max {MAX_BODY_LEN}")]
    BodyTooLong(u8),
    #[error("crc mismatch")]
    CrcMismatch,
    #[error("unknown message type {0}")]
    UnknownType(u8),
}

pub fn encode_frame(msg_type: u8, body: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    if body.len() > MAX_BODY_LEN {
        return Err(WireError::BodyTooLong(body.len() as u8));
    }
    let frame_len = 1 + 1 + 1 + body.len() + 2;
    if out.len() < frame_len {
        return Err(WireError::BufferTooShort);
    }
    out[0] = SYNC_BYTE;
    out[1] = body.len() as u8;
    out[2] = msg_type;
    out[3..3 + body.len()].copy_from_slice(body);
    let crc = crc16_ccitt_false(&out[2..3 + body.len()]);
    let crc_off = 3 + body.len();
    out[crc_off] = (crc >> 8) as u8;
    out[crc_off + 1] = crc as u8;
    Ok(frame_len)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFrame {
    pub msg_type: u8,
    pub body: Vec<u8>,
}

enum ParseState {
    HuntSync,
    Len,
    Type { len: u8 },
    Body {
        msg_type: u8,
        len: u8,
        buf: Vec<u8>,
    },
    CrcHigh {
        msg_type: u8,
        body: Vec<u8>,
    },
    CrcLow {
        msg_type: u8,
        body: Vec<u8>,
        crc_high: u8,
    },
}

pub struct FrameParser {
    state: ParseState,
}

impl Default for FrameParser {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameParser {
    pub fn new() -> Self {
        Self {
            state: ParseState::HuntSync,
        }
    }

    pub fn push_byte(&mut self, byte: u8) -> Option<Result<ParsedFrame, WireError>> {
        match std::mem::replace(&mut self.state, ParseState::HuntSync) {
            ParseState::HuntSync => {
                if byte == SYNC_BYTE {
                    self.state = ParseState::Len;
                } else {
                    self.state = ParseState::HuntSync;
                }
                None
            }
            ParseState::Len => {
                if byte as usize > MAX_BODY_LEN {
                    self.state = ParseState::HuntSync;
                    Some(Err(WireError::BodyTooLong(byte)))
                } else {
                    self.state = ParseState::Type { len: byte };
                    None
                }
            }
            ParseState::Type { len } => {
                if len == 0 {
                    self.state = ParseState::CrcHigh {
                        msg_type: byte,
                        body: Vec::new(),
                    };
                } else {
                    self.state = ParseState::Body {
                        msg_type: byte,
                        len,
                        buf: Vec::new(),
                    };
                }
                None
            }
            ParseState::Body {
                msg_type,
                len,
                mut buf,
            } => {
                buf.push(byte);
                if buf.len() < len as usize {
                    self.state = ParseState::Body {
                        msg_type,
                        len,
                        buf,
                    };
                } else {
                    self.state = ParseState::CrcHigh {
                        msg_type,
                        body: buf,
                    };
                }
                None
            }
            ParseState::CrcHigh { msg_type, body } => {
                self.state = ParseState::CrcLow {
                    msg_type,
                    body,
                    crc_high: byte,
                };
                None
            }
            ParseState::CrcLow {
                msg_type,
                body,
                crc_high,
            } => {
                let mut crc_input = Vec::with_capacity(1 + body.len());
                crc_input.push(msg_type);
                crc_input.extend_from_slice(&body);
                let expected = crc16_ccitt_false(&crc_input);
                let got = ((crc_high as u16) << 8) | byte as u16;
                if got != expected {
                    Some(Err(WireError::CrcMismatch))
                } else {
                    Some(Ok(ParsedFrame { msg_type, body }))
                }
            }
        }
    }

    pub fn push_bytes(&mut self, data: &[u8]) -> Vec<Result<ParsedFrame, WireError>> {
        let mut out = Vec::new();
        for &b in data {
            if let Some(frame) = self.push_byte(b) {
                out.push(frame);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_decode_roundtrip(msg_type: u8, body: &[u8]) {
        let mut buf = [0u8; 128];
        let n = encode_frame(msg_type, body, &mut buf).unwrap();
        let mut parser = FrameParser::new();
        let frames = parser.push_bytes(&buf[..n]);
        assert_eq!(frames.len(), 1, "body={body:?}");
        let frame = frames[0].as_ref().unwrap();
        assert_eq!(frame.msg_type, msg_type);
        assert_eq!(frame.body, body);
    }

    #[test]
    fn roundtrip_nonempty() {
        encode_decode_roundtrip(0x01, &[1, 2, 3, 4]);
    }

    #[test]
    fn roundtrip_empty_body() {
        encode_decode_roundtrip(0x02, &[]);
    }

    #[test]
    fn rejects_bad_crc() {
        let mut buf = [0u8; 16];
        let n = encode_frame(0x01, &[1, 2, 3], &mut buf).unwrap();
        buf[n - 1] ^= 0xFF;
        let mut parser = FrameParser::new();
        let frames = parser.push_bytes(&buf[..n]);
        assert_eq!(frames[0], Err(WireError::CrcMismatch));
    }

    #[test]
    fn resync_after_garbage_and_split_reads() {
        let mut buf = [0u8; 32];
        let n = encode_frame(0x81, &[9, 8, 7, 6], &mut buf).unwrap();
        let mut stream = Vec::new();
        stream.extend_from_slice(&[0x00, 0xFF, 0x00]);
        stream.extend_from_slice(&buf[..n]);
        let mut parser = FrameParser::new();
        let mut frames = Vec::new();
        for chunk in stream.chunks(2) {
            frames.extend(parser.push_bytes(chunk));
        }
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_ref().unwrap().body, vec![9, 8, 7, 6]);
    }
}
