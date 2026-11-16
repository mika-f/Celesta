//! The questions about a project's `celesta.lock`, asked once per project
//! per app run after it opens: locked versions this computer does not hold
//! (Download), and a lock resolved with another Celesta whose plugins
//! cannot load (Update Plugins). The sizes and the requirement rewrites
//! they show are fetched before the dialog opens, within
//! [`QUESTION_BUDGET`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use celesta_editor_theme::metrics;
use celesta_plugin_host::LockProblem;
use celesta_registry_client::{BundledPlugins, Rewrite, Store, holds_locked_build, plan_update};
use celesta_registry_format::Target;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogDescription, DialogFooter};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Entity, SharedString, Window, div};
use time::OffsetDateTime;

use super::client::EditorRegistry;
use super::state::{RegistryState, unreachable};
use crate::plugin_page::{format_size, registry_host};
use crate::plugins::FoundPlugins;
use crate::problems::{Problem, error_chain};
use crate::toast::Toast;
use crate::view::EditorView;

/// How long fetching a question's sizes and rewrites may hold it back.
const QUESTION_BUDGET: Duration = Duration::from_secs(3);

/// Wide enough for a plugin id, its version, and its size on one line.
/// Designed against a 16px `rem`; see [`metrics::scaled`].
const QUESTION_WIDTH: f32 = 480.;

