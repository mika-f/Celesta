use std::path::PathBuf;

use celesta_registry_format::FormatError;

use crate::transport::TransportError;

/// Why a registry file could not be fetched, verified, or cached.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("{0}")]
    Transport(#[from] TransportError),
    #[error("{path}: {source}")]
    Format {
        /// The served path of the file, such as `/index/vrm.json`.
        path: String,
        source: FormatError,
    },
    #[error("{url} is larger than the size limit")]
    TooLarge { url: String },
    #[error("cache {}: {message}", path.display())]
    Cache { path: PathBuf, message: String },
    #[error("no plugin named {0} in the registry")]
    NoSuchPlugin(String),
}
