//! What the registry says about the plugins found: deprecated and yanked
//! versions, registry copies another copy keeps from running, and an old
//! advisory list, as one Problems warning; plugins an advisory stops, as a
//! Problems error and one notification each per app run.

use std::collections::BTreeSet;
use std::path::PathBuf;

use celesta_plugin_host::PluginSource;
use celesta_registry_format::{Index, SearchList};
use gpui_kit::{Context, SharedString, Window};
use semver::Version;

use super::Fetch;
use crate::plugins::{BlockedPlugin, FoundPlugins};
use crate::problems::{Problem, Source};
use crate::toast::{Toast, ToastKind};
use crate::view::EditorView;
use gpui_kit::component::WindowExt as _;

/// The key of a Blocked notification, one per id, version, and advisory.
pub(crate) type BlockKey = (String, String, String);

/// Marks the Blocked notifications, so each can be closed by its key.
struct BlockedToast;

/// One line of the warning: a title and what to do.
struct Notice {
    title: String,
    detail: String,
}

/// The Problems warning for the registry copies `found` locks or installs
/// globally: deprecated ones (from `search`), yanked versions (from the
/// indexes `cached_index` gives), copies another source keeps from running,
/// and an advisory list older than seven days (`stale`). `None` when there
/// is nothing to say.
pub(crate) fn notices(
    found: &FoundPlugins,
    search: Option<&SearchList>,
    cached_index: impl Fn(&str) -> Option<Index>,
    stale: bool,
) -> Option<Problem> {
    let mut notices = Vec::new();
    let copies = registry_copies(found);

    let mut deprecated = BTreeSet::new();
    for (id, _, _) in &copies {
        let Some(deprecation) = search
            .and_then(|list| list.plugins.iter().find(|entry| entry.id == *id))
            .and_then(|entry| entry.deprecated.as_ref())
        else {
            continue;
        };
        if !deprecated.insert(id.clone()) {
            continue;
        }
        notices.push(Notice {
            title: format!("{id} is deprecated"),
            detail: match &deprecation.replacement {
                Some(replacement) => {
                    format!("Use {replacement} instead. Replace it from the Plugins tab.")
                }
                None => "Remove or replace it from the Plugins tab.".to_owned(),
            },
        });
    }

    for (id, version, _) in &copies {
        let yanked = cached_index(id).is_some_and(|index| {
            index
                .versions
                .iter()
                .any(|entry| entry.version == *version && entry.yanked)
        });
        if yanked {
            notices.push(Notice {
                title: format!("{id} {version} was yanked"),
                detail: "It keeps working. Update it from the Plugins tab when you can.".to_owned(),
            });
        }
    }

    for (id, version, source) in &copies {
        if let Some(folder) = shadowing_folder(found, id, *source) {
            notices.push(Notice {
                title: format!("{id} {version} from the registry is not used"),
                detail: format!("The copy in {} runs instead.", folder.display()),
            });
        }
    }

    if stale {
        notices.push(Notice {
            title: "The advisory list is more than 7 days old".to_owned(),
            detail: "Celesta couldn’t check for blocked plugins recently. Check your connection."
                .to_owned(),
        });
    }

    match notices.as_slice() {
        [] => None,
        [notice] => Some(Problem::new(notice.title.clone(), notice.detail.clone())),
        several => Some(Problem::new(
            format!("{} plugin notices", several.len()),
            several
                .iter()
                .map(|notice| format!("{}. {}", notice.title, notice.detail))
                .collect::<Vec<_>>()
                .join("\n"),
        )),
    }
}

/// Whether there is no verified advisory list newer than seven days: the
/// one discovery read is older, or it read none and the advisory refresh
/// ended without one (`refresh_left_none`). While the first refresh runs,
/// no list is not yet a warning.
fn advisories_stale_now(found: &FoundPlugins, refresh_left_none: bool) -> bool {
    found.advisories_stale || (found.no_advisories && refresh_left_none)
}

