use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;

use celesta_plugin_host::{PluginSource, SearchPath, discover_in};
use celesta_registry_client::{
    DirTransport, InstallError, InstallRequest, Store, Transport, TransportError,
};
use celesta_registry_format::{Hash, tree_hash};
use semver::Version;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{System, ZipWriter};

const BASE: &str = "https://registry.test";
const ID: &str = "acme.tool";
const VERSION: &str = "1.2.0";
const PATH: &str = "/archives/acme.tool/1.2.0/any.zip";

fn manifest(id: &str) -> Vec<u8> {
    format!(r#"{{"id":"{id}","version":"{VERSION}","name":"Tool","celesta":">=0.0.0"}}"#)
        .into_bytes()
}

/// The good plugin: `(name, Unix mode, contents)`.
fn files(id: &str) -> Vec<(&'static str, u32, Vec<u8>)> {
    vec![
        ("celesta-plugin.json", 0o644, manifest(id)),
        ("bin/tool", 0o755, b"#!/bin/sh\necho tool\n".to_vec()),
        ("data/info.txt", 0o644, b"hello".to_vec()),
    ]
}

fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().system(System::Unix)
}

fn archive(build: impl FnOnce(&mut ZipWriter<Cursor<Vec<u8>>>)) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    build(&mut writer);
    writer.finish().unwrap().into_inner()
}

fn add_files(writer: &mut ZipWriter<Cursor<Vec<u8>>>, files: &[(&str, u32, Vec<u8>)]) {
    for (name, mode, contents) in files {
        writer
            .start_file(*name, options().unix_permissions(*mode))
            .unwrap();
        writer.write_all(contents).unwrap();
    }
}

fn plugin_archive(files: &[(&str, u32, Vec<u8>)]) -> Vec<u8> {
    archive(|writer| {
        writer.add_directory("bin/", options()).unwrap();
        writer.add_directory("data/", options()).unwrap();
        add_files(writer, files);
    })
}

/// The tree hash of `files` written out to a folder.
fn tree_of(files: &[(&str, u32, Vec<u8>)]) -> Hash {
    let dir = TempDir::new().unwrap();
    for (name, _, contents) in files {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    tree_hash(dir.path()).unwrap()
}

fn sha256(bytes: &[u8]) -> Hash {
    Hash(Sha256::digest(bytes).into())
}

fn version() -> Version {
    Version::parse(VERSION).unwrap()
}

struct Fixture {
    served: TempDir,
    home: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            served: TempDir::new().unwrap(),
            home: TempDir::new().unwrap(),
        }
    }

    fn root(&self) -> PathBuf {
        self.home.path().join("registry")
    }

    fn store(&self) -> Store {
        Store::new(self.root())
    }

    fn transport(&self) -> DirTransport {
        DirTransport {
            base: BASE.to_owned(),
            root: self.served.path().to_path_buf(),
        }
    }

    fn serve(&self, bytes: &[u8]) {
        let file = self.served.path().join(PATH.trim_start_matches('/'));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }

    /// Serves `bytes` and installs them with `store`, expecting
    /// `tree_hash`, and the size and checksum `bytes` have.
    fn install_with(
        &self,
        store: &Store,
        bytes: &[u8],
        tree_hash: &Hash,
    ) -> Result<PathBuf, InstallError> {
        self.serve(bytes);
        let sha256 = sha256(bytes);
        let version = version();
        let url = format!("{BASE}{PATH}");
        store.install(
            &self.transport(),
            &InstallRequest {
                id: ID,
                version: &version,
                url: &url,
                size: bytes.len() as u64,
                sha256: &sha256,
                tree_hash,
            },
        )
    }

    fn install(&self, bytes: &[u8], tree_hash: &Hash) -> Result<PathBuf, InstallError> {
        self.install_with(&self.store(), bytes, tree_hash)
    }

    /// Nothing is left in the store: no version folder and no temporary one.
    fn assert_clean(&self) {
        assert!(!self.store().path(ID, &version()).exists());
        self.assert_no_temp();
    }

    fn assert_no_temp(&self) {
        let Ok(entries) = fs::read_dir(self.root()) else {
            return;
        };
        for entry in entries {
            let name = entry.unwrap().file_name();
            assert!(
                !name.to_string_lossy().starts_with(".tmp-"),
                "{name:?} was left in the store"
            );
        }
    }
}

