//! A registry served from a temporary folder, signed with the fixture keys
//! (`crates/registry-format/fixtures/keys/README.md`), that tests publish
//! plugin versions to.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use celesta_registry_client::{DirTransport, Registry};
use celesta_registry_format::{Hash, RootKey, key_message, signed_message, tree_hash};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};
use zip::write::SimpleFileOptions;
use zip::{System, ZipWriter};

pub const BASE: &str = "https://registry.test";

/// The Celesta range every published version gives.
pub const CELESTA_RANGE: &str = ">=0.9.0, <0.10.0";

const ONLINE_KEYID: &str = "2026-11";
const ONLINE_EXPIRES: &str = "2099-01-01T00:00:00Z";

/// The first `generated` stamp; each write is one second later, so a
/// rewritten file is never refused as a rollback.
const FIRST_GENERATED: &str = "2026-11-20T00:00:00Z";

pub fn at(text: &str) -> OffsetDateTime {
    OffsetDateTime::parse(text, &Rfc3339).unwrap()
}

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn sha256(bytes: &[u8]) -> Hash {
    Hash(Sha256::digest(bytes).into())
}

/// The files of one build: the manifest and a file naming the target, so
/// each target's build has its own tree hash.
pub fn plugin_files(id: &str, version: &str, target: &str) -> Vec<(String, Vec<u8>)> {
    let manifest = json!({
        "id": id,
        "version": version,
        "name": "Tool",
        "protocol": 1,
        "celesta": CELESTA_RANGE,
    });
    vec![
        (
            "celesta-plugin.json".to_owned(),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        ),
        (
            "data/build.txt".to_owned(),
            format!("{id} {version} for {target}\n").into_bytes(),
        ),
    ]
}