/// The registry copies `found` names, each once: the locked versions, then
/// the ones installed for every project, with the source each runs from.
fn registry_copies(found: &FoundPlugins) -> Vec<(String, Version, PluginSource)> {
    let locked = found.lock.iter().flat_map(|lock| {
        lock.plugins
            .iter()
            .map(|(id, locked)| (id.clone(), locked.version.clone(), PluginSource::Locked))
    });
    let global = found.registry_plugins.iter().map(|installed| {
        (
            installed.id.clone(),
            installed.version.clone(),
            PluginSource::Registry,
        )
    });
    let mut seen = BTreeSet::new();
    locked
        .chain(global)
        .filter(|(id, version, _)| seen.insert((id.clone(), version.clone())))
        .collect()
}

/// The folder of the copy of `id` that runs in place of the registry's
/// copy from `source`: one placed by hand (a plugin path, a project
/// folder, the user's folder) found before it. A locked version running in
/// place of a global one is what the lock is for, so it is not named.
fn shadowing_folder(found: &FoundPlugins, id: &str, source: PluginSource) -> Option<PathBuf> {
    let running = found
        .installed
        .plugins
        .iter()
        .find(|entry| entry.manifest.id == id)?;
    let by_hand = matches!(
        running.source,
        PluginSource::Explicit | PluginSource::Project | PluginSource::User
    );
    (by_hand && running.source < source).then(|| running.directory.clone())
}

/// The Problems error for the plugins an advisory stops; `None` when none
/// is.
pub(crate) fn blocked_problem(found: &FoundPlugins) -> Option<Problem> {
    let mut seen = BTreeSet::new();
    let blocked: Vec<&BlockedPlugin> = found
        .blocked
        .iter()
        .filter(|blocked| seen.insert(block_key(blocked)))
        .collect();
    match blocked.as_slice() {
        [] => None,
        [blocked] => Some(Problem::new(
            stopped_title(blocked),
            stopped_message(blocked),
        )),
        several => Some(Problem::new(
            format!("{} plugins were stopped", several.len()),
            several
                .iter()
                .map(|blocked| {
                    format!(
                        "{} {} — {}",
                        blocked.id,
                        blocked.version,
                        stopped_message(blocked)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )),
    }
}

/// The stopped plugins of `found` not announced yet, each id, version, and
/// advisory once.
pub(crate) fn newly_blocked<'a>(
    found: &'a FoundPlugins,
    announced: &BTreeSet<BlockKey>,
) -> Vec<&'a BlockedPlugin> {
    let mut seen = BTreeSet::new();
    found
        .blocked
        .iter()
        .filter(|blocked| {
            let key = block_key(blocked);
            !announced.contains(&key) && seen.insert(key)
        })
        .collect()
}

pub(crate) fn block_key(blocked: &BlockedPlugin) -> BlockKey {
    (
        blocked.id.clone(),
        blocked.version.clone(),
        blocked.advisory.clone(),
    )
}

/// `vrm 0.2.1 was stopped`.
fn stopped_title(blocked: &BlockedPlugin) -> String {
    format!("{} {} was stopped", blocked.id, blocked.version)
}

/// `CELESTA-2026-0003: runs files outside the project. Fixed in 0.2.3.`
fn stopped_message(blocked: &BlockedPlugin) -> String {
    let title = blocked.title.trim_end_matches('.');
    let fix = if blocked.bundled {
        " Update Celesta to get a fixed version.".to_owned()
    } else if let Some(fixed) = &blocked.fixed {
        format!(" Fixed in {fixed}.")
    } else {
        String::new()
    };
    format!("{}: {title}.{fix}", blocked.advisory)
}

/// How a stopped plugin is updated to its fixed version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BlockedUpdate {
    /// The project at this root locks it.
    Locked(PathBuf),
    /// It is installed for every project.
    Global,
}

