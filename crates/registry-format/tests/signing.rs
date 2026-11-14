use std::fs;
use std::path::{Path, PathBuf};

use celesta_registry_format::{Config, FormatError, Keyring, RootKey};
use ed25519_dalek::SigningKey;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn signed_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("signed")
        .join(rel)
}

fn signed(rel: &str) -> Vec<u8> {
    fs::read(signed_path(rel)).unwrap()
}

fn now() -> OffsetDateTime {
    OffsetDateTime::parse("2026-11-14T00:00:00Z", &Rfc3339).unwrap()
}

fn root() -> RootKey {
    RootKey::from_base64(&String::from_utf8(signed("root.pub")).unwrap()).unwrap()
}

fn keyring() -> Keyring {
    let config = Config::parse(&signed("config.json")).unwrap();
    Keyring::from_config(&config, &root(), now()).unwrap()
}

#[test]
fn signed_fixtures_verify() {
    let ring = keyring();
    for (path, rel) in [
        ("/index/vrm.json", "index/vrm.json"),
        ("/search.json", "search.json"),
        ("/advisories.json", "advisories.json"),
    ] {
        ring.verify(path, &signed(rel), &signed(&format!("{rel}.sig")))
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
    }
}

#[test]
fn wrong_path_fails() {
    let err = keyring()
        .verify(
            "/index/other.json",
            &signed("index/vrm.json"),
            &signed("index/vrm.json.sig"),
        )
        .unwrap_err();
    assert!(matches!(err, FormatError::Signature(_)), "{err:?}");
}

#[test]
fn flipped_byte_fails() {
    let mut bytes = signed("index/vrm.json");
    bytes[0] ^= 1;
    let err = keyring()
        .verify("/index/vrm.json", &bytes, &signed("index/vrm.json.sig"))
        .unwrap_err();
    assert!(matches!(err, FormatError::Signature(_)), "{err:?}");
}

#[test]
fn unknown_keyid_is_unknown_key() {
    let sig = String::from_utf8(signed("index/vrm.json.sig"))
        .unwrap()
        .replace("2026-11", "nope");
    let err = keyring()
        .verify("/index/vrm.json", &signed("index/vrm.json"), sig.as_bytes())
        .unwrap_err();
    assert!(
        matches!(err, FormatError::UnknownKey(ref k) if k == "nope"),
        "{err:?}"
    );
}

#[test]
fn expired_key_is_dropped() {
    let err = keyring()
        .verify(
            "/expired/advisories.json",
            &signed("expired/advisories.json"),
            &signed("expired/advisories.json.sig"),
        )
        .unwrap_err();
    assert!(
        matches!(err, FormatError::UnknownKey(ref k) if k == "expired"),
        "{err:?}"
    );
}

#[test]
fn key_expiring_exactly_now_is_dropped() {
    let config = Config::parse(&signed("config.json")).unwrap();
    let at = config.keys[0].expires;
    let err = Keyring::from_config(&config, &root(), at).unwrap_err();
    assert!(matches!(err, FormatError::Signature(_)), "{err:?}");
}

#[test]
fn different_root_is_rejected() {
    let other = RootKey(SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes());
    let config = Config::parse(&signed("config.json")).unwrap();
    let err = Keyring::from_config(&config, &other, now()).unwrap_err();
    assert!(matches!(err, FormatError::Signature(_)), "{err:?}");
}

#[test]
fn non_canonical_expires_does_not_verify() {
    // The same instant, written with a non-UTC offset or fractional seconds,
    // rebuilds a different message than the one the root signed.
    let text = String::from_utf8(signed("config.json")).unwrap();
    for spelled in ["2099-01-01T01:00:00+01:00", "2099-01-01T00:00:00.5Z"] {
        let changed = text.replace("2099-01-01T00:00:00Z", spelled);
        assert_ne!(text, changed);
        let config = Config::parse(changed.as_bytes()).unwrap();
        let err = Keyring::from_config(&config, &root(), now()).unwrap_err();
        assert!(
            matches!(err, FormatError::Signature(_)),
            "{spelled}: {err:?}"
        );
    }
}

#[test]
fn utc_offset_spelling_normalizes_to_z() {
    // `+00:00` parses to the same UTC instant and is rebuilt as `Z`, which is
    // what the root signed, so it still verifies.
    let text = String::from_utf8(signed("config.json")).unwrap();
    let changed = text.replace("2099-01-01T00:00:00Z", "2099-01-01T00:00:00+00:00");
    let config = Config::parse(changed.as_bytes()).unwrap();
    Keyring::from_config(&config, &root(), now()).unwrap();
}

#[test]
fn malformed_sig_json_is_signature_error() {
    let ring = keyring();
    let bytes = signed("index/vrm.json");
    let good = String::from_utf8(signed("index/vrm.json.sig")).unwrap();
    let cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"not json".to_vec(),
        b"{}".to_vec(),
        br#"{"keyid":"2026-11","signature":"!!!"}"#.to_vec(),
        br#"{"keyid":"2026-11","signature":"AAAA"}"#.to_vec(),
        format!("{good} trailing").into_bytes(),
    ];
    for sig in cases {
        let err = ring.verify("/index/vrm.json", &bytes, &sig).unwrap_err();
        assert!(matches!(err, FormatError::Signature(_)), "{sig:?}: {err:?}");
    }
}

#[test]
fn root_key_from_base64() {
    assert_eq!(root().0.len(), 32);
    assert!(RootKey::from_base64("AAAA").is_err());
    assert!(RootKey::from_base64("!!!").is_err());
}
