//! The registry work of one window: the search list and the indexes it
//! fetched, and the installs, updates, and removals in flight, one at a
//! time per plugin. Every fetch and operation runs on the background
//! executor; this entity holds what the Plugins tab and the plugin page
//! show, so a row and the page show the same install.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use celesta_plugin_host::{Lockfile, UserSettings};
use celesta_registry_client::{
    ClientError, OfferInput, OfferScope, Progress, Registry, Rewrite, TransportError,
    add_dependency, install_global, install_locked, offered_requirement, offered_version,
    prereleases_by_default, remove_dependency, update_dependencies,
};
use celesta_registry_format::{Index, SearchList};
use gpui_kit::{Context, EventEmitter, Task};
use semver::{Comparator, Op, Version, VersionReq};
use time::OffsetDateTime;

use super::client::EditorRegistry;
use crate::problems::error_chain;

/// How often a download's progress reaches the entity: at most ten times a
/// second, however often the archive is read.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// How often the advisories and the search list are fetched again.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// What is being done to a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Work {
    /// Installing this version for the user, without a project.
    InstallGlobal {
        version: Version,
    },
    Add,
    Pin,
    Update,
    Remove,
    Uninstall,
    /// Downloading what the project's `celesta.lock` names.
    Download,
}

/// One plugin's work in flight, and how far its download has come.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InFlight {
    pub(crate) work: Work,
    pub(crate) done: u64,
    pub(crate) total: u64,
}

