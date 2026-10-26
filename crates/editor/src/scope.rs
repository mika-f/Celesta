//! What an edit of a shared element writes: for **this scene**, at the
//! scene's call `S` of the component `C` the element `E` is written in
//! (lifting a literal into a new prop of `C` when needed), or the **shared
//! value** every scene uses unless it sets its own. The choice and its
//! memory per element, the options the occurrence index and the scene
//! calls allow, and [`scoped_row`], the one per-row rule the rows (shown
//! value, locks, removal) and the commit read; [`commit_gate`] decides
//! whether a commit goes, waits for the index, or is refused.

use crate::element_edit::{ElementTargets, PropKey, WindowTargets, editable, reason_text};
use crate::element_rows::removable;
use crate::impact::ImpactFilter;
use crate::occurrences::OccurrenceState;
use celesta_react_bridge::{
    ElementKind, ElementOccurrences, InstanceOccurrences, LayerSourceReport, OccurrenceIndex,
    ReactChapter, SourcePos, outermost_chapter_at, scenes_of,
};
use celesta_source_edit::{
    DefaultTarget, EditTarget, EditValue, LiftLock, LockReason, element_at, written_attributes,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Where an edit of a shared element writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Scope {
    /// At `S`, for the instances in the current scene only.
    ThisScene,
    /// The value every scene uses unless it sets its own.
    Shared,
}

/// Why a scope, or a row under it, can't be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScopeLock {
    /// No call of `C` is found in the chain.
    NoCall { component: String },
    /// A component between the scene and `S`.
    PassesThrough { component: String },
    /// The scenes show one chain through `S` (`.map()` or `<Scenes>`).
    ReusedCall,
    /// `S` makes instances in other scenes too.
    OtherScenes { component: String },
    /// This scene calls `C` more than once, and `S` draws the element more
    /// than once in it: a write at `S` would change every copy it draws.
    DrawsMoreThanOnce { component: String },
    /// `C`'s first parameter is not an object pattern.
    NotObjectPattern { component: String },
    /// `C`'s props type is imported.
    ImportedPropsType { component: String },
    /// `C`'s props type can't take the field `param`.
    PropsTypeNotEditable { component: String, param: String },
    /// A keyframe, a `style` property, or a literal input.
    PerSceneUnsupported,
    /// `param` has no default and every call passes it.
    EverySceneSets { param: String },
    /// This scene's call passes `param`, so a shared edit would not show
    /// here: under "Shared value" the row, a drag, a scrub and an inline
    /// text edit of it are locked until that value is removed at `S`.
    SetsOwn { param: String },
    /// `param`'s default is written as an expression.
    DefaultIsExpression { param: String },
    /// No pattern key in `C` holds `param`'s default.
    DefaultNotFound { param: String },
    /// The write goes to a file outside the project: `C`'s, or `S`'s.
    OutsideProject,
}

impl ScopeLock {
    /// The reason shown under the control, or on the row.
    pub(crate) fn text(&self) -> String {
        match self {
            Self::NoCall { component } => reason_text(&LockReason::CallSiteNotFound {
                component: component.clone(),
            }),
            Self::PassesThrough { component } => format!("Passes through `<{component}>`"),
            Self::ReusedCall => {
                "The scenes reuse the same call (`.map()` or `defineScenes`)".to_owned()
            }
            Self::OtherScenes { component } => {
                format!("`<{component}>` is called here for other scenes too")
            }
            Self::DrawsMoreThanOnce { component } => {
                format!("`<{component}>` here draws this element more than once")
            }
            Self::NotObjectPattern { component } => {
                format!("`{component}` doesn't take its props as `{{ … }}`")
            }
            Self::ImportedPropsType { component } => {
                format!("`{component}`'s props type is in another file")
            }
            Self::PropsTypeNotEditable { component, param } => {
                format!("Can't add `{param}` to `{component}`'s props type")
            }
            Self::PerSceneUnsupported => "Can't be set per scene yet".to_owned(),
            Self::EverySceneSets { param } => format!("Every scene sets `{param}` itself"),
            Self::SetsOwn { param } => format!("This scene sets its own `{param}`"),
            Self::DefaultIsExpression { param } => {
                format!("`{param}`'s shared value is written as an expression")
            }
            Self::DefaultNotFound { param } => {
                format!("Can't find where `{param}`'s shared value is written")
            }
            Self::OutsideProject => reason_text(&LockReason::OutsideProject),
        }
    }
}

/// The lock a failed lift puts on "This scene", for `E` in `component`.
pub(crate) fn lift_scope_lock(lock: &LiftLock, component: &str) -> ScopeLock {
    match lock {
        LiftLock::NoComponent => ScopeLock::NoCall {
            component: component.to_owned(),
        },
        LiftLock::NotObjectPattern { component } => ScopeLock::NotObjectPattern {
            component: component.clone(),
        },
        LiftLock::ImportedPropsType { component } => ScopeLock::ImportedPropsType {
            component: component.clone(),
        },
        LiftLock::PropsTypeNotEditable { component, param } => ScopeLock::PropsTypeNotEditable {
            component: component.clone(),
            param: param.clone(),
        },
        LiftLock::NotALiteral => ScopeLock::PerSceneUnsupported,
    }
}

/// Each instance's call of C (instance order) and the props it writes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SceneCalls {
    /// The element the calls were read for.
    pub(crate) element: SourcePos,
    /// The occurrence index's generation they were read from.
    pub(crate) generation: u64,
    /// One per instance of the element: its call of `C` and the attributes
    /// written there, `None` when its chain has no entry there or the call
    /// can't be read.
    pub(crate) calls: Vec<Option<(SourcePos, Vec<String>)>>,
}

/// The chain entry of each instance of `element` (in instance order) at
/// the index `call` has in the chain of the instance `report` describes;
/// `None` for an instance whose chain is shorter, and for every instance
/// when `call` is not in that chain.
pub(crate) fn calls_of(
    report: &LayerSourceReport,
    element: &ElementOccurrences,
    call: &SourcePos,
) -> Vec<Option<SourcePos>> {
    let at = call_index(report, call);
    element
        .instances
        .iter()
        .map(|instance| at.and_then(|at| instance.chain.get(at)).cloned())
        .collect()
}

