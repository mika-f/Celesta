//! Garbage collection of the store: which versions no project, and no
//! user setting, names any more.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use celesta_plugin_host::{Lockfile, UserSettings};
use celesta_registry_format::is_plugin_id;
use semver::Version;

use crate::store::Store;

/// How long a `.tmp-` folder may sit before it is taken for an install
/// that died.
const TEMPORARY_AGE: Duration = Duration::from_secs(60 * 60);

/// What a collection would remove.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GcPlan {
    /// The `<id>/<version>` folders nothing names, in id and version order.
    pub versions: Vec<(String, Version)>,
    /// `.tmp-*` folders in the store that were last changed over an hour
    /// before the plan's `now`.
    pub temporary: Vec<PathBuf>,
    /// Recorded project roots whose `celesta.lock` is gone or unreadable.
    pub forgotten: Vec<PathBuf>,
}

/// What [`plan_gc`] reads.
pub struct GcInput<'a> {
    pub store: &'a Store,
    pub settings: &'a UserSettings,
    /// The project the command runs in, whether or not it is recorded.
    pub current: Option<&'a Path>,
    pub now: SystemTime,
}

/// Lists what a collection would remove; changes nothing.
///
/// Kept are the versions `registryPlugins` names and those the lock of
/// `current` and of each recorded project names. A folder that is neither
/// `.tmp-*` nor a plugin id, and a folder in an id folder that is not a
/// version, is left alone.
pub fn plan_gc(input: &GcInput) -> io::Result<GcPlan> {
    let record = input.store.read_projects()?;
    let mut kept: HashSet<(String, Version)> = input
        .settings
        .registry_plugins
        .iter()
        .map(|plugin| (plugin.id.clone(), plugin.version.clone()))
        .collect();
    let mut keep_lock = |root: &Path| match Lockfile::load(root) {
        Ok(Some(lock)) => {
            kept.extend(
                lock.plugins
                    .into_iter()
                    .map(|(id, locked)| (id, locked.version)),
            );
            true
        }
        Ok(None) | Err(_) => false,
    };
    if let Some(current) = input.current {
        keep_lock(current);
    }
    let forgotten = record
        .projects
        .iter()
        .filter(|root| !keep_lock(root))
        .cloned()
        .collect();

    let mut plan = GcPlan {
        forgotten,
        ..GcPlan::default()
    };
    let entries = match fs::read_dir(input.store.root()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(plan),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with(".tmp-") {
            let age = entry
                .metadata()?
                .modified()
                .ok()
                .and_then(|modified| input.now.duration_since(modified).ok());
            if age.is_some_and(|age| age > TEMPORARY_AGE) {
                plan.temporary.push(entry.path());
            }
        } else if is_plugin_id(&name) {
            for version in fs::read_dir(entry.path())? {
                let version = version?;
                if !version.file_type()?.is_dir() {
                    continue;
                }
                let Some(version) = version
                    .file_name()
                    .to_str()
                    .and_then(|text| Version::parse(text).ok())
                else {
                    continue;
                };
                if !kept.contains(&(name.clone(), version.clone())) {
                    plan.versions.push((name.clone(), version));
                }
            }
        }
    }
    plan.versions.sort();
    plan.temporary.sort();
    Ok(plan)
}

/// Removes what `plan` lists, then the id folders that are left empty, and
/// rewrites `.projects.json` without the forgotten projects. Something
/// already gone is not an error.
pub fn run_gc(store: &Store, plan: &GcPlan) -> io::Result<()> {
    for (id, version) in &plan.versions {
        remove_dir_all(&store.path(id, version))?;
        // Only an empty folder is removed.
        let _ = fs::remove_dir(store.root().join(id));
    }
    for folder in &plan.temporary {
        remove_dir_all(folder)?;
    }
    if !plan.forgotten.is_empty() {
        let mut record = store.read_projects()?;
        record
            .projects
            .retain(|project| !plan.forgotten.contains(project));
        store.write_projects(&record)?;
    }
    Ok(())
}