/// How to update `blocked` from its notification: a version the registry
/// installed, with a fixed version, that the open project locks or the
/// user installed for every project. A bundled copy is fixed by updating
/// Celesta, and a copy placed by hand by its owner.
pub(crate) fn blocked_update(
    found: &FoundPlugins,
    blocked: &BlockedPlugin,
) -> Option<BlockedUpdate> {
    if blocked.fixed.is_none() || blocked.bundled || !blocked.from_store {
        return None;
    }
    let matches =
        |id: &str, version: &Version| id == blocked.id && version.to_string() == blocked.version;
    let locked = found.lock.as_ref().is_some_and(|lock| {
        lock.plugins
            .iter()
            .any(|(id, locked)| matches(id, &locked.version))
    });
    if let (true, Some(root)) = (locked, &found.project) {
        return Some(BlockedUpdate::Locked(root.clone()));
    }
    found
        .registry_plugins
        .iter()
        .any(|installed| matches(&installed.id, &installed.version))
        .then_some(BlockedUpdate::Global)
}

impl EditorView {
    /// Sets the registry notices for what was found and what the registry
    /// work has fetched (the search list, the indexes); reads nothing from
    /// disk.
    pub(crate) fn sync_registry_notices(&mut self, cx: &gpui_kit::App) {
        let state = self.registry.as_ref().map(|registry| registry.read(cx));
        let search = state.and_then(|state| match &state.search {
            Fetch::Loaded { value, .. } => Some(value),
            _ => None,
        });
        let problem = notices(
            &self.found_plugins,
            search,
            |id| match state?.indexes.get(id)? {
                Fetch::Loaded { value, .. } => Some(value.clone()),
                _ => None,
            },
            advisories_stale_now(
                &self.found_plugins,
                state.is_some_and(|state| state.advisories_missing),
            ),
        );
        self.problems.set(Source::RegistryNotices, problem);
    }

