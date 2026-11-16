//! What the right dock's Plugins tab lists, from the last discovery and the
//! window's registry work: the plugins this project locks, the ones
//! installed for every project (from the registry, and the ones that come
//! with Celesta), the ones placed by hand, and the added folders. Each copy
//! of a plugin is a row of its own, with its state, the one action that
//! applies to it, and the commands of its menu. Reads nothing from disk.

use std::collections::BTreeSet;
use std::path::PathBuf;

use celesta_plugin_host::{
    CELESTA_VERSION, LockProblem, LockedPlugin, PluginEntry, PluginRole, PluginSource,
};
use celesta_registry_format::{Kind, SearchEntry, SearchList, Target};
use semver::Version;

use crate::plugins::{BlockedPlugin, FoundPlugins};
use crate::registry::{Fetch, InFlight, RegistryState, Work};

/// The Plugins tab's sections, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TabModel {
    /// `None` outside a project whose lock is read: the section is left out.
    pub project: Option<ProjectSection>,
    pub global: Vec<TabRow>,
    pub by_hand: Vec<TabRow>,
    pub folders: Vec<PathBuf>,
    pub skipped: Vec<String>,
}

/// The plugins the open project's `celesta.lock` names.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProjectSection {
    /// `celesta.lock · 0.10.0-alpha.3`, or `No celesta.lock`.
    pub lock_label: String,
    pub action: Option<SectionAction>,
    pub rows: Vec<TabRow>,
}

/// The heading's action for the project's plugins as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SectionAction {
    /// Downloads the locked versions with a build here that are missing.
    Download { count: usize },
    /// Locks what `celesta.config.json` declares and the lock misses.
    UpdatePlugins,
    /// A job is changing the project.
    Working { done: u64, total: u64 },
}

/// One copy of a plugin.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TabRow {
    pub id: String,
    pub version: String,
    /// `Media`, `Content`, or `Components`; `Plugin` when nothing says.
    pub kind: &'static str,
    pub state: RowState,
    pub action: Option<RowAction>,
    pub menu: Vec<MenuCommand>,
    /// The copy's folder, when there is one on this computer.
    pub directory: Option<PathBuf>,
    /// It comes from one of the project's own plugin folders, whose
    /// decoders the tab counts.
    pub project_folder: bool,
}

impl TabRow {
    /// The row's second line: its kind and its state.
    pub(crate) fn detail(&self) -> String {
        format!("{} · {}", self.kind, self.state.label())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RowState {
    On,
    Off,
    /// It waits for the user to allow it as it is now.
    Waiting {
        changed: bool,
        from_the_internet: bool,
    },
    /// A security advisory stops it.
    Blocked {
        advisory: String,
    },
    Deprecated {
        replacement: Option<String>,
    },
    Yanked,
    /// A locked or installed version this computer does not hold.
    NotInstalled,
    /// A locked version with no build for this computer.
    NoBuildHere,
    /// The copy that comes with Celesta, and it runs.
    Bundled,
    /// Another copy of its id runs: `runs` names where that one is from.
    NotUsed {
        runs: String,
    },
    /// Being installed, updated, or downloaded.
    Working {
        done: u64,
        total: u64,
    },
    /// Being removed from the project or uninstalled.
    Removing,
    /// An update found nothing newer that runs here.
    NoNewer {
        celesta: String,
    },
}

impl RowState {
    /// The state part of the row's second line.
    pub fn label(&self) -> String {
        match self {
            Self::On => "On".to_owned(),
            Self::Off => "Off".to_owned(),
            Self::Waiting { changed: true, .. } => "Changed: waits for approval".to_owned(),
            Self::Waiting { .. } => "Waits for approval".to_owned(),
            Self::Blocked { advisory } => format!("Blocked · {advisory}"),
            Self::Deprecated {
                replacement: Some(replacement),
            } => format!("Deprecated · use {replacement}"),
            Self::Deprecated { replacement: None } => "Deprecated".to_owned(),
            Self::Yanked => "Yanked".to_owned(),
            Self::NotInstalled => "Not installed".to_owned(),
            Self::NoBuildHere => "No build for this computer".to_owned(),
            Self::Bundled => "Bundled · On".to_owned(),
            Self::NotUsed { runs } => format!("Not used: {runs} runs"),
            Self::Working { .. } => "Installing…".to_owned(),
            Self::Removing => "Removing…".to_owned(),
            Self::NoNewer { celesta } => {
                format!("No newer version works with Celesta {celesta}")
            }
        }
    }
}

/// The row's one visible action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RowAction {
    Update { to: Version },
    Download,
    Allow { fingerprint: String },
}

impl RowAction {
    /// The button's text.
    pub(crate) fn title(&self) -> String {
        match self {
            Self::Update { to } => format!("Update to {to}"),
            Self::Download => "Download".to_owned(),
            Self::Allow { .. } => "Allow".to_owned(),
        }
    }
}

/// A command in a row's ⋯ menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuCommand {
    TurnOff,
    TurnOn,
    ShowFolder,
    #[allow(dead_code)] // The plugin page of a later change adds it to the menus.
    ShowInFindPlugins,
    RemoveFromProject,
    Uninstall,
}

