mod common;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use celesta_plugin_host::{LOCK_FILE, Lockfile, PROJECT_CONFIG, UserSettings};
use celesta_registry_client::{
    ClientError, Environment, ProjectError, Registry, Store, Transport, TransportError, Update,
    install_global, install_locked, lock_project,
};
use celesta_registry_format::{Hash, Target, tree_hash};
use common::{BASE, TestRegistry, at, plugin_files, tree_of, write_files};
use semver::{Version, VersionReq};
use serde_json::json;
use tempfile::TempDir;
use time::OffsetDateTime;

const ID: &str = "acme.tool";
const MAC: &str = "macos-aarch64";
const WIN: &str = "windows-x86_64";
const LINUX: &str = "linux-x86_64";
const RELEASE: [&str; 3] = [MAC, WIN, LINUX];

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn now() -> OffsetDateTime {
    at("2026-11-20T01:00:00Z")
}

fn host() -> Target {
    Target::parse(MAC).unwrap()
}

/// A transport that fails the test if anything is fetched.
struct Panicking;

impl Transport for Panicking {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        panic!("fetched {url}");
    }
}

/// One machine: its registry cache and store, in folders of their own.
struct Machine {
    home: TempDir,
    registry: Registry,
    store: Store,
    celesta: Version,
    host: Target,
    bundled: Option<PathBuf>,
}

impl Machine {
    fn new(registry: &TestRegistry) -> Self {
        let home = TempDir::new().unwrap();
        let client = registry.client(&home.path().join("registry-cache"));
        Self::with_registry(home, client)
    }

    /// A machine whose registry fetches nothing.
    fn offline(registry: &TestRegistry) -> Self {
        let home = TempDir::new().unwrap();
        let client = Registry::new(
            BASE,
            registry.root_key(),
            Box::new(Panicking),
            home.path().join("registry-cache"),
        );
        Self::with_registry(home, client)
    }

    fn with_registry(home: TempDir, registry: Registry) -> Self {
        let store = Store::new(home.path().join("registry"));
        Self {
            home,
            registry,
            store,
            celesta: v("0.9.0"),
            host: host(),
            bundled: None,
        }
    }

    fn env(&self) -> Environment<'_> {
        Environment {
            registry: &self.registry,
            store: &self.store,
            celesta: &self.celesta,
            host: &self.host,
            bundled: self.bundled.as_deref(),
            now: now(),
        }
    }

    fn installed(&self, id: &str, version: &str) -> Option<Hash> {
        tree_hash(&self.store.path(id, &v(version))).ok()
    }
}

/// A project folder whose `celesta.config.json` depends on `dependencies`.
fn project(dependencies: &[(&str, &str)]) -> TempDir {
    let root = TempDir::new().unwrap();
    write_config(root.path(), dependencies);
    root
}

fn write_config(root: &Path, dependencies: &[(&str, &str)]) {
    let dependencies: serde_json::Map<_, _> = dependencies
        .iter()
        .map(|(id, req)| ((*id).to_owned(), json!(req)))
        .collect();
    fs::write(
        root.join(PROJECT_CONFIG),
        serde_json::to_vec(&json!({ "plugins": { "dependencies": dependencies } })).unwrap(),
    )
    .unwrap();
}

/// The tree hash of the host build of `version` as published.
fn host_tree(version: &str) -> Hash {
    tree_of(&plugin_files(ID, version, MAC))
}

/// A registry with `acme.tool` 1.0.0 for every release target, a project
/// depending on it, and the project locked on a first machine.
fn locked() -> (TestRegistry, TempDir) {
    let registry = TestRegistry::new();
    registry.publish(ID, "1.0.0", &RELEASE);
    let root = project(&[(ID, "1")]);
    let first = Machine::new(&registry);
    lock_project(&first.env(), root.path(), &Update::Keep).unwrap();
    (registry, root)
}

