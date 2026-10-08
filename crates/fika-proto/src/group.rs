//! Group identifiers (SPEC §9.2): low 28 bits of FNV-1a over the NFC,
//! lower-cased, UTF-8 group name.

use unicode_normalization::UnicodeNormalization;

use crate::callsign::fnv1a32;

pub fn canonical(name: &str) -> String {
    name.trim().nfc().collect::<String>().to_lowercase()
}

pub fn group_id(name: &str) -> u32 {
    fnv1a32(canonical(name).as_bytes()) & 0x0FFF_FFFF
}

/// 12-bit tag used in beacons.
pub fn group_tag(name: &str) -> u16 {
    (group_id(name) & 0xFFF) as u16
}

pub fn tag_of_id(id: u32) -> u16 {
    (id & 0xFFF) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_and_normalisation_insensitive() {
        assert_eq!(group_id("Fika"), group_id("fika"));
        assert_eq!(group_id("SM6 Lördag"), group_id("sm6 lo\u{0308}rdag"));
        assert_ne!(group_id("fika"), group_id("kaffe"));
        assert!(group_id("anything") < (1 << 28));
        assert_eq!(tag_of_id(group_id("fika")), group_tag("fika"));
    }
}