impl MenuCommand {
    /// The menu item's text.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::TurnOff => "Turn Off",
            Self::TurnOn => "Turn On",
            Self::ShowFolder => "Show Folder",
            Self::ShowInFindPlugins => "Show in Find plugins",
            Self::RemoveFromProject => "Remove from this project",
            Self::Uninstall => "Uninstall",
        }
    }
}

/// The tab for what `found` holds and the work `registry` (`None`: the
/// window has none) is doing.
pub(crate) fn tab_model(found: &FoundPlugins, registry: Option<&RegistryState>) -> TabModel {
    let reading = Reading::new(found, registry);
    // The blocked copies a section claims; the rest are by hand.
    let mut claimed = BTreeSet::new();
    let project = found
        .project
        .as_ref()
        .map(|root| reading.project_section(root, &mut claimed));
    let global = reading.global_rows(&mut claimed);
    let by_hand = reading.by_hand_rows(&claimed);
    TabModel {
        project,
        global,
        by_hand,
        folders: found.folders.clone(),
        skipped: found.skipped.clone(),
    }
}

/// What every section reads.
struct Reading<'a> {
    found: &'a FoundPlugins,
    registry: Option<&'a RegistryState>,
    search: Option<&'a SearchList>,
    host: Target,
}

impl<'a> Reading<'a> {
    fn new(found: &'a FoundPlugins, registry: Option<&'a RegistryState>) -> Self {
        let search = registry.and_then(|state| match &state.search {
            Fetch::Loaded { value, .. } => Some(value),
            _ => None,
        });
        Self {
            found,
            registry,
            search,
            host: Target::host(),
        }
    }

    fn project_section(
        &self,
        root: &std::path::Path,
        claimed: &mut BTreeSet<(String, String)>,
    ) -> ProjectSection {
        let found = self.found;
        let lock_label = match (&found.lock, &found.lock_problem) {
            (Some(lock), _) => format!("celesta.lock · {}", lock.celesta),
            (None, Some(LockProblem::Unreadable(_))) => "celesta.lock can’t be read".to_owned(),
            (None, _) => "No celesta.lock".to_owned(),
        };
        let rows: Vec<TabRow> = found
            .lock
            .iter()
            .flat_map(|lock| lock.plugins.iter())
            .map(|(id, locked)| {
                let has_build = found
                    .lock
                    .as_ref()
                    .is_some_and(|lock| lock.artifact(id, &self.host).is_some());
                self.locked_row(id, locked, has_build, claimed)
            })
            .collect();
        let busy = self
            .registry
            .is_some_and(|registry| registry.project_busy(root));
        let (done, total) = rows
            .iter()
            .filter_map(|row| match row.state {
                RowState::Working { done, total } => Some((done, total)),
                _ => None,
            })
            .fold((0, 0), |(done, total), (d, t)| (done + d, total + t));
        let missing = rows
            .iter()
            .filter(|row| row.state == RowState::NotInstalled)
            .count();
        let action = if busy || rows.iter().any(TabRow::working) {
            Some(SectionAction::Working { done, total })
        } else if matches!(
            found.lock_problem,
            Some(LockProblem::NoLock | LockProblem::OutOfDate { .. })
        ) {
            Some(SectionAction::UpdatePlugins)
        } else if missing > 0 {
            Some(SectionAction::Download { count: missing })
        } else {
            None
        };
        ProjectSection {
            lock_label,
            action,
            rows,
        }
    }

    fn locked_row(
        &self,
        id: &str,
        locked: &LockedPlugin,
        has_build: bool,
        claimed: &mut BTreeSet<(String, String)>,
    ) -> TabRow {
        let version = locked.version.to_string();
        let (entry, on) = self.entry(id, PluginSource::Locked);
        let blocked = self.blocked(id, &version, None);
        if blocked.is_some() {
            claimed.insert((id.to_owned(), version.clone()));
        }
        let state = if let Some(working) = self.working(id) {
            working
        } else if let Some(blocked) = blocked {
            RowState::Blocked {
                advisory: blocked.advisory.clone(),
            }
        } else if entry.is_none() {
            if !has_build {
                RowState::NoBuildHere
            } else if let Some(runs) = self.runs_before(id, PluginSource::Locked) {
                RowState::NotUsed { runs }
            } else {
                RowState::NotInstalled
            }
        } else if !on {
            RowState::Off
        } else {
            self.notice(id, &locked.version)
        };
        let action = match state {
            RowState::NotInstalled => Some(RowAction::Download),
            _ => self
                .update_within_requirement(id, &locked.version)
                .filter(|_| state.updatable())
                .map(|to| RowAction::Update { to }),
        };
        let mut menu = toggle(entry.is_some(), on);
        if entry.is_some() {
            menu.push(MenuCommand::ShowFolder);
        }
        menu.push(MenuCommand::RemoveFromProject);
        TabRow {
            id: id.to_owned(),
            version,
            kind: entry.map_or_else(|| self.registry_kind(id), entry_kind),
            state,
            action,
            menu,
            directory: entry.map(|entry| entry.directory.clone()),
            project_folder: false,
        }
    }

