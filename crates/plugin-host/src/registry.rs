//! The plugins the registry client installed into its store
//! ([`registry_store_dir`], one folder per version at `<id>/<version>`), as
//! discovery finds them: the versions the project's `celesta.lock` names
//! ([`PluginSource::Locked`]; in the app only once the user trusts the
//! project), and the ones the user installed without
//! a project ([`PluginSource::Registry`]). A store folder runs without the
//! approval prompt, and only while its tree hash is the one recorded at
//! install: the lock's for this host, or the one in the user's settings.

use crate::advisories::cached_advisories;
use crate::child::PluginChild;
use crate::locations::{PluginSource, SearchPath, registry_store_dir};
use crate::lock::Lockfile;
use crate::manifest::{
    CELESTA_VERSION, DiscoveryError, MANIFEST, ManifestFilter, PluginEntry, PluginRead, PluginRole,
    discover_in_with, is_valid_id, read_manifest,
};
use crate::settings::UserSettings;
use celesta_registry_format::{AdvisoryList, Hash, Target, tree_hash};
use std::path::{Path, PathBuf};

/// A project's lock, with the project's root folder.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectLock {
    pub root: PathBuf,
    pub lock: Lockfile,
}

/// What discovery reads besides the folders it searches: where the store
/// is, the cached advisory list, and the open project's lock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegistryContext {
    /// The store ([`registry_store_dir`]); `None` without a user folder.
    pub store: Option<PathBuf>,
    /// The cached advisory list ([`crate::cached_advisories`]): a plugin it
    /// stops runs from no source.
    pub advisories: Option<AdvisoryList>,
    /// The project's lock: a trusted project's in the app, any project's
    /// for command-line tools; None for no lock.
    pub project: Option<ProjectLock>,
}

impl RegistryContext {
    /// Store, cached advisories, and `project`'s lock when `settings` trusts it.
    /// A list or lock that cannot be read is reported
    /// ([`DiscoveryError::Advisories`], [`DiscoveryError::Lock`]) and not
    /// used.
    pub fn current(project: Option<&Path>, settings: &UserSettings) -> (Self, Vec<DiscoveryError>) {
        Self::reading(project.filter(|root| settings.is_trusted(root)))
    }

    /// [`RegistryContext::current`] with `root`'s lock whether or not the
    /// user trusts it: what command-line tools read
    /// ([`crate::InstalledPlugins::find_for_project`]).
    pub fn for_project(root: &Path) -> (Self, Vec<DiscoveryError>) {
        Self::reading(Some(root))
    }

    /// Store, cached advisories, and `project`'s lock.
    fn reading(project: Option<&Path>) -> (Self, Vec<DiscoveryError>) {
        let mut errors = Vec::new();
        let (advisories, advisories_error) = cached_advisories();
        errors.extend(advisories_error.map(DiscoveryError::Advisories));
        let project = project.and_then(|root| match Lockfile::load(root) {
            Ok(lock) => lock.map(|lock| ProjectLock {
                root: root.to_path_buf(),
                lock,
            }),
            Err(error) => {
                errors.push(DiscoveryError::Lock(error.to_string()));
                None
            }
        });
        let context = Self {
            store: registry_store_dir(),
            advisories,
            project,
        };
        (context, errors)
    }

    /// Also stops what `other` stops, such as another registry's cached
    /// list: its advisories are added, and the newer `generated` is kept.
    pub fn merge_advisories(&mut self, other: AdvisoryList) {
        match &mut self.advisories {
            Some(list) => {
                list.generated = list.generated.max(other.generated);
                list.advisories.extend(other.advisories);
            }
            None => self.advisories = Some(other),
        }
    }

    /// The plugins under `paths` (as [`crate::search_paths`] gives them) and
    /// in the store, with what was skipped and why. A plugin `settings`
    /// turns off is listed but takes no id (see [`discover_in_with`]).
    pub(crate) fn discover(
        &self,
        paths: Vec<SearchPath>,
        settings: &UserSettings,
    ) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
        let store = StoreSearch::new(self, settings);
        discover_in_with(
            &store.around(paths),
            |entry| settings.is_disabled(&entry.manifest.id, &entry.directory),
            self.advisories.as_ref(),
            &store,
        )
    }
}

/// Why a locked version's store folder could not run, which decides what is
/// said about the lock besides the read error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LockedFailure {
    /// Its files, id, or version are not what was installed.
    Differs,
    /// It does not load with the running Celesta.
    Incompatible,
    /// It could not be read; that error already says why.
    Other,
}

impl LockedFailure {
    /// How `error`, from reading a locked store folder, failed it.
    pub(crate) fn of(error: &DiscoveryError) -> Self {
        match error {
            DiscoveryError::TreeHashMismatch { .. } => Self::Differs,
            DiscoveryError::Incompatible { .. } => Self::Incompatible,
            _ => Self::Other,
        }
    }
}