fn remove_dir_all(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs::File;

    use celesta_plugin_host::{LOCK_FILE, LockedPlugin, RegistryPlugin};
    use celesta_registry_format::Hash;
    use tempfile::TempDir;

    use super::*;
    use crate::store::PROJECTS_FILE;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn store_with(folders: &[(&str, &str)]) -> (TempDir, Store) {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("registry");
        fs::create_dir_all(&root).unwrap();
        for (id, version) in folders {
            fs::create_dir_all(root.join(id).join(version)).unwrap();
        }
        (dir, Store::new(root))
    }

    /// A project folder whose lock names `plugins`.
    fn project_locking(plugins: &[(&str, &str)]) -> TempDir {
        let root = TempDir::new().unwrap();
        let lock = Lockfile {
            celesta: v("0.9.0"),
            plugins: plugins
                .iter()
                .map(|(id, version)| {
                    (
                        (*id).to_owned(),
                        LockedPlugin {
                            celesta: "*".parse().unwrap(),
                            registry: "https://registry.example".to_owned(),
                            targets: BTreeMap::new(),
                            version: v(version),
                            other: Default::default(),
                        },
                    )
                })
                .collect(),
            targets: Vec::new(),
            other: Default::default(),
        };
        lock.save(root.path()).unwrap();
        root
    }

    fn plan(
        store: &Store,
        settings: &UserSettings,
        current: Option<&Path>,
        now: SystemTime,
    ) -> GcPlan {
        plan_gc(&GcInput {
            store,
            settings,
            current,
            now,
        })
        .unwrap()
    }

    #[test]
    fn gc_keeps_what_locks_and_settings_name() {
        let (_dir, store) = store_with(&[
            ("a", "1.0.0"),
            ("a", "1.1.0"),
            ("b", "2.0.0"),
            ("c", "0.1.0"),
        ]);
        let recorded = project_locking(&[("a", "1.1.0")]);
        store.record_project(recorded.path()).unwrap();
        let current = project_locking(&[("c", "0.1.0")]);
        let settings = UserSettings {
            registry_plugins: vec![RegistryPlugin {
                id: "b".to_owned(),
                version: v("2.0.0"),
                tree_hash: Hash([0; 32]),
            }],
            ..UserSettings::default()
        };

        let plan = plan(&store, &settings, Some(current.path()), SystemTime::now());
        assert_eq!(plan.versions, vec![("a".to_owned(), v("1.0.0"))]);
        assert!(plan.forgotten.is_empty());
    }

    #[test]
    fn gc_forgets_projects_without_a_lock() {
        let (_dir, store) = store_with(&[("a", "1.0.0")]);
        let gone = project_locking(&[("a", "1.0.0")]);
        store.record_project(gone.path()).unwrap();
        let path = fs::canonicalize(gone.path()).unwrap();
        drop(gone);

        let plan = plan(&store, &UserSettings::default(), None, SystemTime::now());
        assert_eq!(plan.forgotten, vec![path]);
        assert_eq!(plan.versions, vec![("a".to_owned(), v("1.0.0"))]);
    }

    #[test]
    fn gc_removes_old_temporary_folders_only() {
        let (_dir, store) = store_with(&[]);
        let now = SystemTime::now();
        for (name, age) in [(".tmp-old", 2 * 3600), (".tmp-new", 60)] {
            let folder = store.root().join(name);
            fs::create_dir(&folder).unwrap();
            File::open(&folder)
                .unwrap()
                .set_modified(now - Duration::from_secs(age))
                .unwrap();
        }
        fs::create_dir(store.root().join("Not An Id")).unwrap();
        fs::write(store.root().join(PROJECTS_FILE), "{}").unwrap();

        let plan = plan(&store, &UserSettings::default(), None, now);
        assert_eq!(plan.temporary, vec![store.root().join(".tmp-old")]);
        assert!(plan.versions.is_empty());
    }

    #[test]
    fn run_gc_removes_and_rewrites_the_record() {
        let (_dir, store) = store_with(&[("a", "1.0.0"), ("b", "1.0.0"), ("b", "2.0.0")]);
        let kept = project_locking(&[("b", "2.0.0")]);
        let gone = project_locking(&[]);
        store.record_project(kept.path()).unwrap();
        store.record_project(gone.path()).unwrap();
        fs::remove_file(gone.path().join(LOCK_FILE)).unwrap();
        // A key a newer build wrote.
        let mut record = store.read_projects().unwrap();
        record.other.insert("note".to_owned(), "kept".into());
        store.write_projects(&record).unwrap();
        let old = store.root().join(".tmp-old");
        fs::create_dir(&old).unwrap();
        let now = SystemTime::now() + Duration::from_secs(2 * 3600);

        let plan = plan(&store, &UserSettings::default(), None, now);
        run_gc(&store, &plan).unwrap();

        assert!(!store.root().join("a").exists(), "empty id folder is gone");
        assert!(!store.path("b", &v("1.0.0")).exists());
        assert!(store.path("b", &v("2.0.0")).exists());
        assert!(!old.exists());
        let record = store.read_projects().unwrap();
        assert_eq!(
            record.projects,
            vec![fs::canonicalize(kept.path()).unwrap()]
        );
        assert_eq!(record.other["note"], "kept");
    }
}
