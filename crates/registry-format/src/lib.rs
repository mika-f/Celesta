//! The plugin registry's client-facing file formats.
//!
//! The JSON Schemas live in `schema/` and the fixtures in `fixtures/`; this
//! crate holds their typed parsing. Signature verification lives here; hashing is added later.

mod error;
mod files;
mod ids;
mod keyring;
mod signing;

pub use error::FormatError;
pub use files::{
    Advisory, AdvisoryList, Artifact, Config, Deprecation, Hash, Index, Kind, OnlineKey,
    SearchEntry, SearchList, Source, Target, VersionEntry,
};
pub use ids::{is_registry_id, publisher_of};
pub use keyring::{Keyring, RootKey};
pub use signing::{key_message, signed_message};
