//! What the plugin page shows, from the registry's files, the last
//! discovery, and the window's registry work: the list's rows and footer
//! line, a plugin's details and version table, and the footer's buttons
//! with the reason a disabled one gives. Reads nothing from disk.

use celesta_plugin_host::PluginSource;
use celesta_registry_client::{NoOffer, OfferInput, OfferScope, offered_version};
use celesta_registry_format::{AdvisoryList, Index, SearchList, Target, VersionEntry};
use semver::{Prerelease, Version};
use time::OffsetDateTime;

use crate::plugins::FoundPlugins;
use crate::plugins_tab_model::registry_kind_name;
use crate::registry::{Fetch, RegistryState};

/// What the footer says when no project is open to add a plugin to.
const PROJECT_BUSY: &str = "Another plugin change is running for this project";
const SETTINGS_BUSY: &str = "Another plugin change is running";
const OUTSIDE_A_PROJECT: &str = "Add plugins to a project with celesta.config.json";

/// One row of the list column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListRow {
    pub id: String,
    /// `Media`, `Content`, or `Components`.
    pub kind: &'static str,
    /// The registry's latest version.
    pub latest: Option<String>,
    pub prerelease: bool,
    /// `Installed`, `In this project`, `No build for this computer`, or
    /// `Deprecated`.
    pub note: Option<&'static str>,
}