    fn global_rows(&self, claimed: &mut BTreeSet<(String, String)>) -> Vec<TabRow> {
        let found = self.found;
        let installed = found
            .registry_plugins
            .iter()
            .map(|installed| (installed.id.as_str(), installed.version.clone()));
        // A copy from the store the settings do not name, as a test set
        // made by hand can hold.
        let unlisted = self.entries().filter_map(|(entry, _)| {
            let listed = found
                .registry_plugins
                .iter()
                .any(|installed| installed.id == entry.manifest.id);
            (entry.source == PluginSource::Registry && !listed)
                .then(|| Version::parse(&entry.manifest.version).ok())
                .flatten()
                .map(|version| (entry.manifest.id.as_str(), version))
        });
        let mut rows: Vec<(u8, TabRow)> = installed
            .chain(unlisted)
            .map(|(id, version)| (0, self.registry_row(id, &version, claimed)))
            .collect();
        rows.extend(self.bundled_rows(claimed).into_iter().map(|row| (1, row)));
        rows.sort_by(|(a_rank, a), (b_rank, b)| (&a.id, a_rank).cmp(&(&b.id, b_rank)));
        rows.into_iter().map(|(_, row)| row).collect()
    }

    fn registry_row(
        &self,
        id: &str,
        version: &Version,
        claimed: &mut BTreeSet<(String, String)>,
    ) -> TabRow {
        let (entry, on) = self.entry(id, PluginSource::Registry);
        let shown = version.to_string();
        let blocked = self.blocked(id, &shown, None);
        if blocked.is_some() {
            claimed.insert((id.to_owned(), shown.clone()));
        }
        let state = if let Some(working) = self.working(id) {
            working
        } else if let Some(blocked) = blocked {
            RowState::Blocked {
                advisory: blocked.advisory.clone(),
            }
        } else if entry.is_none() {
            match self.runs_before(id, PluginSource::Registry) {
                Some(runs) => RowState::NotUsed { runs },
                None => RowState::NotInstalled,
            }
        } else if !on {
            RowState::Off
        } else {
            self.notice(id, version)
        };
        let action = match state {
            RowState::NotInstalled => Some(RowAction::Download),
            _ => self
                .newer_offered(id, version)
                .filter(|_| state.updatable())
                .map(|to| RowAction::Update { to }),
        };
        let mut menu = toggle(entry.is_some(), on);
        if entry.is_some() {
            menu.push(MenuCommand::ShowFolder);
        }
        menu.push(MenuCommand::Uninstall);
        TabRow {
            id: id.to_owned(),
            version: shown,
            kind: entry.map_or_else(|| self.registry_kind(id), entry_kind),
            state,
            action,
            menu,
            directory: entry.map(|entry| entry.directory.clone()),
            project_folder: false,
        }
    }

    /// Each copy in Celesta's own folder: the one that runs, one the user
    /// turned off, one an advisory stops, and one another copy of its id
    /// keeps from running.
    fn bundled_rows(&self, claimed: &mut BTreeSet<(String, String)>) -> Vec<TabRow> {
        let found = self.found;
        let row = |entry: &PluginEntry, state: RowState| {
            let menu = match state {
                RowState::Bundled => vec![MenuCommand::TurnOff, MenuCommand::ShowFolder],
                RowState::Off => vec![MenuCommand::TurnOn, MenuCommand::ShowFolder],
                _ => vec![MenuCommand::ShowFolder],
            };
            TabRow {
                id: entry.manifest.id.clone(),
                version: entry.manifest.version.clone(),
                kind: entry_kind(entry),
                state,
                action: None,
                menu,
                directory: Some(entry.directory.clone()),
                project_folder: false,
            }
        };
        let mut rows: Vec<TabRow> = self
            .entries()
            .filter(|(entry, _)| entry.source == PluginSource::Bundled)
            .map(|(entry, on)| row(entry, if on { RowState::Bundled } else { RowState::Off }))
            .collect();
        for copy in &found.bundled {
            let listed = self.entries().any(|(entry, _)| {
                entry.source == PluginSource::Bundled && entry.directory == copy.directory
            });
            if listed {
                continue;
            }
            let id = copy.manifest.id.as_str();
            let state = if let Some(blocked) = self.blocked(id, &copy.manifest.version, Some(true))
            {
                claimed.insert((id.to_owned(), blocked.version.clone()));
                RowState::Blocked {
                    advisory: blocked.advisory.clone(),
                }
            } else if let Some(taker) = self.taker(id) {
                if taker.directory == copy.directory {
                    // A lock's version that this copy satisfies.
                    RowState::Bundled
                } else {
                    RowState::NotUsed {
                        runs: source_name(taker.source).to_owned(),
                    }
                }
            } else {
                continue;
            };
            rows.push(row(copy, state));
        }
        // Stopped bundled copies the folder's own listing did not give.
        for blocked in found.blocked.iter().filter(|blocked| blocked.bundled) {
            let key = (blocked.id.clone(), blocked.version.clone());
            if claimed.insert(key) {
                rows.push(blocked_row(blocked, self.registry_kind(&blocked.id)));
            }
        }
        rows
    }

