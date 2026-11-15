//! The store of registry plugins: one folder per version, at
//! `<root>/<id>/<version>`, each the extracted archive the registry lists
//! for that version, checked against its size, SHA-256, and tree hash
//! before it is renamed into place.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use celesta_plugin_host::{MANIFEST, create_private_dir_all, registry_store_dir};
use celesta_registry_format::{FormatError, Hash, is_plugin_id, tree_hash};
use semver::Version;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::transport::{Transport, TransportError};

/// The most bytes one archive may unpack to.
pub const MAX_UNPACKED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// The most entries one archive may hold.
pub const MAX_ENTRIES: usize = 100_000;

/// The Unix file type bits of a mode, and the two types an archive may hold.
const TYPE_MASK: u32 = 0o170_000;
const TYPE_DIR: u32 = 0o040_000;
const TYPE_FILE: u32 = 0o100_000;
const TYPE_LINK: u32 = 0o120_000;

/// The archive's name in the temporary folder, and the plugin folder it is
/// extracted to there.
const ARCHIVE: &str = "archive.zip";
const PLUGIN: &str = "plugin";

const BUFFER: usize = 64 * 1024;

/// One archive to install: what the lock or index says of the version's
/// build for this host.
#[derive(Clone, Copy, Debug)]
pub struct InstallRequest<'a> {
    pub id: &'a str,
    pub version: &'a Version,
    pub url: &'a str,
    pub size: u64,
    pub sha256: &'a Hash,
    pub tree_hash: &'a Hash,
}