/// The plugins whose id or description contains `query`, ignoring case
/// (all of them for an empty one), by id.
pub(crate) fn list_rows(
    search: &SearchList,
    query: &str,
    found: &FoundPlugins,
    host: &Target,
) -> Vec<ListRow> {
    let query = query.to_lowercase();
    let mut rows: Vec<ListRow> = search
        .plugins
        .iter()
        .filter(|entry| {
            entry.id.to_lowercase().contains(&query)
                || entry.description.to_lowercase().contains(&query)
        })
        .map(|entry| {
            let locked = found
                .lock
                .as_ref()
                .is_some_and(|lock| lock.plugins.contains_key(&entry.id));
            let has_build = found
                .lock
                .as_ref()
                .is_some_and(|lock| lock.artifact(&entry.id, host).is_some());
            let installed = found
                .registry_plugins
                .iter()
                .any(|installed| installed.id == entry.id);
            let note = if locked && !has_build {
                Some("No build for this computer")
            } else if installed {
                Some("Installed")
            } else if locked {
                Some("In this project")
            } else if entry.deprecated.is_some() {
                Some("Deprecated")
            } else {
                None
            };
            ListRow {
                id: entry.id.clone(),
                kind: registry_kind_name(&entry.kind),
                latest: entry.latest.as_ref().map(ToString::to_string),
                prerelease: entry
                    .latest
                    .as_ref()
                    .is_some_and(|latest| latest.pre != Prerelease::EMPTY),
                note,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows
}

/// The line under the list: how many plugins, from which registry, and
/// when the list was fetched; or why there is no list. `host` is the
/// registry's host name.
pub(crate) fn list_footer(search: &Fetch<SearchList>, host: &str, now: OffsetDateTime) -> String {
    match search {
        Fetch::Idle | Fetch::Loading => "Loading the plugin list…".to_owned(),
        Fetch::Failed(_) => format!("Couldn’t reach {host}"),
        Fetch::Loaded {
            at, offline: true, ..
        } => format!("Offline · showing the list from {}", clock(*at)),
        Fetch::Loaded { value, at, .. } => {
            let count = value.plugins.len();
            let noun = if count == 1 { "plugin" } else { "plugins" };
            format!("{count} {noun} · {host} · updated {}", ago(now - *at))
        }
    }
}

/// `2026-11-16 09:30 UTC`.
fn clock(at: OffsetDateTime) -> String {
    let at = at.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// How long ago, in the footer's words.
fn ago(elapsed: time::Duration) -> String {
    let minutes = elapsed.whole_minutes();
    if minutes < 1 {
        "just now".to_owned()
    } else if minutes < 60 {
        format!("{minutes} min ago")
    } else if minutes < 24 * 60 {
        format!("{} h ago", minutes / 60)
    } else {
        format!("{} d ago", minutes / (24 * 60))
    }
}

/// A byte count in decimal units with one decimal: `1.2 MB`.
fn format_size(bytes: u64) -> String {
    const UNITS: [(&str, f64); 3] = [("kB", 1e3), ("MB", 1e6), ("GB", 1e9)];
    // The largest unit the size reaches, then one more when it rounds up
    // to 1000 of that one (999 999 B is 1.0 MB, not 1000.0 kB).
    let mut unit = UNITS.iter().rposition(|(_, size)| bytes as f64 >= *size);
    if let Some(at) = unit {
        let rounded = (bytes as f64 / UNITS[at].1 * 10.0).round() / 10.0;
        if rounded >= 1000.0 && at + 1 < UNITS.len() {
            unit = Some(at + 1);
        }
    }
    match unit {
        Some(at) => format!("{:.1} {}", bytes as f64 / UNITS[at].1, UNITS[at].0),
        None => format!("{bytes} B"),
    }
}

/// The detail column's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Details {
    /// `Media plugin by celesta`.
    pub kind: String,
    pub latest: String,
    /// The targets the latest version has builds for.
    pub builds: Vec<String>,
    /// `repository · workflow · commit`.
    pub source: Option<String>,
    /// Every copy this computer has, and whether it runs: `Bundled
    /// 0.10.0-alpha.3 · not used`.
    pub here: Vec<String>,
    /// The advisories that name this plugin, one line each.
    pub advisories: Vec<String>,
    pub versions: Vec<VersionRow>,
}

/// One row of the versions table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VersionRow {
    pub version: String,
    /// `Published`, `Deprecated`, `Yanked`, `Withdrawn`, or `Blocked`.
    pub state: &'static str,
    /// The size of the build for this computer, `1.2 MB`.
    pub size: Option<String>,
}

/// The details of the plugin `index` describes; pre-releases are left out
/// of the table unless `include_prereleases`.
pub(crate) fn details(
    index: &Index,
    found: &FoundPlugins,
    advisories: Option<&AdvisoryList>,
    host: &Target,
    include_prereleases: bool,
) -> Details {
    let id = index.id.as_str();
    let alive = || {
        index
            .versions
            .iter()
            .filter(|entry| !entry.yanked && !entry.withdrawn)
    };
    // Without the checkbox the header agrees with the table, unless only
    // pre-releases exist: then the newest of them, so it is not empty.
    let stable = |entry: &&VersionEntry| include_prereleases || entry.version.pre.is_empty();
    let latest = alive()
        .filter(stable)
        .max_by(|a, b| a.version.cmp(&b.version))
        .or_else(|| alive().max_by(|a, b| a.version.cmp(&b.version)));
    let mut versions: Vec<&VersionEntry> = index
        .versions
        .iter()
        .filter(|entry| include_prereleases || entry.version.pre == Prerelease::EMPTY)
        .collect();
    versions.sort_by(|a, b| b.version.cmp(&a.version));
    Details {
        kind: format!(
            "{} plugin by {}",
            registry_kind_name(&index.kind),
            index.publisher
        ),
        latest: latest.map_or_else(
            || "None published".to_owned(),
            |entry| entry.version.to_string(),
        ),
        builds: latest
            .iter()
            .flat_map(|entry| entry.targets.keys())
            .map(ToString::to_string)
            .collect(),
        source: latest
            .and_then(|entry| entry.source.as_ref())
            .map(|source| {
                let commit: String = source.commit.chars().take(7).collect();
                format!("{} · {} · {commit}", source.repository, source.workflow)
            }),
        here: here(id, found),
        advisories: advisories
            .into_iter()
            .flat_map(|list| list.advisories.iter())
            .filter(|advisory| advisory.plugin == id)
            .map(|advisory| {
                let mut line = format!(
                    "{} · {} · {}",
                    advisory.id, advisory.title, advisory.versions
                );
                if let Some(fixed) = &advisory.fixed {
                    line.push_str(&format!(" · fixed in {fixed}"));
                }
                line
            })
            .collect(),
        versions: versions
            .into_iter()
            .map(|entry| VersionRow {
                version: entry.version.to_string(),
                state: if entry.withdrawn {
                    "Withdrawn"
                } else if entry.yanked {
                    "Yanked"
                } else if advisories.is_some_and(|list| list.blocking(id, &entry.version).is_some())
                {
                    "Blocked"
                } else if entry.deprecated.is_some() {
                    "Deprecated"
                } else {
                    "Published"
                },
                size: entry
                    .targets
                    .get(host)
                    .or_else(|| entry.targets.get(&Target::ANY))
                    .map(|artifact| format_size(artifact.size)),
            })
            .collect(),
    }
}

/// The copies of `id` this computer has: the bundled one, the one
/// installed for every project, and the one this project locks, each with
/// whether it is the one that runs (said only when a copy runs).
fn here(id: &str, found: &FoundPlugins) -> Vec<String> {
    let running = found
        .installed
        .plugins
        .iter()
        .find(|entry| entry.manifest.id == id);
    let line = |label: &str, version: &str, source: PluginSource| {
        let state = match running {
            Some(entry) if entry.source == source && entry.manifest.version == version => " · runs",
            Some(_) => " · not used",
            None => "",
        };
        format!("{label} {version}{state}")
    };
    let mut lines = Vec::new();
    lines.extend(
        found
            .bundled
            .iter()
            .filter(|entry| entry.manifest.id == id)
            .map(|entry| line("Bundled", &entry.manifest.version, PluginSource::Bundled)),
    );
    lines.extend(
        found
            .registry_plugins
            .iter()
            .filter(|installed| installed.id == id)
            .map(|installed| {
                line(
                    "Installed",
                    &installed.version.to_string(),
                    PluginSource::Registry,
                )
            }),
    );
    lines.extend(
        found
            .lock
            .iter()
            .filter_map(|lock| lock.plugins.get(id))
            .map(|locked| line("Locked", &locked.version.to_string(), PluginSource::Locked)),
    );
    lines
}

/// What the detail column's footer offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Footer {
    pub primary: FooterButton,
    pub secondary: Option<FooterButton>,
    /// Under the buttons, why the primary one is disabled.
    pub reason: Option<String>,
    /// Bytes done and total while the plugin is being installed.
    pub progress: Option<(u64, u64)>,
    /// The red line from the last attempt.
    pub failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FooterButton {
    pub label: String,
    pub enabled: bool,
    pub command: PageCommand,
}

/// What a footer button does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PageCommand {
    /// Installs this version for every project (media plugins).
    Install(Version),
    /// Adds the plugin to the open project at this version.
    Add(Version),
    /// Pins a media plugin in the open project at this version.
    Pin(Version),
    /// Updates what is installed to the offered version.
    Update,
    None,
}

impl FooterButton {
    fn new(label: impl Into<String>, command: PageCommand) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            command,
        }
    }

    fn disabled(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: false,
            command: PageCommand::None,
        }
    }
}