    /// The plugins from the user's folder, the folders they added, the
    /// project's folders, and the plugin path, by where they came from (in
    /// search order), then by id; and the stopped ones no other section
    /// lists.
    fn by_hand_rows(&self, claimed: &BTreeSet<(String, String)>) -> Vec<TabRow> {
        let found = self.found;
        let by_hand = |source: PluginSource| {
            matches!(
                source,
                PluginSource::Explicit
                    | PluginSource::Project
                    | PluginSource::User
                    | PluginSource::Folder
            )
        };
        let row = |entry: &PluginEntry, state: RowState, action: Option<RowAction>| {
            let menu = match state {
                RowState::Off => vec![MenuCommand::TurnOn, MenuCommand::ShowFolder],
                _ => vec![MenuCommand::TurnOff, MenuCommand::ShowFolder],
            };
            (
                entry.source,
                TabRow {
                    id: entry.manifest.id.clone(),
                    version: entry.manifest.version.clone(),
                    kind: entry_kind(entry),
                    state,
                    action,
                    menu,
                    directory: Some(entry.directory.clone()),
                    project_folder: entry.source == PluginSource::Project,
                },
            )
        };
        let on = found
            .installed
            .plugins
            .iter()
            .filter(|entry| by_hand(entry.source))
            .map(|entry| {
                // A plugin process with no build for this computer.
                let unbuilt = !entry.manifest.executable.is_empty() && entry.executable().is_none();
                let state = if unbuilt {
                    RowState::NoBuildHere
                } else {
                    RowState::On
                };
                row(entry, state, None)
            });
        let waiting = found
            .pending
            .iter()
            .filter(|pending| by_hand(pending.entry.source))
            .map(|pending| {
                row(
                    &pending.entry,
                    RowState::Waiting {
                        changed: pending.changed,
                        from_the_internet: pending.from_the_internet,
                    },
                    Some(RowAction::Allow {
                        fingerprint: pending.fingerprint.clone(),
                    }),
                )
            });
        let off = found
            .disabled
            .iter()
            .filter(|entry| by_hand(entry.source))
            .map(|entry| row(entry, RowState::Off, None));
        let mut rows: Vec<(PluginSource, TabRow)> = on.chain(waiting).chain(off).collect();
        rows.sort_by(|(a_source, a), (b_source, b)| (a_source, &a.id).cmp(&(b_source, &b.id)));
        let stopped = found
            .blocked
            .iter()
            .filter(|blocked| !claimed.contains(&(blocked.id.clone(), blocked.version.clone())))
            .map(|blocked| blocked_row(blocked, self.registry_kind(&blocked.id)));
        rows.into_iter()
            .map(|(_, row)| row)
            .chain(stopped)
            .collect()
    }

    /// Every listed copy, and whether it may run (it is not turned off).
    fn entries(&self) -> impl Iterator<Item = (&'a PluginEntry, bool)> + 'a {
        let found = self.found;
        let on = found.installed.plugins.iter().map(|entry| (entry, true));
        let off = found.disabled.iter().map(|entry| (entry, false));
        on.chain(off)
    }

    /// The copy of `id` from `source`, and whether it may run.
    fn entry(&self, id: &str, source: PluginSource) -> (Option<&'a PluginEntry>, bool) {
        self.entries()
            .find(|(entry, _)| entry.source == source && entry.manifest.id == id)
            .map_or((None, false), |(entry, on)| (Some(entry), on))
    }

    /// The copy of `id` that runs, or waits in its place.
    fn taker(&self, id: &str) -> Option<&'a PluginEntry> {
        let found = self.found;
        found
            .installed
            .plugins
            .iter()
            .chain(found.pending.iter().map(|pending| &pending.entry))
            .find(|entry| entry.manifest.id == id)
    }

    /// Where the copy of `id` that runs in place of one from `source`
    /// comes from, when that copy is found before `source`.
    fn runs_before(&self, id: &str, source: PluginSource) -> Option<String> {
        self.taker(id)
            .filter(|taker| taker.source < source)
            .map(|taker| source_name(taker.source).to_owned())
    }