/// Whether `directory` is this build of a plugin: its manifest names `id`
/// and `version`, and its files hash to `tree_hash`. A missing or
/// unparsable manifest is no build. It does not check the manifest's
/// `celesta` range or the advisories.
pub fn is_build(directory: &Path, id: &str, version: &semver::Version, tree_hash: &Hash) -> bool {
    #[derive(serde::Deserialize)]
    struct Named {
        id: String,
        version: String,
    }
    let Ok(text) = std::fs::read(directory.join(MANIFEST)) else {
        return false;
    };
    let Ok(named) = serde_json::from_slice::<Named>(&text) else {
        return false;
    };
    named.id == id
        && semver::Version::parse(&named.version).is_ok_and(|found| found == *version)
        && celesta_registry_format::tree_hash(directory).is_ok_and(|found| found == *tree_hash)
}

/// Checks whether a verified, enabled store plugin answers the startup handshake.
/// The caller must exclude plugins blocked by advisories. Missing or changed
/// builds and plugins without an executable for this platform return `Ok(false)`.
/// A successful handshake returns `Ok(true)`.
pub fn probe_registry_plugin(
    directory: &Path,
    id: &str,
    version: &semver::Version,
    tree_hash: &Hash,
    settings: &UserSettings,
) -> Result<bool, String> {
    if !is_valid_id(id)
        || !directory.is_absolute()
        || settings.is_disabled(id, directory)
        || !is_build(directory, id, version, tree_hash)
    {
        return Ok(false);
    }
    let Some((entry, _)) = read_manifest(
        directory,
        PluginSource::Registry,
        ManifestFilter::NativeProgram,
    )
    .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };
    let Some(executable) = entry.executable() else {
        return Ok(false);
    };
    let mut child = PluginChild::start_quiet(&executable, directory)?;
    let hello = if entry.has_role(PluginRole::Media) {
        child.hello_media(serde_json::Map::new())
    } else {
        child.hello_content(id)
    };
    hello.map_err(|error| child.explain_start_error(&error))?;
    Ok(true)
}

/// A version in the store, and what its folder must hold to run.
#[derive(Debug)]
struct StoreFolder {
    id: String,
    version: semver::Version,
    /// Where it is installed; `None` without a store.
    path: Option<PathBuf>,
    /// The tree hash it was installed with; `None` for a locked version
    /// with no build for this host.
    tree_hash: Option<Hash>,
}

impl StoreFolder {
    /// Whether `entry` is what was installed: the same id and version, and
    /// the same files.
    fn holds(&self, entry: &PluginEntry) -> bool {
        self.tree_hash
            .as_ref()
            .is_some_and(|expected| is_build(&entry.directory, &self.id, &self.version, expected))
    }

    /// Whether `entry` has this id and version.
    fn is(&self, entry: &PluginEntry) -> bool {
        entry.manifest.id == self.id
            && semver::Version::parse(&entry.manifest.version)
                .is_ok_and(|version| version == self.version)
    }

    /// Why this locked version was not used.
    fn unsatisfied(&self, reason: impl Into<String>) -> DiscoveryError {
        DiscoveryError::LockUnsatisfied {
            id: self.id.clone(),
            version: self.version.to_string(),
            reason: reason.into(),
        }
    }
}

/// The store folders discovery searches, what each must hold to run, and
/// how a locked version whose folder holds nothing is settled.
#[derive(Debug, Default)]
pub(crate) struct StoreSearch {
    /// Each version the lock names, in id order.
    locked: Vec<StoreFolder>,
    /// Each version the user installed globally, in the settings' order.
    global: Vec<StoreFolder>,
}

impl StoreSearch {
    fn new(context: &RegistryContext, settings: &UserSettings) -> Self {
        // Ids that cannot name a plugin are left out, so a folder is never
        // looked for outside the store.
        let folder = |id: &str, version: &semver::Version| {
            context
                .store
                .as_ref()
                .filter(|_| is_valid_id(id))
                .map(|store| store.join(id).join(version.to_string()))
        };
        let host = Target::host();
        let locked = context
            .project
            .iter()
            .flat_map(|project| &project.lock.plugins)
            .map(|(id, locked)| StoreFolder {
                id: id.clone(),
                version: locked.version.clone(),
                path: folder(id, &locked.version),
                tree_hash: context
                    .project
                    .as_ref()
                    .and_then(|project| project.lock.artifact(id, &host))
                    .map(|artifact| artifact.tree_hash),
            })
            .collect();
        let global = settings
            .registry_plugins
            .iter()
            .map(|installed| StoreFolder {
                id: installed.id.clone(),
                version: installed.version.clone(),
                path: folder(&installed.id, &installed.version),
                tree_hash: Some(installed.tree_hash),
            })
            .collect();
        Self { locked, global }
    }

    /// `paths` with the store folders in their places: each version the
    /// lock names with a build for this host, in id order, after the
    /// explicit folders; each one the user installed globally, in the
    /// settings' order, after the user's plugin folder.
    fn around(&self, mut paths: Vec<SearchPath>) -> Vec<SearchPath> {
        paths.extend(self.paths());
        // Stable, so the folders of each source keep their order: the
        // derived order of sources is the search order.
        paths.sort_by_key(|path| path.source);
        paths
    }