#[test]
fn lock_then_install_on_another_machine() {
    let registry = TestRegistry::new();
    registry.publish(ID, "1.0.0", &RELEASE);
    let root = project(&[(ID, "1")]);

    let first = Machine::new(&registry);
    let outcome = lock_project(&first.env(), root.path(), &Update::Keep).unwrap();
    assert_eq!(outcome.installed, vec![(ID.to_owned(), v("1.0.0"))]);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    let lock = Lockfile::load(root.path()).unwrap().unwrap();
    assert_eq!(lock, outcome.lock);
    assert_eq!(lock.celesta, v("0.9.0"));
    let locked = &lock.plugins[ID];
    assert_eq!(locked.version, v("1.0.0"));
    assert_eq!(locked.registry, BASE);
    assert_eq!(locked.targets.len(), 3);
    let expected = lock.artifact(ID, &host()).unwrap().tree_hash;
    assert_eq!(expected, host_tree("1.0.0"));
    assert_eq!(first.installed(ID, "1.0.0"), Some(expected));

    // Locking again keeps the lock and installs nothing.
    let again = lock_project(&first.env(), root.path(), &Update::Keep).unwrap();
    assert!(again.installed.is_empty());
    assert_eq!(again.lock, lock);

    // The first machine's store is gone; a second one installs the same
    // folder from the lock.
    fs::remove_dir_all(first.home.path().join("registry")).unwrap();
    let lock_bytes = fs::read(root.path().join(LOCK_FILE)).unwrap();
    let second = Machine::new(&registry);
    let outcome = install_locked(&second.env(), root.path(), false).unwrap();
    assert_eq!(outcome.installed, vec![(ID.to_owned(), v("1.0.0"))]);
    assert!(outcome.skipped.is_empty());
    assert_eq!(second.installed(ID, "1.0.0"), Some(expected));
    assert_eq!(fs::read(root.path().join(LOCK_FILE)).unwrap(), lock_bytes);
}

#[test]
fn a_satisfied_lock_needs_no_network() {
    let registry = TestRegistry::new();
    registry.publish(ID, "1.0.0", &RELEASE);
    let root = project(&[(ID, "1")]);
    let machine = Machine::new(&registry);
    lock_project(&machine.env(), root.path(), &Update::Keep).unwrap();

    let offline = Machine {
        store: machine.store.clone(),
        ..Machine::offline(&registry)
    };
    let outcome = install_locked(&offline.env(), root.path(), false).unwrap();
    assert!(outcome.installed.is_empty());
    assert!(outcome.skipped.is_empty());
}

#[test]
fn a_bundled_copy_satisfies_install() {
    let (registry, root) = locked();
    let bundled = TempDir::new().unwrap();
    write_files(&bundled.path().join(ID), &plugin_files(ID, "1.0.0", MAC));

    let mut machine = Machine::offline(&registry);
    machine.bundled = Some(bundled.path().to_path_buf());
    let outcome = install_locked(&machine.env(), root.path(), false).unwrap();
    assert!(outcome.installed.is_empty());
    assert_eq!(machine.installed(ID, "1.0.0"), None);

    // A bundled copy with other files does not.
    write_files(
        &bundled.path().join(ID),
        &[("data/extra.txt".to_owned(), b"x".to_vec())],
    );
    let error = install_locked(&machine.env(), root.path(), true).unwrap_err();
    assert!(matches!(error, ProjectError::Missing { .. }), "{error}");
}

#[test]
fn offline_reports_what_is_missing() {
    let (registry, root) = locked();
    let machine = Machine::offline(&registry);
    let error = install_locked(&machine.env(), root.path(), true).unwrap_err();
    assert!(matches!(error, ProjectError::Missing { .. }), "{error:?}");
    assert_eq!(
        error.to_string(),
        "acme.tool 1.0.0 is not installed, and this run is offline"
    );
}

#[test]
fn a_withdrawn_locked_version_fails() {
    let (registry, root) = locked();
    registry.edit(ID, "1.0.0", |entry| {
        entry["yanked"] = json!(true);
        entry["withdrawn"] = json!(true);
    });
    let machine = Machine::new(&registry);
    let error = install_locked(&machine.env(), root.path(), false).unwrap_err();
    assert!(matches!(error, ProjectError::Withdrawn { .. }), "{error:?}");
    assert_eq!(
        error.to_string(),
        "acme.tool 1.0.0 is no longer distributed"
    );
    assert_eq!(machine.installed(ID, "1.0.0"), None);
}

