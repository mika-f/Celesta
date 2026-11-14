use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use celesta_plugin_host::{create_private_dir_all, unsafe_reason};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::ClientError;

/// The file in the cache that records, per served path, the newest
/// `generated` timestamp verified there.
const STAMPS: &str = "stamps.json";

/// Held across a fetched file's check-then-write and its stamp update, so a
/// refresh thread that outlives its budget cannot lose or lower a stamp
/// another thread records. It serializes threads of this process only.
static CACHE_LOCK: Mutex<()> = Mutex::new(());

/// Takes the cache lock. Poison is safe to ignore: the guard holds `()` and
/// every write is an atomic rename.
pub(crate) fn lock() -> MutexGuard<'static, ()> {
    CACHE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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

/// Records `generated` at `path`, keeping the newer of it and any stamp
/// there. The caller holds the cache lock, which `_held` proves.
pub(crate) fn record_stamp(
    _held: &MutexGuard<'static, ()>,
    cache: &Path,
    path: &str,
    generated: OffsetDateTime,
) -> Result<(), ClientError> {
    let mut stamps = read_stamps(cache)?;
    let generated = match stamp(cache, path)? {
        Some(newest) => generated.max(newest),
        None => generated,
    };
    let text = generated
        .format(&Rfc3339)
        .map_err(|e| cache_error(&cache.join(STAMPS), e))?;
    stamps.insert(path.to_owned(), text);
    let json = serde_json::to_vec_pretty(&stamps).map_err(|e| cache_error(cache, e))?;
    write_private(&cache.join(STAMPS), &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_stamp_never_replaces_a_newer_one() {
        let dir = tempfile::tempdir().unwrap();
        let newer = OffsetDateTime::from_unix_timestamp(1_800_000_100).unwrap();
        let older = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        record_stamp(&lock(), dir.path(), "/a.json", newer).unwrap();
        record_stamp(&lock(), dir.path(), "/a.json", older).unwrap();
        assert_eq!(stamp(dir.path(), "/a.json").unwrap(), Some(newer));
    }

    #[test]
    fn concurrent_stamps_are_all_kept() {
        let dir = tempfile::tempdir().unwrap();
        let base = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let record = |path: &'static str| {
            let cache = dir.path();
            move || {
                for i in 0..100 {
                    record_stamp(&lock(), cache, path, base + time::Duration::seconds(i)).unwrap();
                }
            }
        };
        std::thread::scope(|scope| {
            scope.spawn(record("/a.json"));
            scope.spawn(record("/b.json"));
        });
        let last = base + time::Duration::seconds(99);
        assert_eq!(stamp(dir.path(), "/a.json").unwrap(), Some(last));
        assert_eq!(stamp(dir.path(), "/b.json").unwrap(), Some(last));
    }
}