/// Where `call` is in the chain of `report` as the index writes chains:
/// without the wrappers.
fn call_index(report: &LayerSourceReport, call: &SourcePos) -> Option<usize> {
    report
        .chain
        .iter()
        .filter(|entry| entry.kind != ElementKind::Wrapper)
        .position(|entry| SourcePos::from(entry) == *call)
}

/// The scene calls of `element` at the occurrence index's `generation`:
/// each of `calls` (see [`calls_of`]) with the attributes written there,
/// reading the files with `read`. A call whose file can't be read, or
/// whose element is not found there, is `None`.
pub(crate) fn read_scene_calls(
    read: impl Fn(&Path) -> Result<String, String>,
    element: SourcePos,
    generation: u64,
    calls: Vec<Option<SourcePos>>,
) -> SceneCalls {
    let mut texts: HashMap<&Path, Option<String>> = HashMap::new();
    let calls = calls
        .iter()
        .map(|call| {
            let call = call.as_ref()?;
            let text = texts
                .entry(call.file.as_path())
                .or_insert_with(|| read(&call.file).ok())
                .as_deref()?;
            let at = element_at(text, call.line, call.column)?;
            Some((call.clone(), written_attributes(text, &at)))
        })
        .collect();
    SceneCalls {
        element,
        generation,
        calls,
    }
}

/// `calls` when they still describe the scenes: read at the occurrence
/// index's `generation`, for `element`, and at each instance's entry at
/// `call`'s place in the chain of `report` (see [`calls_of`]).
pub(crate) fn current_calls<'c>(
    calls: Option<&'c SceneCalls>,
    generation: u64,
    report: &LayerSourceReport,
    element: &ElementOccurrences,
    call: &SourcePos,
) -> Option<&'c SceneCalls> {
    let calls =
        calls.filter(|calls| calls.generation == generation && calls.element == element.element)?;
    let at = calls_of(report, element, call);
    let same = at.len() == calls.calls.len()
        && at
            .iter()
            .zip(&calls.calls)
            .all(|(at, read)| match (at, read) {
                (Some(at), Some((read, _))) => at == read,
                // An entry whose call could not be read.
                (Some(_), None) | (None, None) => true,
                (None, Some(_)) => false,
            });
    same.then_some(calls)
}

/// What the scope control offers for a shared element.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScopeOptions {
    pub(crate) this_scene: Result<(), ScopeLock>,
    pub(crate) shared: Result<(), ScopeLock>,
    /// The scenes (names, in chapter order) whose calls use the shared
    /// value.
    pub(crate) shared_reaches: Vec<String>,
    /// The scenes whose calls set their own value for a parameter the
    /// element's rows read.
    pub(crate) sets_own: Vec<String>,
    /// When the current scene calls `C` more than once: "This scene" then
    /// writes the selected instance's call `S` only, the other calls
    /// keeping their values. `None` without chapters.
    pub(crate) called_here: Option<CalledHere>,
    /// Whether `S` draws the selected instance alone in the current scene:
    /// "This scene" then edits the element as that one instance, though
    /// the frame renders it more than once (see [`targets_in_effect`]).
    pub(crate) drawn_alone: bool,
}

/// The current scene's calls of `C`, when it calls `C` more than once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CalledHere {
    pub(crate) component: String,
    /// How many calls of `C` draw the element in the current scene.
    pub(crate) times: usize,
}

impl ScopeOptions {
    fn option(&self, scope: Scope) -> &Result<(), ScopeLock> {
        match scope {
            Scope::ThisScene => &self.this_scene,
            Scope::Shared => &self.shared,
        }
    }
}

/// Whether the selected element offers a scope.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScopeState {
    /// One instance, or not written in a called component: design 1's
    /// impact line, no control.
    NotShared,
    /// The occurrence index can't answer yet (or failed): `note` says why.
    Unknown {
        note: String,
    },
    Options(ScopeOptions),
}

const SCANNING: &str = "Scanning scenes…";
const NOT_SCANNED: &str = "The scenes are not scanned";

/// The scope options of editing the element `report` describes, whose
/// targets are `targets`, with the playhead at `frame`. The current scene
/// is the outermost chapter at `frame`, as the impact line reads it.
/// "This scene" is locked, in this order, when no call `S` is found, a
/// component sits between the scene and `S`, the scenes reuse one chain
/// through `S`, `S` makes instances in other scenes, this scene calls `C`
/// more than once and `S` draws the element more than once, or no row can
/// be written for this scene (as the one instance `S` draws, when it draws
/// it alone); "Shared value" when no row can be written for it. `calls`,
/// when read for this element and index, names the scenes that set their
/// own value.
pub(crate) fn scope_state(
    report: &LayerSourceReport,
    targets: &ElementTargets,
    occurrences: Option<&OccurrenceState>,
    calls: Option<&SceneCalls>,
    chapters: &[ReactChapter],
    frame: u64,
) -> ScopeState {
    let Some(component) = targets.enclosing.as_deref() else {
        return ScopeState::NotShared;
    };
    let (index, refreshing) = match scanned(occurrences) {
        Ok(scanned) => scanned,
        Err(state) => return state,
    };
    let Some((element, _)) = index.instance_for(report) else {
        return not_found(refreshing);
    };
    let shared = element.instances.len() > 1;
    options_over(
        report, targets, component, element, shared, calls, chapters, frame,
    )
}

/// The index in use and whether it is refreshing, or the state while it
/// can't answer.
fn scanned(occurrences: Option<&OccurrenceState>) -> Result<(&OccurrenceIndex, bool), ScopeState> {
    let unknown = |note: &str| ScopeState::Unknown {
        note: note.to_owned(),
    };
    match occurrences {
        // No sweep runs (Select layers is off): none would answer.
        None => Err(unknown(NOT_SCANNED)),
        Some(OccurrenceState::Scanning) => Err(unknown(SCANNING)),
        Some(OccurrenceState::Failed(message)) => {
            Err(unknown(&format!("Couldn't scan scenes: {message}")))
        }
        Some(OccurrenceState::Ready(index)) => Ok((index, false)),
        Some(OccurrenceState::Refreshing(index)) => Ok((index, true)),
    }
}

