use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// The most bytes read from one JSON file or signature.
pub const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;

/// Why a body could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("{0} was not found")]
    NotFound(String),
    #[error("could not fetch {url}: {message}")]
    Failed { url: String, message: String },
}

/// Opens the bodies of registry URLs.
pub trait Transport: Send + Sync {
    /// The body at `url`; `NotFound` for a 404.
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError>;
}

/// Serves `<base><path>` from `<root><path>`, for tests and local registries.
#[derive(Debug, Clone)]
pub struct DirTransport {
    pub base: String,
    pub root: PathBuf,
}

impl Transport for DirTransport {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        let failed = |message: &str| TransportError::Failed {
            url: url.to_owned(),
            message: message.to_owned(),
        };
        let path = url
            .strip_prefix(self.base.as_str())
            .filter(|path| path.starts_with('/'))
            .ok_or_else(|| failed("not under the registry's base URL"))?;
        let relative = Path::new(path.trim_start_matches('/'));
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(failed("not a plain path"));
        }
        match std::fs::File::open(self.root.join(relative)) {
            Ok(file) => Ok(Box::new(file)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(TransportError::NotFound(url.to_owned()))
            }
            Err(error) => Err(failed(&error.to_string())),
        }
    }
}