    /// Raises one notification for each plugin an advisory newly stops,
    /// once per id, version, and advisory while the app runs, with
    /// **Update to X** when the registry has a fixed version to update to.
    pub(crate) fn announce_blocks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fresh: Vec<BlockedPlugin> = newly_blocked(&self.found_plugins, &self.announced_blocks)
            .into_iter()
            .cloned()
            .collect();
        let view = cx.entity().downgrade();
        for blocked in fresh {
            let key = block_key(&blocked);
            self.announced_blocks.insert(key.clone());
            let id = SharedString::from(format!("{}@{}@{}", key.0, key.1, key.2));
            let toast = Toast::new(ToastKind::Error, stopped_title(&blocked))
                .message(stopped_message(&blocked));
            let view = view.clone();
            let toast = match (
                blocked.fixed.clone(),
                blocked_update(&self.found_plugins, &blocked),
            ) {
                (Some(fixed), Some(update)) => {
                    let plugin = blocked.id.clone();
                    let toast_id = id.clone();
                    toast.action(format!("Update to {fixed}"), move |_, window, cx| {
                        window.remove_notification1::<BlockedToast>(toast_id.clone(), cx);
                        let plugin = plugin.clone();
                        let update = update.clone();
                        view.update(cx, |this, cx| {
                            this.registry_work(window, cx, move |state, cx| match update {
                                BlockedUpdate::Locked(root) => {
                                    state.update_planned(root, BTreeSet::from([plugin]), cx)
                                }
                                BlockedUpdate::Global => state.update_global(&plugin, cx),
                            });
                        })
                        .ok();
                    })
                }
                _ => toast.action("Show in Problems", move |_, _, cx| {
                    view.update(cx, |this, cx| this.show_problems(cx)).ok();
                }),
            };
            window.push_notification(toast.build(cx).id1::<BlockedToast>(id), cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_plugin_host::{
        InstalledPlugins, LockedPlugin, Lockfile, Manifest, PluginEntry, RegistryPlugin,
    };
    use celesta_registry_format::Hash;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn lock(plugins: &[(&str, &str)]) -> Lockfile {
        Lockfile {
            celesta: version(celesta_plugin_host::CELESTA_VERSION),
            plugins: plugins
                .iter()
                .map(|(id, at)| {
                    let locked = LockedPlugin {
                        celesta: semver::VersionReq::STAR,
                        registry: "https://registry.test".into(),
                        targets: BTreeMap::new(),
                        version: version(at),
                        other: Default::default(),
                    };
                    ((*id).to_owned(), locked)
                })
                .collect(),
            targets: Vec::new(),
            other: Default::default(),
        }
    }

    fn search(entries: serde_json::Value) -> SearchList {
        serde_json::from_value(json!({
            "generated": "2026-11-20T00:00:00Z",
            "plugins": entries,
        }))
        .unwrap()
    }

    fn search_entry(id: &str, deprecated: Option<serde_json::Value>) -> serde_json::Value {
        let mut entry = json!({
            "id": id,
            "publisher": "acme",
            "kind": { "roles": ["content"], "program": false, "module": true },
            "description": "A test plugin.",
            "latest": "1.0.0",
        });
        if let Some(deprecated) = deprecated {
            entry["deprecated"] = deprecated;
        }
        entry
    }

    fn index(id: &str, yanked: &[&str]) -> Index {
        let versions: Vec<_> = yanked
            .iter()
            .map(|version| {
                json!({
                    "version": version,
                    "celesta": "*",
                    "protocol": 1,
                    "published": "2026-11-20T00:00:00Z",
                    "yanked": true,
                    "withdrawn": false,
                    "targets": {},
                })
            })
            .collect();
        serde_json::from_value(json!({
            "id": id,
            "generated": "2026-11-20T00:00:00Z",
            "publisher": "acme",
            "kind": { "roles": ["content"], "program": false, "module": true },
            "description": "A test plugin.",
            "versions": versions,
        }))
        .unwrap()
    }

    fn blocked(id: &str, version: &str, fixed: Option<&str>, bundled: bool) -> BlockedPlugin {
        BlockedPlugin {
            id: id.to_owned(),
            version: version.to_owned(),
            advisory: "CELESTA-2026-0003".to_owned(),
            title: "runs files outside the project".to_owned(),
            fixed: fixed.map(str::to_owned),
            bundled,
            from_store: !bundled,
            directory: PathBuf::from(format!("/store/{id}/{version}")),
        }
    }

    fn running(id: &str, source: PluginSource, directory: &Path) -> PluginEntry {
        PluginEntry {
            manifest: Manifest {
                id: id.into(),
                name: None,
                version: "1.0.0".into(),
                protocol: 1,
                celesta: Some("*".into()),
                executable: BTreeMap::new(),
                module: None,
                types: None,
                components: Vec::new(),
                roles: Vec::new(),
                media: None,
            },
            directory: directory.to_path_buf(),
            source,
            replaces: None,
        }
    }

    #[test]
    fn deprecated_and_yanked_become_one_warning() {
        let found = FoundPlugins {
            project: Some(PathBuf::from("/prism")),
            lock: Some(lock(&[("voicevox", "0.9.0")])),
            registry_plugins: vec![RegistryPlugin {
                id: "ffmpeg-x264".to_owned(),
                version: version("0.10.0-alpha.2"),
                tree_hash: Hash([0; 32]),
            }],
            ..FoundPlugins::default()
        };
        let list = search(json!([search_entry(
            "voicevox",
            Some(json!({ "message": "Renamed.", "replacement": "aivis-speech" }))
        )]));
        let indexes = BTreeMap::from([(
            "ffmpeg-x264".to_owned(),
            index("ffmpeg-x264", &["0.10.0-alpha.2"]),
        )]);
        let problem =
            notices(&found, Some(&list), |id| indexes.get(id).cloned(), false).expect("a warning");
        assert_eq!(problem.title, "2 plugin notices");
        assert_eq!(
            problem.detail,
            "voicevox is deprecated. Use aivis-speech instead. Replace it from the Plugins tab.\n\
             ffmpeg-x264 0.10.0-alpha.2 was yanked. It keeps working. Update it from the Plugins tab when you can."
        );
        assert_eq!(
            Source::RegistryNotices.severity(),
            crate::problems::Severity::Warning
        );

        // One alone is the warning itself.
        let alone = notices(&found, Some(&list), |_| None, false).unwrap();
        assert_eq!(alone.title, "voicevox is deprecated");
        assert_eq!(
            alone.detail,
            "Use aivis-speech instead. Replace it from the Plugins tab."
        );
    }

    #[test]
    fn a_deprecation_without_a_replacement_says_remove_or_replace() {
        let found = FoundPlugins {
            lock: Some(lock(&[("voicevox", "0.9.0")])),
            ..FoundPlugins::default()
        };
        let list = search(json!([search_entry(
            "voicevox",
            Some(json!({ "message": "Gone." }))
        )]));
        let problem = notices(&found, Some(&list), |_| None, false).unwrap();
        assert_eq!(problem.detail, "Remove or replace it from the Plugins tab.");
    }

    #[test]
    fn nothing_to_say_is_no_warning() {
        let found = FoundPlugins {
            lock: Some(lock(&[("voicevox", "0.9.0")])),
            ..FoundPlugins::default()
        };
        let list = search(json!([search_entry("voicevox", None)]));
        assert_eq!(notices(&found, Some(&list), |_| None, false), None);
        assert_eq!(notices(&found, None, |_| None, false), None);
    }

    #[test]
    fn an_old_advisory_list_is_a_warning() {
        let problem = notices(&FoundPlugins::default(), None, |_| None, true).unwrap();
        assert_eq!(problem.title, "The advisory list is more than 7 days old");
        assert_eq!(
            problem.detail,
            "Celesta couldn’t check for blocked plugins recently. Check your connection."
        );
    }

    #[test]
    fn no_advisory_list_is_stale_once_the_refresh_ended_without_one() {
        let none = FoundPlugins {
            no_advisories: true,
            ..FoundPlugins::default()
        };
        // The refresh failed and nothing was cached: a warning.
        assert!(advisories_stale_now(&none, true));
        let problem = notices(&none, None, |_| None, advisories_stale_now(&none, true));
        assert_eq!(
            problem.map(|problem| problem.title),
            Some("The advisory list is more than 7 days old".to_owned())
        );
        // The first refresh still runs (or it succeeded): none yet.
        assert!(!advisories_stale_now(&none, false));
        assert_eq!(
            notices(&none, None, |_| None, advisories_stale_now(&none, false)),
            None
        );
        // A list that is read and recent is no warning, whatever the refresh did.
        assert!(!advisories_stale_now(&FoundPlugins::default(), true));
        let old = FoundPlugins {
            advisories_stale: true,
            ..FoundPlugins::default()
        };
        assert!(advisories_stale_now(&old, false));
    }

    #[test]
    fn a_registry_copy_a_hand_placed_one_shadows_is_named() {
        let folder = PathBuf::from("/home/me/plugins/vrm");
        let found = FoundPlugins {
            registry_plugins: vec![RegistryPlugin {
                id: "vrm".to_owned(),
                version: version("0.2.3"),
                tree_hash: Hash([0; 32]),
            }],
            installed: InstalledPlugins {
                plugins: vec![running("vrm", PluginSource::User, &folder)],
                ..InstalledPlugins::default()
            },
            ..FoundPlugins::default()
        };
        let problem = notices(&found, None, |_| None, false).unwrap();
        assert_eq!(problem.title, "vrm 0.2.3 from the registry is not used");
        assert_eq!(
            problem.detail,
            format!("The copy in {} runs instead.", folder.display())
        );

        // A locked version running in place of the global one is the lock's
        // point.
        let locked = FoundPlugins {
            installed: InstalledPlugins {
                plugins: vec![running("vrm", PluginSource::Locked, &folder)],
                ..InstalledPlugins::default()
            },
            ..found
        };
        assert_eq!(notices(&locked, None, |_| None, false), None);
    }

    #[test]
    fn a_new_block_is_announced_once() {
        let found = FoundPlugins {
            blocked: vec![
                blocked("vrm", "0.2.1", Some("0.2.3"), false),
                // A second copy of the same version under the same advisory.
                blocked("vrm", "0.2.1", Some("0.2.3"), false),
            ],
            ..FoundPlugins::default()
        };
        let mut announced = BTreeSet::new();
        let fresh = newly_blocked(&found, &announced);
        assert_eq!(fresh.len(), 1);
        announced.extend(fresh.into_iter().map(block_key));
        assert!(newly_blocked(&found, &announced).is_empty());

        // Another version, or another advisory, is new.
        let mut later = found.clone();
        later
            .blocked
            .push(blocked("vrm", "0.2.2", Some("0.2.3"), false));
        let mut other = blocked("vrm", "0.2.1", None, false);
        other.advisory = "CELESTA-2026-0004".to_owned();
        later.blocked.push(other);
        let fresh: Vec<BlockKey> = newly_blocked(&later, &announced)
            .into_iter()
            .map(block_key)
            .collect();
        assert_eq!(
            fresh,
            [
                ("vrm".into(), "0.2.2".into(), "CELESTA-2026-0003".into()),
                ("vrm".into(), "0.2.1".into(), "CELESTA-2026-0004".into()),
            ]
        );
    }

    #[test]
    fn the_notification_names_the_advisory_and_the_fix() {
        let fixed = blocked("vrm", "0.2.1", Some("0.2.3"), false);
        assert_eq!(stopped_title(&fixed), "vrm 0.2.1 was stopped");
        assert_eq!(
            stopped_message(&fixed),
            "CELESTA-2026-0003: runs files outside the project. Fixed in 0.2.3."
        );
        let bundled = blocked("avfoundation", "0.10.0", Some("0.10.1"), true);
        assert_eq!(
            stopped_message(&bundled),
            "CELESTA-2026-0003: runs files outside the project. Update Celesta to get a fixed version."
        );
        let unfixed = blocked("vrm", "0.2.1", None, false);
        assert_eq!(
            stopped_message(&unfixed),
            "CELESTA-2026-0003: runs files outside the project."
        );
    }

    #[test]
    fn blocked_plugins_are_one_error() {
        let one = FoundPlugins {
            blocked: vec![blocked("vrm", "0.2.1", Some("0.2.3"), false)],
            ..FoundPlugins::default()
        };
        let problem = blocked_problem(&one).unwrap();
        assert_eq!(problem.title, "vrm 0.2.1 was stopped");
        assert_eq!(
            Source::Advisories.severity(),
            crate::problems::Severity::Error
        );

        let two = FoundPlugins {
            blocked: vec![
                blocked("vrm", "0.2.1", Some("0.2.3"), false),
                blocked("beep", "1.0.0", None, false),
            ],
            ..FoundPlugins::default()
        };
        let problem = blocked_problem(&two).unwrap();
        assert_eq!(problem.title, "2 plugins were stopped");
        assert_eq!(
            problem.detail,
            "vrm 0.2.1 — CELESTA-2026-0003: runs files outside the project. Fixed in 0.2.3.\n\
             beep 1.0.0 — CELESTA-2026-0003: runs files outside the project."
        );
        assert_eq!(blocked_problem(&FoundPlugins::default()), None);
    }

    #[test]
    fn a_blocked_plugin_updates_where_it_was_installed() {
        let stopped = blocked("vrm", "0.2.1", Some("0.2.3"), false);
        let locked = FoundPlugins {
            project: Some(PathBuf::from("/prism")),
            lock: Some(lock(&[("vrm", "0.2.1")])),
            ..FoundPlugins::default()
        };
        assert_eq!(
            blocked_update(&locked, &stopped),
            Some(BlockedUpdate::Locked(PathBuf::from("/prism")))
        );
        let global = FoundPlugins {
            registry_plugins: vec![RegistryPlugin {
                id: "vrm".to_owned(),
                version: version("0.2.1"),
                tree_hash: Hash([0; 32]),
            }],
            ..FoundPlugins::default()
        };
        assert_eq!(
            blocked_update(&global, &stopped),
            Some(BlockedUpdate::Global)
        );
        // No fixed version, a bundled copy, or neither: nothing to update.
        assert_eq!(
            blocked_update(&global, &blocked("vrm", "0.2.1", None, false)),
            None
        );
        assert_eq!(
            blocked_update(&global, &blocked("vrm", "0.2.1", Some("0.2.3"), true)),
            None
        );
        assert_eq!(blocked_update(&FoundPlugins::default(), &stopped), None);
    }
}