/// The state when the index has nothing for what is edited: a refreshing
/// index misses an element a lift moved, which the sweep under way finds.
fn not_found(refreshing: bool) -> ScopeState {
    ScopeState::Unknown {
        note: if refreshing {
            SCANNING
        } else {
            "Not found in the scene scan"
        }
        .to_owned(),
    }
}

/// The scope state of writing the timing window `window` around the
/// element `report` describes, with the playhead at `frame`, and the scene
/// calls that count for it. The window's instances are those of any
/// element the index renders through it; it is shared when one of them
/// comes through another call of the component it is written in (its
/// chain doesn't end as the selected one's does above the window), so
/// one written once shares nothing with the element's other instances, and
/// one in a component that several scenes call around other elements is
/// shared. `calls`, read for the element's instances at the window's call
/// (see [`calls_of`]), are carried to each window instance whose chain
/// ends as an element instance's does from that call.
pub(crate) fn window_scope_state(
    report: &LayerSourceReport,
    window: &WindowTargets,
    occurrences: Option<&OccurrenceState>,
    calls: Option<&SceneCalls>,
    chapters: &[ReactChapter],
    frame: u64,
) -> (ScopeState, Option<SceneCalls>) {
    let targets = &window.targets;
    let Some(component) = targets.enclosing.as_deref() else {
        return (ScopeState::NotShared, None);
    };
    let (index, refreshing) = match scanned(occurrences) {
        Ok(scanned) => scanned,
        Err(state) => return (state, None),
    };
    let instances: Vec<InstanceOccurrences> = index
        .elements
        .iter()
        .flat_map(|element| &element.instances)
        .filter(|instance| instance.wrappers.contains(&window.source))
        .cloned()
        .collect();
    let above = report
        .chain
        .iter()
        .position(|entry| SourcePos::from(entry) == window.source);
    let (Some(above), false) = (above, instances.is_empty()) else {
        return (not_found(refreshing), None);
    };
    // The selected instance's chain above the window (the index's chains
    // leave the wrappers out).
    let tail: Vec<SourcePos> = report.chain[above..]
        .iter()
        .filter(|entry| entry.kind != ElementKind::Wrapper)
        .map(SourcePos::from)
        .collect();
    let shared = instances
        .iter()
        .any(|instance| !instance.chain.ends_with(&tail));
    let element = ElementOccurrences {
        element: window.source.clone(),
        instances,
    };
    let calls = calls.and_then(|calls| window_calls(report, index, &element, targets, calls));
    let state = options_over(
        report,
        targets,
        component,
        &element,
        shared,
        calls.as_ref(),
        chapters,
        frame,
    );
    (state, calls)
}

/// `calls`, read for the instances of the element `report` describes, for
/// the instances of `window` (see [`window_scope_state`]); `None` when they
/// are not for that element.
fn window_calls(
    report: &LayerSourceReport,
    index: &OccurrenceIndex,
    window: &ElementOccurrences,
    targets: &ElementTargets,
    calls: &SceneCalls,
) -> Option<SceneCalls> {
    let (element, _) = index.instance_for(report)?;
    if calls.element != element.element || calls.calls.len() != element.instances.len() {
        return None;
    }
    let at = targets
        .component
        .as_ref()
        .and_then(|component| call_index(report, &component.call_site));
    let mapped = window
        .instances
        .iter()
        .map(|instance| {
            let at = at?;
            element
                .instances
                .iter()
                .zip(&calls.calls)
                .find(|(through, _)| {
                    through.wrappers.contains(&window.element)
                        && through
                            .chain
                            .get(at..)
                            .is_some_and(|tail| !tail.is_empty() && instance.chain.ends_with(tail))
                })
                .and_then(|(_, call)| call.clone())
        })
        .collect();
    Some(SceneCalls {
        element: window.element.clone(),
        generation: calls.generation,
        calls: mapped,
    })
}

/// The options of an edit reaching the instances of `element` (`shared`
/// when they are more than the selected one), whose targets are `targets`,
/// written in `component`: see [`scope_state`].
#[allow(clippy::too_many_arguments)]
fn options_over(
    report: &LayerSourceReport,
    targets: &ElementTargets,
    component: &str,
    element: &ElementOccurrences,
    shared: bool,
    calls: Option<&SceneCalls>,
    chapters: &[ReactChapter],
    frame: u64,
) -> ScopeState {
    if !shared {
        return ScopeState::NotShared;
    }
    let calls = calls.filter(|calls| {
        calls.element == element.element && calls.calls.len() == element.instances.len()
    });
    let scene = Scene::at(chapters, frame);

    let params = read_params(targets);
    let sets_own = |index: usize| {
        calls
            .and_then(|calls| calls.calls[index].as_ref())
            .is_some_and(|(_, written)| writes_any(written, &params))
    };
    let scenes = |own: bool| {
        let ranges: Vec<(u64, u64)> = element
            .instances
            .iter()
            .enumerate()
            .filter(|(index, _)| sets_own(*index) == own)
            .flat_map(|(_, instance)| instance.ranges.iter().copied())
            .collect();
        scenes_of(&ranges, chapters)
            .into_iter()
            .map(|chapter| chapter.name.clone())
            .collect()
    };

    let call = targets
        .component
        .as_ref()
        .map(|component| &component.call_site);
    let (times, drawn_alone) = call.map_or((0, false), |call| here(report, element, &scene, call));
    let this_scene = confined(report, targets, element, &scene, component).and_then(|()| {
        row_lock(
            Scope::ThisScene,
            this_scene_targets(targets, drawn_alone),
            calls,
        )
    });
    ScopeState::Options(ScopeOptions {
        this_scene,
        shared: row_lock(Scope::Shared, targets, calls),
        shared_reaches: scenes(false),
        sets_own: scenes(true),
        called_here: (times > 1).then(|| CalledHere {
            component: component.to_owned(),
            times,
        }),
        drawn_alone,
    })
}

