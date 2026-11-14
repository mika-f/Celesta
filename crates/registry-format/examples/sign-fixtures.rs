//! Regenerates `fixtures/signed/` deterministically.
//!
//! Run: `cargo run -p celesta-registry-format --example sign-fixtures`
//!
//! The keys are derived from fixed seeds and are for tests only. Ed25519
//! signatures are deterministic, so a second run changes nothing.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use celesta_registry_format::{key_message, signed_message};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

const API: &str = "https://registry.celesta.studio/api/v1";
const ONLINE_KEYID: &str = "2026-11";
const ONLINE_EXPIRES: &str = "2099-01-01T00:00:00Z";
const EXPIRED_KEYID: &str = "expired";
const EXPIRED_EXPIRES: &str = "2000-01-01T00:00:00Z";

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn write_json(path: &Path, value: &serde_json::Value) {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    write(path, text.as_bytes());
}

/// Copies `source` to `out/rel`, then writes `out/rel.sig` over `served`.
fn sign_file(out: &Path, rel: &str, source: &Path, served: &str, keyid: &str, key: &SigningKey) {
    // `Keyring::verify` refuses any other path, so never sign one.
    assert!(
        served.starts_with('/') && !served.contains('\n'),
        "not a served path: {served:?}"
    );
    let bytes = fs::read(source).unwrap();
    write(&out.join(rel), &bytes);
    let signature = key.sign(&signed_message(served, &bytes));
    write_json(
        &out.join(format!("{rel}.sig")),
        &json!({ "keyid": keyid, "signature": b64(&signature.to_bytes()) }),
    );
}

fn main() {
    let root_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let valid = root_dir.join("fixtures/valid");
    let out = root_dir.join("fixtures/signed");

    let root = SigningKey::from_bytes(&[0x01; 32]);
    let online = SigningKey::from_bytes(&[0x02; 32]);
    let expired = SigningKey::from_bytes(&[0x03; 32]);

    let keys: Vec<_> = [
        (ONLINE_KEYID, &online, ONLINE_EXPIRES),
        (EXPIRED_KEYID, &expired, EXPIRED_EXPIRES),
    ]
    .into_iter()
    .map(|(keyid, key, expires)| {
        let public = b64(key.verifying_key().as_bytes());
        let signature = root.sign(&key_message(keyid, &public, expires));
        json!({
            "keyid": keyid,
            "public": public,
            "expires": expires,
            "signature": b64(&signature.to_bytes()),
        })
    })
    .collect();
    write_json(
        &out.join("config.json"),
        &json!({ "format": 1, "api": API, "keys": keys }),
    );
    // config.json is signed like every other served file, by an online key it
    // certifies.
    sign_file(
        &out,
        "config.json",
        &out.join("config.json"),
        "/config.json",
        ONLINE_KEYID,
        &online,
    );
    write(
        &out.join("root.pub"),
        format!("{}\n", b64(root.verifying_key().as_bytes())).as_bytes(),
    );

    let files = [
        ("index/vrm.json", "index-vrm.json", "/index/vrm.json"),
        ("search.json", "search.json", "/search.json"),
        ("advisories.json", "advisories.json", "/advisories.json"),
    ];
    for (rel, source, served) in files {
        sign_file(
            &out,
            rel,
            &valid.join(source),
            served,
            ONLINE_KEYID,
            &online,
        );
    }
    sign_file(
        &out,
        "expired/advisories.json",
        &valid.join("advisories.json"),
        "/advisories.json",
        EXPIRED_KEYID,
        &expired,
    );
}