/// A registry file being fetched, or what the last fetch gave.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Fetch<T> {
    Idle,
    Loading,
    Loaded {
        value: T,
        /// When it was fetched; offline, when the registry generated the
        /// cached copy.
        #[allow(dead_code)] // The plugin page of a later change reads it.
        at: OffsetDateTime,
        /// The registry could not be reached; this is the copy verified
        /// and cached by an earlier fetch.
        offline: bool,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RegistryEvent {
    /// Store or settings changed: rediscover; `project` is Some(root) when its lock changed, so it reloads.
    PluginsChanged { project: Option<PathBuf> },
    Failed {
        id: Option<String>,
        title: String,
        detail: String,
    },
}

pub(crate) struct RegistryState {
    client: Arc<EditorRegistry>,
    /// Whether the plugin page offers pre-releases; remembered while the
    /// app runs, not saved.
    pub(crate) include_prereleases: bool,
    pub(crate) search: Fetch<SearchList>,
    pub(crate) indexes: BTreeMap<String, Fetch<Index>>,
    pub(crate) in_flight: BTreeMap<String, InFlight>,
    /// The page's red line, per id; cleared by the next attempt.
    pub(crate) failures: BTreeMap<String, String>,
    /// "No newer version works with Celesta …" until the next check.
    pub(crate) no_newer: BTreeSet<String>,
    /// The open project's root, as `project_changed` was last told: a
    /// project's work is announced only while that project is open.
    project: Option<PathBuf>,
    /// The projects a job is changing (their `celesta.config.json` and
    /// `celesta.lock`): one job per project at a time, so none restores or
    /// rewrites a file over another's change.
    busy_projects: BTreeSet<PathBuf>,
    /// A job is changing the user's settings (`registryPlugins`), which is
    /// read, changed, and saved whole: one such job at a time.
    settings_busy: bool,
    /// `search.json` is being fetched.
    searching: bool,
    /// The daily advisory and search list refresh.
    timers: Option<Task<()>>,
}

impl EventEmitter<RegistryEvent> for RegistryState {}

/// One operation: what it does, to which plugins, and for which project.
struct Job {
    work: Work,
    /// The plugins it is for, marked in flight from the start.
    ids: Vec<String>,
    /// The project whose files it changes; `None` for the user's own
    /// plugins.
    project: Option<PathBuf>,
    /// What the page says when it fails, unless the registry could not be
    /// reached.
    failure: String,
    run: Run,
}

/// The blocking part of a job, told how each download comes along.
type Run =
    Box<dyn FnOnce(&EditorRegistry, &(dyn Fn(Progress) + Sync)) -> anyhow::Result<Done> + Send>;

/// What a finished job did.
struct Done {
    /// Whether the store, the settings, or the project's files changed.
    changed: bool,
    /// The ids that have no newer version after all.
    no_newer: Vec<String>,
}

impl Done {
    const CHANGED: Self = Self {
        changed: true,
        no_newer: Vec::new(),
    };
}

/// The latest progress per id, written by the download and read by the
/// entity every [`PROGRESS_INTERVAL`].
type Sampled = BTreeMap<String, (u64, u64)>;

impl RegistryState {
    pub(crate) fn new(client: Arc<EditorRegistry>) -> Self {
        Self {
            include_prereleases: prereleases_by_default(client.celesta()),
            client,
            search: Fetch::Idle,
            indexes: BTreeMap::new(),
            in_flight: BTreeMap::new(),
            failures: BTreeMap::new(),
            no_newer: BTreeSet::new(),
            project: None,
            busy_projects: BTreeSet::new(),
            settings_busy: false,
            searching: false,
            timers: None,
        }
    }

    pub(crate) fn client(&self) -> &Arc<EditorRegistry> {
        &self.client
    }

    /// Whether a job is changing the project at `root`, which refuses
    /// another one until it ends.
    pub(crate) fn project_busy(&self, root: &Path) -> bool {
        self.busy_projects.contains(root)
    }

    /// A project opened (`None`: none, or one whose lock is not read):
    /// what other projects' work finds is not announced while it is open.
    /// Opening another project checks `search.json` for updates.
    pub(crate) fn project_changed(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.project != root {
            self.project = root;
            self.fetch_search(cx);
        }
    }

    /// Fetches `search.json`; when the registry cannot be reached, the
    /// last verified list is shown instead (`Loaded { offline: true }`).
    /// A list already shown stays while it is fetched again.
    pub(crate) fn fetch_search(&mut self, cx: &mut Context<Self>) {
        if self.searching {
            return;
        }
        self.searching = true;
        if !matches!(self.search, Fetch::Loaded { .. }) {
            self.search = Fetch::Loading;
        }
        cx.notify();
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move {
                    fetch(&client, Registry::search, |registry| {
                        registry.cached_search().map(|list| {
                            let at = list.generated;
                            (list, at)
                        })
                    })
                })
                .await;
            this.update(cx, |state, cx| {
                state.searching = false;
                if matches!(fetched, Fetch::Loaded { offline: false, .. }) {
                    // A new check: an update may work now.
                    state.no_newer.clear();
                }
                state.search = fetched;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetches `id`'s index, else shows the cached one offline, as
    /// [`Self::fetch_search`] does.
    #[allow(dead_code)] // The plugin page of a later change calls it.
    pub(crate) fn fetch_index(&mut self, id: &str, cx: &mut Context<Self>) {
        let entry = self.indexes.entry(id.to_owned()).or_insert(Fetch::Idle);
        match entry {
            Fetch::Loading => return,
            Fetch::Loaded { .. } => {}
            _ => *entry = Fetch::Loading,
        }
        cx.notify();
        let client = self.client.clone();
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let fetch_id = id.clone();
            let fetched = cx
                .background_executor()
                .spawn(async move {
                    fetch(
                        &client,
                        |registry, now| registry.index(&fetch_id, now),
                        |registry| {
                            registry.cached_index(&fetch_id).map(|index| {
                                let at = index.generated;
                                (index, at)
                            })
                        },
                    )
                })
                .await;
            this.update(cx, |state, cx| {
                state.indexes.insert(id, fetched);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Installs exactly `version` of `id` for the user and records it in
    /// `registryPlugins`. `false`: work for `id` is already in flight.
    pub(crate) fn install_global(
        &mut self,
        id: &str,
        version: Version,
        cx: &mut Context<Self>,
    ) -> bool {
        let requirement = exactly(&version);
        let plugin = id.to_owned();
        self.start(
            Job {
                work: Work::InstallGlobal { version },
                ids: vec![id.to_owned()],
                project: None,
                failure: format!("Couldn’t install {id}"),
                run: Box::new(move |client, progress| {
                    let registry = client.registry()?;
                    let env = client.environment(&registry, Some(progress));
                    install_global(&env, client.settings_path(), &plugin, &requirement)?;
                    Ok(Done::CHANGED)
                }),
            },
            cx,
        )
    }

    /// Adds `id` to the project at `root` with the requirement
    /// `^version`, locks it, and installs it; `pin` pins a media plugin.
    #[allow(dead_code)] // The plugin page of a later change calls it.
    pub(crate) fn add(
        &mut self,
        root: PathBuf,
        id: &str,
        version: Version,
        pin: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let requirement = offered_requirement(&version).to_string();
        let plugin = id.to_owned();
        let project = root.clone();
        self.start(
            Job {
                work: if pin { Work::Pin } else { Work::Add },
                ids: vec![id.to_owned()],
                project: Some(root),
                failure: if pin {
                    format!("Couldn’t pin {id} to this project")
                } else {
                    format!("Couldn’t add {id} to this project")
                },
                run: Box::new(move |client, progress| {
                    let registry = client.registry()?;
                    let env = client.environment(&registry, Some(progress));
                    add_dependency(&env, &project, &plugin, &requirement, pin)?;
                    Ok(Done::CHANGED)
                }),
            },
            cx,
        )
    }

    /// Applies `rewrites` to the project at `root` and locks and installs
    /// `ids` again; an id whose locked version stays is one with no newer
    /// version.
    pub(crate) fn update(
        &mut self,
        root: PathBuf,
        ids: BTreeSet<String>,
        rewrites: Vec<Rewrite>,
        cx: &mut Context<Self>,
    ) -> bool {
        let failure = match ids.iter().collect::<Vec<_>>().as_slice() {
            [id] => format!("Couldn’t update {id}"),
            _ => "Couldn’t update this project’s plugins".to_owned(),
        };
        let project = root.clone();
        let updated = ids.clone();
        self.start(
            Job {
                work: Work::Update,
                ids: ids.into_iter().collect(),
                project: Some(root),
                failure,
                run: Box::new(move |client, progress| {
                    let before = Lockfile::load(&project)?;
                    let registry = client.registry()?;
                    let env = client.environment(&registry, Some(progress));
                    let outcome = update_dependencies(&env, &project, &updated, &rewrites)?;
                    let version = |lock: &Lockfile, id: &str| {
                        lock.plugins.get(id).map(|locked| locked.version.clone())
                    };
                    let no_newer = updated
                        .iter()
                        .filter(|id| {
                            before.as_ref().and_then(|lock| version(lock, id))
                                == version(&outcome.lock, id)
                        })
                        .cloned()
                        .collect();
                    Ok(Done {
                        changed: true,
                        no_newer,
                    })
                }),
            },
            cx,
        )
    }

    /// Installs the version of the user's `id` the page offers (under
    /// [`Self::include_prereleases`]) when it is newer than the installed
    /// one; otherwise `id` has no newer version.
    pub(crate) fn update_global(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let include_prereleases = self.include_prereleases;
        let plugin = id.to_owned();
        self.start(
            Job {
                work: Work::Update,
                ids: vec![id.to_owned()],
                project: None,
                failure: format!("Couldn’t update {id}"),
                run: Box::new(move |client, progress| {
                    let settings = UserSettings::load(client.settings_path())?;
                    let installed = settings
                        .registry_plugins
                        .iter()
                        .find(|installed| installed.id == plugin)
                        .map(|installed| installed.version.clone());
                    let registry = client.registry()?;
                    let env = client.environment(&registry, Some(progress));
                    let index = registry.index(&plugin, env.now)?;
                    let advisories = registry.cached_advisories()?;
                    let offered = offered_version(&OfferInput {
                        index: &index,
                        advisories: advisories.as_ref(),
                        celesta: client.celesta(),
                        host: client.host(),
                        targets: &[],
                        include_prereleases,
                        scope: OfferScope::Global,
                    });
                    match offered {
                        Ok(offered) if installed.is_none_or(|installed| offered > installed) => {
                            install_global(
                                &env,
                                client.settings_path(),
                                &plugin,
                                &exactly(&offered),
                            )?;
                            Ok(Done::CHANGED)
                        }
                        _ => Ok(Done {
                            changed: false,
                            no_newer: vec![plugin],
                        }),
                    }
                }),
            },
            cx,
        )
    }

    /// Removes `id` from the project at `root` and from its lock.
    pub(crate) fn remove(&mut self, root: PathBuf, id: &str, cx: &mut Context<Self>) -> bool {
        let plugin = id.to_owned();
        let project = root.clone();
        self.start(
            Job {
                work: Work::Remove,
                ids: vec![id.to_owned()],
                project: Some(root),
                failure: format!("Couldn’t remove {id} from this project"),
                run: Box::new(move |_, _| {
                    remove_dependency(&project, &plugin)?;
                    Ok(Done::CHANGED)
                }),
            },
            cx,
        )
    }

    /// Drops the user's `registryPlugins` entry for `id`; the store keeps
    /// its files until `celesta plugin gc`.
    pub(crate) fn uninstall(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let plugin = id.to_owned();
        self.start(
            Job {
                work: Work::Uninstall,
                ids: vec![id.to_owned()],
                project: None,
                failure: format!("Couldn’t uninstall {id}"),
                run: Box::new(move |client, _| {
                    let path = client.settings_path();
                    let mut settings = UserSettings::load(path)?;
                    let before = settings.registry_plugins.len();
                    settings
                        .registry_plugins
                        .retain(|installed| installed.id != plugin);
                    let changed = settings.registry_plugins.len() != before;
                    if changed {
                        settings.save(path)?;
                    }
                    Ok(Done {
                        changed,
                        no_newer: Vec::new(),
                    })
                }),
            },
            cx,
        )
    }

    /// Downloads what the lock of the project at `root` names and the store
    /// does not hold. `false`: a job for that project is already running.
    pub(crate) fn download_locked(&mut self, root: PathBuf, cx: &mut Context<Self>) -> bool {
        let project = root.clone();
        self.start(
            Job {
                work: Work::Download,
                ids: Vec::new(),
                project: Some(root),
                failure: "Couldn’t download this project’s plugins".to_owned(),
                run: Box::new(move |client, progress| {
                    let registry = client.registry()?;
                    let env = client.environment(&registry, Some(progress));
                    install_locked(&env, &project, false)?;
                    Ok(Done::CHANGED)
                }),
            },
            cx,
        )
    }

    /// Fetches `advisories.json` again, and announces a change when the
    /// verified list stops or frees something it did not before. A failed
    /// fetch keeps the cached list, quietly: it is tried again later.
    pub(crate) fn refresh_advisories(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let changed = cx
                .background_executor()
                .spawn(async move {
                    let Ok(registry) = client.registry() else {
                        return false;
                    };
                    // No cached list stops nothing, as an empty one does.
                    let before = registry
                        .cached_advisories()
                        .ok()
                        .flatten()
                        .map(|list| list.advisories)
                        .unwrap_or_default();
                    registry
                        .refresh_advisories(OffsetDateTime::now_utc())
                        .is_ok_and(|list| list.advisories != before)
                })
                .await;
            if changed {
                this.update(cx, |_, cx| {
                    cx.emit(RegistryEvent::PluginsChanged { project: None });
                })
                .ok();
            }
        })
        .detach();
    }

    /// Refreshes the advisories and the search list now and every 24 hours,
    /// for as long as this entity lives.
    pub(crate) fn start_timers(&mut self, cx: &mut Context<Self>) {
        self.timers = Some(cx.spawn(async move |this, cx| {
            loop {
                let refreshed = this.update(cx, |state, cx| {
                    state.refresh_advisories(cx);
                    state.fetch_search(cx);
                });
                if refreshed.is_err() {
                    break;
                }
                cx.background_executor().timer(CHECK_INTERVAL).await;
            }
        }));
    }

    /// Marks the job's plugins in flight and runs it on the background
    /// executor; `false` when one of them is in flight already, or another
    /// job is changing the same project (or, for the user's own plugins,
    /// the settings).
    fn start(&mut self, job: Job, cx: &mut Context<Self>) -> bool {
        if job.ids.iter().any(|id| self.in_flight.contains_key(id)) {
            return false;
        }
        match &job.project {
            Some(root) => {
                if !self.busy_projects.insert(root.clone()) {
                    return false;
                }
            }
            None => {
                if self.settings_busy {
                    return false;
                }
                self.settings_busy = true;
            }
        }
        let Job {
            work,
            ids,
            project,
            failure,
            run,
        } = job;
        for id in &ids {
            self.failures.remove(id);
            self.in_flight.insert(
                id.clone(),
                InFlight {
                    work: work.clone(),
                    done: 0,
                    total: 0,
                },
            );
        }
        cx.notify();

        let client = self.client.clone();
        let sampled = Arc::new(Mutex::new(Sampled::new()));
        cx.spawn(async move |this, cx| {
            let task = cx.background_executor().spawn({
                let sampled = sampled.clone();
                async move {
                    let report = |progress: Progress| {
                        sampled
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .insert(progress.id.to_owned(), (progress.done, progress.total));
                    };
                    run(&client, &report)
                }
            });
            // The ids a download reported that were not marked in flight.
            let adopted = Rc::new(RefCell::new(Vec::new()));
            // Shows the download's progress while it runs; dropped, so
            // stopped, once it has finished.
            let _progress = cx.spawn({
                let this = this.clone();
                let work = work.clone();
                let adopted = adopted.clone();
                async move |cx| {
                    loop {
                        cx.background_executor().timer(PROGRESS_INTERVAL).await;
                        let progress = std::mem::take(
                            &mut *sampled.lock().unwrap_or_else(PoisonError::into_inner),
                        );
                        if progress.is_empty() {
                            continue;
                        }
                        let shown = this.update(cx, |state, cx| {
                            state.show_progress(progress, &work, &mut adopted.borrow_mut());
                            cx.notify();
                        });
                        if shown.is_err() {
                            break;
                        }
                    }
                }
            });
            let result = task.await;
            this.update(cx, |state, cx| {
                for id in ids.iter().chain(adopted.borrow().iter()) {
                    state.in_flight.remove(id);
                }
                match &project {
                    Some(root) => {
                        state.busy_projects.remove(root);
                    }
                    None => state.settings_busy = false,
                }
                // The files on disk are consistent either way; another
                // project open now does not hear about this one's.
                if project.is_none() || project == state.project {
                    state.finish(ids, project, failure, result, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        true
    }

    fn show_progress(&mut self, progress: Sampled, work: &Work, adopted: &mut Vec<String>) {
        for (id, (done, total)) in progress {
            let in_flight = self.in_flight.entry(id.clone()).or_insert_with(|| {
                adopted.push(id);
                InFlight {
                    work: work.clone(),
                    done: 0,
                    total: 0,
                }
            });
            in_flight.done = done;
            in_flight.total = total;
        }
    }

    /// Announces what a job did.
    fn finish(
        &mut self,
        ids: Vec<String>,
        project: Option<PathBuf>,
        failure: String,
        result: anyhow::Result<Done>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(done) => {
                self.no_newer.extend(done.no_newer);
                if done.changed {
                    cx.emit(RegistryEvent::PluginsChanged { project });
                }
            }
            Err(error) => {
                let title = if unreachable(&error) {
                    match ids.as_slice() {
                        [id] => format!("Couldn’t download {id}. Check your connection and try again."),
                        _ => "Couldn’t download this project’s plugins. Check your connection and try again.".to_owned(),
                    }
                } else {
                    failure
                };
                for id in &ids {
                    self.failures.insert(id.clone(), title.clone());
                }
                cx.emit(RegistryEvent::Failed {
                    id: match ids.as_slice() {
                        [id] => Some(id.clone()),
                        _ => None,
                    },
                    title,
                    detail: error_chain(error.as_ref()),
                });
            }
        }
    }
}

/// The verified file `fresh` fetches, else, when the registry cannot be
/// reached, the copy `cached` reads with when it was generated. A file the
/// registry does not serve (404) is a failure, not offline.
fn fetch<T>(
    client: &EditorRegistry,
    fresh: impl FnOnce(&Registry, OffsetDateTime) -> Result<T, ClientError>,
    cached: impl FnOnce(&Registry) -> Option<(T, OffsetDateTime)>,
) -> Fetch<T> {
    let registry = match client.registry() {
        Ok(registry) => registry,
        Err(error) => return Fetch::Failed(error_chain(&error)),
    };
    let now = OffsetDateTime::now_utc();
    match fresh(&registry, now) {
        Ok(value) => Fetch::Loaded {
            value,
            at: now,
            offline: false,
        },
        Err(error @ ClientError::Transport(TransportError::Failed { .. })) => {
            match cached(&registry) {
                Some((value, at)) => Fetch::Loaded {
                    value,
                    at,
                    offline: true,
                },
                None => Fetch::Failed(error_chain(&error)),
            }
        }
        Err(error) => Fetch::Failed(error_chain(&error)),
    }
}

/// Whether `error` comes from the registry or an archive not being
/// reachable, which the user may fix by trying again.
fn unreachable(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<TransportError>(),
            Some(TransportError::Failed { .. })
        )
    })
}

/// The requirement that accepts `version` alone: `=version`.
fn exactly(version: &Version) -> VersionReq {
    VersionReq {
        comparators: vec![Comparator {
            op: Op::Exact,
            major: version.major,
            minor: Some(version.minor),
            patch: Some(version.patch),
            pre: version.pre.clone(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::scratch_dir;
    use celesta_plugin_host::{LOCK_FILE, PROJECT_CONFIG};
    use celesta_registry_client::testing::TestRegistry;
    use gpui_kit::prelude::*;
    use gpui_kit::{Entity, TestAppContext};
    use serde_json::json;
    use std::path::Path;

    const BAD_HASH: &str =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    /// A test registry serving `acme.codec` and `acme.charts` 1.0.0, a
    /// user folder of its own, and the entity's events as they come.
    struct Fixture {
        served: TestRegistry,
        user: PathBuf,
        state: Entity<RegistryState>,
        events: Rc<RefCell<Vec<RegistryEvent>>>,
    }

    impl Fixture {
        fn new(cx: &mut TestAppContext) -> Self {
            let served = TestRegistry::file();
            served.publish("acme.codec", "1.0.0", &["any"]);
            served.publish("acme.charts", "1.0.0", &["any"]);
            let user = scratch_dir("registry-state");
            let (state, events) = listened(cx, &served, &user);
            Self {
                served,
                user,
                state,
                events,
            }
        }

        fn project(&self, name: &str) -> PathBuf {
            let root = self.user.join(name);
            std::fs::create_dir_all(&root).unwrap();
            root
        }

        /// Tells the state that the project at `root` is open, as an open
        /// does.
        fn open(&self, cx: &mut TestAppContext, root: Option<&Path>) {
            self.state.update(cx, |state, cx| {
                state.project_changed(root.map(Path::to_path_buf), cx);
            });
        }

        fn events(&self) -> Vec<RegistryEvent> {
            self.events.borrow().clone()
        }

        fn read<R>(&self, cx: &mut TestAppContext, read: impl FnOnce(&RegistryState) -> R) -> R {
            self.state.read_with(cx, |state, _| read(state))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.user);
        }
    }

    /// A state for `served` with the user folder `user`, and the events it
    /// emits.
    fn listened(
        cx: &mut TestAppContext,
        served: &TestRegistry,
        user: &Path,
    ) -> (Entity<RegistryState>, Rc<RefCell<Vec<RegistryEvent>>>) {
        let client = Arc::new(EditorRegistry::for_test(served, user));
        let state = cx.new(|_| RegistryState::new(client));
        let events = Rc::new(RefCell::new(Vec::new()));
        cx.update(|cx| {
            let events = events.clone();
            cx.subscribe(&state, move |_, event: &RegistryEvent, _| {
                events.borrow_mut().push(event.clone());
            })
            .detach();
        });
        (state, events)
    }

    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn search_entry(id: &str, latest: &str) -> serde_json::Value {
        json!({
            "id": id,
            "publisher": "acme",
            "kind": { "roles": ["content"], "program": false, "module": true },
            "description": "A test plugin.",
            "latest": latest,
        })
    }

    #[gpui_kit::test]
    fn install_global_records_the_version_and_emits(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let started = fixture.state.update(cx, |state, cx| {
            state.install_global("acme.codec", version("1.0.0"), cx)
        });
        assert!(started);
        assert_eq!(
            fixture.read(cx, |state| state.in_flight.get("acme.codec").cloned()),
            Some(InFlight {
                work: Work::InstallGlobal {
                    version: version("1.0.0")
                },
                done: 0,
                total: 0,
            })
        );

        cx.run_until_parked();
        let settings = UserSettings::load(&fixture.user.join("settings.json")).unwrap();
        let installed: Vec<_> = settings
            .registry_plugins
            .iter()
            .map(|plugin| (plugin.id.as_str(), plugin.version.to_string()))
            .collect();
        assert_eq!(installed, [("acme.codec", "1.0.0".to_owned())]);
        assert!(
            fixture
                .user
                .join("store/acme.codec/1.0.0/celesta-plugin.json")
                .is_file()
        );
        assert_eq!(
            fixture.events(),
            [RegistryEvent::PluginsChanged { project: None }]
        );
        assert!(fixture.read(cx, |state| state.in_flight.is_empty()));
    }

    #[gpui_kit::test]
    fn work_on_an_id_in_flight_is_refused(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let (first, second) = fixture.state.update(cx, |state, cx| {
            (
                state.install_global("acme.codec", version("1.0.0"), cx),
                state.install_global("acme.codec", version("1.0.0"), cx),
            )
        });
        assert!(first);
        assert!(!second, "acme.codec is in flight");
        cx.run_until_parked();
        assert_eq!(fixture.events().len(), 1, "{:?}", fixture.events());

        // Once it finished, it may be asked for again.
        let again = fixture.state.update(cx, |state, cx| {
            state.install_global("acme.codec", version("1.0.0"), cx)
        });
        assert!(again);
        cx.run_until_parked();
        assert_eq!(fixture.events().len(), 2);
    }

    #[gpui_kit::test]
    fn a_result_for_a_previous_project_is_dropped(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let root_a = fixture.project("a");
        let root_b = fixture.project("b");
        fixture.open(cx, Some(&root_a));
        let started = fixture.state.update(cx, |state, cx| {
            state.add(root_a.clone(), "acme.charts", version("1.0.0"), false, cx)
        });
        assert!(started);
        fixture.open(cx, Some(&root_b));

        cx.run_until_parked();
        assert_eq!(fixture.events(), []);
        assert!(fixture.read(cx, |state| state.in_flight.is_empty()));
        // The work itself was done.
        assert!(root_a.join(LOCK_FILE).is_file());
        let config = std::fs::read_to_string(root_a.join(PROJECT_CONFIG)).unwrap();
        assert!(config.contains(r#""acme.charts": "^1.0.0""#), "{config}");
    }

    #[gpui_kit::test]
    fn a_result_for_the_open_project_is_announced(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let root = fixture.project("open");
        fixture.open(cx, Some(&root));
        fixture.state.update(cx, |state, cx| {
            state.add(root.clone(), "acme.charts", version("1.0.0"), false, cx)
        });
        // Opened again (Reload Plugins), it is still the same project.
        fixture.open(cx, Some(&root));

        cx.run_until_parked();
        assert_eq!(
            fixture.events(),
            [RegistryEvent::PluginsChanged {
                project: Some(root)
            }]
        );
    }

    #[gpui_kit::test]
    fn a_result_for_a_project_opened_again_is_announced(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let root_a = fixture.project("a");
        let root_b = fixture.project("b");
        fixture.open(cx, Some(&root_a));
        fixture.state.update(cx, |state, cx| {
            state.add(root_a.clone(), "acme.charts", version("1.0.0"), false, cx)
        });
        fixture.open(cx, Some(&root_b));
        fixture.open(cx, Some(&root_a));

        cx.run_until_parked();
        assert_eq!(
            fixture.events(),
            [RegistryEvent::PluginsChanged {
                project: Some(root_a)
            }]
        );
    }

    #[gpui_kit::test]
    fn a_second_job_on_the_same_project_is_refused(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let root = fixture.project("busy");
        let other = fixture.project("other");
        fixture.open(cx, Some(&root));
        let started = fixture.state.update(cx, |state, cx| {
            [
                state.add(root.clone(), "acme.charts", version("1.0.0"), false, cx),
                // Other plugins, the same project's files.
                state.add(root.clone(), "acme.codec", version("1.0.0"), false, cx),
                state.remove(root.clone(), "acme.codec", cx),
                state.download_locked(root.clone(), cx),
                // Another project's files are its own.
                state.add(other.clone(), "acme.codec", version("1.0.0"), false, cx),
            ]
        });
        assert_eq!(started, [true, false, false, false, true]);
        cx.run_until_parked();

        // Once the first is done, the project takes the next.
        let next = fixture.state.update(cx, |state, cx| {
            state.add(root.clone(), "acme.codec", version("1.0.0"), false, cx)
        });
        assert!(next);
        cx.run_until_parked();
        let config = std::fs::read_to_string(root.join(PROJECT_CONFIG)).unwrap();
        assert!(
            config.contains("acme.charts") && config.contains("acme.codec"),
            "{config}"
        );
        let lock = Lockfile::load(&root).unwrap().unwrap();
        assert_eq!(
            lock.plugins.keys().collect::<Vec<_>>(),
            ["acme.charts", "acme.codec"]
        );
    }

    #[gpui_kit::test]
    fn a_second_global_install_is_refused(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let started = fixture.state.update(cx, |state, cx| {
            [
                state.install_global("acme.codec", version("1.0.0"), cx),
                // Another plugin, the same settings.
                state.install_global("acme.charts", version("1.0.0"), cx),
                state.uninstall("acme.charts", cx),
                state.update_global("acme.charts", cx),
            ]
        });
        assert_eq!(started, [true, false, false, false]);
        cx.run_until_parked();

        let next = fixture.state.update(cx, |state, cx| {
            state.install_global("acme.charts", version("1.0.0"), cx)
        });
        assert!(next);
        cx.run_until_parked();
        let settings = UserSettings::load(&fixture.user.join("settings.json")).unwrap();
        let mut ids: Vec<_> = settings
            .registry_plugins
            .iter()
            .map(|plugin| plugin.id.as_str())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, ["acme.charts", "acme.codec"]);
    }

    #[gpui_kit::test]
    fn opening_another_project_checks_the_search_list(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        fixture
            .served
            .search(json!([search_entry("acme.charts", "1.0.0")]));
        let root = fixture.project("checked");
        fixture.open(cx, Some(&root));
        cx.run_until_parked();
        assert!(matches!(
            fixture.read(cx, |state| state.search.clone()),
            Fetch::Loaded { offline: false, .. }
        ));
    }

    #[cfg(unix)]
    #[gpui_kit::test]
    fn offline_search_uses_the_cached_list(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        fixture
            .served
            .search(json!([search_entry("acme.charts", "1.0.0")]));
        fixture.state.update(cx, |state, cx| state.fetch_search(cx));
        assert_eq!(
            fixture.read(cx, |state| state.search.clone()),
            Fetch::Loading
        );
        cx.run_until_parked();
        let online = match fixture.read(cx, |state| state.search.clone()) {
            Fetch::Loaded {
                value,
                offline: false,
                ..
            } => value,
            other => panic!("{other:?}"),
        };
        assert_eq!(online.plugins[0].id, "acme.charts");

        // The served folder can no longer be read: the registry is out of
        // reach.
        let served = fixture.served.served_dir();
        set_mode(served, 0o000);
        fixture.state.update(cx, |state, cx| state.fetch_search(cx));
        cx.run_until_parked();
        let offline = fixture.read(cx, |state| state.search.clone());

        // Without a cached list there is nothing to show.
        let other = scratch_dir("registry-state-uncached");
        let (uncached, _) = listened(cx, &fixture.served, &other);
        uncached.update(cx, |state, cx| state.fetch_search(cx));
        cx.run_until_parked();
        let failed = uncached.read_with(cx, |state, _| state.search.clone());
        set_mode(served, 0o700);
        let _ = std::fs::remove_dir_all(&other);

        assert!(
            matches!(&offline, Fetch::Loaded { value, offline: true, at } if *value == online && *at == online.generated),
            "{offline:?}"
        );
        assert!(matches!(failed, Fetch::Failed(_)), "{failed:?}");
    }

    #[gpui_kit::test]
    fn a_list_the_registry_does_not_serve_is_not_offline(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        fixture
            .served
            .search(json!([search_entry("acme.charts", "1.0.0")]));
        fixture.state.update(cx, |state, cx| state.fetch_search(cx));
        cx.run_until_parked();
        assert!(matches!(
            fixture.read(cx, |state| state.search.clone()),
            Fetch::Loaded { offline: false, .. }
        ));

        // The registry answers, but has no such file (404): the cached list
        // is not shown as if offline.
        std::fs::remove_file(fixture.served.served_dir().join("search.json")).unwrap();
        fixture.state.update(cx, |state, cx| state.fetch_search(cx));
        cx.run_until_parked();
        let fetched = fixture.read(cx, |state| state.search.clone());
        assert!(matches!(fetched, Fetch::Failed(_)), "{fetched:?}");
    }

    #[gpui_kit::test]
    fn a_failed_add_reports_and_clears_in_flight(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        fixture.served.edit("acme.charts", "1.0.0", |entry| {
            entry["targets"]["any"]["sha256"] = json!(BAD_HASH);
        });
        let root = fixture.project("bad");
        fixture.open(cx, Some(&root));
        fixture.state.update(cx, |state, cx| {
            state.add(root.clone(), "acme.charts", version("1.0.0"), false, cx)
        });

        cx.run_until_parked();
        let events = fixture.events();
        let [
            RegistryEvent::Failed {
                id: Some(id),
                title,
                detail,
            },
        ] = events.as_slice()
        else {
            panic!("{events:?}");
        };
        assert_eq!(id, "acme.charts");
        assert_eq!(title, "Couldn’t add acme.charts to this project");
        assert!(detail.contains("SHA-256"), "{detail}");
        assert!(fixture.read(cx, |state| state.in_flight.is_empty()));
        assert_eq!(
            fixture.read(cx, |state| state.failures.get("acme.charts").cloned()),
            Some(title.clone())
        );
        // The project is as it was: it had no settings or lock.
        assert!(!root.join(PROJECT_CONFIG).exists());
        assert!(!root.join(LOCK_FILE).exists());

        // The next attempt clears the red line.
        fixture.state.update(cx, |state, cx| {
            state.add(root.clone(), "acme.charts", version("1.0.0"), false, cx)
        });
        assert!(fixture.read(cx, |state| !state.failures.contains_key("acme.charts")));
        cx.run_until_parked();
    }

    #[cfg(unix)]
    #[gpui_kit::test]
    fn an_unreachable_registry_asks_to_try_again(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let served = fixture.served.served_dir();
        set_mode(served, 0o000);
        fixture.state.update(cx, |state, cx| {
            state.install_global("acme.codec", version("1.0.0"), cx)
        });
        cx.run_until_parked();
        set_mode(served, 0o700);

        let events = fixture.events();
        assert!(
            matches!(
                events.as_slice(),
                [RegistryEvent::Failed { title, .. }]
                    if title == "Couldn’t download acme.codec. Check your connection and try again."
            ),
            "{events:?}"
        );
    }

    #[gpui_kit::test]
    fn a_changed_advisory_list_rediscovers(cx: &mut TestAppContext) {
        let fixture = Fixture::new(cx);
        let advisory = json!({
            "id": "CEL-2026-0001", "plugin": "acme.charts", "versions": "<1.3.0",
            "title": "Reads files outside the project", "description": "Details.",
            "fixed": "1.3.0", "published": "2026-11-01T00:00:00Z",
        });
        // The first list, empty, stops nothing more than no list did.
        fixture.served.advisories(json!([]));
        fixture
            .state
            .update(cx, |state, cx| state.refresh_advisories(cx));
        cx.run_until_parked();
        assert_eq!(fixture.events(), []);

        fixture.served.advisories(json!([advisory]));
        fixture
            .state
            .update(cx, |state, cx| state.refresh_advisories(cx));
        cx.run_until_parked();
        assert_eq!(
            fixture.events(),
            [RegistryEvent::PluginsChanged { project: None }]
        );

        // Signed again with the same advisories: nothing to rediscover.
        fixture.served.advisories(json!([advisory]));
        fixture
            .state
            .update(cx, |state, cx| state.refresh_advisories(cx));
        cx.run_until_parked();
        assert_eq!(fixture.events().len(), 1);
    }
}