/// What opening a project asks about its lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LockQuestion {
    /// Locked versions with a build for this computer that neither the
    /// store nor a bundled copy holds.
    Download {
        project: String,
        items: Vec<DownloadItem>,
    },
    /// The lock was resolved with another Celesta (`locked`, as
    /// `major.minor`), and `ids` cannot load with this one.
    OtherCelesta {
        project: String,
        locked: String,
        ids: Vec<String>,
    },
    /// The This project heading's Update Plugins, when requirements in
    /// `celesta.config.json` can no longer be met.
    Requirements { ids: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DownloadItem {
    pub id: String,
    pub version: String,
    /// The download's size, from the plugin's index; `None` when it could
    /// not be fetched in time.
    pub size: Option<u64>,
}

impl LockQuestion {
    pub(crate) fn title(&self) -> String {
        match self {
            Self::Download { project, items } => match items.len() {
                1 => format!("Download 1 plugin for “{project}”?"),
                count => format!("Download {count} plugins for “{project}”?"),
            },
            Self::OtherCelesta {
                project, locked, ..
            } => format!("“{project}” was locked with Celesta {locked}"),
            Self::Requirements { .. } => "Update this project’s plugins?".to_owned(),
        }
    }

    /// The plugins Update Plugins resolves again; none for Download.
    fn ids(&self) -> &[String] {
        match self {
            Self::Download { .. } => &[],
            Self::OtherCelesta { ids, .. } | Self::Requirements { ids } => ids,
        }
    }
}

/// The question the open project's lock raises, if any. Blocking: it reads
/// the store and hashes the bundled copies. `None` when nothing is missing
/// or the project is not a saved trusted one with `celesta.config.json`.
pub(crate) fn lock_question(
    found: &FoundPlugins,
    store: &Store,
    bundled: Option<&Path>,
    host: &Target,
) -> Option<LockQuestion> {
    let root = found.project.as_deref()?;
    let project = project_name(root);
    match &found.lock_problem {
        Some(LockProblem::OtherCelesta { locked, ids, .. }) => {
            return Some(LockQuestion::OtherCelesta {
                project,
                locked: format!("{}.{}", locked.major, locked.minor),
                ids: ids.clone(),
            });
        }
        // Without a lock that matches celesta.config.json there is nothing
        // to download; the heading offers Update Plugins.
        Some(LockProblem::Unsatisfied { .. }) | None => {}
        Some(_) => return None,
    }
    let lock = found.lock.as_ref()?;
    let bundled = BundledPlugins::new(bundled);
    let items: Vec<DownloadItem> = lock
        .plugins
        .iter()
        .filter(|(id, locked)| {
            // A version without a build here is not asked about: there is
            // nothing to download for this computer.
            lock.artifact(id, host).is_some_and(|artifact| {
                !holds_locked_build(store, &bundled, id, &locked.version, &artifact.tree_hash)
            })
        })
        .map(|(id, locked)| DownloadItem {
            id: id.clone(),
            version: locked.version.to_string(),
            size: None,
        })
        .collect();
    (!items.is_empty()).then_some(LockQuestion::Download { project, items })
}

/// `prism` for `/Users/me/prism`.
fn project_name(root: &Path) -> String {
    root.file_name().map_or_else(
        || root.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The other-Celesta question for `found`'s lock, without reading
/// anything.
fn other_celesta_question(found: &FoundPlugins) -> Option<LockQuestion> {
    let root = found.project.as_deref()?;
    match &found.lock_problem {
        Some(LockProblem::OtherCelesta { locked, ids, .. }) => Some(LockQuestion::OtherCelesta {
            project: project_name(root),
            locked: format!("{}.{}", locked.major, locked.minor),
            ids: ids.clone(),
        }),
        _ => None,
    }
}

/// What Update Plugins rewrites in `celesta.config.json`, as far as it was
/// found before the question opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// These rewrites, none when every id resolves within its requirement.
    Rewrites(Vec<Rewrite>),
    /// The registry did not answer in time; they are planned when Update
    /// Plugins runs.
    Unknown,
    /// An id cannot be updated at all, for this reason.
    Failed(String),
}

/// A question with what it shows fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Prepared {
    /// The project it is about.
    pub root: PathBuf,
    pub question: LockQuestion,
    /// Empty for Download.
    pub plan: Plan,
    /// Where the plugins come from: `registry.celesta.studio`.
    pub host: String,
}

/// Fetches what `question` shows: the download sizes, or the rewrites
/// Update Plugins makes. Blocking, for at most [`QUESTION_BUDGET`]; what
/// is not fetched by then is left unknown.
pub(crate) fn prepare(
    question: LockQuestion,
    found: &FoundPlugins,
    root: PathBuf,
    client: &Arc<EditorRegistry>,
    include_prereleases: bool,
) -> Prepared {
    let host = registry_host(&client.access().base).to_owned();
    match question {
        LockQuestion::Download { project, mut items } => {
            let wanted: Vec<(String, String)> = items
                .iter()
                .map(|item| (item.id.clone(), item.version.clone()))
                .collect();
            let client = client.clone();
            let sizes = within(QUESTION_BUDGET, move |sizes| {
                let Ok(registry) = client.registry_within(QUESTION_BUDGET) else {
                    return;
                };
                let now = OffsetDateTime::now_utc();
                for (id, version) in wanted {
                    let Ok(index) = registry.index(&id, now) else {
                        continue;
                    };
                    let size = index
                        .versions
                        .iter()
                        .find(|entry| entry.version.to_string() == version)
                        .and_then(|entry| {
                            entry
                                .targets
                                .get(client.host())
                                .or_else(|| entry.targets.get(&Target::ANY))
                        })
                        .map(|artifact| artifact.size);
                    if sizes.send((id, size)).is_err() {
                        return;
                    }
                }
            });
            for (id, size) in sizes {
                if let Some(item) = items.iter_mut().find(|item| item.id == id) {
                    item.size = size;
                }
            }
            Prepared {
                root,
                question: LockQuestion::Download { project, items },
                plan: Plan::Rewrites(Vec::new()),
                host,
            }
        }
        question => {
            let ids = declared(question.ids(), found);
            let client = client.clone();
            let project = root.clone();
            let planned = within(QUESTION_BUDGET, move |plan| {
                let planned = (|| -> anyhow::Result<Vec<Rewrite>> {
                    let registry = client.registry_within(QUESTION_BUDGET)?;
                    let env = client.environment(&registry, None);
                    Ok(plan_update(&env, &project, &ids, include_prereleases)?)
                })();
                let _ = plan.send(planned);
            });
            let plan = match planned.into_iter().next() {
                None => Plan::Unknown,
                Some(Ok(rewrites)) => Plan::Rewrites(rewrites),
                Some(Err(error)) if unreachable(&error) => Plan::Unknown,
                Some(Err(error)) => Plan::Failed(error_chain(error.as_ref())),
            };
            Prepared {
                root,
                question,
                plan,
                host,
            }
        }
    }
}

/// The ids of `ids` that `celesta.config.json` declares: a locked plugin
/// it no longer names has no requirement to plan for.
fn declared(ids: &[String], found: &FoundPlugins) -> BTreeSet<String> {
    ids.iter()
        .filter(|id| found.dependencies.contains_key(*id))
        .cloned()
        .collect()
}

/// What `work` sends before `budget` passes. `work` runs on a thread of
/// its own, which is left to finish once the budget has passed.
fn within<T: Send + 'static>(
    budget: Duration,
    work: impl FnOnce(mpsc::Sender<T>) + Send + 'static,
) -> Vec<T> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || work(sender));
    let deadline = Instant::now() + budget;
    let mut received = Vec::new();
    while let Ok(value) = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        received.push(value);
    }
    received
}