/// How the current scene draws `element` through `call` (`S`): how many
/// calls of `C` draw it there (the chain entries at `S`'s place, see
/// [`calls_of`]; an instance with none there is no call; 0 without
/// chapters, where the timeline is no scene of its own), and whether `S`
/// draws it alone ([`drawn_alone`]).
fn here(
    report: &LayerSourceReport,
    element: &ElementOccurrences,
    scene: &Scene,
    call: &SourcePos,
) -> (usize, bool) {
    let calls: HashSet<SourcePos> = calls_of(report, element, call)
        .into_iter()
        .zip(&element.instances)
        .filter(|(_, instance)| scene.current.is_some() && scene.shows(instance))
        .filter_map(|(call, _)| call)
        .collect();
    (calls.len(), drawn_alone(element, scene, call))
}

/// Whether `call` (`S`) draws one instance of `element` in `scene`, one
/// copy at a time (not in a `.map()`).
fn drawn_alone(element: &ElementOccurrences, scene: &Scene, call: &SourcePos) -> bool {
    let mut here = element
        .instances
        .iter()
        .filter(|instance| instance.chain.contains(call) && scene.shows(instance));
    matches!(
        (here.next(), here.next()),
        (Some(instance), None) if instance.max_per_frame <= 1
    )
}

/// The current scene, as [`crate::impact::impact_view`] reads it.
struct Scene<'c> {
    chapters: &'c [ReactChapter],
    current: Option<&'c ReactChapter>,
    /// Its frames: the whole timeline without chapters of any length,
    /// `None` in a gap between them.
    frames: Option<(u64, u64)>,
}

impl<'c> Scene<'c> {
    fn at(chapters: &'c [ReactChapter], frame: u64) -> Self {
        let current = outermost_chapter_at(chapters, frame);
        let no_scenes = chapters
            .iter()
            .all(|chapter| chapter.duration_in_frames == 0);
        let frames = match current {
            Some(chapter) => Some((chapter.from, chapter.end())),
            None if no_scenes => Some((0, u64::MAX)),
            None => None,
        };
        Self {
            chapters,
            current,
            frames,
        }
    }

    /// Whether `instance` shows in this scene.
    fn shows(&self, instance: &InstanceOccurrences) -> bool {
        self.frames.is_some_and(|(start, end)| {
            instance
                .ranges
                .iter()
                .any(|&(from, to)| from < end && start < to)
        })
    }

    /// Whether `instance` shows in another scene.
    fn elsewhere(&self, instance: &InstanceOccurrences) -> bool {
        scenes_of(&instance.ranges, self.chapters)
            .into_iter()
            .any(|chapter| self.current.is_none_or(|current| current.id != chapter.id))
    }
}

/// Whether `S` is confined to the current scene: see [`scope_state`]'s
/// order.
fn confined(
    report: &LayerSourceReport,
    targets: &ElementTargets,
    element: &ElementOccurrences,
    scene: &Scene,
    component: &str,
) -> Result<(), ScopeLock> {
    let no_call = || ScopeLock::NoCall {
        component: component.to_owned(),
    };
    let call = targets
        .component
        .as_ref()
        .map(|component| &component.call_site)
        .ok_or_else(no_call)?;
    let at = report
        .chain
        .iter()
        .position(|entry| SourcePos::from(entry) == *call)
        .ok_or_else(no_call)?;
    let outside = &report.chain[at + 1..];
    // A `<Scenes>` scene function adds no entry: a component here sits
    // between the scene and `C`.
    if let Some(between) = outside
        .iter()
        .find(|entry| entry.kind == ElementKind::Component)
    {
        return Err(ScopeLock::PassesThrough {
            component: between.name.clone(),
        });
    }
    let through: Vec<&InstanceOccurrences> = element
        .instances
        .iter()
        .filter(|instance| instance.chain.contains(call))
        .collect();
    let spread = through.iter().any(|instance| scene.elsewhere(instance));
    let scenes_wrapper = outside
        .iter()
        .any(|entry| entry.kind == ElementKind::Wrapper && entry.name == "Scenes");
    if spread && scenes_wrapper {
        return Err(ScopeLock::ReusedCall);
    }
    if spread {
        return Err(ScopeLock::OtherScenes {
            component: component.to_owned(),
        });
    }
    // Another call in this scene keeps its value: `S` must draw the
    // selected instance alone.
    let another_here = element
        .instances
        .iter()
        .any(|instance| !instance.chain.contains(call) && scene.shows(instance));
    if another_here && !drawn_alone(element, scene, call) {
        return Err(ScopeLock::DrawsMoreThanOnce {
            component: component.to_owned(),
        });
    }
    Ok(())
}

/// The parameters of `C` the element's rows and inputs read.
fn read_params(targets: &ElementTargets) -> Vec<&str> {
    targets
        .entries()
        .into_iter()
        .filter(|(key, _)| !matches!(key, PropKey::CallSite(_)))
        .filter_map(|(_, target)| match &target.target {
            EditTarget::PassThrough { param, .. } => Some(param.as_str()),
            _ => None,
        })
        .collect()
}

/// Whether a call writing the attributes `written` sets its own value for
/// any of `params`.
fn writes_any(written: &[String], params: &[&str]) -> bool {
    written.iter().any(|name| params.contains(&name.as_str()))
}

