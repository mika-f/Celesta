use std::collections::{BTreeMap, BTreeSet};

use celesta_plugin_host::{LockedArtifact, LockedPlugin, Lockfile};
use celesta_registry_client::{ResolveError, ResolveInput, ResolveWarning, Update, resolve};
use celesta_registry_format::{AdvisoryList, Index, Target};
use semver::{Version, VersionReq};
use serde_json::{Value, json};

const REGISTRY: &str = "https://registry.test";
const MAC: &str = "macos-aarch64";
const WIN: &str = "windows-x86_64";
const LINUX: &str = "linux-x86_64";

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn req(text: &str) -> VersionReq {
    VersionReq::parse(text).unwrap()
}

fn t(text: &str) -> Target {
    Target::parse(text).unwrap()
}

fn hash(byte: u8) -> String {
    format!("sha256:{}", format!("{byte:02x}").repeat(32))
}

/// One version entry: `(version, celesta range, targets)`.
fn version(version: &str, celesta: &str, targets: &[&str]) -> Value {
    let targets: serde_json::Map<String, Value> = targets
        .iter()
        .map(|target| {
            (
                (*target).to_owned(),
                json!({
                    "url": format!("https://registry.test/{version}/{target}.zip"),
                    "size": 10,
                    "sha256": hash(1),
                    "treeHash": hash(2),
                }),
            )
        })
        .collect();
    json!({
        "version": version,
        "celesta": celesta,
        "protocol": 1,
        "published": "2026-01-01T00:00:00Z",
        "yanked": false,
        "withdrawn": false,
        "targets": targets,
    })
}

fn with(mut entry: Value, key: &str, value: Value) -> Value {
    entry[key] = value;
    entry
}

fn build(id: &str, roles: &[&str], versions: Vec<Value>) -> Index {
    let bytes = serde_json::to_vec(&json!({
        "format": 1,
        "id": id,
        "generated": "2026-01-01T00:00:00Z",
        "publisher": "example",
        "kind": { "roles": roles, "program": false, "module": true },
        "description": "test",
        "versions": versions,
    }))
    .unwrap();
    Index::parse(&bytes, id).unwrap()
}

fn all() -> [&'static str; 3] {
    [MAC, WIN, LINUX]
}

fn indexes(list: Vec<Index>) -> BTreeMap<String, Index> {
    list.into_iter().map(|i| (i.id.clone(), i)).collect()
}

fn deps(list: &[(&str, &str)]) -> BTreeMap<String, VersionReq> {
    list.iter()
        .map(|(id, r)| ((*id).to_owned(), req(r)))
        .collect()
}

fn project() -> Vec<Target> {
    all().iter().map(|s| t(s)).collect()
}

struct Case {
    dependencies: BTreeMap<String, VersionReq>,
    indexes: BTreeMap<String, Index>,
    advisories: Option<AdvisoryList>,
    celesta: Version,
    targets: Vec<Target>,
    host: Target,
    previous: Option<Lockfile>,
    update: Update,
}

impl Case {
    fn new(dependencies: &[(&str, &str)], list: Vec<Index>) -> Self {
        Self {
            dependencies: deps(dependencies),
            indexes: indexes(list),
            advisories: None,
            celesta: v("0.10.0"),
            targets: project(),
            host: t(MAC),
            previous: None,
            update: Update::Keep,
        }
    }

    fn run(&self) -> Result<celesta_registry_client::Resolution, ResolveError> {
        resolve(&ResolveInput {
            dependencies: &self.dependencies,
            indexes: &self.indexes,
            advisories: self.advisories.as_ref(),
            celesta: &self.celesta,
            targets: &self.targets,
            host: &self.host,
            registry: REGISTRY,
            previous: self.previous.as_ref(),
            update: &self.update,
        })
    }
}

fn locked_version(case: &Case, id: &str) -> Version {
    case.run().unwrap().lock.plugins[id].version.clone()
}

fn advisory(id: &str, plugin: &str, versions: &str) -> AdvisoryList {
    serde_json::from_value(json!({
        "generated": "2026-01-01T00:00:00Z",
        "advisories": [{
            "id": id,
            "plugin": plugin,
            "versions": versions,
            "title": "bad",
            "description": "bad",
            "published": "2026-01-01T00:00:00Z",
        }],
    }))
    .unwrap()
}

