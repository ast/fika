//! CRC-16/CCITT-FALSE (SPEC §7.7): poly 0x1021, init 0xFFFF, no reflection.

pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in bytes {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_value() {
        // Standard check value for CRC-16/CCITT-FALSE over "123456789".
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }
}
