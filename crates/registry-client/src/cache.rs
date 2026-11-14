use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use celesta_plugin_host::{create_private_dir_all, unsafe_reason};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::ClientError;

/// The file in the cache that records, per served path, the newest
/// `generated` timestamp verified there.
const STAMPS: &str = "stamps.json";

fn cache_error(path: &Path, message: impl std::fmt::Display) -> ClientError {
    ClientError::Cache {
        path: path.to_owned(),
        message: message.to_string(),
    }
}

/// Where the served `path` (such as `/index/vrm.json`) is cached.
pub(crate) fn file_path(cache: &Path, path: &str) -> PathBuf {
    cache.join(path.trim_start_matches('/'))
}

/// Writes `bytes` to `file` through a temp file renamed over it, creating
/// its folders; on Unix the folders are 0700 and the file 0600.
pub(crate) fn write_private(file: &Path, bytes: &[u8]) -> Result<(), ClientError> {
    let folder = file
        .parent()
        .ok_or_else(|| cache_error(file, "has no folder"))?;
    create_private_dir_all(folder).map_err(|e| cache_error(folder, e))?;
    // A temp file is created 0600 on Unix.
    let mut temp = tempfile::NamedTempFile::new_in(folder).map_err(|e| cache_error(folder, e))?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|e| cache_error(file, e))?;
    temp.persist(file).map_err(|e| cache_error(file, e.error))?;
    Ok(())
}

/// The bytes cached at `file`, or `None` when there are none, or when other
/// users could have changed the file.
pub(crate) fn read_trusted(file: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(file).ok()?;
    if unsafe_reason(file).is_some() {
        return None;
    }
    Some(bytes)
}

fn read_stamps(cache: &Path) -> Result<BTreeMap<String, String>, ClientError> {
    let file = cache.join(STAMPS);
    match std::fs::read(&file) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| cache_error(&file, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(cache_error(&file, e)),
    }
}

/// The newest `generated` verified at the served `path`, if any. An
/// unreadable stamp file is an error, since ignoring it would switch off
/// the rollback check.
pub(crate) fn stamp(cache: &Path, path: &str) -> Result<Option<OffsetDateTime>, ClientError> {
    let stamps = read_stamps(cache)?;
    let Some(text) = stamps.get(path) else {
        return Ok(None);
    };
    OffsetDateTime::parse(text, &Rfc3339)
        .map(Some)
        .map_err(|e| cache_error(&cache.join(STAMPS), format!("{path}: {e}")))
}

pub(crate) fn record_stamp(
    cache: &Path,
    path: &str,
    generated: OffsetDateTime,
) -> Result<(), ClientError> {
    let mut stamps = read_stamps(cache)?;
    let text = generated
        .format(&Rfc3339)
        .map_err(|e| cache_error(&cache.join(STAMPS), e))?;
    stamps.insert(path.to_owned(), text);
    let json = serde_json::to_vec_pretty(&stamps).map_err(|e| cache_error(cache, e))?;
    write_private(&cache.join(STAMPS), &json)
}