/// Why an archive was not installed. Nothing is left in the store when
/// one is not.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{0}")]
    Transport(#[from] TransportError),
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    /// `found` is at most `expected + 1`: reading stops there.
    #[error("the archive is not the {expected} bytes the registry lists (read {found})")]
    Size { expected: u64, found: u64 },
    #[error("{url} does not match the SHA-256 the registry lists")]
    Checksum { url: String },
    /// `entry` is empty when the archive names two entries the same, as
    /// the reader keeps only one of them.
    #[error("the archive's entry {entry:?} {reason}")]
    Unsafe { entry: String, reason: String },
    #[error("the archive unpacks to more entries or bytes than the store allows")]
    TooLarge,
    #[error("the unpacked plugin's tree hash is {found}, not the {expected} the registry lists")]
    TreeHash { expected: Hash, found: Hash },
    #[error("{0}")]
    Manifest(String),
    #[error("{0}")]
    Format(#[from] FormatError),
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> InstallError + '_ {
    move |source| InstallError::Io {
        path: path.to_owned(),
        source,
    }
}

fn unsafe_entry(entry: &str, reason: &str) -> InstallError {
    InstallError::Unsafe {
        entry: entry.to_owned(),
        reason: reason.to_owned(),
    }
}

/// The per-user folder of plugins installed from the registry.
#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
    max_bytes: u64,
    max_entries: usize,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self::with_limits(root, MAX_UNPACKED_BYTES, MAX_ENTRIES)
    }

    /// A store whose archives may unpack to at most `max_bytes` in at most
    /// `max_entries` entries, for tests.
    pub fn with_limits(root: PathBuf, max_bytes: u64, max_entries: usize) -> Self {
        Self {
            root,
            max_bytes,
            max_entries,
        }
    }

    /// The store in its standard place ([`registry_store_dir`]); `None`
    /// when there is no user folder.
    pub fn standard() -> Option<Self> {
        registry_store_dir().map(Self::new)
    }

    /// The folder of one version.
    pub fn path(&self, id: &str, version: &Version) -> PathBuf {
        self.root.join(id).join(version.to_string())
    }

    /// Whether the version is there with this tree hash.
    pub fn contains(&self, id: &str, version: &Version, tree_hash: &Hash) -> bool {
        is_plugin_id(id) && holds(&self.path(id, version), tree_hash)
    }

    /// Downloads, checks, extracts, hashes, and renames into place; returns the directory.
    ///
    /// A version already there with the requested tree hash is returned
    /// without a download. The work happens in a `.tmp-` folder in the
    /// store, which no plugin id names, and is removed whatever happens.
    pub fn install(
        &self,
        transport: &dyn Transport,
        request: &InstallRequest,
    ) -> Result<PathBuf, InstallError> {
        // An id that is not one path part could name a folder outside the
        // store.
        if !is_plugin_id(request.id) {
            return Err(FormatError::InvalidId(request.id.to_owned()).into());
        }
        let destination = self.path(request.id, request.version);
        if holds(&destination, request.tree_hash) {
            return Ok(destination);
        }
        create_private_dir_all(&self.root).map_err(io_error(&self.root))?;
        // Created 0700 on Unix; in the store, so the rename stays on one
        // file system.
        let temp = tempfile::Builder::new()
            .prefix(".tmp-")
            .tempdir_in(&self.root)
            .map_err(io_error(&self.root))?;

        let archive = temp.path().join(ARCHIVE);
        download(transport, request, &archive)?;
        let plugin = temp.path().join(PLUGIN);
        self.extract(&archive, &plugin)?;

        let found = tree_hash(&plugin)?;
        if found != *request.tree_hash {
            return Err(InstallError::TreeHash {
                expected: *request.tree_hash,
                found,
            });
        }
        check_manifest(&plugin, request)?;

        let folder = self.root.join(request.id);
        create_private_dir_all(&folder).map_err(io_error(&folder))?;
        if fs::symlink_metadata(&destination).is_ok() {
            // Another install finished first.
            if holds(&destination, request.tree_hash) {
                return Ok(destination);
            }
            // Out of the way first, as Windows cannot rename onto a
            // folder; it is deleted with the temporary folder.
            fs::rename(&destination, temp.path().join("old")).map_err(io_error(&destination))?;
        }
        match fs::rename(&plugin, &destination) {
            Ok(()) => Ok(destination),
            Err(_) if holds(&destination, request.tree_hash) => Ok(destination),
            Err(source) => Err(InstallError::Io {
                path: destination,
                source,
            }),
        }
    }

    /// Extracts `archive` into the new folder `plugin`. Every entry must be
    /// a folder or a regular file with a plain relative name, named once;
    /// the entries and the bytes written are counted against the limits.
    fn extract(&self, archive: &Path, plugin: &Path) -> Result<(), InstallError> {
        let zip_error = |error: zip::result::ZipError| InstallError::Io {
            path: archive.to_owned(),
            source: io::Error::other(error),
        };
        let file = File::open(archive).map_err(io_error(archive))?;
        let mut zip = ZipArchive::new(BufReader::new(file)).map_err(zip_error)?;
        // The reader keeps one entry per name, so a duplicate shows only
        // here.
        if zip.has_duplicate_names() {
            return Err(unsafe_entry(
                "",
                "shares its name with another (the archive has duplicate names)",
            ));
        }
        if zip.len() > self.max_entries {
            return Err(InstallError::TooLarge);
        }
        make_dir(plugin).map_err(io_error(plugin))?;
        let mut names = HashSet::new();
        let mut written = 0u64;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(zip_error)?;
            let name = std::str::from_utf8(entry.name_raw())
                .map_err(|_| {
                    unsafe_entry(&String::from_utf8_lossy(entry.name_raw()), "is not UTF-8")
                })?
                .to_owned();
            let (parts, named_dir) =
                entry_parts(&name).map_err(|reason| unsafe_entry(&name, reason))?;
            let is_dir = match entry.unix_mode().map(|mode| mode & TYPE_MASK) {
                None | Some(0) => named_dir,
                Some(TYPE_DIR) => true,
                Some(TYPE_FILE) if named_dir => {
                    return Err(unsafe_entry(&name, "is a file named like a folder"));
                }
                Some(TYPE_FILE) => false,
                Some(TYPE_LINK) => return Err(unsafe_entry(&name, "is a symbolic link")),
                Some(_) => return Err(unsafe_entry(&name, "is not a regular file or folder")),
            };
            if !names.insert(parts.join("/")) {
                return Err(unsafe_entry(&name, "shares its name with another"));
            }

            let (last, parents) = parts.split_last().expect("a name has a part");
            let mut path = plugin.to_owned();
            for part in parents {
                path.push(part);
                ensure_dir(&path, &name)?;
            }
            path.push(last);
            if is_dir {
                ensure_dir(&path, &name)?;
                continue;
            }
            let executable = entry.unix_mode().is_some_and(|mode| mode & 0o111 != 0);
            let mut out = create_file(&path).map_err(|source| {
                if source.kind() == io::ErrorKind::AlreadyExists {
                    unsafe_entry(&name, "shares its name with another")
                } else {
                    InstallError::Io {
                        path: path.clone(),
                        source,
                    }
                }
            })?;
            let budget = self.max_bytes - written;
            written += copy_within(&mut entry, &mut out, budget, archive, &path)?;
            set_mode(&path, if executable { 0o755 } else { 0o644 }).map_err(io_error(&path))?;
        }
        Ok(())
    }
}

