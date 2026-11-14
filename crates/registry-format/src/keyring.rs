//! Trust in registry files: a pinned root key certifies short-lived online
//! keys, and the online keys sign every served file.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::error::FormatError;
use crate::files::{Config, timestamp};
use crate::signing::{is_served_path, key_message, signed_message};

/// The pinned Ed25519 root public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootKey(pub [u8; 32]);

impl RootKey {
    /// Parses a standard-base64 public key; surrounding whitespace is ignored.
    pub fn from_base64(s: &str) -> Result<Self, FormatError> {
        let bytes = STANDARD
            .decode(s.trim())
            .map_err(|e| FormatError::Signature(format!("root key: {e}")))?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| FormatError::Signature("root key: expected 32 bytes".to_owned()))?;
        Ok(Self(key))
    }
}

/// The online keys that are currently trusted, in config order.
#[derive(Debug, Clone)]
pub struct Keyring {
    keys: Vec<(String, VerifyingKey)>,
}

#[derive(Deserialize)]
struct SigFile {
    keyid: String,
    signature: String,
}

impl Keyring {
    /// Keeps the online keys whose root signature verifies and that have not
    /// expired at `now` (`expires <= now` is expired).
    ///
    /// The certified message is built from the exact `public` and `expires`
    /// strings in `config.json` (`OnlineKey::public_b64` and
    /// `OnlineKey::expires_text`), never re-encoded from the parsed values.
    /// The key and its expiry are decoded from those same strings; the
    /// derived `OnlineKey::public` and `OnlineKey::expires` are ignored, and
    /// a key whose strings do not decode or parse is skipped.
    ///
    /// Fails with `Signature` when no key remains. This does not check
    /// `config.json`'s own signature; [`Keyring::from_signed_config`] does.
    pub fn from_config(
        config: &Config,
        root: &RootKey,
        now: OffsetDateTime,
    ) -> Result<Self, FormatError> {
        let no_key = || FormatError::Signature("no valid signing key".to_owned());
        let root = VerifyingKey::from_bytes(&root.0).map_err(|_| no_key())?;
        let mut keys = Vec::new();
        for key in &config.keys {
            // Trust follows the raw strings; `public` and `expires` are not read.
            let Ok(expires) = timestamp::parse(&key.expires_text) else {
                continue;
            };
            if expires <= now {
                continue;
            }
            let message = key_message(&key.keyid, &key.public_b64, &key.expires_text);
            let certified = root
                .verify_strict(&message, &Signature::from_bytes(&key.signature))
                .is_ok();
            if !certified {
                continue;
            }
            let Ok(public) = STANDARD
                .decode(&key.public_b64)
                .map_err(drop)
                .and_then(|b| <[u8; 32]>::try_from(b).map_err(drop))
                .and_then(|b| VerifyingKey::from_bytes(&b).map_err(drop))
            else {
                continue;
            };
            keys.push((key.keyid.clone(), public));
        }
        if keys.is_empty() {
            return Err(no_key());
        }
        Ok(Self { keys })
    }

    /// Reads `config.json` and its `config.json.sig`: builds the keyring from
    /// the root-certified keys (see [`Keyring::from_config`]), then verifies
    /// the config's own signature, which one of those keys made over the
    /// served path `/config.json`. Clients use this, not `from_config`.
    pub fn from_signed_config(
        config_bytes: &[u8],
        sig_json: &[u8],
        root: &RootKey,
        now: OffsetDateTime,
    ) -> Result<(Config, Self), FormatError> {
        let config = Config::parse(config_bytes)?;
        let ring = Self::from_config(&config, root, now)?;
        ring.verify("/config.json", config_bytes, sig_json)?;
        Ok((config, ring))
    }

    /// Verifies `bytes`, served at `path`, against its `.sig` file contents.
    ///
    /// `path` must start with `/` and hold no `\n`; any other path is a
    /// `Signature` error.
    pub fn verify(&self, path: &str, bytes: &[u8], sig_json: &[u8]) -> Result<(), FormatError> {
        if !is_served_path(path) {
            return Err(FormatError::Signature(format!(
                "{path:?}: not a served path"
            )));
        }
        let bad = |why: &str| FormatError::Signature(format!("{path}: {why}"));
        let sig: SigFile = serde_json::from_slice(sig_json).map_err(|_| bad("malformed .sig"))?;
        let (_, key) = self
            .keys
            .iter()
            .find(|(id, _)| *id == sig.keyid)
            .ok_or_else(|| FormatError::UnknownKey(sig.keyid.clone()))?;
        let raw = STANDARD
            .decode(&sig.signature)
            .map_err(|_| bad("malformed signature"))?;
        let raw: [u8; 64] = raw.try_into().map_err(|_| bad("malformed signature"))?;
        key.verify_strict(&signed_message(path, bytes), &Signature::from_bytes(&raw))
            .map_err(|_| FormatError::Signature(path.to_owned()))
    }
}
