//! Offline (non-authenticated) accounts.

use md5::{Digest, Md5};

pub fn is_valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Same as Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)` — what
/// offline-mode servers assign — formatted without dashes.
pub fn offline_uuid(name: &str) -> String {
    let mut bytes: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(is_valid_name("Steve"));
        assert!(is_valid_name("a_b_1"));
        assert!(!is_valid_name("ab"));
        assert!(!is_valid_name("seventeen_chars_x"));
        assert!(!is_valid_name("Стив"));
        assert!(!is_valid_name("bad name"));
    }

    #[test]
    fn uuid_matches_java() {
        // UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes())
        assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }
}