/// Which instances an edit of the element changes under `scope` (the
/// hovered option, else the chosen one), for the timeline bands and the
/// header: "This scene", those `S` makes; "Shared value", all but those
/// whose call sets its own value (as `calls` know them) when every row it
/// writes passes through, else all (a value written on the element
/// changes every instance); no scope, design 2's rule (those of
/// [`ElementTargets::written_at`], else all).
pub(crate) fn impact_filter(
    scope: Option<Scope>,
    targets: &ElementTargets,
    calls: Option<&SceneCalls>,
) -> ImpactFilter {
    let call = targets
        .component
        .as_ref()
        .map(|component| &component.call_site);
    match (scope, call) {
        (Some(Scope::ThisScene), Some(call)) => ImpactFilter::Through(call.clone()),
        (Some(Scope::Shared), _) if !all_pass_through(targets) => ImpactFilter::All,
        (Some(Scope::Shared), _) => {
            let params = read_params(targets);
            let own = calls
                .into_iter()
                .flat_map(|calls| &calls.calls)
                .flatten()
                .filter(|(_, written)| writes_any(written, &params))
                .map(|(call, _)| call.clone());
            ImpactFilter::Except(own.collect())
        }
        _ => targets.written_at().map_or(ImpactFilter::All, |site| {
            ImpactFilter::Through(site.clone())
        }),
    }
}

/// Whether every row and input "Shared value" writes for the element
/// passes through to a parameter of `C` (Component-section rows are
/// outside the scope).
fn all_pass_through(targets: &ElementTargets) -> bool {
    targets
        .entries()
        .into_iter()
        .filter(|(key, target)| {
            !matches!(key, PropKey::CallSite(_)) && in_scope(Scope::Shared, &target.target)
        })
        .all(|(_, target)| matches!(target.target, EditTarget::PassThrough { .. }))
}

/// The scope the impact shows: the `hovered` option while it is allowed
/// (a preview), else the one an edit uses ([`chosen_scope`]); `None` when
/// that is locked too.
pub(crate) fn previewed_scope(
    hovered: Option<Scope>,
    options: &ScopeOptions,
    remembered: Option<Scope>,
) -> Option<Scope> {
    hovered
        .filter(|scope| options.option(*scope).is_ok())
        .or_else(|| chosen_scope(remembered, options).ok())
}

/// The targets an edit of the element reads under `scope`: under "This
/// scene", when `S` draws the selected instance alone in this scene
/// (`options.drawn_alone`), the element as that one instance
/// ([`ElementTargets::one`]), whose values written on the element lift to
/// `S` though the frame renders the element more than once; else
/// `targets`, where design 2's `Repeated` lock holds those values, since
/// writing them on the element would change every copy.
pub(crate) fn targets_in_effect<'t>(
    targets: &'t ElementTargets,
    scope: Option<Scope>,
    options: &ScopeOptions,
) -> &'t ElementTargets {
    match scope {
        Some(Scope::ThisScene) => this_scene_targets(targets, options.drawn_alone),
        Some(Scope::Shared) | None => targets,
    }
}

/// The targets "This scene" writes: the element as one instance when `S`
/// draws the selected instance alone (`drawn_alone`) and the frame renders
/// it more than once, else `targets`.
fn this_scene_targets(targets: &ElementTargets, drawn_alone: bool) -> &ElementTargets {
    match &targets.one {
        Some(one) if drawn_alone => one,
        _ => targets,
    }
}

/// What an edit changes under `scope`, said while a drag runs.
pub(crate) fn scope_in_effect(scope: Scope) -> &'static str {
    match scope {
        Scope::ThisScene => "Edits change this scene",
        Scope::Shared => "Edits change the shared value",
    }
}

/// [`scope_in_effect`] in a scene whose calls of `C` are `called_here`:
/// under "This scene" in a scene calling `C` more than once, the selected
/// instance's call only.
pub(crate) fn scope_in_effect_here(scope: Scope, called_here: Option<&CalledHere>) -> String {
    match (scope, called_here) {
        (Scope::ThisScene, Some(called)) => {
            format!("Edits change this `<{}>` only", called.component)
        }
        _ => scope_in_effect(scope).to_owned(),
    }
}

/// The "This scene" tab's tooltip while it is not locked.
pub(crate) fn this_scene_tooltip(options: &ScopeOptions) -> &'static str {
    match options.called_here {
        Some(_) => "Write the edit for this call only",
        None => "Write the edit for the instances in this scene only",
    }
}

/// The remove button's tooltip on a row that writes `row` under `scope`
/// (`None` for a row outside the scope, a Component-section one too): under
/// "This scene", and under "Shared value" on a row this scene sets itself,
/// the button deletes the value passed at `S`, so the shared value applies
/// again.
pub(crate) fn remove_tooltip(scope: Option<Scope>, row: &ScopedRow) -> &'static str {
    match (scope, row) {
        (Some(Scope::ThisScene), ScopedRow::AsResolved)
        | (Some(Scope::Shared), ScopedRow::Locked(ScopeLock::SetsOwn { .. })) => {
            "Use the shared value"
        }
        _ => "Remove",
    }
}

/// Whether `scope` decides what `target` writes: an editable target, and
/// under "Shared value" any pass-through, since its default is written in
/// `C` whatever `S` holds. A call site that is not found leaves a bare
/// `Locked(CallSiteNotFound)`, no pass-through (and no `defaults` entry);
/// "This scene" is locked as `NoCall` then.
fn in_scope(scope: Scope, target: &EditTarget) -> bool {
    editable(target) || scope == Scope::Shared && matches!(target, EditTarget::PassThrough { .. })
}

/// `scope`'s text-level lock: the first row's, when no editable row or
/// input of the element can be written under it. Component-section rows
/// are outside the scope. A row this scene sets itself
/// ([`ScopeLock::SetsOwn`]) locks the row only, never the option: its
/// removal at `S` is how the shared value gets edited from this scene.
fn row_lock(
    scope: Scope,
    targets: &ElementTargets,
    calls: Option<&SceneCalls>,
) -> Result<(), ScopeLock> {
    let mut first = None;
    for (key, target) in targets.entries() {
        if matches!(key, PropKey::CallSite(_)) || !in_scope(scope, &target.target) {
            continue;
        }
        match scoped_row(scope, targets, &key, calls) {
            ScopedRow::Locked(ScopeLock::SetsOwn { .. }) => return Ok(()),
            ScopedRow::Locked(lock) => {
                first.get_or_insert(lock);
            }
            _ => return Ok(()),
        }
    }
    first.map_or(Ok(()), Err)
}