/// Whether `folder` is there with this tree hash.
fn holds(folder: &Path, expected: &Hash) -> bool {
    tree_hash(folder).is_ok_and(|found| found == *expected)
}

/// Downloads the request's archive to `archive`, reading at most one byte
/// more than its size, and checks its size and SHA-256.
fn download(
    transport: &dyn Transport,
    request: &InstallRequest,
    archive: &Path,
) -> Result<(), InstallError> {
    let failed = |error: io::Error| {
        InstallError::Transport(TransportError::Failed {
            url: request.url.to_owned(),
            message: error.to_string(),
        })
    };
    let mut body = transport
        .open(request.url)?
        .take(request.size.saturating_add(1));
    let mut out = File::create(archive).map_err(io_error(archive))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; BUFFER];
    let mut found = 0u64;
    loop {
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(failed(error)),
        };
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read]).map_err(io_error(archive))?;
        found += read as u64;
    }
    if found != request.size {
        return Err(InstallError::Size {
            expected: request.size,
            found,
        });
    }
    if Hash(hasher.finalize().into()) != *request.sha256 {
        return Err(InstallError::Checksum {
            url: request.url.to_owned(),
        });
    }
    Ok(())
}

/// The parts of an entry's name, and whether it ends in `/` (a folder).
/// The name must be relative, without `\`, `:`, or NUL, and each part must
/// be neither empty, `.`, nor `..`.
fn entry_parts(name: &str) -> Result<(Vec<&str>, bool), &'static str> {
    if name.starts_with('/') {
        return Err("is an absolute path");
    }
    if name.contains(['\\', ':', '\0']) {
        return Err("contains '\\', ':', or NUL");
    }
    let (rest, is_dir) = match name.strip_suffix('/') {
        Some(rest) => (rest, true),
        None => (name, false),
    };
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.iter().any(|part| part.is_empty()) {
        return Err("has an empty part");
    }
    if parts.iter().any(|part| matches!(*part, "." | "..")) {
        return Err("has a '.' or '..' part");
    }
    Ok((parts, is_dir))
}

/// Copies `entry` to `out`, failing with `TooLarge` past `budget` bytes;
/// returns the bytes copied.
fn copy_within(
    entry: &mut impl Read,
    out: &mut File,
    budget: u64,
    archive: &Path,
    path: &Path,
) -> Result<u64, InstallError> {
    let mut buffer = vec![0; BUFFER];
    let mut copied = 0u64;
    loop {
        let read = match entry.read(&mut buffer) {
            Ok(0) => return Ok(copied),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(archive)(error)),
        };
        copied += read as u64;
        if copied > budget {
            return Err(InstallError::TooLarge);
        }
        out.write_all(&buffer[..read]).map_err(io_error(path))?;
    }
}

/// Creates the folder `path`, for the entry `name`, if it is not there; a
/// file there is refused.
fn ensure_dir(path: &Path, name: &str) -> Result<(), InstallError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(unsafe_entry(name, "is inside a file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            make_dir(path).map_err(io_error(path))
        }
        Err(source) => Err(InstallError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Creates a new folder, on Unix 0755 whatever the umask.
fn make_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)?;
    set_mode(path, 0o755)
}

/// Creates a new file, on Unix readable by the user alone until its mode
/// is set.
fn create_file(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Sets `path`'s Unix mode; nothing elsewhere.
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// The `id` and `version` of `celesta-plugin.json`, which must be the
/// request's. Discovery checks the rest of the manifest.
fn check_manifest(plugin: &Path, request: &InstallRequest) -> Result<(), InstallError> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        id: String,
        version: String,
    }
    let path = plugin.join(MANIFEST);
    let text =
        fs::read(&path).map_err(|error| InstallError::Manifest(format!("{MANIFEST}: {error}")))?;
    let manifest: Manifest = serde_json::from_slice(&text)
        .map_err(|error| InstallError::Manifest(format!("{MANIFEST}: {error}")))?;
    if manifest.id != request.id {
        return Err(InstallError::Manifest(format!(
            "{MANIFEST} names {:?}, not {:?}",
            manifest.id, request.id
        )));
    }
    match Version::parse(&manifest.version) {
        Ok(version) if version == *request.version => Ok(()),
        _ => Err(InstallError::Manifest(format!(
            "{MANIFEST} gives version {:?}, not {}",
            manifest.version, request.version
        ))),
    }
}