fn locked(version: &str, celesta: &str) -> LockedPlugin {
    let artifact = LockedArtifact {
        sha256: celesta_registry_format::Hash([1; 32]),
        tree_hash: celesta_registry_format::Hash([2; 32]),
    };
    LockedPlugin {
        celesta: req(celesta),
        registry: REGISTRY.to_owned(),
        targets: BTreeMap::from([(t(MAC), artifact)]),
        version: v(version),
        other: Default::default(),
    }
}

fn lock(celesta: &str, plugins: Vec<(&str, LockedPlugin)>) -> Lockfile {
    Lockfile {
        celesta: v(celesta),
        targets: project(),
        plugins: plugins
            .into_iter()
            .map(|(i, p)| (i.to_owned(), p))
            .collect(),
        other: Default::default(),
    }
}

#[test]
fn the_newest_matching_version_is_chosen() {
    let case = Case::new(
        &[("a", "^0.2")],
        vec![build(
            "a",
            &[],
            vec![
                version("0.2.0", ">=0.10.0", &all()),
                version("0.2.3", ">=0.10.0", &all()),
                version("0.3.0", ">=0.10.0", &all()),
            ],
        )],
    );
    let resolution = case.run().unwrap();
    let entry = &resolution.lock.plugins["a"];
    assert_eq!(entry.version, v("0.2.3"));
    assert_eq!(entry.registry, REGISTRY);
    assert_eq!(entry.celesta, req(">=0.10.0"));
    assert_eq!(entry.targets.len(), 3);
    assert_eq!(resolution.lock.celesta, v("0.10.0"));
    assert!(resolution.warnings.is_empty());
}

#[test]
fn lock_targets_are_sorted() {
    let mut case = Case::new(
        &[("a", "*")],
        vec![build("a", &[], vec![version("1.0.0", "*", &all())])],
    );
    case.targets = vec![t(WIN), t(MAC), t(LINUX)];
    let targets = case.run().unwrap().lock.targets;
    assert_eq!(targets, vec![t(LINUX), t(MAC), t(WIN)]);
}

#[test]
fn yanked_withdrawn_and_blocked_versions_are_skipped() {
    let mut case = Case::new(
        &[("a", "*")],
        vec![build(
            "a",
            &[],
            vec![
                version("1.0.0", "*", &all()),
                with(version("1.1.0", "*", &all()), "yanked", json!(true)),
                with(version("1.2.0", "*", &all()), "withdrawn", json!(true)),
                version("1.3.0", "*", &all()),
            ],
        )],
    );
    case.advisories = Some(advisory("ADV-1", "a", "=1.3.0"));
    assert_eq!(locked_version(&case, "a"), v("1.0.0"));
}

#[test]
fn a_locked_yanked_version_is_kept() {
    let mut case = Case::new(
        &[("a", "*")],
        vec![build(
            "a",
            &[],
            vec![
                with(version("1.0.0", "*", &all()), "yanked", json!(true)),
                version("1.1.0", "*", &all()),
            ],
        )],
    );
    case.previous = Some(lock("0.10.0", vec![("a", locked("1.0.0", "*"))]));
    let resolution = case.run().unwrap();
    assert_eq!(resolution.lock.plugins["a"].version, v("1.0.0"));
    assert_eq!(
        resolution.warnings,
        vec![ResolveWarning::LockedYanked {
            id: "a".into(),
            version: v("1.0.0")
        }]
    );
}

#[test]
fn a_blocked_or_withdrawn_lock_is_re_resolved() {
    let mut case = Case::new(
        &[("a", "*"), ("b", "*")],
        vec![
            build(
                "a",
                &[],
                vec![version("1.0.0", "*", &all()), version("1.1.0", "*", &all())],
            ),
            build(
                "b",
                &[],
                vec![
                    version("1.0.0", "*", &all()),
                    with(version("1.1.0", "*", &all()), "withdrawn", json!(true)),
                ],
            ),
        ],
    );
    case.advisories = Some(advisory("ADV-1", "a", "=1.1.0"));
    case.previous = Some(lock(
        "0.10.0",
        vec![("a", locked("1.1.0", "*")), ("b", locked("1.1.0", "*"))],
    ));
    let lock = case.run().unwrap().lock;
    assert_eq!(lock.plugins["a"].version, v("1.0.0"));
    assert_eq!(lock.plugins["b"].version, v("1.0.0"));
}