/// The scope an edit uses: the `remembered` choice while it is allowed
/// (refused with its lock, never widened, once it is not), else "This
/// scene", else "Shared value".
pub(crate) fn chosen_scope(
    remembered: Option<Scope>,
    options: &ScopeOptions,
) -> Result<Scope, ScopeLock> {
    if let Some(scope) = remembered {
        return options.option(scope).clone().map(|()| scope);
    }
    match (&options.this_scene, &options.shared) {
        (Ok(()), _) => Ok(Scope::ThisScene),
        (Err(_), Ok(())) => Ok(Scope::Shared),
        (Err(lock), Err(_)) => Err(lock.clone()),
    }
}

/// The scope a commit made now keeps until it is written (held, or a drag
/// until its release): the `remembered` choice, else the option `shown` had
/// in effect, so a commit goes as the control showed it, or is refused
/// with the lock when that option is locked by then ([`commit_gate`]).
/// `None` when no control showed an option, and for an element that is
/// not shared, whatever was chosen for it before: it writes as resolved.
pub(crate) fn commit_scope(remembered: Option<Scope>, shown: &ScopeState) -> Option<Scope> {
    match shown {
        ScopeState::NotShared => None,
        ScopeState::Options(options) => remembered.or_else(|| chosen_scope(None, options).ok()),
        ScopeState::Unknown { .. } => remembered,
    }
}

/// More scenes than this are counted instead of named.
const NAMED_SCENES: usize = 3;

fn scenes_label(scenes: &[String]) -> String {
    if scenes.len() > NAMED_SCENES {
        format!("{} scenes", scenes.len())
    } else {
        scenes.join(", ")
    }
}

/// The line under the scope control: with "This scene" `chosen` (in effect,
/// or previewed) in a scene calling `C` more than once, that it edits the
/// selected instance's call only; else the reason a locked option (the
/// `chosen` one first) is disabled; otherwise the scenes the shared value
/// reaches and those that set their own.
pub(crate) fn scope_line(options: &ScopeOptions, chosen: Option<Scope>) -> String {
    if let (Some(Scope::ThisScene), Ok(()), Some(called)) =
        (chosen, &options.this_scene, &options.called_here)
    {
        return format!(
            "Edits this `<{}>` only · this scene calls it {} times",
            called.component, called.times
        );
    }
    let locked = chosen
        .into_iter()
        .chain([Scope::ThisScene, Scope::Shared])
        .find_map(|scope| options.option(scope).as_ref().err());
    if let Some(lock) = locked {
        return lock.text();
    }
    let reach = match (&options.shared_reaches[..], &options.sets_own[..]) {
        ([], []) => "Shared value is used everywhere".to_owned(),
        ([], _) => "Shared value is used in no scene".to_owned(),
        (reaches, _) => format!("Shared value is used in {}", scenes_label(reaches)),
    };
    match &options.sets_own[..] {
        [] => reach,
        [one] => format!("{reach} · {one} sets its own"),
        own => format!("{reach} · {} set their own", scenes_label(own)),
    }
}

/// What a row writes under a scope.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScopedRow {
    /// Where it resolved, as without a scope.
    AsResolved,
    /// Lift the value at the element into a new prop of `C` and pass it at
    /// `S`.
    Lift,
    /// The default of the parameter the row passes through to, which `S`
    /// leaves out: `shown` is its value (`None` when none is written yet).
    Default {
        shown: Option<EditValue>,
    },
    Locked(ScopeLock),
}

impl ScopedRow {
    /// Whether removing the row's value removes it where it resolved: a
    /// row written as resolved, and, under "Shared value", one this scene
    /// sets itself (its value at `S` goes, so the shared value applies).
    pub(crate) fn removes_as_resolved(&self) -> bool {
        matches!(
            self,
            Self::AsResolved | Self::Locked(ScopeLock::SetsOwn { .. })
        )
    }
}

/// What the row (or input) `key` of `targets` writes under `scope`.
///
/// "This scene": a value passed through to `S` writes there; a literal or
/// an absent prop at the element is lifted (unless the lift is locked);
/// a keyframe, a `style` property or a literal input can't be set per
/// scene. "Shared value": a pass-through writes its parameter's default
/// (see [`ScopedRow::Default`]), or is locked when every call passes it
/// without one, or the default is an expression or has no pattern to go
/// in, whatever `S` passes; else, when `S` passes it (an expression too),
/// it is locked as [`ScopeLock::SetsOwn`]: the edit would not show in this
/// scene, which shows its own value, until that is removed at `S`
/// ([`ScopedRow::removes_as_resolved`]); anything else writes as
/// resolved. Any other target that is locked as resolved, a
/// Component-section row, and every row of an element in no component
/// write as resolved under either. A lift or a default that would write a
/// file outside the project (`C`'s, or `S`'s for a lift) is locked.
pub(crate) fn scoped_row(
    scope: Scope,
    targets: &ElementTargets,
    key: &PropKey,
    calls: Option<&SceneCalls>,
) -> ScopedRow {
    let row = unchecked_row(scope, targets, key, calls);
    let outside = |file: &Path| targets.outside.iter().any(|outside| outside == file);
    let call_site = targets
        .component
        .as_ref()
        .map(|component| component.call_site.file.as_path());
    let writes_outside = match &row {
        ScopedRow::Lift => outside(&targets.file) || call_site.is_some_and(outside),
        ScopedRow::Default { .. } => outside(&targets.file),
        ScopedRow::AsResolved | ScopedRow::Locked(_) => false,
    };
    if writes_outside {
        ScopedRow::Locked(ScopeLock::OutsideProject)
    } else {
        row
    }
}

