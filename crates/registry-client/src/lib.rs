//! The client side of the plugin registry: fetching its signed files over a
//! `Transport`, verifying them, refusing rollbacks, and caching them, and
//! installing the plugin archives they list into the store.

mod cache;
mod error;
mod gc;
mod http;
mod project;
mod refresh;
mod registry;
mod resolve;
mod store;
#[cfg(feature = "testing")]
pub mod testing;
mod transport;

pub use error::ClientError;
pub use gc::{GcInput, GcPlan, plan_gc, run_gc};
pub use http::{CLI_ADVISORY_TIMEOUT, HttpTransport};
pub use project::{
    BundledPlugins, Environment, InstallOutcome, LockOutcome, ProjectError, holds_locked_build,
    install_global, install_locked, lock_project,
};
pub use refresh::{AdvisoryRefresh, refresh_advisories_within};
pub use registry::{OFFICIAL_REGISTRY, Registry, official_root_key};
pub use resolve::{Resolution, ResolveError, ResolveInput, ResolveWarning, Update, resolve};
pub use store::{
    InstallError, InstallRequest, MAX_ENTRIES, MAX_UNPACKED_BYTES, PROJECTS_FILE, Store,
};
pub use transport::{DirTransport, MAX_JSON_BYTES, Transport, TransportError};
