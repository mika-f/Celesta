//! The tree hash of a plugin directory.
//!
//! ```text
//! entries = every regular file under dir, walked without following links,
//!           except OS junk files (below)
//! key     = relative path, components joined with "/"
//! sort entries by key, byte order
//! sha256( "celesta-tree-v1\n"
//!         || for each: u64le(len(key)) || key || u64le(len(contents)) || contents )
//! ```
//!
//! A regular file named `.DS_Store`, `Thumbs.db`, or `desktop.ini`, compared
//! ASCII case-insensitively, is skipped at any depth: operating systems create
//! them in folders on their own. Publishing refuses archives that contain those
//! names, so skipping them cannot hide registry content.
//!
//! Empty directories and file modes are not hashed. The registry service
//! implements the same algorithm byte for byte.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{FormatError, Hash};

const PREFIX: &[u8] = b"celesta-tree-v1\n";

/// File names the tree hash skips, compared ASCII case-insensitively.
const JUNK: [&str; 3] = [".DS_Store", "Thumbs.db", "desktop.ini"];

/// Whether the tree hash skips a file with this name: `.DS_Store`,
/// `Thumbs.db`, or `desktop.ini`, compared ASCII case-insensitively.
pub fn is_os_junk_name(name: &str) -> bool {
    JUNK.iter().any(|j| j.eq_ignore_ascii_case(name))
}

fn fail(path: &Path, why: impl Into<String>) -> FormatError {
    FormatError::TreeHash(path.to_path_buf(), why.into())
}

/// Hashes the regular files under `dir`; see the module docs for the algorithm.
pub fn tree_hash(dir: &Path) -> Result<Hash, FormatError> {
    let meta = fs::symlink_metadata(dir).map_err(|e| fail(dir, e.to_string()))?;
    if meta.file_type().is_symlink() {
        return Err(fail(dir, "symbolic link"));
    }
    if !meta.is_dir() {
        return Err(fail(dir, "not a directory"));
    }

    let mut entries = Vec::new();
    walk(dir, "", &mut entries)?;
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut hasher = Sha256::new();
    hasher.update(PREFIX);
    for (key, path) in entries {
        let contents = fs::read(&path).map_err(|e| fail(&path, e.to_string()))?;
        hasher.update((key.len() as u64).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update((contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    Ok(Hash(hasher.finalize().into()))
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), FormatError> {
    let read = fs::read_dir(dir).map_err(|e| fail(dir, e.to_string()))?;
    for entry in read {
        let entry = entry.map_err(|e| fail(dir, e.to_string()))?;
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| fail(&path, "not UTF-8"))?;
        let meta = fs::symlink_metadata(&path).map_err(|e| fail(&path, e.to_string()))?;
        let ty = meta.file_type();
        let junk = is_os_junk_name(&name);
        let key = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if ty.is_symlink() {
            return Err(fail(&path, "symbolic link"));
        } else if ty.is_dir() {
            walk(&path, &key, out)?;
        } else if ty.is_file() {
            if !junk {
                out.push((key, path));
            }
        } else {
            return Err(fail(&path, "not a regular file"));
        }
    }
    Ok(())
}