fn archive(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().system(System::Unix);
    writer.add_directory("data/", options).unwrap();
    for (name, contents) in files {
        writer
            .start_file(name.as_str(), options.unix_permissions(0o644))
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// The tree hash of `files` written out to a folder.
pub fn tree_of(files: &[(String, Vec<u8>)]) -> Hash {
    let dir = TempDir::new().unwrap();
    write_files(dir.path(), files);
    tree_hash(dir.path()).unwrap()
}

pub fn write_files(dir: &Path, files: &[(String, Vec<u8>)]) {
    for (name, contents) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

pub struct TestRegistry {
    served: TempDir,
    online: SigningKey,
    root: SigningKey,
    generated: Cell<OffsetDateTime>,
    /// Each plugin's index, as last written.
    indexes: RefCell<BTreeMap<String, Value>>,
}

impl TestRegistry {
    /// A registry with a signed `config.json` and no plugins.
    pub fn new() -> Self {
        let registry = Self {
            served: TempDir::new().unwrap(),
            online: SigningKey::from_bytes(&[0x02; 32]),
            root: SigningKey::from_bytes(&[0x01; 32]),
            generated: Cell::new(at(FIRST_GENERATED)),
            indexes: RefCell::default(),
        };
        let public = b64(registry.online.verifying_key().as_bytes());
        let certified = registry
            .root
            .sign(&key_message(ONLINE_KEYID, &public, ONLINE_EXPIRES));
        let config = json!({
            "format": 1,
            "generated": registry.stamp(),
            "api": format!("{BASE}/api/v1"),
            "keys": [{
                "keyid": ONLINE_KEYID,
                "public": public,
                "expires": ONLINE_EXPIRES,
                "signature": b64(&certified.to_bytes()),
            }],
        });
        registry.write_signed("/config.json", &config);
        registry
    }

    pub fn root_key(&self) -> RootKey {
        RootKey(self.root.verifying_key().to_bytes())
    }

    pub fn transport(&self) -> DirTransport {
        DirTransport {
            base: BASE.to_owned(),
            root: self.served.path().to_path_buf(),
        }
    }

    /// A client of this registry that caches in `cache`.
    pub fn client(&self, cache: &Path) -> Registry {
        Registry::new(
            BASE,
            self.root_key(),
            Box::new(self.transport()),
            cache.to_path_buf(),
        )
    }

    /// The next `generated` stamp.
    fn stamp(&self) -> String {
        let stamp = self.generated.get();
        self.generated.set(stamp + Duration::seconds(1));
        stamp.format(&Rfc3339).unwrap()
    }

    fn served_path(&self, path: &str) -> PathBuf {
        self.served.path().join(path.trim_start_matches('/'))
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let file = self.served_path(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }

    /// Writes `value` at the served `path` with its `.sig`.
    fn write_signed(&self, path: &str, value: &Value) {
        let bytes = serde_json::to_vec_pretty(value).unwrap();
        let signature = self.online.sign(&signed_message(path, &bytes));
        let sig = json!({ "keyid": ONLINE_KEYID, "signature": b64(&signature.to_bytes()) });
        self.write(path, &bytes);
        self.write(
            &format!("{path}.sig"),
            &serde_json::to_vec_pretty(&sig).unwrap(),
        );
    }

    /// Publishes `version` of `id` with a build for each of `targets`, with
    /// `roles` as the plugin's kind.
    pub fn publish_with_roles(&self, id: &str, version: &str, targets: &[&str], roles: &[&str]) {
        let mut builds = serde_json::Map::new();
        for target in targets {
            let files = plugin_files(id, version, target);
            let bytes = archive(&files);
            let path = format!("/archives/{id}/{version}/{target}.zip");
            self.write(&path, &bytes);
            builds.insert(
                (*target).to_owned(),
                json!({
                    "url": format!("{BASE}{path}"),
                    "size": bytes.len(),
                    "sha256": sha256(&bytes).to_string(),
                    "treeHash": tree_of(&files).to_string(),
                }),
            );
        }
        let entry = json!({
            "version": version,
            "celesta": CELESTA_RANGE,
            "protocol": 1,
            "published": FIRST_GENERATED,
            "yanked": false,
            "withdrawn": false,
            "targets": builds,
        });
        let mut indexes = self.indexes.borrow_mut();
        let index = indexes.entry(id.to_owned()).or_insert_with(|| {
            json!({
                "format": 1,
                "id": id,
                "publisher": id.split_once('.').map_or("celesta", |(p, _)| p),
                "kind": { "roles": [], "program": false, "module": true },
                "description": "A test plugin.",
                "versions": [],
            })
        });
        index["kind"]["roles"] = json!(roles);
        index["versions"].as_array_mut().unwrap().push(entry);
        drop(indexes);
        self.write_index(id);
    }

    pub fn publish(&self, id: &str, version: &str, targets: &[&str]) {
        self.publish_with_roles(id, version, targets, &["content"]);
    }

    /// Applies `change` to the entry of `version` in `id`'s index and
    /// writes the index again.
    pub fn edit(&self, id: &str, version: &str, change: impl FnOnce(&mut Value)) {
        let mut indexes = self.indexes.borrow_mut();
        let entry = indexes
            .get_mut(id)
            .unwrap()
            .get_mut("versions")
            .and_then(Value::as_array_mut)
            .unwrap()
            .iter_mut()
            .find(|entry| entry["version"] == version)
            .unwrap();
        change(entry);
        drop(indexes);
        self.write_index(id);
    }

    fn write_index(&self, id: &str) {
        let mut index = self.indexes.borrow()[id].clone();
        index["generated"] = json!(self.stamp());
        self.write_signed(&format!("/index/{id}.json"), &index);
    }

    /// Serves `advisories` as `advisories.json`.
    pub fn advisories(&self, advisories: Value) {
        let list = json!({
            "format": 1,
            "generated": self.stamp(),
            "advisories": advisories,
        });
        self.write_signed("/advisories.json", &list);
    }
}