/// Fails the test if asked for anything.
struct Refuse;

impl Transport for Refuse {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        panic!("asked for {url}");
    }
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[test]
fn an_archive_installs_and_hashes() {
    let fixture = Fixture::new();
    let files = files(ID);
    let tree = tree_of(&files);
    let installed = fixture.install(&plugin_archive(&files), &tree).unwrap();
    assert_eq!(installed, fixture.root().join(ID).join(VERSION));
    assert_eq!(fixture.store().path(ID, &version()), installed);
    assert_eq!(tree_hash(&installed).unwrap(), tree);
    assert!(fixture.store().contains(ID, &version(), &tree));
    assert!(!fixture.store().contains(ID, &version(), &sha256(b"other")));
    assert_eq!(
        fs::read(installed.join("data/info.txt")).unwrap(),
        b"hello".to_vec()
    );
    #[cfg(unix)]
    {
        assert_eq!(mode(&installed.join("bin/tool")), 0o755);
        assert_eq!(mode(&installed.join("data/info.txt")), 0o644);
        assert_eq!(mode(&installed.join("celesta-plugin.json")), 0o644);
        assert_eq!(mode(&installed.join("bin")), 0o755);
        assert_eq!(mode(&installed), 0o755);
        assert_eq!(mode(&fixture.root().join(ID)), 0o700);
        assert_eq!(mode(&fixture.root()), 0o700);
    }
    fixture.assert_no_temp();
}

#[test]
fn a_wrong_checksum_is_refused() {
    let fixture = Fixture::new();
    let files = files(ID);
    let bytes = plugin_archive(&files);
    fixture.serve(&bytes);
    let version = version();
    let url = format!("{BASE}{PATH}");
    let error = fixture
        .store()
        .install(
            &fixture.transport(),
            &InstallRequest {
                id: ID,
                version: &version,
                url: &url,
                size: bytes.len() as u64,
                sha256: &sha256(b"something else"),
                tree_hash: &tree_of(&files),
            },
        )
        .unwrap_err();
    assert!(
        matches!(&error, InstallError::Checksum { url: u } if *u == url),
        "{error:?}"
    );
    fixture.assert_clean();
}

#[test]
fn a_wrong_size_is_refused() {
    let fixture = Fixture::new();
    let files = files(ID);
    let bytes = plugin_archive(&files);
    fixture.serve(&bytes);
    let version = version();
    let url = format!("{BASE}{PATH}");
    let real = bytes.len() as u64;
    for listed in [real - 1, real + 1] {
        let error = fixture
            .store()
            .install(
                &fixture.transport(),
                &InstallRequest {
                    id: ID,
                    version: &version,
                    url: &url,
                    size: listed,
                    sha256: &sha256(&bytes),
                    tree_hash: &tree_of(&files),
                },
            )
            .unwrap_err();
        assert!(
            matches!(error, InstallError::Size { expected, .. } if expected == listed),
            "{error:?}"
        );
        fixture.assert_clean();
    }
}

/// Serves an endless body of zeros.
struct Endless;

impl Transport for Endless {
    fn open(&self, _url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        Ok(Box::new(std::io::repeat(0)))
    }
}

#[test]
fn an_endless_body_is_cut_off() {
    let fixture = Fixture::new();
    let version = version();
    let error = fixture
        .store()
        .install(
            &Endless,
            &InstallRequest {
                id: ID,
                version: &version,
                url: &format!("{BASE}{PATH}"),
                size: 1000,
                sha256: &sha256(b""),
                tree_hash: &sha256(b""),
            },
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            InstallError::Size {
                expected: 1000,
                found: 1001
            }
        ),
        "{error:?}"
    );
    fixture.assert_clean();
}

