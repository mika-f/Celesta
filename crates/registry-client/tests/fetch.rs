use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use celesta_plugin_host::load_cached_advisories;
use celesta_registry_client::{
    ClientError, DirTransport, MAX_JSON_BYTES, Registry, Transport, TransportError,
};
use celesta_registry_format::{FormatError, RootKey};
use tempfile::TempDir;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const BASE: &str = "https://registry.test";

fn signed() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../registry-format/fixtures/signed")
}

fn at(text: &str) -> OffsetDateTime {
    OffsetDateTime::parse(text, &Rfc3339).unwrap()
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Fixture {
    served: TempDir,
    cache: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let served = TempDir::new().unwrap();
        copy_dir(&signed(), served.path());
        Self {
            served,
            cache: TempDir::new().unwrap(),
        }
    }

    fn registry(&self, transport: Box<dyn Transport>) -> Registry {
        let root = fs::read_to_string(signed().join("root.pub")).unwrap();
        Registry::new(
            BASE,
            RootKey::from_base64(&root).unwrap(),
            transport,
            self.cache.path().to_path_buf(),
        )
    }

    fn dir_registry(&self) -> Registry {
        self.registry(Box::new(DirTransport {
            base: BASE.into(),
            root: self.served.path().to_path_buf(),
        }))
    }
}

struct Failing;

impl Transport for Failing {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        Err(TransportError::Failed {
            url: url.into(),
            message: "offline".into(),
        })
    }
}

struct Huge;

impl Transport for Huge {
    fn open(&self, _: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        Ok(Box::new(Cursor::new(vec![
            b' ';
            MAX_JSON_BYTES as usize + 1
        ])))
    }
}

fn format_source(error: ClientError) -> FormatError {
    match error {
        ClientError::Format { source, .. } => source,
        other => panic!("expected a format error, got {other}"),
    }
}

const NOW: &str = "2026-11-20T01:00:00Z";

#[test]
fn files_verify_and_are_cached() {
    let fixture = Fixture::new();
    let registry = fixture.dir_registry();
    let index = registry.index("vrm", at(NOW)).unwrap();
    assert_eq!(index.id, "vrm");
    assert!(registry.search(at(NOW)).is_ok());
    fs::remove_dir_all(fixture.served.path()).unwrap();
    assert_eq!(registry.cached_index("vrm").unwrap().id, "vrm");
    assert!(registry.cached_index("other").is_none());
}

#[test]
fn a_bad_signature_is_refused() {
    let fixture = Fixture::new();
    let file = fixture.served.path().join("index/vrm.json");
    let mut bytes = fs::read(&file).unwrap();
    let at_byte = bytes.iter().position(|&b| b == b'V').unwrap();
    bytes[at_byte] = b'W';
    fs::write(&file, bytes).unwrap();
    let registry = fixture.dir_registry();
    let error = registry.index("vrm", at(NOW)).unwrap_err();
    assert!(error.to_string().contains("/index/vrm.json"), "{error}");
    assert!(matches!(format_source(error), FormatError::Signature(_)));
    assert!(registry.cached_index("vrm").is_none());
    assert!(!fixture.cache.path().join("index/vrm.json").exists());
}

#[test]
fn an_older_file_is_a_rollback() {
    let fixture = Fixture::new();
    fs::write(
        fixture.cache.path().join("stamps.json"),
        r#"{"/index/vrm.json":"2026-12-01T00:00:00Z"}"#,
    )
    .unwrap();
    let error = fixture.dir_registry().index("vrm", at(NOW)).unwrap_err();
    assert!(matches!(format_source(error), FormatError::Rollback));
    assert!(!fixture.cache.path().join("index/vrm.json").exists());
}

#[test]
fn a_future_list_is_refused() {
    let fixture = Fixture::new();
    let now = at("2026-11-20T00:00:00Z") - time::Duration::minutes(11);
    let error = fixture.dir_registry().refresh_advisories(now).unwrap_err();
    assert!(matches!(format_source(error), FormatError::FutureDated));
    assert!(!fixture.cache.path().join("advisories.json").exists());
}

#[test]
fn a_failed_refresh_keeps_the_cached_list() {
    let fixture = Fixture::new();
    fixture.dir_registry().refresh_advisories(at(NOW)).unwrap();
    let cached = fixture.cache.path().join("advisories.json");
    let before = fs::read(&cached).unwrap();
    let registry = fixture.registry(Box::new(Failing));
    assert!(registry.refresh_advisories(at(NOW)).is_err());
    assert_eq!(fs::read(&cached).unwrap(), before);
    assert!(load_cached_advisories(&cached).unwrap().is_some());
    assert!(registry.cached_advisories().is_some());
}

#[test]
fn an_expired_key_does_not_drop_the_cache() {
    let fixture = Fixture::new();
    let registry = fixture.dir_registry();
    registry.refresh_advisories(at(NOW)).unwrap();
    let cached = fixture.cache.path().join("advisories.json");
    let before = fs::read(&cached).unwrap();
    assert!(
        registry
            .refresh_advisories(at("2100-01-01T00:00:00Z"))
            .is_err()
    );
    assert_eq!(fs::read(&cached).unwrap(), before);
    assert!(load_cached_advisories(&cached).unwrap().is_some());
}

#[test]
fn oversized_files_are_refused() {
    let fixture = Fixture::new();
    let error = fixture
        .registry(Box::new(Huge))
        .refresh_advisories(at(NOW))
        .unwrap_err();
    assert!(matches!(error, ClientError::TooLarge { .. }), "{error}");
}

#[test]
fn a_missing_plugin_is_named() {
    let fixture = Fixture::new();
    let error = fixture.dir_registry().index("nope", at(NOW)).unwrap_err();
    assert!(matches!(error, ClientError::NoSuchPlugin(id) if id == "nope"));
}
