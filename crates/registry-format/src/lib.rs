//! The plugin registry's client-facing file formats.
//!
//! The JSON Schemas live in `schema/` and the fixtures in `fixtures/`; this
//! crate holds their typed parsing. Signature verification and the tree hash live here.

mod advisories;
mod error;
mod files;
mod ids;
mod keyring;
mod signing;
mod tree;

pub use error::FormatError;
pub use files::{
    Advisory, AdvisoryList, Artifact, Config, Deprecation, Hash, Index, Kind, OnlineKey,
    SearchEntry, SearchList, Source, Target, VersionEntry,
};
pub use ids::{is_registry_id, publisher_of};
pub use keyring::{Keyring, RootKey};
pub use signing::{key_message, signed_message};
pub use tree::tree_hash;