#[test]
fn a_blocked_locked_version_fails() {
    let (registry, root) = locked();
    registry.advisories(json!([{
        "id": "CELESTA-2026-0009",
        "plugin": ID,
        "versions": "<1.0.1",
        "title": "Bad",
        "description": "Does bad things.",
        "fixed": "1.0.1",
        "published": "2026-11-20T00:00:00Z",
    }]));
    let machine = Machine::new(&registry);
    machine.registry.refresh_advisories(now()).unwrap();
    let error = install_locked(&machine.env(), root.path(), false).unwrap_err();
    match &error {
        ProjectError::Blocked {
            id,
            version,
            advisory,
        } => {
            assert_eq!((id.as_str(), version), (ID, &v("1.0.0")));
            assert_eq!(advisory, "CELESTA-2026-0009");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(error.to_string().contains("CELESTA-2026-0009"), "{error}");
    assert_eq!(machine.installed(ID, "1.0.0"), None);
}

#[test]
fn a_lock_the_index_contradicts_is_refused() {
    let (registry, root) = locked();
    registry.edit(ID, "1.0.0", |entry| {
        entry["targets"][MAC]["treeHash"] = json!(Hash([0x07; 32]).to_string());
    });
    let machine = Machine::new(&registry);
    let error = install_locked(&machine.env(), root.path(), false).unwrap_err();
    assert!(
        matches!(&error, ProjectError::LockMismatch { id, .. } if id == ID),
        "{error:?}"
    );
    assert_eq!(machine.installed(ID, "1.0.0"), None);
}

#[test]
fn an_unusable_advisory_cache_stops_install_locked() {
    let (registry, root) = locked();
    let machine = Machine::new(&registry);
    let cache = machine.home.path().join("registry-cache");
    fs::create_dir_all(&cache).unwrap();
    let file = cache.join("advisories.json");
    fs::write(&file, "not an advisory list").unwrap();
    let error = install_locked(&machine.env(), root.path(), false).unwrap_err();
    assert!(
        matches!(&error, ProjectError::Client(ClientError::Cache { path, .. }) if path == &file),
        "{error:?}"
    );
    assert_eq!(machine.installed(ID, "1.0.0"), None);
}

#[test]
fn a_version_that_gains_a_host_build_is_not_a_lock_mismatch() {
    let registry = TestRegistry::new();
    registry.publish(ID, "1.0.0", &["any"]);
    let root = project(&[(ID, "1")]);
    lock_project(&Machine::new(&registry).env(), root.path(), &Update::Keep).unwrap();
    // Later the version gains a build for the host, with other files.
    registry.edit(ID, "1.0.0", |entry| {
        let any = entry["targets"]["any"].clone();
        let mut host = any;
        host["treeHash"] = json!(Hash([0x09; 32]).to_string());
        entry["targets"][MAC] = host;
    });
    let machine = Machine::new(&registry);
    let outcome = install_locked(&machine.env(), root.path(), false).unwrap();
    assert_eq!(outcome.installed, vec![(ID.to_owned(), v("1.0.0"))]);
}

#[test]
fn a_locked_yanked_version_still_installs() {
    let (registry, root) = locked();
    registry.edit(ID, "1.0.0", |entry| entry["yanked"] = json!(true));
    let machine = Machine::new(&registry);
    let outcome = install_locked(&machine.env(), root.path(), false).unwrap();
    assert_eq!(outcome.installed, vec![(ID.to_owned(), v("1.0.0"))]);
    assert_eq!(machine.installed(ID, "1.0.0"), Some(host_tree("1.0.0")));
}

#[test]
fn install_locked_refuses_another_celesta_minor() {
    let (registry, root) = locked();
    let mut machine = Machine::new(&registry);
    machine.celesta = v("0.10.0");
    let error = install_locked(&machine.env(), root.path(), false).unwrap_err();
    assert!(
        matches!(&error, ProjectError::OtherCelesta { ids, .. } if ids == &[ID.to_owned()]),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "celesta.lock was resolved with Celesta 0.9.0, and this is Celesta 0.10.0; \
         acme.tool cannot load with it. Run `celesta plugin update` to resolve them again"
    );
}

#[test]
fn install_locked_refuses_a_stale_lock() {
    let (registry, root) = locked();
    write_config(root.path(), &[(ID, "2"), ("acme.other", "1")]);
    let machine = Machine::new(&registry);
    let error = install_locked(&machine.env(), root.path(), false).unwrap_err();
    assert!(
        matches!(&error, ProjectError::OutOfDate { ids } if ids == &["acme.other", ID]),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "celesta.lock does not match celesta.config.json for acme.other, acme.tool; \
         run `celesta plugin update`"
    );
}

#[test]
fn install_locked_without_a_lock_fails() {
    let registry = TestRegistry::new();
    let root = project(&[(ID, "1")]);
    let machine = Machine::offline(&registry);
    let error = install_locked(&machine.env(), root.path(), false).unwrap_err();
    assert!(matches!(error, ProjectError::NoLock), "{error:?}");
}

#[test]
fn a_pinned_media_plugin_without_a_host_build_is_skipped() {
    let registry = TestRegistry::new();
    registry.publish_with_roles("acme.codec", "1.0.0", &[WIN], &["media"]);
    let root = project(&[("acme.codec", "1")]);
    let machine = Machine::new(&registry);
    let outcome = lock_project(&machine.env(), root.path(), &Update::Keep).unwrap();
    assert!(outcome.installed.is_empty());
    assert!(outcome.lock.plugins.contains_key("acme.codec"));

    let outcome = install_locked(&machine.env(), root.path(), false).unwrap();
    assert!(outcome.installed.is_empty());
    assert_eq!(
        outcome.skipped,
        vec![(
            "acme.codec".to_owned(),
            v("1.0.0"),
            "no build for macos-aarch64".to_owned()
        )]
    );
}

#[test]
fn install_global_records_the_tree_hash() {
    let registry = TestRegistry::new();
    registry.publish(ID, "1.0.0", &[MAC]);
    let machine = Machine::new(&registry);
    let settings = machine.home.path().join("settings/settings.json");

    let first = install_global(
        &machine.env(),
        &settings,
        ID,
        &VersionReq::parse("1").unwrap(),
    )
    .unwrap();
    assert_eq!(first.id, ID);
    assert_eq!(first.version, v("1.0.0"));
    assert_eq!(first.tree_hash, host_tree("1.0.0"));
    assert_eq!(machine.installed(ID, "1.0.0"), Some(first.tree_hash));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(
        saved["registryPlugins"],
        json!([{ "id": ID, "version": "1.0.0", "treeHash": first.tree_hash.to_string() }])
    );

    registry.publish(ID, "1.1.0", &["any"]);
    let second = install_global(
        &machine.env(),
        &settings,
        ID,
        &VersionReq::parse("1").unwrap(),
    )
    .unwrap();
    assert_eq!(second.version, v("1.1.0"));
    assert_eq!(second.tree_hash, tree_of(&plugin_files(ID, "1.1.0", "any")));
    assert_eq!(
        UserSettings::load(&settings).unwrap().registry_plugins,
        vec![second]
    );
}

#[test]
fn install_global_needs_a_host_build() {
    let registry = TestRegistry::new();
    registry.publish_with_roles("acme.codec", "1.0.0", &[WIN], &["media"]);
    let machine = Machine::new(&registry);
    let settings = machine.home.path().join("settings.json");
    let error = install_global(
        &machine.env(),
        &settings,
        "acme.codec",
        &VersionReq::parse("1").unwrap(),
    )
    .unwrap_err();
    assert!(matches!(error, ProjectError::Resolve(_)), "{error:?}");
    assert!(
        error.to_string().contains("no build for macos-aarch64"),
        "{error}"
    );
    assert!(!settings.exists());
}
