//! The client side of the plugin registry: fetching its signed files over a
//! `Transport`, verifying them, refusing rollbacks, and caching them.

mod cache;
mod error;
mod registry;
mod transport;

pub use error::ClientError;
pub use registry::{OFFICIAL_REGISTRY, Registry, official_root_key};
pub use transport::{DirTransport, MAX_JSON_BYTES, Transport, TransportError};
