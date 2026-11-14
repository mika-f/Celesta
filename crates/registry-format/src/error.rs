use std::path::PathBuf;

/// Failures while parsing or verifying registry files.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported format version {0}")]
    Format(u64),
    #[error("index id mismatch: expected {expected}, found {found}")]
    IdMismatch { expected: String, found: String },
    #[error("invalid plugin id: {0}")]
    InvalidId(String),
    #[error("invalid target: {0}")]
    InvalidTarget(String),
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(String),
    #[error("invalid hash: {0}")]
    InvalidHash(String),
    #[error("invalid advisory range: {0}")]
    InvalidRange(String),
    #[error("signature error: {0}")]
    Signature(String),
    #[error("key expired: {0}")]
    KeyExpired(String),
    #[error("unknown key: {0}")]
    UnknownKey(String),
    #[error("rollback detected")]
    Rollback,
    #[error("advisory list is dated in the future")]
    FutureDated,
    #[error("tree hash error at {}: {}", .0.display(), .1)]
    TreeHash(PathBuf, String),
}