#[test]
fn a_wrong_tree_hash_is_refused() {
    let fixture = Fixture::new();
    let files = files(ID);
    let wrong = sha256(b"not the tree");
    let error = fixture
        .install(&plugin_archive(&files), &wrong)
        .unwrap_err();
    assert!(
        matches!(&error, InstallError::TreeHash { expected, found }
            if *expected == wrong && *found == tree_of(&files)),
        "{error:?}"
    );
    fixture.assert_clean();
}

#[test]
fn a_wrong_manifest_id_is_refused() {
    let fixture = Fixture::new();
    let files = files("acme.other");
    let error = fixture
        .install(&plugin_archive(&files), &tree_of(&files))
        .unwrap_err();
    assert!(matches!(error, InstallError::Manifest(_)), "{error:?}");
    fixture.assert_clean();
    assert!(!fixture.root().join("acme.other").exists());
}

#[test]
fn zip_slip_is_refused() {
    for name in [
        "../evil", "/abs", "a\\b", "C:x", "a//b", "./a", "a/./b", "a/../b", "nul\0x",
    ] {
        let fixture = Fixture::new();
        let mut files = files(ID);
        files.push((name, 0o644, b"evil".to_vec()));
        let error = fixture
            .install(&plugin_archive(&files), &tree_of(&files[..3]))
            .unwrap_err();
        assert!(
            matches!(&error, InstallError::Unsafe { entry, .. } if entry == name),
            "{name:?}: {error:?}"
        );
        fixture.assert_clean();
        assert!(!fixture.home.path().join("evil").exists());
        assert!(!fixture.root().join("evil").exists());
    }
}

#[test]
fn links_are_refused() {
    let fixture = Fixture::new();
    let files = files(ID);
    let bytes = archive(|writer| {
        add_files(writer, &files);
        writer
            .add_symlink("link", "/etc/passwd", options().unix_permissions(0o777))
            .unwrap();
    });
    let error = fixture.install(&bytes, &tree_of(&files)).unwrap_err();
    assert!(
        matches!(&error, InstallError::Unsafe { entry, .. } if entry == "link"),
        "{error:?}"
    );
    fixture.assert_clean();
}

/// `bytes` with every `from` replaced by `to`, which has the same length.
fn replace(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let mut out = bytes.to_vec();
    let mut at = 0;
    while let Some(found) = out[at..].windows(from.len()).position(|w| w == from) {
        out[at + found..at + found + from.len()].copy_from_slice(to);
        at += found + from.len();
    }
    out
}

#[test]
fn duplicate_names_are_refused() {
    let files = files(ID);
    // The writer refuses duplicates, so rename one entry in the bytes.
    let same = replace(
        &archive(|writer| {
            add_files(writer, &files);
            add_files(
                writer,
                &[
                    ("dup1.txt", 0o644, b"one".to_vec()),
                    ("dup2.txt", 0o644, b"two".to_vec()),
                ],
            );
        }),
        b"dup2.txt",
        b"dup1.txt",
    );
    // A file and a folder of the same name.
    let file_and_folder = archive(|writer| {
        add_files(writer, &files);
        add_files(writer, &[("x", 0o644, b"x".to_vec())]);
        writer.add_directory("x/", options()).unwrap();
    });
    for bytes in [same, file_and_folder] {
        let fixture = Fixture::new();
        let error = fixture.install(&bytes, &tree_of(&files)).unwrap_err();
        assert!(matches!(error, InstallError::Unsafe { .. }), "{error:?}");
        fixture.assert_clean();
    }
}