/// The footer for the plugin `index` describes, given the cached
/// `advisories` the offer reads.
pub(crate) fn footer(
    index: &Index,
    found: &FoundPlugins,
    registry: &RegistryState,
    advisories: Option<&AdvisoryList>,
    host: &Target,
) -> Footer {
    let id = index.id.as_str();
    let failure = registry.failures.get(id).cloned();
    if let Some(working) = registry.in_flight.get(id) {
        return Footer {
            primary: FooterButton::disabled("Installing…"),
            secondary: None,
            reason: None,
            progress: Some((working.done, working.total)),
            failure,
        };
    }

    let media = index.kind.is_media();
    // A project whose targets are not read builds for this computer's.
    let project_targets = if found.targets.is_empty() {
        std::slice::from_ref(host)
    } else {
        &found.targets
    };
    let offer = |scope| {
        offered_version(&OfferInput {
            index,
            advisories,
            celesta: registry.client().celesta(),
            host,
            targets: project_targets,
            include_prereleases: registry.include_prereleases,
            scope,
        })
    };

    let (label, scope, command): (_, _, fn(Version) -> PageCommand) = if media {
        ("Install", OfferScope::Global, PageCommand::Install)
    } else {
        ("Add to this project", OfferScope::Project, PageCommand::Add)
    };
    let mut reason = None;
    // What the primary button would change, which another job may hold.
    let mut changes = Changes::Nothing;
    let primary = match offer(scope) {
        Ok(offered) => match held(id, &offered, found) {
            Held::Offered => FooterButton::disabled("Installed"),
            Held::Older { locked } => {
                changes = if locked {
                    Changes::Project
                } else {
                    Changes::Settings
                };
                FooterButton::new(format!("Update to {offered}"), PageCommand::Update)
            }
            Held::No if !media && found.project.is_none() => {
                reason = Some(OUTSIDE_A_PROJECT.to_owned());
                FooterButton::disabled(label)
            }
            Held::No => {
                changes = if media {
                    Changes::Settings
                } else {
                    Changes::Project
                };
                FooterButton::new(label, command(offered))
            }
        },
        Err(no_offer) => {
            reason = Some(if !media && found.project.is_none() {
                OUTSIDE_A_PROJECT.to_owned()
            } else {
                no_offer_reason(&no_offer)
            });
            FooterButton::disabled(label)
        }
    };

    let busy = |changes: Changes| match changes {
        Changes::Project
            if found
                .project
                .as_deref()
                .is_some_and(|root| registry.project_busy(root)) =>
        {
            Some(PROJECT_BUSY)
        }
        Changes::Settings if registry.settings_busy() => Some(SETTINGS_BUSY),
        _ => None,
    };
    let mut primary = primary;
    if let Some(why) = busy(changes) {
        primary = FooterButton::disabled(primary.label);
        reason = Some(why.to_owned());
    }

    let secondary = (media && found.project.is_some()).then(|| {
        let locked_at = |version: &Version| {
            found
                .lock
                .as_ref()
                .and_then(|lock| lock.plugins.get(id))
                .is_some_and(|locked| locked.version == *version)
        };
        match offer(OfferScope::Pin) {
            Ok(pinned) if !locked_at(&pinned) => {
                FooterButton::new("Pin to this project", PageCommand::Pin(pinned))
            }
            _ => FooterButton::disabled("Pin to this project"),
        }
    });

    let secondary = secondary.map(|button| match busy(Changes::Project) {
        Some(_) if button.enabled => FooterButton::disabled(button.label),
        _ => button,
    });

    Footer {
        primary,
        secondary,
        reason,
        progress: None,
        failure,
    }
}

/// What a button writes, which one job at a time may do.
#[derive(Clone, Copy)]
enum Changes {
    Nothing,
    /// The project's `celesta.config.json` and `celesta.lock`.
    Project,
    /// The user's settings (`registryPlugins`).
    Settings,
}

/// Whether the store holds the offered version of a plugin.
enum Held {
    /// This version, or a newer one, is installed, for every project or in
    /// this project's lock. A bundled copy does not count.
    Offered,
    /// Only an older version is; `locked` when this project's lock holds it.
    Older {
        locked: bool,
    },
    No,
}