    /// The store folders to search.
    fn paths(&self) -> impl Iterator<Item = SearchPath> + '_ {
        let locked = self
            .locked
            .iter()
            .filter(|folder| folder.tree_hash.is_some())
            .map(|folder| (folder, PluginSource::Locked));
        let global = self
            .global
            .iter()
            .map(|folder| (folder, PluginSource::Registry));
        locked.chain(global).filter_map(|(folder, source)| {
            Some(SearchPath {
                path: folder.path.clone()?,
                source,
            })
        })
    }

    /// The plugin in the store folder `search`, checked against what was
    /// installed there ([`DiscoveryError::TreeHashMismatch`] when it is not
    /// that); `None` when nothing is.
    pub(crate) fn read(&self, search: &SearchPath) -> Option<PluginRead> {
        let directory = &search.path;
        let (entry, skipped) = match read_manifest(directory, search.source, ManifestFilter::Any) {
            Ok(Some(read)) => read,
            Ok(None) => return None,
            Err(error) => return Some(Err(error)),
        };
        let folders = match search.source {
            PluginSource::Locked => &self.locked,
            _ => &self.global,
        };
        let installed = folders
            .iter()
            .find(|folder| folder.path.as_ref() == Some(directory));
        if installed.is_some_and(|installed| installed.holds(&entry)) {
            return Some(Ok((entry, skipped)));
        }
        Some(Err(DiscoveryError::TreeHashMismatch {
            path: directory.clone(),
            id: installed.map_or(entry.manifest.id, |installed| installed.id.clone()),
        }))
    }

    /// Settles each version the lock names that cannot run from the store.
    /// One with no build for this host is reported as
    /// [`DiscoveryError::LockNoBuild`]. One whose folder held nothing
    /// (`missing`), or whose folder held something that cannot run
    /// (`failed`, already reported), is reported as
    /// [`DiscoveryError::LockUnsatisfied`], unless a bundled copy in `read`
    /// has its id, version, and tree hash. That copy runs as the locked
    /// version ([`PluginSource::Locked`], replacing
    /// [`PluginSource::Bundled`]), ranked first for its id after the
    /// explicit folders.
    pub(crate) fn settle(
        &self,
        missing: &[PathBuf],
        failed: &[(PathBuf, LockedFailure)],
        read: &mut Vec<PluginRead>,
    ) {
        for locked in &self.locked {
            let Some(expected) = locked.tree_hash else {
                read.push(Err(DiscoveryError::LockNoBuild {
                    id: locked.id.clone(),
                    version: locked.version.to_string(),
                    host: Target::host().as_str().to_owned(),
                }));
                continue;
            };
            let failure = locked.path.as_ref().and_then(|path| {
                failed
                    .iter()
                    .find_map(|(failed, failure)| (failed == path).then_some(*failure))
            });
            if failure.is_none()
                && locked
                    .path
                    .as_ref()
                    .is_some_and(|path| !missing.contains(path))
            {
                // Runs from the store (unless a later stage stops it).
                continue;
            }
            let differs = "its installed files differ from the lock";
            let bundled = read.iter().enumerate().find_map(|(at, read)| {
                let (entry, _) = read.as_ref().ok()?;
                (entry.source == PluginSource::Bundled && locked.is(entry)).then_some((at, entry))
            });
            let Some((at, entry)) = bundled else {
                // A store folder that failed holds the locked version in
                // some form, so the pin is said to be broken rather than
                // absent, and why when the read error does not say.
                let reason = match failure {
                    None => "it is not installed".to_owned(),
                    Some(LockedFailure::Differs) => differs.to_owned(),
                    // No command of its own: `LockProblem::Unsatisfied`
                    // names the one to run.
                    Some(LockedFailure::Incompatible) => {
                        format!("it does not load with Celesta {CELESTA_VERSION}")
                    }
                    Some(LockedFailure::Other) => continue,
                };
                read.push(Err(locked.unsatisfied(reason)));
                continue;
            };
            if !tree_hash(&entry.directory).is_ok_and(|hash| hash == expected) {
                read.push(Err(locked.unsatisfied(differs)));
                continue;
            }
            let mut copy = read.remove(at);
            if let Ok((entry, _)) = &mut copy {
                entry.source = PluginSource::Locked;
                entry.replaces = Some(PluginSource::Bundled);
            }
            // Before every other copy of its id but an explicit one.
            let first = read.iter().position(|read| {
                matches!(read, Ok((other, _)) if other.manifest.id == locked.id
                    && other.source > PluginSource::Locked)
            });
            read.insert(first.map_or(at, |first| first.min(at)), copy);
        }
    }
}