/// Asks `prepared` unless its project was asked about already in this app
/// run or another dialog is up; `false` when it did not ask. A question
/// held back by another dialog is asked the next time the project opens.
pub(crate) fn ask_once(
    asked: &mut BTreeSet<PathBuf>,
    prepared: Prepared,
    registry: Entity<RegistryState>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if asked.contains(&prepared.root) || window.has_active_dialog(cx) {
        return false;
    }
    asked.insert(prepared.root.clone());
    open_question(prepared, registry, window, cx);
    true
}

/// Starts `start` on `registry`, saying so when it refuses (another change
/// to the project is running).
fn start_work(
    registry: &Entity<RegistryState>,
    window: &mut Window,
    cx: &mut App,
    start: impl FnOnce(&mut RegistryState, &mut Context<RegistryState>) -> bool,
) {
    if !registry.update(cx, start) {
        window.push_notification(
            Toast::note(
                "Another plugin change is running. Try again when it finishes.",
                cx,
            ),
            cx,
        );
    }
}

/// Opens the dialog for `prepared`: **Not Now** leaves the project as it
/// is; **Download** or **Update Plugins** starts the registry work.
fn open_question(
    prepared: Prepared,
    registry: Entity<RegistryState>,
    window: &mut Window,
    cx: &mut App,
) {
    let prepared = Arc::new(prepared);
    window.open_dialog(cx, move |dialog, _, cx| {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let mono = theme.mono_font_family.clone();
        let hairline = theme.border;
        let danger = theme.danger;
        let (body, action, blocked): (AnyElement, &'static str, Option<String>) =
            match &prepared.question {
                LockQuestion::Download { items, .. } => {
                    let row = |id: SharedString, version: SharedString, size: SharedString| {
                        h_flex()
                            .py_1()
                            .gap_3()
                            .border_t_1()
                            .border_color(hairline)
                            .child(div().flex_1().min_w_0().child(id))
                            .child(
                                div()
                                    .w(gpui_kit::rems(8.))
                                    .font_family(mono.clone())
                                    .child(version),
                            )
                            .child(
                                div()
                                    .w(gpui_kit::rems(5.))
                                    .flex()
                                    .justify_end()
                                    .font_family(mono.clone())
                                    .text_color(muted)
                                    .child(size),
                            )
                    };
                    let total = items
                        .iter()
                        .map(|item| item.size)
                        .sum::<Option<u64>>()
                        .map_or_else(|| "—".to_owned(), format_size);
                    let rows = items.iter().map(|item| {
                        row(
                            item.id.clone().into(),
                            item.version.clone().into(),
                            item.size.map_or_else(|| "—".to_owned(), format_size).into(),
                        )
                    });
                    let body =
                        v_flex()
                            .gap_3()
                            .child(DialogDescription::new().child(format!(
                                "This project’s celesta.lock names plugins that are not on this \
                             computer. They come from {}.",
                                prepared.host
                            )))
                            .child(v_flex().text_sm().children(rows).child(
                                row("Total".into(), "".into(), total.into()).text_color(muted),
                            ))
                            .into_any_element();
                    (body, "Download", None)
                }
                question => {
                    let description = match question {
                        LockQuestion::Requirements { .. } => {
                            "Some requirements in celesta.config.json can no longer be met."
                        }
                        _ => {
                            "These plugins can’t load with this Celesta. Update Plugins \
                              resolves them again and rewrites celesta.lock."
                        }
                    };
                    let ids = question.ids().iter().map(|id| div().child(id.clone()));
                    let (rewrites, blocked) = match &prepared.plan {
                        Plan::Rewrites(rewrites) => (rewrites.as_slice(), None),
                        Plan::Unknown => (&[][..], None),
                        Plan::Failed(reason) => (&[][..], Some(reason.clone())),
                    };
                    let rewrites = rewrites.iter().map(|rewrite| {
                        div().text_color(muted).child(format!(
                            "Changes {} in celesta.config.json from {} to {}",
                            rewrite.id, rewrite.from, rewrite.to
                        ))
                    });
                    let body = v_flex()
                        .gap_3()
                        .child(DialogDescription::new().child(description))
                        .child(v_flex().gap_1().text_sm().children(ids))
                        .child(v_flex().gap_1().text_sm().children(rewrites))
                        .children(
                            blocked
                                .clone()
                                .map(|reason| div().text_sm().text_color(danger).child(reason)),
                        )
                        .into_any_element();
                    (body, "Update Plugins", blocked)
                }
            };
        let prepared = prepared.clone();
        let registry = registry.clone();
        let can_run = blocked.is_none();
        dialog
            .title(SharedString::from(prepared.question.title()))
            .w(metrics::scaled_px(QUESTION_WIDTH))
            .child(body)
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(
                            Button::new("lock-question-later")
                                .label("Not Now")
                                .outline(),
                        ),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new("lock-question-run")
                                .label(action)
                                .primary()
                                .disabled(!can_run),
                        ),
                    ),
            )
            .on_ok(move |_, window, cx| {
                if !can_run {
                    return true;
                }
                let root = prepared.root.clone();
                let ids: BTreeSet<String> = prepared.question.ids().iter().cloned().collect();
                match (&prepared.question, &prepared.plan) {
                    (LockQuestion::Download { .. }, _) => {
                        start_work(&registry, window, cx, |state, cx| {
                            state.download_locked(root, cx)
                        });
                    }
                    (_, Plan::Rewrites(rewrites)) => {
                        let rewrites = rewrites.clone();
                        start_work(&registry, window, cx, |state, cx| {
                            state.update(root, ids, rewrites, cx)
                        });
                    }
                    (_, _) => {
                        start_work(&registry, window, cx, |state, cx| {
                            state.update_planned(root, ids, cx)
                        });
                    }
                }
                true
            })
    });
}

