use std::fs;
use std::path::{Path, PathBuf};

use celesta_registry_format::{FormatError, Hash, tree_hash};

fn basic() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/trees/basic")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn copy_of_basic() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    copy_dir(&basic(), t.path());
    t
}

#[test]
fn basic_matches_recorded_hash() {
    let want =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/trees/basic.hash"))
            .unwrap();
    assert_eq!(
        tree_hash(&basic()).unwrap(),
        Hash::parse(want.trim()).unwrap()
    );
}

#[test]
fn changed_byte_differs() {
    let t = copy_of_basic();
    let f = t.path().join("bin/macos-aarch64");
    let mut b = fs::read(&f).unwrap();
    b[0] ^= 1;
    fs::write(&f, b).unwrap();
    assert_ne!(tree_hash(t.path()).unwrap(), tree_hash(&basic()).unwrap());
}

#[test]
fn renamed_file_differs() {
    let t = copy_of_basic();
    fs::rename(
        t.path().join("licenses/LICENSE-MIT"),
        t.path().join("licenses/LICENSE-APACHE"),
    )
    .unwrap();
    assert_ne!(tree_hash(t.path()).unwrap(), tree_hash(&basic()).unwrap());
}

#[test]
fn extra_empty_directory_is_equal() {
    let t = copy_of_basic();
    fs::create_dir(t.path().join("empty")).unwrap();
    assert_eq!(tree_hash(t.path()).unwrap(), tree_hash(&basic()).unwrap());
}

#[test]
fn creation_order_does_not_matter() {
    let a = tempfile::tempdir().unwrap();
    fs::create_dir(a.path().join("d")).unwrap();
    fs::write(a.path().join("a"), "1").unwrap();
    fs::write(a.path().join("d/x"), "2").unwrap();
    fs::write(a.path().join("b"), "3").unwrap();

    let b = tempfile::tempdir().unwrap();
    fs::write(b.path().join("b"), "3").unwrap();
    fs::create_dir(b.path().join("d")).unwrap();
    fs::write(b.path().join("d/x"), "2").unwrap();
    fs::write(b.path().join("a"), "1").unwrap();

    assert_eq!(tree_hash(a.path()).unwrap(), tree_hash(b.path()).unwrap());
}

#[test]
fn keys_sort_by_bytes_not_by_component() {
    // "a-b" sorts before "a/b" ('-' < '/'), though component-wise "a" < "a-b".
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join("a")).unwrap();
    fs::write(t.path().join("a/b"), "x").unwrap();
    fs::write(t.path().join("a-b"), "y").unwrap();
    let mut h = b"celesta-tree-v1\n".to_vec();
    h.extend_from_slice(&entry("a-b", b"y"));
    h.extend_from_slice(&entry("a/b", b"x"));
    assert_eq!(tree_hash(t.path()).unwrap().0, sha(&h));
}

fn entry(key: &str, data: &[u8]) -> Vec<u8> {
    let mut v = (key.len() as u64).to_le_bytes().to_vec();
    v.extend_from_slice(key.as_bytes());
    v.extend_from_slice(&(data.len() as u64).to_le_bytes());
    v.extend_from_slice(data);
    v
}

fn sha(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).into()
}

#[test]
fn empty_directory_hashes_the_prefix_only() {
    let t = tempfile::tempdir().unwrap();
    assert_eq!(tree_hash(t.path()).unwrap().0, sha(b"celesta-tree-v1\n"));
}

#[cfg(unix)]
#[test]
fn changed_mode_is_equal() {
    use std::os::unix::fs::PermissionsExt;
    let t = copy_of_basic();
    let f = t.path().join("bin/macos-aarch64");
    fs::set_permissions(&f, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(tree_hash(t.path()).unwrap(), tree_hash(&basic()).unwrap());
}

#[cfg(unix)]
#[test]
fn symlink_is_rejected() {
    let t = copy_of_basic();
    std::os::unix::fs::symlink("celesta-plugin.json", t.path().join("link")).unwrap();
    match tree_hash(t.path()) {
        Err(FormatError::TreeHash(p, why)) => {
            assert_eq!(p, t.path().join("link"));
            assert_eq!(why, "symbolic link");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn symlinked_root_is_rejected() {
    let t = copy_of_basic();
    let l = tempfile::tempdir().unwrap();
    let link = l.path().join("root");
    std::os::unix::fs::symlink(t.path(), &link).unwrap();
    assert!(matches!(
        tree_hash(&link),
        Err(FormatError::TreeHash(_, w)) if w == "symbolic link"
    ));
}

#[test]
fn os_junk_files_are_skipped_at_any_depth() {
    let want = tree_hash(&basic()).unwrap();
    for name in [
        ".DS_Store",
        ".ds_store",
        "Thumbs.db",
        "THUMBS.DB",
        "desktop.ini",
        "Desktop.INI",
    ] {
        for dir in ["", "bin", "licenses", "new/deep"] {
            let t = copy_of_basic();
            let at = t.path().join(dir);
            fs::create_dir_all(&at).unwrap();
            fs::write(at.join(name), "junk").unwrap();
            assert_eq!(tree_hash(t.path()).unwrap(), want, "{dir}/{name}");
        }
    }
}

#[test]
fn names_near_junk_names_are_hashed() {
    let want = tree_hash(&basic()).unwrap();
    for name in [
        ".DS_Store.bak",
        "Thumbs.db~",
        "x.DS_Store",
        "desktop.ini.txt",
        "Thumbs",
    ] {
        let t = copy_of_basic();
        fs::write(t.path().join(name), "junk").unwrap();
        assert_ne!(tree_hash(t.path()).unwrap(), want, "{name}");
    }
    // Only regular files are skipped: a directory with a junk name is walked.
    let t = copy_of_basic();
    fs::create_dir(t.path().join(".DS_Store")).unwrap();
    fs::write(t.path().join(".DS_Store/x"), "1").unwrap();
    assert_ne!(tree_hash(t.path()).unwrap(), want);
}