fn held(id: &str, offered: &Version, found: &FoundPlugins) -> Held {
    let installed = found
        .registry_plugins
        .iter()
        .filter(|installed| installed.id == id)
        .map(|installed| &installed.version);
    let locked = found
        .lock
        .iter()
        .filter_map(|lock| lock.plugins.get(id))
        .map(|locked| &locked.version);
    // An older one in the lock is what an update rewrites, if both exist.
    let mut older = false;
    let mut from_lock = locked.peekable();
    let in_lock = from_lock.peek().is_some();
    for version in installed.chain(from_lock) {
        if version >= offered {
            return Held::Offered;
        }
        older = true;
    }
    if older {
        Held::Older { locked: in_lock }
    } else {
        Held::No
    }
}

fn no_offer_reason(no_offer: &NoOffer) -> String {
    match no_offer {
        NoOffer::NoBuild { .. } => "No build for this computer".to_owned(),
        NoOffer::NeedsCelesta { celesta, .. } => format!("Needs Celesta {celesta}"),
        NoOffer::OnlyPrereleases { .. } => {
            "Only pre-releases are available. Turn on Include pre-releases.".to_owned()
        }
        NoOffer::NothingPublished => "No version can be installed".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use celesta_plugin_host::{
        CELESTA_VERSION, InstalledPlugins, LockedArtifact, LockedPlugin, Lockfile, Manifest,
        PluginEntry, PluginRole, RegistryPlugin,
    };
    use celesta_registry_client::testing::TestRegistry;
    use celesta_registry_format::{Artifact, Deprecation, Hash, Kind, SearchEntry};
    use semver::VersionReq;
    use serde_json::json;

    use super::*;
    use crate::registry::{EditorRegistry, InFlight, Work};
    use crate::scratch::scratch_dir;

    fn host() -> Target {
        Target::parse("macos-aarch64").unwrap()
    }

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn kind(media: bool) -> Kind {
        Kind {
            roles: vec![if media { "media" } else { "content" }.into()],
            program: true,
            module: false,
        }
    }

    fn search_entry(id: &str, description: &str, latest: &str) -> SearchEntry {
        SearchEntry {
            id: id.into(),
            publisher: "acme".into(),
            kind: kind(false),
            description: description.into(),
            latest: Some(version(latest)),
            deprecated: None,
        }
    }

    fn search(entries: Vec<SearchEntry>) -> SearchList {
        SearchList {
            generated: OffsetDateTime::UNIX_EPOCH,
            plugins: entries,
        }
    }

    fn artifact(size: u64) -> Artifact {
        Artifact {
            url: "https://example.test/a.tar.gz".into(),
            size,
            sha256: Hash([1; 32]),
            tree_hash: Hash([2; 32]),
        }
    }

    fn entry(v: &str, size: u64) -> VersionEntry {
        VersionEntry {
            version: version(v),
            // This Celesta, pre-release or not.
            celesta: VersionReq::parse(&format!(">={CELESTA_VERSION}")).unwrap(),
            protocol: 1,
            published: OffsetDateTime::UNIX_EPOCH,
            yanked: false,
            withdrawn: false,
            deprecated: None,
            source: None,
            targets: BTreeMap::from([(host(), artifact(size))]),
        }
    }

    fn index(id: &str, media: bool, versions: Vec<VersionEntry>) -> Index {
        Index {
            id: id.into(),
            generated: OffsetDateTime::UNIX_EPOCH,
            publisher: "celesta".into(),
            kind: kind(media),
            description: String::new(),
            homepage: None,
            repository: None,
            deprecated: None,
            versions,
        }
    }

    fn locked(v: &str, targets: &[Target]) -> LockedPlugin {
        LockedPlugin {
            celesta: VersionReq::STAR,
            registry: "https://registry.celesta.studio".into(),
            targets: targets
                .iter()
                .map(|target| {
                    (
                        target.clone(),
                        LockedArtifact {
                            sha256: Hash([1; 32]),
                            tree_hash: Hash([2; 32]),
                        },
                    )
                })
                .collect(),
            version: version(v),
            other: Default::default(),
        }
    }

    /// A project whose lock holds `plugins`.
    fn project(plugins: Vec<(&str, LockedPlugin)>) -> FoundPlugins {
        FoundPlugins {
            project: Some(PathBuf::from("/projects/demo")),
            lock: Some(Lockfile {
                celesta: version("0.10.0-alpha.3"),
                plugins: plugins
                    .into_iter()
                    .map(|(id, locked)| (id.to_owned(), locked))
                    .collect(),
                targets: Vec::new(),
                other: Default::default(),
            }),
            targets: vec![host()],
            ..FoundPlugins::default()
        }
    }

    fn installed(id: &str, v: &str) -> RegistryPlugin {
        RegistryPlugin {
            id: id.into(),
            version: version(v),
            tree_hash: Hash([3; 32]),
        }
    }

    fn bundled(id: &str, v: &str) -> PluginEntry {
        PluginEntry {
            manifest: Manifest {
                id: id.into(),
                name: None,
                version: v.into(),
                protocol: 1,
                celesta: Some("*".into()),
                executable: BTreeMap::new(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: vec![PluginRole::Media],
                media: None,
            },
            directory: PathBuf::from("Bundled").join(id),
            source: PluginSource::Bundled,
            replaces: None,
        }
    }

    /// The window's registry work, in a folder of its own.
    struct Registry {
        _served: TestRegistry,
        user: PathBuf,
        state: RegistryState,
    }

    impl Drop for Registry {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.user);
        }
    }

    impl Registry {
        fn new(include_prereleases: bool) -> Self {
            let served = TestRegistry::new();
            let user = scratch_dir("plugin-page-model");
            let client = Arc::new(EditorRegistry::for_test(&served, &user));
            let mut state = RegistryState::new(client);
            state.include_prereleases = include_prereleases;
            Self {
                _served: served,
                user,
                state,
            }
        }
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
    }

    #[test]
    fn search_matches_id_and_description_ignoring_case() {
        let list = search(vec![
            search_entry("acme.charts", "Draws bar charts", "1.0.0"),
            search_entry("acme.maps", "Maps of the WORLD", "1.0.0"),
            search_entry("other.tool", "A tool", "1.0.0"),
        ]);
        let found = FoundPlugins::default();
        let ids = |query: &str| -> Vec<String> {
            list_rows(&list, query, &found, &host())
                .into_iter()
                .map(|row| row.id)
                .collect()
        };
        assert_eq!(ids("ACME"), ["acme.charts", "acme.maps"]);
        assert_eq!(ids("world"), ["acme.maps"]);
        assert_eq!(ids("bar CHARTS"), ["acme.charts"]);
        assert!(ids("nothing like it").is_empty());
    }

    #[test]
    fn an_empty_query_lists_everything() {
        let list = search(vec![
            search_entry("b.second", "", "1.0.0"),
            search_entry("a.first", "", "2.0.0-beta.1"),
        ]);
        let rows = list_rows(&list, "", &FoundPlugins::default(), &host());
        assert_eq!(rows.len(), 2);
        // By id, with the latest and whether it is a pre-release.
        assert_eq!(rows[0].id, "a.first");
        assert_eq!(rows[0].latest.as_deref(), Some("2.0.0-beta.1"));
        assert!(rows[0].prerelease);
        assert!(!rows[1].prerelease);
        assert_eq!(rows[0].kind, "Content");
    }

    #[test]
    fn rows_say_what_this_computer_has() {
        let mut deprecated = search_entry("e.old", "", "1.0.0");
        deprecated.deprecated = Some(Deprecation {
            message: "Use another".into(),
            replacement: None,
        });
        let list = search(vec![
            search_entry("a.global", "", "1.0.0"),
            search_entry("b.locked", "", "1.0.0"),
            search_entry("c.nobuild", "", "1.0.0"),
            search_entry("d.plain", "", "1.0.0"),
            deprecated,
        ]);
        let mut found = project(vec![
            ("b.locked", locked("1.0.0", &[host()])),
            (
                "c.nobuild",
                locked("1.0.0", &[Target::parse("linux-x86_64").unwrap()]),
            ),
        ]);
        found.registry_plugins = vec![installed("a.global", "1.0.0")];
        let notes: Vec<_> = list_rows(&list, "", &found, &host())
            .into_iter()
            .map(|row| row.note)
            .collect();
        assert_eq!(
            notes,
            [
                Some("Installed"),
                Some("In this project"),
                Some("No build for this computer"),
                None,
                Some("Deprecated"),
            ]
        );
    }

    #[test]
    fn the_footer_says_offline_and_when() {
        let fetched = OffsetDateTime::from_unix_timestamp(1_795_000_000).unwrap();
        let loaded = |offline| Fetch::Loaded {
            value: search(vec![search_entry("a.b", "", "1.0.0")]),
            at: fetched,
            offline,
        };
        assert_eq!(
            list_footer(&loaded(true), "registry.celesta.studio", now()),
            "Offline · showing the list from 2026-11-18 11:06 UTC"
        );
        assert_eq!(
            list_footer(
                &loaded(false),
                "registry.celesta.studio",
                fetched + time::Duration::minutes(2)
            ),
            "1 plugin · registry.celesta.studio · updated 2 min ago"
        );
        let four = Fetch::Loaded {
            value: search(
                (0..4)
                    .map(|n| search_entry(&format!("a.p{n}"), "", "1.0.0"))
                    .collect(),
            ),
            at: fetched,
            offline: false,
        };
        assert_eq!(
            list_footer(
                &four,
                "registry.celesta.studio",
                fetched + time::Duration::seconds(10)
            ),
            "4 plugins · registry.celesta.studio · updated just now"
        );
    }

    #[test]
    fn the_footer_says_unreachable_without_a_cache() {
        assert_eq!(
            list_footer(
                &Fetch::Failed("timeout".into()),
                "registry.celesta.studio",
                now()
            ),
            "Couldn’t reach registry.celesta.studio"
        );
    }

    #[test]
    fn ago_changes_unit_at_the_boundaries() {
        let minutes = |m| ago(time::Duration::minutes(m));
        assert_eq!(ago(time::Duration::seconds(59)), "just now");
        assert_eq!(minutes(1), "1 min ago");
        assert_eq!(minutes(59), "59 min ago");
        assert_eq!(minutes(60), "1 h ago");
        assert_eq!(minutes(24 * 60 - 1), "23 h ago");
        assert_eq!(minutes(24 * 60), "1 d ago");
    }

    #[test]
    fn a_newer_held_version_is_not_downgraded() {
        let registry = Registry::new(false);
        let index = index(
            "celesta.avf",
            true,
            vec![entry("1.0.0", 10), entry("2.0.0-beta.1", 10)],
        );
        let found = FoundPlugins {
            registry_plugins: vec![installed("celesta.avf", "2.0.0-beta.1")],
            ..FoundPlugins::default()
        };
        let footer = footer(&index, &found, &registry.state, None, &host());
        assert_eq!(footer.primary.label, "Installed");
        assert!(!footer.primary.enabled);
    }

    #[test]
    fn a_busy_project_disables_the_project_buttons() {
        let mut registry = Registry::new(false);
        registry
            .state
            .set_project_busy(PathBuf::from("/projects/demo"));
        let busy = "Another plugin change is running for this project";
        let add = footer(
            &index("acme.charts", false, vec![entry("1.0.0", 10)]),
            &project(Vec::new()),
            &registry.state,
            None,
            &host(),
        );
        assert_eq!(add.primary.label, "Add to this project");
        assert!(!add.primary.enabled);
        assert_eq!(add.primary.command, PageCommand::None);
        assert_eq!(add.reason.as_deref(), Some(busy));

        // A media plugin installs for the user, but pins in the project.
        let media = footer(
            &index("celesta.avf", true, vec![entry("1.0.0", 10)]),
            &project(Vec::new()),
            &registry.state,
            None,
            &host(),
        );
        assert!(media.primary.enabled);
        let pin = media.secondary.unwrap();
        assert_eq!(pin.label, "Pin to this project");
        assert!(!pin.enabled);

        // An update of the locked version rewrites the lock.
        let newer = index(
            "acme.charts",
            false,
            vec![entry("1.0.0", 10), entry("1.1.0", 10)],
        );
        let update = footer(
            &newer,
            &project(vec![("acme.charts", locked("1.0.0", &[host()]))]),
            &registry.state,
            None,
            &host(),
        );
        assert_eq!(update.primary.label, "Update to 1.1.0");
        assert!(!update.primary.enabled);
        assert_eq!(update.reason.as_deref(), Some(busy));
    }

    #[test]
    fn busy_settings_disable_the_global_buttons() {
        let mut registry = Registry::new(false);
        registry.state.set_settings_busy(true);
        let index = index("celesta.avf", true, vec![entry("1.0.0", 10)]);
        let footer = footer(&index, &project(Vec::new()), &registry.state, None, &host());
        assert_eq!(footer.primary.label, "Install");
        assert!(!footer.primary.enabled);
        assert_eq!(
            footer.reason.as_deref(),
            Some("Another plugin change is running")
        );
        // Pinning writes the project, not the settings.
        assert!(footer.secondary.unwrap().enabled);
    }

    #[test]
    fn the_header_follows_the_checkbox() {
        let both = index(
            "celesta.avf",
            true,
            vec![entry("1.0.0", 10), entry("1.1.0-beta.1", 10)],
        );
        let found = FoundPlugins::default();
        assert_eq!(details(&both, &found, None, &host(), false).latest, "1.0.0");
        assert_eq!(
            details(&both, &found, None, &host(), true).latest,
            "1.1.0-beta.1"
        );
        // Only pre-releases: the newest of them rather than nothing.
        let alpha = index("a.b", false, vec![entry("0.1.0-alpha.1", 10)]);
        assert_eq!(
            details(&alpha, &found, None, &host(), false).latest,
            "0.1.0-alpha.1"
        );
    }

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(format_size(999_999), "1.0 MB");
        assert_eq!(format_size(949_000), "949.0 kB");
        assert_eq!(format_size(1_215_625), "1.2 MB");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(12_400), "12.4 kB");
        assert_eq!(format_size(2_500_000_000), "2.5 GB");
    }

    #[test]
    fn media_plugins_install_and_pin() {
        let registry = Registry::new(false);
        let index = index("celesta.avf", true, vec![entry("1.0.0", 10)]);
        let found = project(Vec::new());
        let footer = footer(&index, &found, &registry.state, None, &host());
        assert_eq!(footer.primary.label, "Install");
        assert!(footer.primary.enabled);
        assert_eq!(
            footer.primary.command,
            PageCommand::Install(version("1.0.0"))
        );
        let pin = footer.secondary.expect("a project can pin");
        assert_eq!(pin.label, "Pin to this project");
        assert!(pin.enabled);
        assert_eq!(pin.command, PageCommand::Pin(version("1.0.0")));

        // Outside a project there is nothing to pin in.
        let outside = super::footer(
            &index,
            &FoundPlugins::default(),
            &registry.state,
            None,
            &host(),
        );
        assert_eq!(outside.primary.label, "Install");
        assert!(outside.secondary.is_none());
    }

    #[test]
    fn other_plugins_add_to_the_project() {
        let registry = Registry::new(false);
        let index = index("acme.charts", false, vec![entry("1.0.0", 10)]);
        let footer = footer(&index, &project(Vec::new()), &registry.state, None, &host());
        assert_eq!(footer.primary.label, "Add to this project");
        assert_eq!(footer.primary.command, PageCommand::Add(version("1.0.0")));
        assert!(footer.secondary.is_none());
        assert_eq!(footer.reason, None);
    }

    #[test]
    fn add_is_disabled_outside_a_project_with_the_reason() {
        let registry = Registry::new(false);
        let index = index("acme.charts", false, vec![entry("1.0.0", 10)]);
        let footer = footer(
            &index,
            &FoundPlugins::default(),
            &registry.state,
            None,
            &host(),
        );
        assert_eq!(footer.primary.label, "Add to this project");
        assert!(!footer.primary.enabled);
        assert_eq!(footer.primary.command, PageCommand::None);
        assert_eq!(
            footer.reason.as_deref(),
            Some("Add plugins to a project with celesta.config.json")
        );
    }

    #[test]
    fn a_bundled_copy_alone_keeps_install_available() {
        let registry = Registry::new(false);
        let index = index("celesta.avf", true, vec![entry("1.0.0", 10)]);
        let found = FoundPlugins {
            bundled: vec![bundled("celesta.avf", "1.0.0")],
            ..FoundPlugins::default()
        };
        let footer = footer(&index, &found, &registry.state, None, &host());
        assert_eq!(footer.primary.label, "Install");
        assert!(footer.primary.enabled);
    }

    #[test]
    fn the_store_copy_makes_it_installed() {
        let registry = Registry::new(false);
        let media = index("celesta.avf", true, vec![entry("1.0.0", 10)]);
        let found = FoundPlugins {
            registry_plugins: vec![installed("celesta.avf", "1.0.0")],
            ..FoundPlugins::default()
        };
        let footer_of = |index: &Index, found: &FoundPlugins| {
            footer(index, found, &registry.state, None, &host())
        };
        let installed_footer = footer_of(&media, &found);
        assert_eq!(installed_footer.primary.label, "Installed");
        assert!(!installed_footer.primary.enabled);

        // The project's lock counts as well.
        let charts = index("acme.charts", false, vec![entry("1.0.0", 10)]);
        let locked_footer = footer_of(
            &charts,
            &project(vec![("acme.charts", locked("1.0.0", &[host()]))]),
        );
        assert_eq!(locked_footer.primary.label, "Installed");
        assert!(!locked_footer.primary.enabled);

        // An older one offers the update.
        let newer = index(
            "celesta.avf",
            true,
            vec![entry("1.0.0", 10), entry("1.1.0", 10)],
        );
        let update = footer_of(&newer, &found);
        assert_eq!(update.primary.label, "Update to 1.1.0");
        assert!(update.primary.enabled);
        assert_eq!(update.primary.command, PageCommand::Update);
    }

    #[test]
    fn only_prereleases_explain_the_checkbox() {
        let index = index("acme.charts", false, vec![entry("0.1.0-alpha.1", 10)]);
        let off = Registry::new(false);
        let footer_off = footer(&index, &project(Vec::new()), &off.state, None, &host());
        assert!(!footer_off.primary.enabled);
        assert_eq!(
            footer_off.reason.as_deref(),
            Some("Only pre-releases are available. Turn on Include pre-releases.")
        );
        let on = Registry::new(true);
        let footer_on = footer(&index, &project(Vec::new()), &on.state, None, &host());
        assert!(footer_on.primary.enabled);
        assert_eq!(footer_on.reason, None);
    }

    #[test]
    fn no_offer_gives_its_reason() {
        let registry = Registry::new(false);
        let reason = |index: &Index| {
            footer(index, &project(Vec::new()), &registry.state, None, &host())
                .reason
                .unwrap()
        };
        let mut other_target = entry("1.0.0", 10);
        other_target.targets =
            BTreeMap::from([(Target::parse("linux-x86_64").unwrap(), artifact(10))]);
        assert_eq!(
            reason(&index("acme.charts", false, vec![other_target])),
            "No build for this computer"
        );
        let mut future = entry("1.0.0", 10);
        future.celesta = VersionReq::parse(">=99.0.0").unwrap();
        assert_eq!(
            reason(&index("acme.charts", false, vec![future])),
            "Needs Celesta >=99.0.0"
        );
        let mut yanked = entry("1.0.0", 10);
        yanked.yanked = true;
        assert_eq!(
            reason(&index("acme.charts", false, vec![yanked])),
            "No version can be installed"
        );
    }

    #[test]
    fn work_in_flight_shows_installing_and_progress() {
        let mut registry = Registry::new(false);
        registry.state.in_flight.insert(
            "acme.charts".into(),
            InFlight {
                work: Work::Add,
                done: 300,
                total: 1000,
            },
        );
        registry
            .state
            .failures
            .insert("acme.charts".into(), "Couldn’t download acme.charts".into());
        let index = index("acme.charts", false, vec![entry("1.0.0", 10)]);
        let footer = footer(&index, &project(Vec::new()), &registry.state, None, &host());
        assert_eq!(footer.primary.label, "Installing…");
        assert!(!footer.primary.enabled);
        assert_eq!(footer.progress, Some((300, 1000)));
        assert_eq!(
            footer.failure.as_deref(),
            Some("Couldn’t download acme.charts")
        );
    }

    #[test]
    fn a_failure_shows_under_the_buttons() {
        let mut registry = Registry::new(false);
        registry
            .state
            .failures
            .insert("acme.charts".into(), "Couldn’t download acme.charts".into());
        let index = index("acme.charts", false, vec![entry("1.0.0", 10)]);
        let footer = footer(&index, &project(Vec::new()), &registry.state, None, &host());
        assert_eq!(footer.primary.label, "Add to this project");
        assert_eq!(
            footer.failure.as_deref(),
            Some("Couldn’t download acme.charts")
        );
        assert_eq!(footer.progress, None);
    }

    #[test]
    fn the_version_table_hides_prereleases_when_excluded() {
        let mut yanked = entry("0.9.0", 500);
        yanked.yanked = true;
        let mut deprecated = entry("0.8.0", 2_000);
        deprecated.deprecated = Some(Deprecation {
            message: "Old".into(),
            replacement: None,
        });
        let index = index(
            "celesta.avf",
            true,
            vec![
                entry("1.0.0", 1_215_625),
                entry("1.1.0-beta.1", 1_000),
                yanked,
                deprecated,
            ],
        );
        let found = FoundPlugins::default();
        let off = details(&index, &found, None, &host(), false);
        let rows: Vec<_> = off
            .versions
            .iter()
            .map(|row| (row.version.as_str(), row.state, row.size.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                ("1.0.0", "Published", Some("1.2 MB")),
                ("0.9.0", "Yanked", Some("500 B")),
                ("0.8.0", "Deprecated", Some("2.0 kB")),
            ]
        );
        let on = details(&index, &found, None, &host(), true);
        assert_eq!(on.versions[0].version, "1.1.0-beta.1");
        assert_eq!(on.versions.len(), 4);
        assert_eq!(on.kind, "Media plugin by celesta");
        assert_eq!(on.latest, "1.1.0-beta.1");
        assert_eq!(on.builds, ["macos-aarch64"]);
    }

    #[test]
    fn the_details_name_blocked_versions_and_advisories() {
        let index = index(
            "acme.charts",
            false,
            vec![entry("1.0.0", 10), entry("1.1.0", 10)],
        );
        let list: AdvisoryList = serde_json::from_value(json!({
            "generated": "2026-11-01T00:00:00Z",
            "advisories": [{
                "id": "CEL-1",
                "plugin": "acme.charts",
                "versions": "<1.1.0",
                "title": "Reads any file",
                "description": "d",
                "fixed": "1.1.0",
                "published": "2026-11-01T00:00:00Z",
            }],
        }))
        .unwrap();
        let details = details(
            &index,
            &FoundPlugins::default(),
            Some(&list),
            &host(),
            false,
        );
        let states: Vec<_> = details.versions.iter().map(|row| row.state).collect();
        assert_eq!(states, ["Published", "Blocked"]);
        assert_eq!(
            details.advisories,
            ["CEL-1 · Reads any file · <1.1.0 · fixed in 1.1.0"]
        );
    }

    #[test]
    fn here_lists_each_copy_and_which_one_runs() {
        let index = index("celesta.avf", true, vec![entry("1.0.0", 10)]);
        let mut found = project(vec![("celesta.avf", locked("1.0.0", &[host()]))]);
        found.bundled = vec![bundled("celesta.avf", "0.10.0-alpha.3")];
        found.registry_plugins = vec![installed("celesta.avf", "1.0.0")];
        let mut running = bundled("celesta.avf", "1.0.0");
        running.source = PluginSource::Registry;
        found.installed = InstalledPlugins {
            plugins: vec![running],
            ..Default::default()
        };
        let here = details(&index, &found, None, &host(), false).here;
        assert_eq!(
            here,
            [
                "Bundled 0.10.0-alpha.3 · not used",
                "Installed 1.0.0 · runs",
                "Locked 1.0.0 · not used",
            ]
        );
    }

    #[test]
    fn the_source_says_the_repository_workflow_and_short_commit() {
        let mut with_source = entry("1.0.0", 10);
        with_source.source = Some(celesta_registry_format::Source {
            repository: "github.com/acme/charts".into(),
            commit: "0123456789abcdef".into(),
            workflow: "release.yml".into(),
        });
        let index = index("acme.charts", false, vec![with_source]);
        let details = details(&index, &FoundPlugins::default(), None, &host(), false);
        assert_eq!(
            details.source.as_deref(),
            Some("github.com/acme/charts · release.yml · 0123456")
        );
    }
}