/// [`scoped_row`] before the files it writes are checked.
fn unchecked_row(
    scope: Scope,
    targets: &ElementTargets,
    key: &PropKey,
    calls: Option<&SceneCalls>,
) -> ScopedRow {
    let (Some(component), Some(target)) = (targets.enclosing.as_deref(), targets.entry(key)) else {
        return ScopedRow::AsResolved;
    };
    if matches!(key, PropKey::CallSite(_)) || !in_scope(scope, &target.target) {
        return ScopedRow::AsResolved;
    }
    match &target.target {
        EditTarget::PassThrough {
            param,
            target: at_call,
            ..
        } => match scope {
            Scope::ThisScene => ScopedRow::AsResolved,
            // However `S` passes it: the default is written in `C`.
            Scope::Shared => shared_default(targets, component, param, at_call, calls),
        },
        _ if scope == Scope::Shared => ScopedRow::AsResolved,
        at_element => {
            let liftable = matches!(key, PropKey::Prop(_))
                && !target.path.starts_with("style.")
                && matches!(
                    at_element,
                    EditTarget::Literal { .. } | EditTarget::Missing { .. }
                );
            if !liftable {
                return ScopedRow::Locked(ScopeLock::PerSceneUnsupported);
            }
            match &targets.lift_lock {
                Some(lock) => ScopedRow::Locked(lift_scope_lock(lock, component)),
                None => ScopedRow::Lift,
            }
        }
    }
}

/// What "Shared value" writes for parameter `param` of `component`, which
/// `S` passes as `at_call`.
fn shared_default(
    targets: &ElementTargets,
    component: &str,
    param: &str,
    at_call: &EditTarget,
    calls: Option<&SceneCalls>,
) -> ScopedRow {
    let own = !matches!(at_call, EditTarget::Missing { .. });
    let sets_own = || {
        ScopedRow::Locked(ScopeLock::SetsOwn {
            param: param.to_owned(),
        })
    };
    let default = targets
        .defaults
        .iter()
        .find(|(name, _)| name == param)
        .map(|(_, default)| default);
    match default {
        Some(DefaultTarget::Literal { .. }) if own => sets_own(),
        Some(DefaultTarget::Literal { value, .. }) => ScopedRow::Default {
            shown: Some(value.clone()),
        },
        Some(DefaultTarget::Absent { .. }) if own && every_call_passes(calls, param) => {
            ScopedRow::Locked(ScopeLock::EverySceneSets {
                param: param.to_owned(),
            })
        }
        Some(DefaultTarget::Absent { .. }) if own => sets_own(),
        Some(DefaultTarget::Absent { .. }) => ScopedRow::Default { shown: None },
        Some(DefaultTarget::Expression) => ScopedRow::Locked(ScopeLock::DefaultIsExpression {
            param: param.to_owned(),
        }),
        Some(DefaultTarget::NoPattern) => ScopedRow::Locked(ScopeLock::NotObjectPattern {
            component: component.to_owned(),
        }),
        None => ScopedRow::Locked(ScopeLock::DefaultNotFound {
            param: param.to_owned(),
        }),
    }
}

/// Whether every instance's call is known and passes `param`.
fn every_call_passes(calls: Option<&SceneCalls>, param: &str) -> bool {
    calls.is_some_and(|calls| {
        !calls.calls.is_empty()
            && calls.calls.iter().all(|call| {
                call.as_ref()
                    .is_some_and(|(_, written)| written.iter().any(|name| name == param))
            })
    })
}

/// Why a drag, a scrub or an inline text edit of `key` can't write under
/// `scope` (`None`: as resolved): the row's lock, as [`scoped_row`] gives
/// it, so a gesture is refused exactly when the row is.
pub(crate) fn gesture_lock(
    scope: Option<Scope>,
    targets: &ElementTargets,
    key: &PropKey,
    calls: Option<&SceneCalls>,
) -> Option<ScopeLock> {
    match scoped_row(scope?, targets, key, calls) {
        ScopedRow::Locked(lock) => Some(lock),
        ScopedRow::AsResolved | ScopedRow::Lift | ScopedRow::Default { .. } => None,
    }
}

/// Whether a commit goes now, and under which scope (`None`: as resolved),
/// waits for the occurrence index (with the note the row shows), or is
/// refused (with why).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommitGate {
    Go(Option<Scope>),
    Hold(String),
    Refuse(String),
}

/// The gate for committing `keys` of `targets` in `state`, `remembered`
/// being the choice made for the element. Not shared: as resolved. With
/// options: the chosen scope ([`chosen_scope`]), refused with its lock when
/// the remembered one (or both) is locked. While the index can't answer,
/// values written at `S` as resolved go (a pass-through, unless "Shared
/// value" is remembered, and a Component-section row). Any other commit
/// waits for the sweep while the index is scanning (or refreshing and no
/// longer finds the element), and is refused when no sweep would answer:
/// the scan failed, the index does not find the element, or no scan runs.
/// Component-section rows alone are outside the scope.
pub(crate) fn commit_gate(
    state: &ScopeState,
    remembered: Option<Scope>,
    targets: &ElementTargets,
    keys: &[PropKey],
) -> CommitGate {
    let outside_scope = |key: &PropKey| matches!(key, PropKey::CallSite(_));
    match state {
        ScopeState::NotShared => CommitGate::Go(None),
        _ if keys.iter().all(outside_scope) => CommitGate::Go(None),
        ScopeState::Options(options) => match chosen_scope(remembered, options) {
            Ok(scope) => CommitGate::Go(Some(scope)),
            Err(lock) => CommitGate::Refuse(lock.text()),
        },
        ScopeState::Unknown { note } => {
            let at_s = |key: &PropKey| {
                outside_scope(key)
                    || remembered != Some(Scope::Shared)
                        && targets.target(key).is_some_and(|target| {
                            matches!(target, EditTarget::PassThrough { .. }) && editable(target)
                        })
            };
            if keys.iter().all(at_s) {
                CommitGate::Go(None)
            } else if note == SCANNING {
                CommitGate::Hold(note.clone())
            } else {
                CommitGate::Refuse(note.clone())
            }
        }
    }
}

/// Why a gesture that started while the scenes were being scanned (no
/// scope shown) is not written once they are; nor a held row commit made
/// then that would write the shared value.
pub(crate) const SCANNED_MEANWHILE: &str = "The scenes were scanned while editing; edit again";