#[test]
fn bombs_are_refused() {
    let files = files(ID);
    let tree = tree_of(&files);
    let total: u64 = files.iter().map(|(_, _, c)| c.len() as u64).sum();

    // One byte over the limit on unpacked bytes.
    let fixture = Fixture::new();
    let store = Store::with_limits(fixture.root(), total - 1, 100);
    let error = fixture
        .install_with(&store, &plugin_archive(&files), &tree)
        .unwrap_err();
    assert!(matches!(error, InstallError::TooLarge), "{error:?}");
    fixture.assert_clean();

    // Exactly at the limits installs.
    let fixture = Fixture::new();
    let store = Store::with_limits(fixture.root(), total, 5);
    fixture
        .install_with(&store, &plugin_archive(&files), &tree)
        .unwrap();

    // One entry over the limit on entries (three files and two folders).
    let fixture = Fixture::new();
    let store = Store::with_limits(fixture.root(), total, 4);
    let error = fixture
        .install_with(&store, &plugin_archive(&files), &tree)
        .unwrap_err();
    assert!(matches!(error, InstallError::TooLarge), "{error:?}");
    fixture.assert_clean();

    // A small archive that unpacks to far more than its size.
    let fixture = Fixture::new();
    let mut big = files.clone();
    big.push(("zeros.bin", 0o644, vec![0; 4 * 1024 * 1024]));
    let bytes = archive(|writer| {
        for (name, mode, contents) in &big {
            writer
                .start_file(
                    *name,
                    options()
                        .unix_permissions(*mode)
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(contents).unwrap();
        }
    });
    assert!(bytes.len() < 64 * 1024);
    let store = Store::with_limits(fixture.root(), 1024 * 1024, 100);
    let error = fixture
        .install_with(&store, &bytes, &tree_of(&big))
        .unwrap_err();
    assert!(matches!(error, InstallError::TooLarge), "{error:?}");
    fixture.assert_clean();
}

#[test]
fn a_correct_existing_copy_is_kept() {
    let fixture = Fixture::new();
    let files = files(ID);
    let tree = tree_of(&files);
    let installed = fixture.install(&plugin_archive(&files), &tree).unwrap();
    let version = version();
    let again = fixture
        .store()
        .install(
            &Refuse,
            &InstallRequest {
                id: ID,
                version: &version,
                url: &format!("{BASE}{PATH}"),
                size: 1,
                sha256: &sha256(b""),
                tree_hash: &tree,
            },
        )
        .unwrap();
    assert_eq!(again, installed);
}

#[test]
fn a_tampered_existing_copy_is_replaced() {
    let fixture = Fixture::new();
    let files = files(ID);
    let tree = tree_of(&files);
    let bytes = plugin_archive(&files);
    let installed = fixture.install(&bytes, &tree).unwrap();
    fs::write(installed.join("data/info.txt"), b"tampered").unwrap();
    fs::write(installed.join("extra.txt"), b"planted").unwrap();
    assert!(!fixture.store().contains(ID, &version(), &tree));

    let again = fixture.install(&bytes, &tree).unwrap();
    assert_eq!(again, installed);
    assert_eq!(tree_hash(&again).unwrap(), tree);
    assert!(!again.join("extra.txt").exists());
    fixture.assert_no_temp();
}

#[test]
fn a_stale_temporary_folder_is_ignored() {
    let fixture = Fixture::new();
    let stale = fixture.root().join(".tmp-old").join("plugin");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("celesta-plugin.json"), manifest(ID)).unwrap();

    let files = files(ID);
    let tree = tree_of(&files);
    fixture.install(&plugin_archive(&files), &tree).unwrap();
    assert!(stale.join("celesta-plugin.json").exists());

    let (found, errors) = discover_in(&[SearchPath {
        path: fixture.root(),
        source: PluginSource::Registry,
    }]);
    assert!(found.is_empty(), "{found:?}");
    assert!(
        errors
            .iter()
            .all(|error| !error.to_string().contains(".tmp-old")),
        "{errors:?}"
    );
}

#[cfg(unix)]
#[test]
fn the_store_passes_the_permission_check() {
    use celesta_plugin_host::unsafe_reason;
    for umask in [0o022, 0o002] {
        let fixture = Fixture::new();
        let files = files(ID);
        // SAFETY: umask has no preconditions; it is restored below.
        let old = unsafe { libc::umask(umask) };
        let installed = fixture.install(&plugin_archive(&files), &tree_of(&files));
        unsafe { libc::umask(old) };
        let installed = installed.unwrap();
        assert_eq!(unsafe_reason(&fixture.root()), None);
        assert_eq!(unsafe_reason(&fixture.root().join(ID)), None);
        assert_eq!(unsafe_reason(&installed), None);
        assert_eq!(unsafe_reason(&installed.join("bin")), None);
        assert_eq!(unsafe_reason(&installed.join("bin/tool")), None);
    }
}
