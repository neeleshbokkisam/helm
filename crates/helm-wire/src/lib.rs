pub mod crc;
pub mod frame;

pub use frame::{FrameParser, WireError, encode_frame};