impl PluginSource {
    /// Whether plugins from here are store folders, checked by their tree
    /// hash ([`StoreSearch::read`]).
    pub(crate) fn is_store(self) -> bool {
        matches!(self, Self::Locked | Self::Registry)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::lock::{LockedArtifact, LockedPlugin};
    use crate::manifest::tests::{COMPATIBLE, write_manifest};
    use crate::manifest::{InstalledPlugins, platform};
    use crate::media::MediaOrigin;
    use crate::settings::RegistryPlugin;
    use celesta_plugin::protocol_for_roles;
    use celesta_registry_format::{Target, tree_hash};
    use serde_json::{Map, Value, json};

    /// A content plugin's manifest.
    pub(crate) fn content(id: &str, version: &str) -> Value {
        json!({
            "id": id, "version": version, "protocol": protocol_for_roles(&["content"]),
            "celesta": COMPATIBLE, "roles": ["content"],
        })
    }

    /// A media plugin's manifest, with a program for this platform.
    fn media(id: &str, version: &str) -> Value {
        json!({
            "id": id, "version": version, "protocol": protocol_for_roles(&["media"]),
            "celesta": COMPATIBLE, "roles": ["media"],
            "executable": { platform().0: "bin/media" }, "media": { "decoders": [] },
        })
    }

    /// Writes a plugin to `directory`, with a file besides its manifest, and
    /// gives its tree hash.
    pub(crate) fn write(directory: &Path, manifest: Value) -> Hash {
        write_manifest(directory, manifest);
        std::fs::write(directory.join("lib.txt"), "the plugin's code").unwrap();
        tree_hash(directory).unwrap()
    }

    #[test]
    fn is_build_needs_id_version_and_tree_hash() {
        let folder = tempfile::tempdir().unwrap();
        let dir = folder.path();
        let hash = write(dir, content("acme.charts", "1.2.3"));
        let v = |text: &str| semver::Version::parse(text).unwrap();
        assert!(is_build(dir, "acme.charts", &v("1.2.3"), &hash));
        assert!(!is_build(dir, "acme.other", &v("1.2.3"), &hash));
        assert!(!is_build(dir, "acme.charts", &v("1.2.4"), &hash));
        std::fs::write(dir.join("lib.txt"), "changed").unwrap();
        assert!(!is_build(dir, "acme.charts", &v("1.2.3"), &hash));
        let bare = tempfile::tempdir().unwrap();
        std::fs::write(bare.path().join("lib.txt"), "x").unwrap();
        let bare_hash = tree_hash(bare.path()).unwrap();
        assert!(!is_build(
            bare.path(),
            "acme.charts",
            &v("1.2.3"),
            &bare_hash
        ));
    }

    #[test]
    fn probe_reports_a_build_incompatible_with_this_celesta() {
        let folder = tempfile::tempdir().unwrap();
        let dir = folder.path();
        let mut manifest = media("acme.codec", "1.2.3");
        manifest["celesta"] = json!("<0.1");
        let hash = write(dir, manifest);
        let error = probe_registry_plugin(
            dir,
            "acme.codec",
            &semver::Version::new(1, 2, 3),
            &hash,
            &UserSettings::default(),
        )
        .unwrap_err();
        assert!(error.contains("works with Celesta <0.1"), "{error}");
    }

    #[test]
    fn probe_skips_an_incompatible_module_without_a_native_program() {
        let folder = tempfile::tempdir().unwrap();
        let dir = folder.path();
        let mut manifest = content("acme.module", "1.2.3");
        manifest["celesta"] = json!("<0.1");
        manifest["module"] = json!("index.js");
        std::fs::write(dir.join("index.js"), "").unwrap();
        let hash = write(dir, manifest);
        assert!(
            !probe_registry_plugin(
                dir,
                "acme.module",
                &semver::Version::new(1, 2, 3),
                &hash,
                &UserSettings::default(),
            )
            .unwrap()
        );
    }

    #[cfg(unix)]
    fn probe_answer(mut manifest: Value, answer: Value, script: &str) -> Result<bool, String> {
        use celesta_plugin::protocol::{Message, write_message};
        use std::os::unix::fs::PermissionsExt;

        let folder = tempfile::tempdir().unwrap();
        let dir = folder.path();
        let program = dir.join("plugin.sh");
        std::fs::write(&program, format!("#!/bin/sh\n{script}\ncat answer.bin\n")).unwrap();
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
        write_message(
            &mut std::fs::File::create(dir.join("answer.bin")).unwrap(),
            &Message::new(answer),
        )
        .unwrap();
        manifest["executable"] = json!({ (platform().0): "plugin.sh" });
        let hash = write(dir, manifest);
        probe_registry_plugin(
            dir,
            "acme.codec",
            &semver::Version::new(1, 2, 3),
            &hash,
            &UserSettings::default(),
        )
    }

    #[cfg(unix)]
    #[test]
    fn probe_rejects_the_same_hello_answers_as_the_runtime() {
        let error = probe_answer(
            content("acme.codec", "1.2.3"),
            json!({"type": "hello", "protocol": 1, "plugin": "acme.other"}),
            "echo 'wrong plugin id' >&2",
        )
        .unwrap_err();
        assert!(error.contains("not acme.codec"), "{error}");
        assert!(error.contains("wrong plugin id"), "{error}");
        let error = probe_answer(
            media("acme.codec", "1.2.3"),
            json!({"type": "hello", "protocol": 2, "roles": ["content"]}),
            "echo 'wrong role' >&2",
        )
        .unwrap_err();
        assert!(error.contains("is not a media plugin"), "{error}");
        assert!(error.contains("wrong role"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn a_refused_start_includes_the_plugins_stderr() {
        let error = probe_answer(
            media("acme.codec", "1.2.3"),
            json!({"type": "error", "message": "startup refused"}),
            "echo 'a required library could not be loaded' >&2",
        )
        .unwrap_err();
        assert!(error.contains("startup refused"), "{error}");
        assert!(
            error.contains("a required library could not be loaded"),
            "{error}"
        );
    }

    /// A test's folders: the store, and an explicit, a user, and a bundled
    /// folder of plugins.
    struct Folders {
        root: tempfile::TempDir,
        store: PathBuf,
        explicit: PathBuf,
        user: PathBuf,
        bundled: PathBuf,
    }

    impl Folders {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let folder = |name: &str| {
                let folder = root.path().join(name);
                std::fs::create_dir_all(&folder).unwrap();
                folder
            };
            Self {
                store: folder("registry"),
                explicit: folder("explicit"),
                user: folder("user"),
                bundled: folder("bundled"),
                root,
            }
        }

        /// Installs `manifest` into the store, at its id and version.
        fn install(&self, manifest: Value) -> Hash {
            let directory = self.installed(
                manifest["id"].as_str().unwrap(),
                manifest["version"].as_str().unwrap(),
            );
            write(&directory, manifest)
        }

        fn installed(&self, id: &str, version: &str) -> PathBuf {
            self.store.join(id).join(version)
        }

        fn context(&self, lock: Option<Lockfile>) -> RegistryContext {
            RegistryContext {
                store: Some(self.store.clone()),
                advisories: None,
                project: lock.map(|lock| ProjectLock {
                    root: self.root.path().to_path_buf(),
                    lock,
                }),
            }
        }

        /// What discovery finds in these folders with `context`.
        fn discover(
            &self,
            context: &RegistryContext,
            settings: &UserSettings,
        ) -> (Vec<PluginEntry>, Vec<DiscoveryError>) {
            let search = |path: &PathBuf, source| SearchPath {
                path: path.clone(),
                source,
            };
            context.discover(
                vec![
                    search(&self.explicit, PluginSource::Explicit),
                    search(&self.user, PluginSource::User),
                    search(&self.bundled, PluginSource::Bundled),
                ],
                settings,
            )
        }
    }

    /// A locked plugin as a test gives it: id, version, and each target's
    /// tree hash.
    pub(crate) type Locked<'a> = (&'a str, &'a str, &'a [(&'a str, Hash)]);

    pub(crate) fn lock(plugins: &[Locked]) -> Lockfile {
        Lockfile {
            celesta: semver::Version::new(0, 1, 0),
            targets: Vec::new(),
            plugins: plugins
                .iter()
                .map(|&(id, version, targets)| {
                    let locked = LockedPlugin {
                        // Not `*`, which matches no pre-release Celesta.
                        celesta: semver::VersionReq::parse(COMPATIBLE).unwrap(),
                        registry: "https://registry.celesta.studio".to_owned(),
                        targets: targets
                            .iter()
                            .map(|&(target, tree_hash)| {
                                let artifact = LockedArtifact {
                                    sha256: Hash([0; 32]),
                                    tree_hash,
                                };
                                (Target::parse(target).unwrap(), artifact)
                            })
                            .collect(),
                        version: version.parse().unwrap(),
                        other: Map::new(),
                    };
                    (id.to_owned(), locked)
                })
                .collect(),
            other: Map::new(),
        }
    }

    fn host() -> String {
        Target::host().to_string()
    }

    /// The plugin that took `id`: the first listed with it.
    fn running<'a>(found: &'a [PluginEntry], id: &str) -> &'a PluginEntry {
        found
            .iter()
            .find(|entry| entry.manifest.id == id)
            .unwrap_or_else(|| panic!("{id} is not found: {found:?}"))
    }

    #[test]
    fn a_locked_version_ranks_first() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        let hash = folders.install(content("p", "0.9.0"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(found.len(), 1, "{found:?}");
        let p = running(&found, "p");
        assert_eq!(p.manifest.version, "0.9.0");
        assert_eq!(p.source, PluginSource::Locked);
        assert_eq!(p.directory, folders.installed("p", "0.9.0"));
        assert_eq!(p.replaces, Some(PluginSource::User));
        // A locked version waits for no one's approval: its check is the
        // tree hash.
        let allowed = InstalledPlugins {
            plugins: found,
            ..InstalledPlugins::default()
        }
        .allowed_by(&UserSettings::default());
        assert_eq!(allowed.plugins.plugins.len(), 1);
        assert!(allowed.pending.is_empty());
    }

    #[test]
    fn an_artifact_for_any_target_is_used_too() {
        let folders = Folders::new();
        let hash = folders.install(content("p", "0.9.0"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[("any", hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(running(&found, "p").source, PluginSource::Locked);
    }

    #[test]
    fn a_pinned_bundled_media_plugin_keeps_its_origin() {
        let folders = Folders::new();
        write(&folders.bundled.join("ffmpeg"), media("ffmpeg", "0.9.2"));
        let hash = folders.install(media("ffmpeg", "0.9.0"));
        let context = folders.context(Some(lock(&[("ffmpeg", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        // Unlike a project's own plugin, a locked version may replace a
        // bundled media plugin.
        assert!(errors.is_empty(), "{errors:?}");
        let ffmpeg = running(&found, "ffmpeg");
        assert_eq!(ffmpeg.manifest.version, "0.9.0");
        assert_eq!(ffmpeg.source, PluginSource::Locked);
        assert_eq!(ffmpeg.replaces, Some(PluginSource::Bundled));
        assert_eq!(ffmpeg.media_origin(), MediaOrigin::Bundled);
        let media = InstalledPlugins {
            plugins: found,
            ..InstalledPlugins::default()
        }
        .media_with_sibling(None);
        assert_eq!(media.len(), 1, "{media:?}");
        assert_eq!(media[0].version, "0.9.0");
        assert_eq!(media[0].origin, MediaOrigin::Bundled);
    }

    #[test]
    fn a_locked_plugin_that_replaces_nothing_ranks_as_the_users() {
        let folders = Folders::new();
        let hash = folders.install(media("ffmpeg", "0.9.0"));
        let context = folders.context(Some(lock(&[("ffmpeg", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(running(&found, "ffmpeg").replaces, None);
        assert_eq!(running(&found, "ffmpeg").media_origin(), MediaOrigin::User);
    }

    #[test]
    fn explicit_folders_still_come_first() {
        let folders = Folders::new();
        write(&folders.explicit.join("p"), content("p", "2.0.0"));
        let hash = folders.install(content("p", "0.9.0"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(errors.is_empty(), "{errors:?}");
        let p = running(&found, "p");
        assert_eq!(p.manifest.version, "2.0.0");
        assert_eq!(p.source, PluginSource::Explicit);
        assert_eq!(p.replaces, Some(PluginSource::Locked));
    }

    #[test]
    fn a_bundled_copy_satisfies_the_lock() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        let hash = write(&folders.bundled.join("p"), content("p", "0.9.0"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(errors.is_empty(), "{errors:?}");
        // The bundled copy ranks first for its id, ahead of the user's.
        assert_eq!(found.len(), 1, "{found:?}");
        let p = running(&found, "p");
        assert_eq!(p.manifest.version, "0.9.0");
        assert_eq!(p.source, PluginSource::Locked);
        assert_eq!(p.directory, folders.bundled.join("p"));
        assert_eq!(p.replaces, Some(PluginSource::Bundled));
    }

    #[test]
    fn a_bundled_copy_that_differs_does_not_satisfy_the_lock() {
        let folders = Folders::new();
        write(&folders.bundled.join("p"), content("p", "0.9.0"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), Hash([1; 32]))])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        let [
            DiscoveryError::LockUnsatisfied {
                id,
                version,
                reason,
            },
        ] = &errors[..]
        else {
            panic!("{errors:?}");
        };
        assert_eq!((id.as_str(), version.as_str()), ("p", "0.9.0"));
        assert_eq!(reason, "its installed files differ from the lock");
        assert_eq!(running(&found, "p").source, PluginSource::Bundled);
    }

    #[test]
    fn a_tampered_store_copy_is_skipped() {
        let folders = Folders::new();
        write(&folders.bundled.join("p"), content("p", "0.9.2"));
        let hash = folders.install(content("p", "0.9.0"));
        let installed = folders.installed("p", "0.9.0");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(installed.join("lib.txt"))
            .unwrap();
        std::io::Write::write_all(&mut file, b"!").unwrap();
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        // The bundled copy is another version, so the pin is not in effect.
        let [
            DiscoveryError::TreeHashMismatch { path, id },
            DiscoveryError::LockUnsatisfied { reason, .. },
        ] = &errors[..]
        else {
            panic!("{errors:?}");
        };
        assert_eq!((path, id.as_str()), (&installed, "p"));
        assert_eq!(reason, "its installed files differ from the lock");
        assert_eq!(
            errors[0].to_string(),
            format!(
                "{} was skipped: its files changed since it was installed from the registry; install p again",
                installed.display()
            )
        );
        let p = running(&found, "p");
        assert_eq!(p.manifest.version, "0.9.2");
        assert_eq!(p.source, PluginSource::Bundled);
    }

    #[test]
    fn a_store_copy_that_does_not_load_with_this_celesta_is_not_called_tampered() {
        let folders = Folders::new();
        let mut manifest = content("p", "0.9.0");
        manifest["celesta"] = json!(">=999.0.0");
        let hash = folders.install(manifest);
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(found.is_empty(), "{found:?}");
        let [
            DiscoveryError::Incompatible { .. },
            DiscoveryError::LockUnsatisfied { reason, .. },
        ] = &errors[..]
        else {
            panic!("{errors:?}");
        };
        // LockProblem::Unsatisfied adds the one command to run.
        assert_eq!(
            reason,
            &format!("it does not load with Celesta {CELESTA_VERSION}")
        );
        assert!(
            errors
                .iter()
                .all(|error| !error.to_string().contains("differ")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_store_copy_of_another_version_is_skipped() {
        let folders = Folders::new();
        // The folder for 0.9.0 holds 0.9.1, hashed as it is.
        let directory = folders.installed("p", "0.9.0");
        let hash = write(&directory, content("p", "0.9.1"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(found.is_empty(), "{found:?}");
        assert!(
            matches!(&errors[..], [
                DiscoveryError::TreeHashMismatch { path, .. },
                DiscoveryError::LockUnsatisfied { reason, .. },
            ] if path == &directory && reason == "its installed files differ from the lock"),
            "{errors:?}"
        );
    }

    #[test]
    fn a_bundled_copy_satisfies_the_lock_in_place_of_a_tampered_one() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        let hash = write(&folders.bundled.join("p"), content("p", "0.9.0"));
        assert_eq!(folders.install(content("p", "0.9.0")), hash);
        let installed = folders.installed("p", "0.9.0");
        std::fs::write(installed.join("lib.txt"), "changed").unwrap();
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(
            matches!(&errors[..], [DiscoveryError::TreeHashMismatch { path, .. }] if path == &installed),
            "{errors:?}"
        );
        // The pin is in effect, from the bundled copy, ahead of the user's.
        assert_eq!(found.len(), 1, "{found:?}");
        let p = running(&found, "p");
        assert_eq!(p.manifest.version, "0.9.0");
        assert_eq!(p.source, PluginSource::Locked);
        assert_eq!(p.directory, folders.bundled.join("p"));
        assert_eq!(p.replaces, Some(PluginSource::Bundled));
    }

    #[test]
    fn an_untrusted_projects_lock_is_ignored() {
        let project = tempfile::tempdir().unwrap();
        let locked = lock(&[("p", "0.9.0", &[("any", Hash([1; 32]))])]);
        locked.save(project.path()).unwrap();
        let mut settings = UserSettings::default();
        let (context, _) = RegistryContext::current(Some(project.path()), &settings);
        assert_eq!(context.project, None);
        assert_eq!(context.store, registry_store_dir());
        settings.trust(project.path());
        let (context, _) = RegistryContext::current(Some(project.path()), &settings);
        assert_eq!(context.project.map(|project| project.lock), Some(locked));
        let (context, _) = RegistryContext::current(None, &settings);
        assert_eq!(context.project, None);
    }

    #[test]
    fn a_lock_that_cannot_be_read_is_reported() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join(crate::LOCK_FILE), "not a lock").unwrap();
        let mut settings = UserSettings::default();
        settings.trust(project.path());
        let (context, errors) = RegistryContext::current(Some(project.path()), &settings);
        assert_eq!(context.project, None);
        let lock = project.path().join(crate::LOCK_FILE);
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, DiscoveryError::Lock(text)
                if text.contains(&lock.display().to_string()))),
            "{errors:?}"
        );
    }

    #[test]
    fn a_missing_locked_plugin_is_reported() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), Hash([1; 32]))])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        let [
            DiscoveryError::LockUnsatisfied {
                id,
                version,
                reason,
            },
        ] = &errors[..]
        else {
            panic!("{errors:?}");
        };
        assert_eq!((id.as_str(), version.as_str()), ("p", "0.9.0"));
        assert_eq!(reason, "it is not installed");
        assert_eq!(
            errors[0].to_string(),
            "the project locks p 0.9.0, which was not used: it is not installed"
        );
        let p = running(&found, "p");
        assert_eq!(p.manifest.version, "1.0.0");
        assert_eq!(p.source, PluginSource::User);
    }

    #[test]
    fn no_build_for_this_target_falls_back() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        let hash = folders.install(content("p", "0.9.0"));
        let context = folders.context(Some(lock(&[("p", "0.9.0", &[("plan9-mips", hash)])])));
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        let [
            DiscoveryError::LockNoBuild {
                id,
                version,
                host: on,
            },
        ] = &errors[..]
        else {
            panic!("{errors:?}");
        };
        assert_eq!((id.as_str(), version.as_str()), ("p", "0.9.0"));
        assert_eq!(on, &host());
        assert_eq!(running(&found, "p").source, PluginSource::User);
    }

    #[test]
    fn global_registry_plugins_rank_after_the_user_folder() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        write(&folders.bundled.join("q"), content("q", "0.5.0"));
        let installed = |manifest: Value| {
            let id = manifest["id"].as_str().unwrap().to_owned();
            let version = manifest["version"].as_str().unwrap().parse().unwrap();
            let tree_hash = folders.install(manifest);
            RegistryPlugin {
                id,
                version,
                tree_hash,
            }
        };
        let mut r = installed(content("r", "1.0.0"));
        r.tree_hash = Hash([1; 32]);
        let settings = UserSettings {
            registry_plugins: vec![
                installed(content("p", "2.0.0")),
                installed(content("q", "1.0.0")),
                r,
            ],
            ..UserSettings::default()
        };
        let (found, errors) = folders.discover(&folders.context(None), &settings);
        // `r` was installed with another tree hash.
        assert!(
            matches!(&errors[..], [DiscoveryError::TreeHashMismatch { id, .. }] if id == "r"),
            "{errors:?}"
        );
        let p = running(&found, "p");
        assert_eq!(
            (p.manifest.version.as_str(), p.source),
            ("1.0.0", PluginSource::User)
        );
        assert_eq!(p.replaces, Some(PluginSource::Registry));
        let q = running(&found, "q");
        assert_eq!(
            (q.manifest.version.as_str(), q.source),
            ("1.0.0", PluginSource::Registry)
        );
        assert_eq!(q.replaces, Some(PluginSource::Bundled));
        // A global plugin waits for no one's approval; the user's own does.
        let allowed = InstalledPlugins {
            plugins: found,
            ..InstalledPlugins::default()
        }
        .allowed_by(&settings);
        let ids: Vec<_> = allowed
            .plugins
            .plugins
            .iter()
            .map(|entry| entry.manifest.id.as_str())
            .collect();
        assert_eq!(ids, ["q"]);
        assert_eq!(allowed.pending[0].entry.manifest.id, "p");
    }

    #[test]
    fn a_blocked_locked_plugin_does_not_run() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        let hash = folders.install(content("p", "0.9.0"));
        let mut context = folders.context(Some(lock(&[("p", "0.9.0", &[(&host(), hash)])])));
        let advisories = json!({
            "format": 1, "generated": "2026-11-10T00:00:00Z",
            "advisories": [{
                "id": "CEL-2026-0001", "plugin": "p", "versions": "<1.0.0",
                "title": "Reads files outside the project", "description": "Details.",
                "published": "2026-11-01T00:00:00Z",
            }],
        });
        context.advisories = Some(AdvisoryList::parse(advisories.to_string().as_bytes()).unwrap());
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        assert!(
            matches!(&errors[..], [DiscoveryError::Blocked { version, .. }] if version == "0.9.0"),
            "{errors:?}"
        );
        let p = running(&found, "p");
        assert_eq!(
            (p.manifest.version.as_str(), p.source),
            ("1.0.0", PluginSource::User)
        );
    }

    #[test]
    fn merged_advisories_stop_plugins_from_both_lists() {
        let folders = Folders::new();
        write(&folders.user.join("p"), content("p", "1.0.0"));
        write(&folders.user.join("q"), content("q", "1.0.0"));
        let list = |generated: &str, plugin: &str| {
            let list = json!({
                "format": 1, "generated": generated,
                "advisories": [{
                    "id": format!("CEL-{plugin}"), "plugin": plugin, "versions": "<2.0.0",
                    "title": "Reads files outside the project", "description": "Details.",
                    "published": "2026-11-01T00:00:00Z",
                }],
            });
            AdvisoryList::parse(list.to_string().as_bytes()).unwrap()
        };
        let mut context = folders.context(None);
        context.merge_advisories(list("2026-11-10T00:00:00Z", "p"));
        context.merge_advisories(list("2026-11-12T00:00:00Z", "q"));
        let merged = context.advisories.as_ref().unwrap();
        assert_eq!(merged.advisories.len(), 2);
        assert_eq!(
            merged.generated,
            list("2026-11-12T00:00:00Z", "q").generated
        );
        let (found, errors) = folders.discover(&context, &UserSettings::default());
        let blocked: Vec<_> = errors
            .iter()
            .filter_map(|error| match error {
                DiscoveryError::Blocked { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(blocked, ["p", "q"]);
        assert!(
            found
                .iter()
                .all(|entry| !["p", "q"].contains(&entry.manifest.id.as_str()))
        );
    }

    #[test]
    fn store_folders_take_their_places_in_search_order() {
        let folders = Folders::new();
        let context = folders.context(Some(lock(&[
            ("p", "0.9.0", &[("any", Hash([1; 32]))]),
            // No build for this host: not searched.
            ("q", "1.0.0", &[("plan9-mips", Hash([1; 32]))]),
        ])));
        let settings = UserSettings {
            registry_plugins: vec![RegistryPlugin {
                id: "r".into(),
                version: semver::Version::new(1, 0, 0),
                tree_hash: Hash([1; 32]),
            }],
            ..UserSettings::default()
        };
        let search = |name: &str, source| SearchPath {
            path: folders.explicit.join(name),
            source,
        };
        let paths = StoreSearch::new(&context, &settings).around(vec![
            search("e", PluginSource::Explicit),
            search("project", PluginSource::Project),
            search("user", PluginSource::User),
            search("added", PluginSource::Folder),
            search("bundled", PluginSource::Bundled),
        ]);
        assert_eq!(
            paths,
            [
                search("e", PluginSource::Explicit),
                SearchPath {
                    path: folders.installed("p", "0.9.0"),
                    source: PluginSource::Locked
                },
                search("project", PluginSource::Project),
                search("user", PluginSource::User),
                SearchPath {
                    path: folders.installed("r", "1.0.0"),
                    source: PluginSource::Registry
                },
                search("added", PluginSource::Folder),
                search("bundled", PluginSource::Bundled),
            ]
        );
        // Without a store there is nothing more to search.
        let without = RegistryContext {
            store: None,
            ..context
        };
        let paths =
            StoreSearch::new(&without, &settings).around(vec![search("e", PluginSource::Explicit)]);
        assert_eq!(paths, [search("e", PluginSource::Explicit)]);
    }
}