impl EditorView {
    /// Asks about the open project's lock, once per project per app run:
    /// the question is worked out on the background executor and asked
    /// only while that project is still open and no other dialog is up.
    pub(crate) fn ask_about_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.found_plugins.project.clone() else {
            return;
        };
        let Some(registry) = &self.registry else {
            return;
        };
        let state = registry.read(cx);
        if self.asked_locks.contains(&root) || state.project_busy(&root) {
            return;
        }
        let client = state.client().clone();
        let include_prereleases = state.include_prereleases;
        let found = self.found_plugins.clone();
        cx.spawn_in(window, async move |view, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move {
                    let question =
                        lock_question(&found, client.store(), client.bundled(), client.host())?;
                    Some(prepare(
                        question,
                        &found,
                        root,
                        &client,
                        include_prereleases,
                    ))
                })
                .await;
            let Some(prepared) = prepared else {
                return;
            };
            view.update_in(cx, |this, window, cx| {
                if this.found_plugins.project.as_ref() != Some(&prepared.root) {
                    return;
                }
                let Some(registry) = this.registry.clone() else {
                    return;
                };
                if ask_once(&mut this.asked_locks, prepared, registry, window, cx) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The This project heading's Update Plugins: resolves the plugins the
    /// lock misses or cannot load again. When requirements in
    /// `celesta.config.json` have to change for that, or an id cannot be
    /// updated, a question says so first.
    pub(crate) fn update_project_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let found = &self.found_plugins;
        let Some(root) = found.project.clone() else {
            return;
        };
        let Some(registry) = self.registry.clone() else {
            return;
        };
        let ids: Vec<String> = match &found.lock_problem {
            Some(LockProblem::OutOfDate { ids } | LockProblem::OtherCelesta { ids, .. }) => {
                ids.clone()
            }
            _ => found.dependencies.keys().cloned().collect(),
        };
        let question = other_celesta_question(found)
            .unwrap_or_else(|| LockQuestion::Requirements { ids: ids.clone() });
        let state = registry.read(cx);
        let client = state.client().clone();
        let include_prereleases = state.include_prereleases;
        let found = found.clone();
        cx.spawn_in(window, async move |view, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { prepare(question, &found, root, &client, include_prereleases) })
                .await;
            view.update_in(cx, |this, window, cx| {
                if this.found_plugins.project.as_ref() != Some(&prepared.root) {
                    return;
                }
                let root = prepared.root.clone();
                let ids: BTreeSet<String> = ids.into_iter().collect();
                match (&prepared.question, &prepared.plan) {
                    (LockQuestion::Requirements { .. }, Plan::Rewrites(rewrites))
                        if rewrites.is_empty() =>
                    {
                        this.registry_work(window, cx, |state, cx| {
                            state.update(root, ids, Vec::new(), cx)
                        });
                    }
                    (LockQuestion::Requirements { .. }, Plan::Unknown) => {
                        this.registry_work(window, cx, |state, cx| {
                            state.update_planned(root, ids, cx)
                        });
                    }
                    _ => open_question(prepared, registry, window, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// The Problems entry for requirements an update rewrote without a
/// question listing them first.
pub(crate) fn rewrite_note(rewrites: &[Rewrite]) -> Problem {
    Problem::new(
        "Changed requirements in celesta.config.json",
        rewrites
            .iter()
            .map(|rewrite| format!("{} from {} to {}", rewrite.id, rewrite.from, rewrite.to))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::scratch_dir;
    use celesta_plugin_host::{LockedArtifact, LockedPlugin, Lockfile};
    use celesta_registry_client::testing::{TestRegistry, plugin_files, tree_of, write_files};
    use celesta_registry_format::Hash;
    use gpui_kit::component::Root;
    use gpui_kit::{TestAppContext, VisualTestContext, px, size};
    use semver::Version;
    use std::collections::BTreeMap;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    /// A folder that is removed when the test ends.
    struct Folder(PathBuf);

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A lock naming each `(id, version, targets)`, every build with the
    /// tree hash of the test registry's files for it.
    fn lock(plugins: &[(&str, &str, &[&Target])]) -> Lockfile {
        Lockfile {
            celesta: version(celesta_plugin_host::CELESTA_VERSION),
            plugins: plugins
                .iter()
                .map(|(id, at, targets)| {
                    let targets = targets
                        .iter()
                        .map(|target| {
                            let files = plugin_files(id, at, target.as_str());
                            let artifact = LockedArtifact {
                                sha256: Hash([1; 32]),
                                tree_hash: tree_of(&files),
                            };
                            ((*target).clone(), artifact)
                        })
                        .collect();
                    let locked = LockedPlugin {
                        celesta: semver::VersionReq::STAR,
                        registry: "https://registry.test".into(),
                        targets,
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

    fn found(root: &Path, lock: Lockfile) -> FoundPlugins {
        FoundPlugins {
            project: Some(root.to_path_buf()),
            lock: Some(lock),
            ..FoundPlugins::default()
        }
    }

    fn ids(question: &LockQuestion) -> Vec<(&str, &str)> {
        match question {
            LockQuestion::Download { items, .. } => items
                .iter()
                .map(|item| (item.id.as_str(), item.version.as_str()))
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_missing_locked_version_is_asked_about() {
        let user = Folder(scratch_dir("lock-question"));
        let store = Store::new(user.0.join("store"));
        let host = Target::host();
        let root = user.0.join("prism");
        let found = found(
            &root,
            lock(&[
                ("vrm", "0.2.3", &[&host]),
                ("voicevox", "0.9.0", &[&Target::ANY]),
                ("held", "1.0.0", &[&host]),
            ]),
        );
        // The store holds one of them already.
        write_files(
            &store.path("held", &version("1.0.0")),
            &plugin_files("held", "1.0.0", host.as_str()),
        );
        let question = lock_question(&found, &store, None, &host).expect("a question");
        assert_eq!(ids(&question), [("voicevox", "0.9.0"), ("vrm", "0.2.3")]);
        assert_eq!(question.title(), "Download 2 plugins for “prism”?");

        let one = lock_question(
            &self::found(&root, lock(&[("vrm", "0.2.3", &[&host])])),
            &store,
            None,
            &host,
        )
        .unwrap();
        assert_eq!(one.title(), "Download 1 plugin for “prism”?");

        // Nothing missing, or no saved trusted project: no question.
        let held = self::found(&root, lock(&[("held", "1.0.0", &[&host])]));
        assert_eq!(lock_question(&held, &store, None, &host), None);
        let untrusted = FoundPlugins {
            project: None,
            ..self::found(&root, lock(&[("vrm", "0.2.3", &[&host])]))
        };
        assert_eq!(lock_question(&untrusted, &store, None, &host), None);
    }

    #[test]
    fn a_locked_version_without_a_host_build_is_not_asked_about() {
        let user = Folder(scratch_dir("lock-question"));
        let store = Store::new(user.0.join("store"));
        let host = Target::host();
        let elsewhere = if host.as_str() == "windows-x86_64" {
            Target::parse("linux-x86_64").unwrap()
        } else {
            Target::parse("windows-x86_64").unwrap()
        };
        let found = found(
            &user.0.join("prism"),
            lock(&[
                ("vrm", "0.2.3", &[&elsewhere]),
                ("voicevox", "0.9.0", &[&host]),
            ]),
        );
        let question = lock_question(&found, &store, None, &host).unwrap();
        assert_eq!(ids(&question), [("voicevox", "0.9.0")]);

        let only_elsewhere = self::found(
            &user.0.join("prism"),
            lock(&[("vrm", "0.2.3", &[&elsewhere])]),
        );
        assert_eq!(lock_question(&only_elsewhere, &store, None, &host), None);
    }

    #[test]
    fn a_bundled_copy_satisfies_the_lock() {
        let user = Folder(scratch_dir("lock-question"));
        let store = Store::new(user.0.join("store"));
        let host = Target::host();
        let bundled = user.0.join("bundled");
        write_files(
            &bundled.join("avfoundation"),
            &plugin_files("avfoundation", "0.10.0", host.as_str()),
        );
        let found = found(
            &user.0.join("prism"),
            lock(&[("avfoundation", "0.10.0", &[&host])]),
        );
        assert_eq!(lock_question(&found, &store, Some(&bundled), &host), None);
        assert!(lock_question(&found, &store, None, &host).is_some());
    }

    #[test]
    fn another_celesta_minor_asks_to_update() {
        let user = Folder(scratch_dir("lock-question"));
        let store = Store::new(user.0.join("store"));
        let host = Target::host();
        let mut found = found(&user.0.join("prism"), lock(&[("vrm", "0.2.3", &[&host])]));
        found.lock_problem = Some(LockProblem::OtherCelesta {
            locked: version("0.9.2"),
            running: version("0.10.0"),
            ids: vec!["vrm".to_owned()],
        });
        let question = lock_question(&found, &store, None, &host).unwrap();
        assert_eq!(
            question,
            LockQuestion::OtherCelesta {
                project: "prism".to_owned(),
                locked: "0.9".to_owned(),
                ids: vec!["vrm".to_owned()],
            }
        );
        assert_eq!(question.title(), "“prism” was locked with Celesta 0.9");

        // A lock that does not match celesta.config.json asks nothing: the
        // heading offers Update Plugins.
        found.lock_problem = Some(LockProblem::OutOfDate {
            ids: vec!["vrm".to_owned()],
        });
        assert_eq!(lock_question(&found, &store, None, &host), None);
    }

    #[test]
    fn the_plan_lists_the_rewrites_before_the_question_opens() {
        let served = TestRegistry::file();
        served.publish("acme.charts", "0.11.0", &["any"]);
        let user = Folder(scratch_dir("lock-question"));
        let root = user.0.join("prism");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(celesta_plugin_host::PROJECT_CONFIG),
            r#"{ "plugins": { "dependencies": { "acme.charts": "^0.10.0" } } }"#,
        )
        .unwrap();
        let client = Arc::new(EditorRegistry::for_test(&served, &user.0));
        let found = FoundPlugins {
            project: Some(root.clone()),
            dependencies: BTreeMap::from([("acme.charts".to_owned(), "^0.10.0".parse().unwrap())]),
            ..FoundPlugins::default()
        };
        let question = LockQuestion::Requirements {
            ids: vec!["acme.charts".to_owned(), "orphan".to_owned()],
        };
        let prepared = prepare(question, &found, root, &client, false);
        assert_eq!(
            prepared.plan,
            Plan::Rewrites(vec![Rewrite {
                id: "acme.charts".to_owned(),
                from: "^0.10.0".to_owned(),
                to: "^0.11.0".to_owned(),
            }])
        );
    }

    #[test]
    fn download_sizes_come_from_the_indexes() {
        let served = TestRegistry::file();
        served.publish("acme.charts", "1.0.0", &["any"]);
        let user = Folder(scratch_dir("lock-question"));
        let client = Arc::new(EditorRegistry::for_test(&served, &user.0));
        let question = LockQuestion::Download {
            project: "prism".to_owned(),
            items: vec![
                DownloadItem {
                    id: "acme.charts".to_owned(),
                    version: "1.0.0".to_owned(),
                    size: None,
                },
                DownloadItem {
                    id: "acme.gone".to_owned(),
                    version: "1.0.0".to_owned(),
                    size: None,
                },
            ],
        };
        let prepared = prepare(
            question,
            &FoundPlugins::default(),
            user.0.join("prism"),
            &client,
            false,
        );
        let LockQuestion::Download { items, .. } = prepared.question else {
            panic!("a download question");
        };
        assert!(items[0].size.is_some_and(|size| size > 0), "{items:?}");
        assert_eq!(items[1].size, None, "not in the registry");
    }

    #[gpui_kit::test]
    fn the_question_is_asked_once_per_project(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            celesta_editor_theme::init(cx);
        });
        let served = TestRegistry::file();
        let user = Folder(scratch_dir("lock-question"));
        let client = Arc::new(EditorRegistry::for_test(&served, &user.0));
        let registry = cx.new(|_| RegistryState::new(client));
        let handle = cx.open_window(size(px(1000.), px(800.)), |window, cx| {
            let view = cx.new(|_| Empty);
            Root::new(view, window, cx)
        });
        let mut cx = VisualTestContext::from_window(handle.into(), cx);
        let root = user.0.join("prism");
        let prepared = Prepared {
            root: root.clone(),
            question: LockQuestion::Download {
                project: "prism".to_owned(),
                items: vec![DownloadItem {
                    id: "vrm".to_owned(),
                    version: "0.2.3".to_owned(),
                    size: Some(4_800_000),
                }],
            },
            plan: Plan::Rewrites(Vec::new()),
            host: "registry.celesta.studio".to_owned(),
        };
        let mut asked = BTreeSet::new();
        let first = cx.update(|window, cx| {
            ask_once(&mut asked, prepared.clone(), registry.clone(), window, cx)
        });
        cx.run_until_parked();
        assert!(first);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));

        // Not Now, then the project opens again: no second question.
        cx.update(|window, cx| window.close_dialog(cx));
        cx.run_until_parked();
        let again = cx.update(|window, cx| {
            ask_once(&mut asked, prepared.clone(), registry.clone(), window, cx)
        });
        assert!(!again);
        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

        // Another project is asked about; one held back by another dialog
        // is not marked asked.
        let other = Prepared {
            root: user.0.join("other"),
            ..prepared
        };
        cx.update(|window, cx| {
            window.open_dialog(cx, |dialog, _, _| dialog.title("Allow this plugin to run?"))
        });
        cx.run_until_parked();
        let held_back = cx
            .update(|window, cx| ask_once(&mut asked, other.clone(), registry.clone(), window, cx));
        assert!(!held_back);
        assert!(!asked.contains(&other.root));
        cx.update(|window, cx| window.close_dialog(cx));
        cx.run_until_parked();
        let asked_later = cx
            .update(|window, cx| ask_once(&mut asked, other.clone(), registry.clone(), window, cx));
        assert!(asked_later);
    }

    struct Empty;

    impl Render for Empty {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }
}
