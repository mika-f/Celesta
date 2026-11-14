//! The client side of the plugin registry: fetching its signed files over a
//! `Transport`, verifying them, refusing rollbacks, and caching them.

mod cache;
mod error;
mod registry;
mod resolve;
mod transport;

pub use error::ClientError;
pub use registry::{OFFICIAL_REGISTRY, Registry, official_root_key};
pub use resolve::{Resolution, ResolveError, ResolveInput, ResolveWarning, Update, resolve};
pub use transport::{DirTransport, MAX_JSON_BYTES, Transport, TransportError};