/// [`SCANNED_MEANWHILE`] for a commit held for a reload rather than for
/// the scan: the scenes' options changed while it waited.
pub(crate) const CHANGED_MEANWHILE: &str = "The scenes changed while editing; edit again";

/// Where a held commit came from, and what it waited for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeldOrigin {
    /// A scrub's or an inline text edit's release, not a Properties row.
    pub(crate) gesture: bool,
    /// Held for the scene scan, not for a reload.
    pub(crate) scanning: bool,
}

/// The gate for a commit held with the scope `held` (see [`commit_scope`])
/// once it drains: a gesture's through [`gesture_gate`], as at its release,
/// so it is refused with the locks the gesture would meet; a row commit's
/// through [`drain_gate`]. One held with no option shown and refused for
/// that says the scan only when it waited for one.
pub(crate) fn held_gate(
    held: Option<Scope>,
    origin: HeldOrigin,
    state: &ScopeState,
    targets: &ElementTargets,
    keys: &[PropKey],
    calls: Option<&SceneCalls>,
) -> CommitGate {
    let gate = if origin.gesture {
        gesture_gate(held, state, targets, keys, calls)
    } else {
        drain_gate(held, state, targets, keys)
    };
    match gate {
        CommitGate::Refuse(refusal) if !origin.scanning && refusal == SCANNED_MEANWHILE => {
            CommitGate::Refuse(CHANGED_MEANWHILE.to_owned())
        }
        gate => gate,
    }
}

/// The gate for a row commit held with the scope `held` it was made with
/// (see [`commit_scope`]): [`commit_gate`], but one made with no scope
/// shown (the scenes being scanned) is written only for this scene; when
/// "This scene" is locked by the time it goes, it is refused rather than
/// written to the shared value nobody chose.
pub(crate) fn drain_gate(
    held: Option<Scope>,
    state: &ScopeState,
    targets: &ElementTargets,
    keys: &[PropKey],
) -> CommitGate {
    match commit_gate(state, held, targets, keys) {
        CommitGate::Go(Some(Scope::Shared)) if held.is_none() => {
            CommitGate::Refuse(SCANNED_MEANWHILE.to_owned())
        }
        gate => gate,
    }
}

/// The gate for a drag, a scrub or an inline text edit of `keys` that
/// started under `started`, the scope the control showed then (see
/// [`commit_scope`]): refused with the lock of a row `started` can't write,
/// or whose edit this scene would not show ([`gesture_lock`]); else
/// [`commit_gate`] with `started`, so it goes under that scope or is
/// refused with its lock, never under another. One that started with no
/// scope shown is refused once the options are known. A gesture is never
/// held: while the scenes are scanned it is refused with the scan's note.
pub(crate) fn gesture_gate(
    started: Option<Scope>,
    state: &ScopeState,
    targets: &ElementTargets,
    keys: &[PropKey],
    calls: Option<&SceneCalls>,
) -> CommitGate {
    if let Some(lock) = keys
        .iter()
        .find_map(|key| gesture_lock(started, targets, key, calls))
    {
        return CommitGate::Refuse(lock.text());
    }
    match commit_gate(state, started, targets, keys) {
        CommitGate::Go(Some(_)) if started.is_none() => {
            CommitGate::Refuse(SCANNED_MEANWHILE.to_owned())
        }
        CommitGate::Hold(note) => CommitGate::Refuse(note),
        gate => gate,
    }
}

/// Why a drag, a scrub or an inline text edit of `key` does not start:
/// the refusal [`gesture_gate`] would give it at release under the scope
/// it would keep ([`commit_scope`] of `remembered` and the shown `state`),
/// which includes waiting for the scene scan.
pub(crate) fn gesture_start_refusal(
    remembered: Option<Scope>,
    state: &ScopeState,
    targets: &ElementTargets,
    key: &PropKey,
    calls: Option<&SceneCalls>,
) -> Option<String> {
    let kept = commit_scope(remembered, state);
    match gesture_gate(kept, state, targets, std::slice::from_ref(key), calls) {
        CommitGate::Refuse(refusal) | CommitGate::Hold(refusal) => Some(refusal),
        CommitGate::Go(_) => None,
    }
}

/// `removable` under `scope`: only a row whose removal is as resolved
/// ([`ScopedRow::removes_as_resolved`]).
pub(crate) fn scoped_removable(
    scope: Scope,
    targets: &ElementTargets,
    key: &PropKey,
    calls: Option<&SceneCalls>,
) -> bool {
    scoped_row(scope, targets, key, calls).removes_as_resolved()
        && targets
            .entry(key)
            .is_some_and(|target| removable(targets, key, target))
}

/// The scope chosen per element (its position) for the session.
#[derive(Debug, Default)]
pub(crate) struct ScopeChoices {
    chosen: HashMap<SourcePos, Scope>,
}

impl ScopeChoices {
    pub(crate) fn get(&self, element: &SourcePos) -> Option<Scope> {
        self.chosen.get(element).copied()
    }

    pub(crate) fn set(&mut self, element: SourcePos, scope: Scope) {
        self.chosen.insert(element, scope);
    }

    /// Moves the choice made at `from` to `to`, where a reload put the
    /// element (a lift above it shifts it); nothing when `from` has none.
    pub(crate) fn carry(&mut self, from: &SourcePos, to: &SourcePos) {
        if let Some(scope) = self.chosen.remove(from) {
            self.chosen.insert(to.clone(), scope);
        }
    }

    /// Forgets the choices made for elements in `files`, which a write
    /// changed: their positions may now name other elements. The one at
    /// `keep`, carried for the element being edited, stays.
    pub(crate) fn forget_in(&mut self, files: &[PathBuf], keep: Option<&SourcePos>) {
        self.chosen
            .retain(|element, _| Some(element) == keep || !files.contains(&element.file));
    }

    /// Forgets every choice: files changed outside the editor.
    pub(crate) fn clear(&mut self) {
        self.chosen.clear();
    }
}