#[test]
fn the_celesta_range_is_respected() {
    let mut case = Case::new(
        &[("a", "^0.2")],
        vec![build(
            "a",
            &[],
            vec![
                version("0.2.2", ">=0.9.0, <0.10.0", &all()),
                version("0.2.3", ">=0.10.0, <0.11.0", &all()),
            ],
        )],
    );
    case.celesta = v("0.9.5");
    assert_eq!(locked_version(&case, "a"), v("0.2.2"));
}

#[test]
fn every_project_target_is_preferred() {
    let case = Case::new(
        &[("a", "*")],
        vec![build(
            "a",
            &[],
            vec![
                version("0.2.2", "*", &all()),
                version("0.2.3", "*", &[MAC, LINUX]),
            ],
        )],
    );
    let resolution = case.run().unwrap();
    assert_eq!(resolution.lock.plugins["a"].version, v("0.2.2"));
    assert!(resolution.warnings.is_empty());
}

#[test]
fn missing_targets_fall_back_with_a_warning() {
    let case = Case::new(
        &[("a", "*")],
        vec![build(
            "a",
            &[],
            vec![
                version("0.2.2", "*", &[MAC]),
                version("0.2.3", "*", &[MAC, LINUX]),
            ],
        )],
    );
    let resolution = case.run().unwrap();
    assert_eq!(resolution.lock.plugins["a"].version, v("0.2.3"));
    assert_eq!(
        resolution.warnings,
        vec![ResolveWarning::MissingTargets {
            id: "a".into(),
            version: v("0.2.3"),
            targets: vec![t(WIN)],
        }]
    );
}

#[test]
fn any_covers_every_target() {
    let case = Case::new(
        &[("a", "*")],
        vec![build("a", &[], vec![version("1.0.0", "*", &["any"])])],
    );
    let resolution = case.run().unwrap();
    assert_eq!(resolution.lock.plugins["a"].version, v("1.0.0"));
    assert!(resolution.warnings.is_empty());
    assert!(
        resolution.lock.plugins["a"]
            .targets
            .contains_key(&Target::ANY)
    );
}

#[test]
fn pinned_media_ignore_the_targets() {
    let mut case = Case::new(
        &[("codec", "*")],
        vec![build(
            "codec",
            &["media"],
            vec![version("1.0.0", "*", &[MAC])],
        )],
    );
    case.host = t(LINUX);
    let resolution = case.run().unwrap();
    assert_eq!(resolution.lock.plugins["codec"].version, v("1.0.0"));
    assert!(resolution.warnings.is_empty());
}

#[test]
fn a_minor_upgrade_re_resolves_only_what_cannot_load() {
    let mut case = Case::new(
        &[("a", "*"), ("b", "*")],
        vec![
            build(
                "a",
                &[],
                vec![
                    version("1.0.0", ">=0.9.0, <0.10.0", &all()),
                    version("1.1.0", ">=0.10.0, <0.11.0", &all()),
                ],
            ),
            build(
                "b",
                &[],
                vec![
                    version("2.0.0", ">=0.9.0", &all()),
                    version("2.1.0", ">=0.9.0", &all()),
                ],
            ),
        ],
    );
    let previous = lock(
        "0.9.4",
        vec![
            ("a", locked("1.0.0", ">=0.9.0, <0.10.0")),
            ("b", locked("2.0.0", ">=0.9.0")),
        ],
    );
    let unloadable: BTreeSet<String> = previous.unloadable(&v("0.10.0")).into_iter().collect();
    assert_eq!(unloadable, BTreeSet::from(["a".to_owned()]));
    case.previous = Some(previous);
    case.update = Update::Only(unloadable);
    let lock = case.run().unwrap().lock;
    assert_eq!(lock.plugins["a"].version, v("1.1.0"));
    assert_eq!(lock.plugins["b"].version, v("2.0.0"));
    assert_eq!(lock.celesta, v("0.10.0"));
}

