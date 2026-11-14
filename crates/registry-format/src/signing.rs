//! The byte layouts that registry signatures cover.
//!
//! These layouts are part of the wire format: the registry service builds the
//! same bytes, so they must not change.

/// The message signed for a served file: `celesta-registry-file-v1\n{path}\n`
/// followed by the file's bytes. `path` is the served path, such as
/// `/index/vrm.json`.
pub fn signed_message(path: &str, bytes: &[u8]) -> Vec<u8> {
    let mut out = format!("celesta-registry-file-v1\n{path}\n").into_bytes();
    out.extend_from_slice(bytes);
    out
}

/// The message the root key signs to certify an online key:
/// `celesta-registry-key-v1\n{keyid}\n{public_b64}\n{expires}`, with no
/// trailing newline.
pub fn key_message(keyid: &str, public_b64: &str, expires: &str) -> Vec<u8> {
    format!("celesta-registry-key-v1\n{keyid}\n{public_b64}\n{expires}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_message_layout() {
        assert_eq!(
            signed_message("/search.json", b"{}\n"),
            b"celesta-registry-file-v1\n/search.json\n{}\n".to_vec()
        );
    }

    #[test]
    fn key_message_layout() {
        assert_eq!(
            key_message("2026-11", "AAAA", "2099-01-01T00:00:00Z"),
            b"celesta-registry-key-v1\n2026-11\nAAAA\n2099-01-01T00:00:00Z".to_vec()
        );
    }
}