    /// What stops version `version` of `id`; with `bundled`, only a copy
    /// that is (or is not) Celesta's own.
    fn blocked(&self, id: &str, version: &str, bundled: Option<bool>) -> Option<&'a BlockedPlugin> {
        self.found.blocked.iter().find(|blocked| {
            blocked.id == id
                && blocked.version == version
                && bundled.is_none_or(|bundled| blocked.bundled == bundled)
        })
    }

    /// The row state for work in flight on `id`.
    fn working(&self, id: &str) -> Option<RowState> {
        let InFlight { work, done, total } = self.registry?.in_flight.get(id)?;
        Some(match work {
            Work::Remove | Work::Uninstall => RowState::Removing,
            _ => RowState::Working {
                done: *done,
                total: *total,
            },
        })
    }

    /// What the registry says of a copy that runs: an update found nothing
    /// newer, the version was yanked, or the plugin is deprecated.
    fn notice(&self, id: &str, version: &Version) -> RowState {
        let registry = self.registry;
        if registry.is_some_and(|registry| registry.no_newer.contains(id)) {
            return RowState::NoNewer {
                celesta: CELESTA_VERSION.to_owned(),
            };
        }
        let yanked = registry
            .and_then(|registry| match registry.indexes.get(id) {
                Some(Fetch::Loaded { value, .. }) => Some(value),
                _ => None,
            })
            .and_then(|index| {
                index
                    .versions
                    .iter()
                    .find(|entry| entry.version == *version)
            })
            .is_some_and(|entry| entry.yanked);
        if yanked {
            return RowState::Yanked;
        }
        match self
            .search_entry(id)
            .and_then(|entry| entry.deprecated.as_ref())
        {
            Some(deprecation) => RowState::Deprecated {
                replacement: deprecation.replacement.clone(),
            },
            None => RowState::On,
        }
    }

    fn search_entry(&self, id: &str) -> Option<&'a SearchEntry> {
        self.search?.plugins.iter().find(|entry| entry.id == id)
    }

    /// `search.json`'s latest of `id` when it is newer than `version` and
    /// the project's requirement accepts it.
    fn update_within_requirement(&self, id: &str, version: &Version) -> Option<Version> {
        let requirement = self.found.dependencies.get(id)?;
        self.latest_newer(id, version)
            .filter(|latest| requirement.matches(latest))
    }

    /// `search.json`'s latest of `id` when it is newer than `version`, and
    /// a pre-release only when pre-releases are offered.
    fn newer_offered(&self, id: &str, version: &Version) -> Option<Version> {
        let prereleases = self.registry?.include_prereleases;
        self.latest_newer(id, version)
            .filter(|latest| prereleases || latest.pre.is_empty())
    }

    fn latest_newer(&self, id: &str, version: &Version) -> Option<Version> {
        if self.registry?.no_newer.contains(id) {
            return None;
        }
        self.search_entry(id)?
            .latest
            .clone()
            .filter(|latest| latest > version)
    }

    /// The kind the registry gives `id`, for a copy not on this computer.
    fn registry_kind(&self, id: &str) -> &'static str {
        let indexed = self
            .registry
            .and_then(|registry| match registry.indexes.get(id) {
                Some(Fetch::Loaded { value, .. }) => Some(&value.kind),
                _ => None,
            });
        self.search_entry(id)
            .map(|entry| &entry.kind)
            .or(indexed)
            .map_or("Plugin", registry_kind_name)
    }
}

impl RowState {
    /// Whether an update may be offered in this state: not while work runs
    /// or nothing is installed, nor after one found nothing newer.
    fn updatable(&self) -> bool {
        !matches!(
            self,
            Self::Working { .. }
                | Self::Removing
                | Self::NotInstalled
                | Self::NoBuildHere
                | Self::NoNewer { .. }
        )
    }
}

impl TabRow {
    fn working(&self) -> bool {
        matches!(self.state, RowState::Working { .. } | RowState::Removing)
    }
}

/// Turn Off for a copy that may run, Turn On for one turned off.
fn toggle(listed: bool, on: bool) -> Vec<MenuCommand> {
    match (listed, on) {
        (false, _) => Vec::new(),
        (true, true) => vec![MenuCommand::TurnOff],
        (true, false) => vec![MenuCommand::TurnOn],
    }
}

fn blocked_row(blocked: &BlockedPlugin, kind: &'static str) -> TabRow {
    TabRow {
        id: blocked.id.clone(),
        version: blocked.version.clone(),
        kind,
        state: RowState::Blocked {
            advisory: blocked.advisory.clone(),
        },
        action: None,
        menu: Vec::new(),
        directory: None,
        project_folder: false,
    }
}

/// What a copy on this computer brings.
fn entry_kind(entry: &PluginEntry) -> &'static str {
    if entry.has_role(PluginRole::Media) {
        "Media"
    } else if entry.has_role(PluginRole::Content) && !entry.manifest.executable.is_empty() {
        "Content"
    } else {
        "Components"
    }
}

/// What the registry says a plugin brings.
fn registry_kind_name(kind: &Kind) -> &'static str {
    if kind.is_media() {
        "Media"
    } else if kind.program {
        "Content"
    } else {
        "Components"
    }
}