#[test]
fn keep_does_not_move_a_satisfied_lock() {
    let mut case = Case::new(
        &[("a", "*")],
        vec![build(
            "a",
            &[],
            vec![version("1.0.0", "*", &all()), version("1.1.0", "*", &all())],
        )],
    );
    case.previous = Some(lock("0.10.0", vec![("a", locked("1.0.0", "*"))]));
    assert_eq!(locked_version(&case, "a"), v("1.0.0"));
    case.update = Update::All;
    assert_eq!(locked_version(&case, "a"), v("1.1.0"));
}

#[test]
fn a_kept_entry_and_the_lock_keep_unknown_fields() {
    let mut entry = locked("1.0.0", "*");
    entry.other.insert("extra".into(), json!(1));
    let mut previous = lock("0.10.0", vec![("a", entry)]);
    previous.other.insert("future".into(), json!(true));
    let mut case = Case::new(
        &[("a", "*")],
        vec![build("a", &[], vec![version("1.0.0", "*", &all())])],
    );
    case.previous = Some(previous);
    let lock = case.run().unwrap().lock;
    assert_eq!(lock.plugins["a"].other["extra"], json!(1));
    assert_eq!(lock.other["future"], json!(true));
}

#[test]
fn removed_dependencies_leave_the_lock() {
    let mut case = Case::new(
        &[("a", "*")],
        vec![build("a", &[], vec![version("1.0.0", "*", &all())])],
    );
    case.previous = Some(lock(
        "0.10.0",
        vec![("a", locked("1.0.0", "*")), ("gone", locked("1.0.0", "*"))],
    ));
    let lock = case.run().unwrap().lock;
    assert_eq!(lock.plugins.keys().collect::<Vec<_>>(), vec!["a"]);
}

#[test]
fn a_missing_index_is_an_error() {
    let case = Case::new(&[("a", "*")], vec![]);
    assert_eq!(
        case.run().unwrap_err(),
        ResolveError::NotInIndex("a".into())
    );
}

fn reason(case: &Case) -> String {
    match case.run().unwrap_err() {
        ResolveError::NoVersion { reason, .. } => reason,
        other => panic!("{other:?}"),
    }
}

#[test]
fn no_version_says_why() {
    let yanked = Case::new(
        &[("a", "^1")],
        vec![build(
            "a",
            &[],
            vec![with(version("1.0.0", "*", &all()), "yanked", json!(true))],
        )],
    );
    assert_eq!(reason(&yanked), "yanked");

    let withdrawn = Case::new(
        &[("a", "^1")],
        vec![build(
            "a",
            &[],
            vec![with(
                version("1.0.0", "*", &all()),
                "withdrawn",
                json!(true),
            )],
        )],
    );
    assert_eq!(reason(&withdrawn), "withdrawn");

    let mut blocked = Case::new(
        &[("a", "^1")],
        vec![build("a", &[], vec![version("1.0.0", "*", &all())])],
    );
    blocked.advisories = Some(advisory("ADV-7", "a", "=1.0.0"));
    assert_eq!(reason(&blocked), "blocked by ADV-7");

    let mut range = Case::new(
        &[("a", "^1")],
        vec![build("a", &[], vec![version("1.0.0", ">=0.11.0", &all())])],
    );
    range.celesta = v("0.10.0");
    assert_eq!(reason(&range), "works with Celesta >=0.11.0, not 0.10.0");

    let mut build_missing = Case::new(
        &[("a", "^1")],
        vec![build("a", &[], vec![version("1.0.0", "*", &[WIN])])],
    );
    build_missing.host = t(LINUX);
    assert_eq!(reason(&build_missing), "no build for linux-x86_64");

    let none = Case::new(
        &[("a", "^2")],
        vec![build("a", &[], vec![version("1.0.0", "*", &all())])],
    );
    assert_eq!(reason(&none), "no version matches");
}

#[test]
fn deprecations_are_reported() {
    let case = Case::new(
        &[("a", "*")],
        vec![build(
            "a",
            &[],
            vec![with(
                version("1.0.0", "*", &all()),
                "deprecated",
                json!({ "message": "old", "replacement": "b" }),
            )],
        )],
    );
    assert_eq!(
        case.run().unwrap().warnings,
        vec![ResolveWarning::Deprecated {
            id: "a".into(),
            version: v("1.0.0"),
            message: "old".into(),
            replacement: Some("b".into()),
        }]
    );
}
