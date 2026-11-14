//! Garbage collection of the store: which versions no project, and no
//! user setting, names any more.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use celesta_plugin_host::{LOCK_FILE, Lockfile, UserSettings};
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
    /// Recorded project roots with no `celesta.lock` (or no folder).
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
    // Whether the project has a lock. A lock that is there but does not
    // load may name anything, so it stops the plan.
    let mut keep_lock = |root: &Path| -> io::Result<bool> {
        match Lockfile::load(root) {
            Ok(Some(lock)) => {
                kept.extend(
                    lock.plugins
                        .into_iter()
                        .map(|(id, locked)| (id, locked.version)),
                );
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: {error}; fix it or remove the project before running gc",
                    root.join(LOCK_FILE).display()
                ),
            )),
        }
    };
    if let Some(current) = input.current {
        keep_lock(current)?;
    }
    let mut forgotten = Vec::new();
    for root in &record.projects {
        if !keep_lock(root)? {
            forgotten.push(root.clone());
        }
    }

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
///
/// `GcPlan` is public, so every entry is checked first, and nothing is
/// removed if one is not what [`plan_gc`] lists: a plugin id, a real
/// folder in the store, never a link.
pub fn run_gc(store: &Store, plan: &GcPlan) -> io::Result<()> {
    let invalid = |what: String| io::Error::new(io::ErrorKind::InvalidInput, what);
    for (id, version) in &plan.versions {
        if !is_plugin_id(id) {
            return Err(invalid(format!("{id:?} is not a plugin id")));
        }
        for path in [store.root().join(id), store.path(id, version)] {
            if let Ok(metadata) = fs::symlink_metadata(&path)
                && !metadata.is_dir()
            {
                return Err(invalid(format!("{} is not a folder", path.display())));
            }
        }
    }
    for folder in &plan.temporary {
        let named = folder
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".tmp-"));
        if folder.parent() != Some(store.root()) || !named {
            return Err(invalid(format!(
                "{} is not a temporary folder of the store",
                folder.display()
            )));
        }
        if let Ok(metadata) = fs::symlink_metadata(folder)
            && !metadata.is_dir()
        {
            return Err(invalid(format!("{} is not a folder", folder.display())));
        }
    }
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
        // The planning time is set from the folder's own mtime, which the
        // test never changes.
        let listed = |name: &str, age: Duration| {
            let (_dir, store) = store_with(&[]);
            let folder = store.root().join(name);
            fs::create_dir(&folder).unwrap();
            fs::create_dir(store.root().join("Not An Id")).unwrap();
            fs::write(store.root().join(PROJECTS_FILE), "{}").unwrap();
            let modified = fs::metadata(&folder).unwrap().modified().unwrap();
            let plan = plan(&store, &UserSettings::default(), None, modified + age);
            assert!(plan.versions.is_empty());
            plan.temporary == vec![folder]
        };
        assert!(listed(".tmp-old", Duration::from_secs(3600 + 1)));
        assert!(!listed(".tmp-new", Duration::from_secs(3600 - 1)));
    }

    /// A project whose lock is `text`.
    fn project_with_lock_text(text: &str) -> TempDir {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join(LOCK_FILE), text).unwrap();
        root
    }

    #[test]
    fn gc_refuses_a_lock_in_a_newer_format() {
        let (_dir, store) = store_with(&[("a", "1.0.0")]);
        let newer = project_with_lock_text(r#"{"format": 2}"#);
        store.record_project(newer.path()).unwrap();
        let error = plan_gc(&GcInput {
            store: &store,
            settings: &UserSettings::default(),
            current: None,
            now: SystemTime::now(),
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let message = error.to_string();
        assert!(message.contains(LOCK_FILE), "{message}");
        assert!(message.ends_with("fix it or remove the project before running gc"));
        assert!(store.path("a", &v("1.0.0")).exists());
    }

    #[test]
    fn gc_refuses_an_unparsable_current_lock() {
        let (_dir, store) = store_with(&[("a", "1.0.0")]);
        let current = project_with_lock_text("<<<<<<< HEAD\n");
        let result = plan_gc(&GcInput {
            store: &store,
            settings: &UserSettings::default(),
            current: Some(current.path()),
            now: SystemTime::now(),
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn gc_still_forgets_a_root_without_a_lock() {
        let (_dir, store) = store_with(&[]);
        let bare = TempDir::new().unwrap();
        store.record_project(bare.path()).unwrap();
        let plan = plan(&store, &UserSettings::default(), None, SystemTime::now());
        assert_eq!(plan.forgotten, vec![fs::canonicalize(bare.path()).unwrap()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_id_folder_is_never_followed() {
        let (dir, store) = store_with(&[]);
        let outside = dir.path().join("outside");
        fs::create_dir_all(outside.join("1.0.0")).unwrap();
        std::os::unix::fs::symlink(&outside, store.root().join("a")).unwrap();

        let found = plan(&store, &UserSettings::default(), None, SystemTime::now());
        assert!(found.versions.is_empty());
        let forged = GcPlan {
            versions: vec![("a".to_owned(), v("1.0.0"))],
            ..GcPlan::default()
        };
        assert!(run_gc(&store, &forged).is_err());
        let forged = GcPlan {
            temporary: vec![outside.clone()],
            ..GcPlan::default()
        };
        assert!(run_gc(&store, &forged).is_err());
        assert!(outside.join("1.0.0").exists());
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
