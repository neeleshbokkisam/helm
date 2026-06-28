/// CRC-16/CCITT-FALSE over `data` (poly 0x1021, init 0xFFFF, no reflect, xorout 0).
pub fn crc16_ccitt_false(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for byte in data {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_ffff() {
        assert_eq!(crc16_ccitt_false(b""), 0xFFFF);
    }
}