/// Where a copy that runs in another's place comes from, for "Not used:
/// {source} runs".
fn source_name(source: PluginSource) -> &'static str {
    match source {
        PluginSource::Explicit => "Plugin path",
        PluginSource::Locked => "Locked",
        PluginSource::Project => "Project folder",
        PluginSource::User => "User folder",
        PluginSource::Registry => "Registry",
        PluginSource::Folder => "Added folder",
        PluginSource::Bundled => "Bundled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::BlockedPlugin;
    use crate::registry::EditorRegistry;
    use celesta_plugin_host::{
        InstalledPlugins, LockedArtifact, Lockfile, Manifest, PendingPlugin, RegistryPlugin,
    };
    use celesta_registry_client::testing::TestRegistry;
    use celesta_registry_format::{Deprecation, Hash};
    use semver::VersionReq;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use time::OffsetDateTime;

    const LOCKED_WITH: &str = "0.10.0-alpha.3";

    fn entry(id: &str, version: &str, source: PluginSource, role: PluginRole) -> PluginEntry {
        let (os, _) = celesta_plugin_host::platform();
        PluginEntry {
            manifest: Manifest {
                id: id.into(),
                name: None,
                version: version.into(),
                protocol: 1,
                celesta: Some("*".into()),
                executable: BTreeMap::from([(os.to_owned(), format!("bin/{id}"))]),
                module: None,
                types: None,
                components: Vec::new(),
                roles: vec![role],
                media: None,
            },
            directory: PathBuf::from(format!("{source:?}")).join(id),
            source,
            replaces: None,
        }
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    /// A locked version with builds for `targets`.
    fn locked(at: &str, targets: &[Target]) -> LockedPlugin {
        LockedPlugin {
            celesta: VersionReq::STAR,
            registry: "https://registry.celesta.studio".into(),
            targets: targets
                .iter()
                .map(|target| {
                    let artifact = LockedArtifact {
                        sha256: Hash([1; 32]),
                        tree_hash: Hash([2; 32]),
                    };
                    (target.clone(), artifact)
                })
                .collect(),
            version: version(at),
            other: Default::default(),
        }
    }

    fn lockfile(plugins: Vec<(&str, LockedPlugin)>) -> Lockfile {
        Lockfile {
            celesta: version(LOCKED_WITH),
            plugins: plugins
                .into_iter()
                .map(|(id, locked)| (id.to_owned(), locked))
                .collect(),
            targets: Vec::new(),
            other: Default::default(),
        }
    }

    /// A trusted project whose lock is `lock`.
    fn project(lock: Lockfile) -> FoundPlugins {
        FoundPlugins {
            project: Some(PathBuf::from("/projects/demo")),
            lock: Some(lock),
            ..FoundPlugins::default()
        }
    }

    fn running(plugins: Vec<PluginEntry>) -> InstalledPlugins {
        InstalledPlugins {
            plugins,
            ..Default::default()
        }
    }

    /// The window's registry work, with `search.json` listing `latest`
    /// per id; the test registry serves nothing the model reads.
    struct Registry {
        _served: TestRegistry,
        state: RegistryState,
    }

    impl Registry {
        fn new(latest: &[(&str, &str)]) -> Self {
            let served = TestRegistry::new();
            let unused = std::env::temp_dir().join("celesta-tab-model-unused");
            let client = Arc::new(EditorRegistry::for_test(&served, &unused));
            let mut state = RegistryState::new(client);
            state.search = Fetch::Loaded {
                value: SearchList {
                    generated: OffsetDateTime::UNIX_EPOCH,
                    plugins: latest
                        .iter()
                        .map(|(id, latest)| SearchEntry {
                            id: (*id).to_owned(),
                            publisher: "acme".into(),
                            kind: Kind {
                                roles: vec!["content".into()],
                                program: true,
                                module: false,
                            },
                            description: String::new(),
                            latest: Some(version(latest)),
                            deprecated: None,
                        })
                        .collect(),
                },
                at: OffsetDateTime::UNIX_EPOCH,
                offline: false,
            };
            Self {
                _served: served,
                state,
            }
        }
    }

    fn rows_of(section: &[TabRow]) -> Vec<(&str, String)> {
        section
            .iter()
            .map(|row| (row.id.as_str(), row.detail()))
            .collect()
    }

    #[test]
    fn locked_plugins_form_this_project() {
        let mut found = project(lockfile(vec![
            ("vrm", locked("0.2.1", &[Target::host()])),
            ("voicevox", locked("0.9.0", &[Target::host()])),
        ]));
        found.installed = running(vec![entry(
            "vrm",
            "0.2.1",
            PluginSource::Locked,
            PluginRole::Content,
        )]);
        found.disabled = vec![entry(
            "voicevox",
            "0.9.0",
            PluginSource::Locked,
            PluginRole::Content,
        )];
        let model = tab_model(&found, None);
        let section = model.project.expect("a project section");
        assert_eq!(section.lock_label, "celesta.lock · 0.10.0-alpha.3");
        assert_eq!(section.action, None);
        assert_eq!(
            rows_of(&section.rows),
            [
                ("voicevox", "Content · Off".to_owned()),
                ("vrm", "Content · On".to_owned()),
            ]
        );
        assert_eq!(
            section.rows[1].menu,
            [
                MenuCommand::TurnOff,
                MenuCommand::ShowFolder,
                MenuCommand::RemoveFromProject
            ]
        );
        assert_eq!(section.rows[0].menu[0], MenuCommand::TurnOn);
        assert!(model.global.is_empty() && model.by_hand.is_empty());
    }

    #[test]
    fn no_project_section_without_a_saved_project() {
        let found = FoundPlugins {
            installed: running(vec![entry(
                "alpha",
                "1.0.0",
                PluginSource::User,
                PluginRole::Content,
            )]),
            ..FoundPlugins::default()
        };
        let model = tab_model(&found, None);
        assert_eq!(model.project, None);
        assert_eq!(
            rows_of(&model.by_hand),
            [("alpha", "Content · On".to_owned())]
        );

        let unlocked = FoundPlugins {
            project: Some(PathBuf::from("/projects/demo")),
            lock_problem: Some(LockProblem::NoLock),
            ..FoundPlugins::default()
        };
        let section = tab_model(&unlocked, None).project.unwrap();
        assert_eq!(section.lock_label, "No celesta.lock");
        assert_eq!(section.action, Some(SectionAction::UpdatePlugins));
        assert!(section.rows.is_empty());
    }

    #[test]
    fn bundled_and_registry_copies_are_two_global_rows() {
        let registry_copy = PluginEntry {
            replaces: Some(PluginSource::Bundled),
            ..entry(
                "avfoundation",
                "0.10.0-alpha.3",
                PluginSource::Registry,
                PluginRole::Media,
            )
        };
        let bundled_copy = entry(
            "avfoundation",
            "0.10.0-alpha.2",
            PluginSource::Bundled,
            PluginRole::Media,
        );
        let ffmpeg = entry(
            "ffmpeg",
            "0.10.0-alpha.3",
            PluginSource::Bundled,
            PluginRole::Media,
        );
        let found = FoundPlugins {
            installed: running(vec![registry_copy, ffmpeg.clone()]),
            registry_plugins: vec![RegistryPlugin {
                id: "avfoundation".into(),
                version: version("0.10.0-alpha.3"),
                tree_hash: Hash([3; 32]),
            }],
            bundled: vec![bundled_copy.clone(), ffmpeg],
            ..FoundPlugins::default()
        };
        let model = tab_model(&found, None);
        assert_eq!(
            rows_of(&model.global),
            [
                ("avfoundation", "Media · On".to_owned()),
                ("avfoundation", "Media · Not used: Registry runs".to_owned()),
                ("ffmpeg", "Media · Bundled · On".to_owned()),
            ]
        );
        let not_used = &model.global[1];
        assert_eq!(
            not_used.state,
            RowState::NotUsed {
                runs: "Registry".into()
            }
        );
        assert_eq!(not_used.version, "0.10.0-alpha.2");
        assert_eq!(not_used.directory, Some(bundled_copy.directory));
        assert_eq!(not_used.menu, [MenuCommand::ShowFolder]);
        assert_eq!(
            model.global[0].menu,
            [
                MenuCommand::TurnOff,
                MenuCommand::ShowFolder,
                MenuCommand::Uninstall
            ]
        );
    }

    #[test]
    fn an_update_shows_when_search_latest_is_newer_within_the_requirement() {
        let mut found = project(lockfile(vec![("vrm", locked("0.2.1", &[Target::host()]))]));
        found.installed = running(vec![entry(
            "vrm",
            "0.2.1",
            PluginSource::Locked,
            PluginRole::Content,
        )]);
        found.dependencies =
            BTreeMap::from([("vrm".to_owned(), VersionReq::parse("^0.2").unwrap())]);
        let action = |latest: &str| {
            let registry = Registry::new(&[("vrm", latest)]);
            tab_model(&found, Some(&registry.state))
                .project
                .unwrap()
                .rows[0]
                .action
                .clone()
        };
        assert_eq!(
            action("0.2.3"),
            Some(RowAction::Update {
                to: version("0.2.3")
            })
        );
        assert_eq!(
            RowAction::Update {
                to: version("0.2.3")
            }
            .title(),
            "Update to 0.2.3"
        );
        assert_eq!(action("0.3.0"), None, "outside the requirement");
        assert_eq!(action("0.2.1"), None, "not newer");

        // An update that found nothing newer says so until the next check.
        let mut registry = Registry::new(&[("vrm", "0.2.3")]);
        registry.state.no_newer.insert("vrm".into());
        let row = tab_model(&found, Some(&registry.state))
            .project
            .unwrap()
            .rows[0]
            .clone();
        assert_eq!(row.action, None);
        assert_eq!(
            row.state.label(),
            format!("No newer version works with Celesta {CELESTA_VERSION}")
        );
    }

    #[test]
    fn a_missing_locked_version_offers_download_but_not_one_without_a_host_build() {
        let elsewhere = Target::parse("plan9-mips").unwrap();
        let found = project(lockfile(vec![
            ("acme.charts", locked("1.0.0", &[Target::ANY])),
            ("acme.codec", locked("1.0.0", &[Target::host()])),
            ("acme.far", locked("1.0.0", &[elsewhere])),
        ]));
        let section = tab_model(&found, None).project.unwrap();
        let states: Vec<(&str, &RowState, Option<&RowAction>)> = section
            .rows
            .iter()
            .map(|row| (row.id.as_str(), &row.state, row.action.as_ref()))
            .collect();
        assert_eq!(
            states,
            [
                (
                    "acme.charts",
                    &RowState::NotInstalled,
                    Some(&RowAction::Download)
                ),
                (
                    "acme.codec",
                    &RowState::NotInstalled,
                    Some(&RowAction::Download)
                ),
                ("acme.far", &RowState::NoBuildHere, None),
            ]
        );
        assert_eq!(section.action, Some(SectionAction::Download { count: 2 }));
        assert_eq!(section.rows[2].state.label(), "No build for this computer");
        assert_eq!(section.rows[0].menu, [MenuCommand::RemoveFromProject]);
    }

    #[test]
    fn blocked_rows_name_the_advisory() {
        let blocked = |id: &str, bundled| BlockedPlugin {
            id: id.into(),
            version: "1.0.0".into(),
            advisory: "CELESTA-2026-0003".into(),
            title: "Runs code from any file".into(),
            fixed: Some("1.0.1".into()),
            bundled,
        };
        let mut found = project(lockfile(vec![("vrm", locked("1.0.0", &[Target::host()]))]));
        found.blocked = vec![blocked("vrm", false), blocked("beep", false)];
        let model = tab_model(&found, None);
        let section = model.project.unwrap();
        assert_eq!(
            section.rows[0].state,
            RowState::Blocked {
                advisory: "CELESTA-2026-0003".into()
            }
        );
        assert_eq!(section.rows[0].state.label(), "Blocked · CELESTA-2026-0003");
        assert_eq!(section.action, None, "a stopped version is not downloaded");
        // One no section claims is listed by hand.
        assert_eq!(
            rows_of(&model.by_hand),
            [("beep", "Plugin · Blocked · CELESTA-2026-0003".to_owned())]
        );
        assert!(model.global.is_empty());

        let bundled = FoundPlugins {
            blocked: vec![blocked("ffmpeg", true)],
            ..FoundPlugins::default()
        };
        let model = tab_model(&bundled, None);
        assert_eq!(
            rows_of(&model.global),
            [("ffmpeg", "Plugin · Blocked · CELESTA-2026-0003".to_owned())]
        );
        assert!(model.by_hand.is_empty());
    }

    #[test]
    fn waiting_hand_placed_plugins_offer_allow() {
        let waiting = |id: &str, changed, from_the_internet| PendingPlugin {
            entry: entry(id, "0.1.0", PluginSource::User, PluginRole::Content),
            fingerprint: format!("sha256:{id}"),
            changed,
            from_the_internet,
        };
        let found = FoundPlugins {
            installed: running(vec![entry(
                "zeta",
                "1.0.0",
                PluginSource::Folder,
                PluginRole::Content,
            )]),
            pending: vec![waiting("beep", false, true), waiting("alpha", true, false)],
            disabled: vec![entry(
                "gamma",
                "1.0.0",
                PluginSource::Project,
                PluginRole::Content,
            )],
            ..FoundPlugins::default()
        };
        let model = tab_model(&found, None);
        assert_eq!(
            rows_of(&model.by_hand),
            [
                ("gamma", "Content · Off".to_owned()),
                ("alpha", "Content · Changed: waits for approval".to_owned()),
                ("beep", "Content · Waits for approval".to_owned()),
                ("zeta", "Content · On".to_owned()),
            ]
        );
        let beep = &model.by_hand[2];
        assert_eq!(
            beep.action,
            Some(RowAction::Allow {
                fingerprint: "sha256:beep".into()
            })
        );
        assert_eq!(
            beep.state,
            RowState::Waiting {
                changed: false,
                from_the_internet: true
            }
        );
        assert_eq!(beep.menu, [MenuCommand::TurnOff, MenuCommand::ShowFolder]);
        assert_eq!(model.by_hand[0].menu[0], MenuCommand::TurnOn);
        assert!(model.by_hand[0].project_folder);
        assert_eq!(model.by_hand[3].action, None);
    }

    #[test]
    fn deprecated_and_working_rows_say_so() {
        let mut found = project(lockfile(vec![
            ("voicevox", locked("0.9.0", &[Target::host()])),
            ("vrm", locked("0.2.1", &[Target::host()])),
        ]));
        found.installed = running(vec![
            entry(
                "voicevox",
                "0.9.0",
                PluginSource::Locked,
                PluginRole::Content,
            ),
            entry("vrm", "0.2.1", PluginSource::Locked, PluginRole::Content),
        ]);
        let mut registry = Registry::new(&[("voicevox", "0.9.0")]);
        if let Fetch::Loaded { value, .. } = &mut registry.state.search {
            value.plugins[0].deprecated = Some(Deprecation {
                message: "Replaced".into(),
                replacement: Some("aivis-speech".into()),
            });
        }
        registry.state.in_flight.insert(
            "vrm".into(),
            InFlight {
                work: Work::Update,
                done: 10,
                total: 40,
            },
        );
        let section = tab_model(&found, Some(&registry.state)).project.unwrap();
        assert_eq!(
            rows_of(&section.rows),
            [
                (
                    "voicevox",
                    "Content · Deprecated · use aivis-speech".to_owned()
                ),
                ("vrm", "Content · Installing…".to_owned()),
            ]
        );
        assert_eq!(
            section.action,
            Some(SectionAction::Working {
                done: 10,
                total: 40
            })
        );
    }
}
