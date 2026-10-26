//! What an edit of a shared element writes: for **this scene**, at the
//! scene's call `S` of the component `C` the element `E` is written in
//! (lifting a literal into a new prop of `C` when needed), or the **shared
//! value** every scene uses unless it sets its own. The choice and its
//! memory per element, the options the occurrence index and the scene
//! calls allow, and [`scoped_row`], the one per-row rule the rows (shown
//! value, locks, removal) and the commit read.
#![cfg_attr(not(test), allow(dead_code))]

use crate::element_edit::{ElementTargets, PropKey, editable, reason_text};
use crate::element_rows::removable;
use crate::occurrences::OccurrenceState;
use celesta_react_bridge::{
    ElementKind, ElementOccurrences, InstanceOccurrences, LayerSourceReport, ReactChapter,
    SourcePos, outermost_chapter_at, scenes_of,
};
use celesta_source_edit::{DefaultTarget, EditTarget, EditValue, LiftLock, LockReason};
use std::collections::HashMap;

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
    /// An instance in this scene does not go through `S`.
    CalledTwiceHere { component: String },
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
    /// here (drag, scrub and inline text).
    SetsOwn { param: String },
    /// `param`'s default is written as an expression.
    DefaultIsExpression { param: String },
    /// The occurrence index can't answer yet, or failed.
    Unknown { note: String },
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
            Self::CalledTwiceHere { component } => {
                format!("`<{component}>` is called more than once in this scene")
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
            Self::Unknown { note } => note.clone(),
        }
    }
}

/// The lock a failed lift puts on "This scene", for `E` in `component`.
fn lift_scope_lock(lock: &LiftLock, component: &str) -> ScopeLock {
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
    // The index's chains leave the wrappers out.
    let at = report
        .chain
        .iter()
        .filter(|entry| entry.kind != ElementKind::Wrapper)
        .position(|entry| SourcePos::from(entry) == *call);
    element
        .instances
        .iter()
        .map(|instance| at.and_then(|at| instance.chain.get(at)).cloned())
        .collect()
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

/// The scope options of editing the element `report` describes, whose
/// targets are `targets`, with the playhead at `frame`. The current scene
/// is the outermost chapter at `frame`, as the impact line reads it.
/// "This scene" is locked, in this order, when no call `S` is found, a
/// component sits between the scene and `S`, the scenes reuse one chain
/// through `S`, `S` makes instances in other scenes, an instance in this
/// scene is not made by `S`, or no row can be written for this scene;
/// "Shared value" when no row can be written for it. `calls`, when read
/// for this element and index, names the scenes that set their own value.
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
    let unknown = |note: &str| ScopeState::Unknown {
        note: note.to_owned(),
    };
    let (index, refreshing) = match occurrences {
        None | Some(OccurrenceState::Scanning) => return unknown(SCANNING),
        Some(OccurrenceState::Failed(message)) => {
            return unknown(&format!("Couldn't scan scenes: {message}"));
        }
        Some(OccurrenceState::Ready(index)) => (index, false),
        Some(OccurrenceState::Refreshing(index)) => (index, true),
    };
    let Some((element, _)) = index.instance_for(report) else {
        // A refreshing index misses an element a lift moved: the sweep
        // under way finds it.
        return unknown(if refreshing {
            SCANNING
        } else {
            "Not found in the scene scan"
        });
    };
    if element.instances.len() <= 1 {
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
            .is_some_and(|(_, written)| written.iter().any(|name| params.contains(&name.as_str())))
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

    let this_scene = confined(report, targets, element, &scene, component)
        .and_then(|()| row_lock(Scope::ThisScene, targets, calls));
    ScopeState::Options(ScopeOptions {
        this_scene,
        shared: row_lock(Scope::Shared, targets, calls),
        shared_reaches: scenes(false),
        sets_own: scenes(true),
    })
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
    if element
        .instances
        .iter()
        .any(|instance| !instance.chain.contains(call) && scene.shows(instance))
    {
        return Err(ScopeLock::CalledTwiceHere {
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

/// `scope`'s text-level lock: the first row's, when no editable row or
/// input of the element can be written under it. Component-section rows
/// are outside the scope.
fn row_lock(
    scope: Scope,
    targets: &ElementTargets,
    calls: Option<&SceneCalls>,
) -> Result<(), ScopeLock> {
    let mut first = None;
    for (key, target) in targets.entries() {
        if matches!(key, PropKey::CallSite(_)) || !editable(&target.target) {
            continue;
        }
        match scoped_row(scope, targets, &key, calls) {
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

/// More scenes than this are counted instead of named.
const NAMED_SCENES: usize = 3;

fn scenes_label(scenes: &[String]) -> String {
    if scenes.len() > NAMED_SCENES {
        format!("{} scenes", scenes.len())
    } else {
        scenes.join(", ")
    }
}

/// The line under the scope control: the reason a locked option (the
/// `chosen` one first) is disabled; otherwise the scenes the shared value
/// reaches and those that set their own.
pub(crate) fn scope_line(options: &ScopeOptions, chosen: Option<Scope>) -> String {
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
    /// The default of the parameter the row passes through to: `shown` is
    /// its value (`None` when none is written yet); `own` when `S` passes
    /// the parameter, so the edit does not show in this scene.
    Default {
        shown: Option<EditValue>,
        own: bool,
    },
    Locked(ScopeLock),
}

/// What the row (or input) `key` of `targets` writes under `scope`; see
/// the table in the edit scope plan. A target that is locked as resolved,
/// a Component-section row, and every row of an element in no component
/// write as resolved.
pub(crate) fn scoped_row(
    scope: Scope,
    targets: &ElementTargets,
    key: &PropKey,
    calls: Option<&SceneCalls>,
) -> ScopedRow {
    let (Some(component), Some(target)) = (targets.enclosing.as_deref(), targets.entry(key)) else {
        return ScopedRow::AsResolved;
    };
    if matches!(key, PropKey::CallSite(_)) || !editable(&target.target) {
        return ScopedRow::AsResolved;
    }
    match &target.target {
        EditTarget::PassThrough {
            param,
            target: at_call,
            ..
        } => match scope {
            Scope::ThisScene => ScopedRow::AsResolved,
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
    let default = targets
        .defaults
        .iter()
        .find(|(name, _)| name == param)
        .map(|(_, default)| default);
    match default {
        Some(DefaultTarget::Literal { value, .. }) => ScopedRow::Default {
            shown: Some(value.clone()),
            own,
        },
        Some(DefaultTarget::Absent { .. }) if own && every_call_passes(calls, param) => {
            ScopedRow::Locked(ScopeLock::EverySceneSets {
                param: param.to_owned(),
            })
        }
        Some(DefaultTarget::Absent { .. }) => ScopedRow::Default { shown: None, own },
        Some(DefaultTarget::Expression) => ScopedRow::Locked(ScopeLock::DefaultIsExpression {
            param: param.to_owned(),
        }),
        Some(DefaultTarget::NoPattern) => ScopedRow::Locked(ScopeLock::NotObjectPattern {
            component: component.to_owned(),
        }),
        None => ScopedRow::Locked(ScopeLock::Unknown {
            note: format!("Can't find where `{param}`'s shared value is written"),
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

/// `removable` under `scope`: only an `AsResolved` row.
pub(crate) fn scoped_removable(
    scope: Scope,
    targets: &ElementTargets,
    key: &PropKey,
    calls: Option<&SceneCalls>,
) -> bool {
    scoped_row(scope, targets, key, calls) == ScopedRow::AsResolved
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
}
