//! Editing the selected React layer's props in its JSX source: which source
//! spans hold each prop the editor writes, the state the Layer section and
//! the canvas read, and the commit that writes new values. The resolution
//! lives in `resolve.rs` and the commit in `commit.rs`; this module holds
//! the targets' types, the [`ElementEdit`] state, and its sync with the
//! selection.

mod commit;
mod resolve;

pub(crate) use commit::commit_files;
#[cfg(test)]
pub(crate) use commit::{CommitError, apply_checked, commit_scoped};
pub(crate) use resolve::{
    edit_value, editable, field_of, inner_target, required_of, resolve_targets, schema_field,
    table_default, table_key, text_style_key, type_component_props,
};

use crate::element_rows::{removable, row_label};
use crate::layer_select::SourceLookup;
use crate::occurrences::OccurrenceState;
use crate::property_controls::ControlKind;
use crate::scope::{
    CommitGate, SceneCalls, Scope, ScopeLock, ScopeState, calls_of, chosen_scope, commit_gate,
    current_calls, gesture_lock, read_scene_calls, scope_state, scoped_removable,
};
use crate::view::EditorView;
use celesta_react_bridge::{
    ComponentPropertySchema, ElementOccurrences, LayerSourceReport, PrimitivePropertyField,
    PrimitivePropertyTable, ReceivedProps, ReceivedValue, SourcePos,
};
use celesta_source_edit::{
    DefaultTarget, EditTarget, EditValue, ElementRef, LiftLock, LockReason, format_number,
};
use gpui_kit::Context;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// A row the editor writes: a table prop by its table key (`fontSize`, even
/// when it writes `style.fontSize`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PropKey {
    Prop(String),
    /// Input `name` of the expression row `row` (a table key).
    Input {
        row: String,
        name: String,
    },
    /// The prop `name` written at the call of the component that encloses
    /// the element.
    CallSite(String),
}

impl PropKey {
    /// The row's name in messages: a prop's row label ("Font size"), an
    /// input's or a component prop's name as written.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Prop(name) => row_label(name),
            Self::Input { name, .. } | Self::CallSite(name) => name.clone(),
        }
    }
}

/// The props written at the call of the component that encloses the
/// element: its Component section.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComponentTargets {
    /// The tag at the call site (`PriceTag`), as `tag_name` reads it.
    pub(crate) name: String,
    pub(crate) call_site: SourcePos,
    /// The call in the call site's text, where a removed prop is deleted.
    pub(crate) call: ElementRef,
    /// In source order; `kind` from the registered schema or the literal.
    pub(crate) props: Vec<ComponentProp>,
    /// The schema `registerComponent()` registered for `name`, which typed
    /// the props; a commit types its fresh resolution with it again.
    pub(crate) schema: Option<ComponentPropertySchema>,
}

/// One prop written at the component's call.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComponentProp {
    pub(crate) name: String,
    pub(crate) target: Target,
    pub(crate) kind: ControlKind,
    /// The registered field as a table field (range, options, default),
    /// when the schema declares the prop.
    pub(crate) field: Option<PrimitivePropertyField>,
}

/// A transform prop the editor writes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TransformProp {
    X,
    Y,
    Rotation,
    Scale,
    ScaleX,
    ScaleY,
}

impl TransformProp {
    /// Every transform prop, in row order.
    pub(crate) const ALL: [Self; 6] = [
        Self::X,
        Self::Y,
        Self::Rotation,
        Self::Scale,
        Self::ScaleX,
        Self::ScaleY,
    ];

    /// The row key of this prop.
    pub(crate) fn key(self) -> PropKey {
        PropKey::Prop(self.name().to_owned())
    }

    /// The transform prop `key` names, if any.
    pub(crate) fn from_key(key: &PropKey) -> Option<Self> {
        let PropKey::Prop(name) = key else {
            return None;
        };
        Self::ALL.into_iter().find(|prop| prop.name() == name)
    }

    /// The JSX attribute name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Rotation => "rotation",
            Self::Scale => "scale",
            Self::ScaleX => "scaleX",
            Self::ScaleY => "scaleY",
        }
    }
}

/// The number the runtime passed for `prop`, if it passed one.
pub(crate) fn received_number(props: &ReceivedProps, prop: &str) -> Option<f64> {
    match props.get(prop)? {
        ReceivedValue::Number(value) => Some(value),
        ReceivedValue::String(_) | ReceivedValue::Bool(_) => None,
    }
}

/// Where one row's value lives.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    /// The file `target` is in: the call site's for a `PassThrough`, the
    /// element's otherwise.
    pub(crate) file: PathBuf,
    /// The prop path resolved on the element (`fontSize`, `style.fontSize`,
    /// `children`).
    pub(crate) path: String,
    pub(crate) target: EditTarget,
    /// Where the component enclosing the element is called, for a
    /// `PassThrough`.
    pub(crate) call_site: Option<SourcePos>,
    /// For a row locked as an `Expression`, the identifiers the expression
    /// reads, each with where its value lives (in the order the expression
    /// reads them). Empty otherwise, and for an input.
    pub(crate) inputs: Vec<(String, Target)>,
}

/// Where each row of one element lives.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementTargets {
    /// The element's file.
    pub(crate) file: PathBuf,
    /// The files resolving read: the element's, then the call site's when a
    /// row or an input is a parameter of the enclosing component. A commit
    /// reads them all again.
    pub(crate) files: Vec<PathBuf>,
    pub(crate) element: ElementRef,
    /// The rows of the element's property table in its order, `scale` split
    /// into `scaleX`, `scaleY` when either is written on the element; the
    /// transform props alone without a table.
    pub(crate) targets: Vec<(PropKey, Target)>,
    /// The table the rows were resolved for, so a commit resolves them
    /// again the same way.
    pub(crate) table: Option<PrimitivePropertyTable>,
    /// The props written at the enclosing component's call, when that call
    /// is found and readable.
    pub(crate) component: Option<ComponentTargets>,
    /// The component the element is written in (`C`), as a call names it;
    /// `None` when it is in none (a scene function).
    pub(crate) enclosing: Option<String>,
    /// Where the default of each parameter a row or an input reads is
    /// written in `C`, in the order the rows read them.
    pub(crate) defaults: Vec<(String, DefaultTarget)>,
    /// Why lifting the literals and absent props written on the element
    /// into new props of `C` fails, tried for all of them at once.
    pub(crate) lift_lock: Option<LiftLock>,
    /// The files of `files` outside the project (see
    /// [`lock_outside_project`]): nothing is written to them, a default or
    /// a lift neither.
    pub(crate) outside: Vec<PathBuf>,
}

impl ElementTargets {
    /// `key`'s target: a `PassThrough` wraps the target at the call site.
    pub(crate) fn target(&self, key: &PropKey) -> Option<&EditTarget> {
        self.entry(key).map(|target| &target.target)
    }

    /// The file `key` is written in; the element's when `key` is not a
    /// target.
    pub(crate) fn file_of(&self, key: &PropKey) -> &Path {
        self.entry(key).map_or(&self.file, |target| &target.file)
    }

    /// The target of the transform prop `prop`.
    pub(crate) fn transform(&self, prop: TransformProp) -> Option<&EditTarget> {
        self.target(&prop.key())
    }

    /// The call site an edit of this element's transform writes, when every
    /// editable transform target passes through to that one call site: the
    /// edit then changes only the instances it makes. `None` when nothing is
    /// editable, or when an editable target is written on the element (in
    /// every instance). Other rows do not count.
    pub(crate) fn written_at(&self) -> Option<&SourcePos> {
        let mut sites = self
            .targets
            .iter()
            .filter(|(key, target)| {
                TransformProp::from_key(key).is_some() && editable(&target.target)
            })
            .map(|(_, target)| target.call_site.as_ref());
        let first = sites.next()??;
        sites.all(|site| site == Some(first)).then_some(first)
    }

    /// The row, input, or component prop `key`.
    pub(crate) fn entry(&self, key: &PropKey) -> Option<&Target> {
        let row = |name: &str| {
            self.targets
                .iter()
                .find(|(candidate, _)| matches!(candidate, PropKey::Prop(row) if row == name))
                .map(|(_, target)| target)
        };
        match key {
            PropKey::Prop(name) => row(name),
            PropKey::Input { row: of, name } => row(of)?
                .inputs
                .iter()
                .find(|(input, _)| input == name)
                .map(|(_, target)| target),
            PropKey::CallSite(name) => self
                .component
                .as_ref()?
                .props
                .iter()
                .find(|prop| prop.name == *name)
                .map(|prop| &prop.target),
        }
    }

    /// Every row, each followed by its inputs, then the component's props,
    /// with its key.
    pub(crate) fn entries(&self) -> Vec<(PropKey, &Target)> {
        let mut entries = Vec::new();
        for (key, target) in &self.targets {
            entries.push((key.clone(), target));
            let PropKey::Prop(row) = key else {
                continue;
            };
            for (name, input) in &target.inputs {
                let key = PropKey::Input {
                    row: row.clone(),
                    name: name.clone(),
                };
                entries.push((key, input));
            }
        }
        for prop in self.component.iter().flat_map(|component| &component.props) {
            entries.push((PropKey::CallSite(prop.name.clone()), &prop.target));
        }
        entries
    }
}

/// How many characters of an expression a reason shows.
const SOURCE_SHOWN: usize = 60;

/// `source` on one line: whitespace runs (line breaks too) as one space,
/// cut after [`SOURCE_SHOWN`] characters with "…" (an array or JSX
/// written over many lines would fill the panel).
fn one_line(source: &str) -> String {
    let line = source.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= SOURCE_SHOWN {
        return line;
    }
    let cut: String = line.chars().take(SOURCE_SHOWN).collect();
    format!("{}…", cut.trim_end())
}

/// The Layer section's line for a locked target.
pub(crate) fn reason_text(reason: &LockReason) -> String {
    match reason {
        LockReason::SyntaxError => "The source has syntax errors".to_owned(),
        LockReason::ElementNotFound => "Couldn't find this element in the source".to_owned(),
        LockReason::NotPrimitive => "Set by a component, not written on this element".to_owned(),
        LockReason::Expression { source } => {
            format!("Written as an expression: {}", one_line(source))
        }
        LockReason::UnresolvedIdentifier { name } => {
            format!("Comes from {name}, which can't be traced")
        }
        LockReason::SharedBinding { name, uses } => format!("{name} is used in {uses} places"),
        LockReason::BetweenKeyframes { previous, next, .. } => format!(
            "Between keyframes at {} and {}",
            format_number(*previous),
            format_number(*next)
        ),
        LockReason::OutsideKeyframes => "Outside the keyframes".to_owned(),
        LockReason::UnsupportedCall { callee } => format!("Computed by {callee}()"),
        LockReason::OverriddenByParent => "A layout component sets this value".to_owned(),
        LockReason::Spread => "May be set by a spread ({...props})".to_owned(),
        LockReason::Repeated { count } => format!("Rendered {count} times from this element"),
        LockReason::FrameUncertain => "Can't tell which frame this animation reads".to_owned(),
        LockReason::OutsideProject => "This file is outside the project".to_owned(),
        LockReason::NoStyleObject => "No style object to set this in".to_owned(),
        LockReason::StyleMaySet => "May be set by `style`".to_owned(),
        LockReason::ParameterOfCaller { component } => {
            format!("Comes from `<{component}>`'s caller")
        }
        LockReason::CallSiteNotFound { component } => {
            format!("Can't find where `<{component}>` is called")
        }
    }
}

/// A finished resolution, for the layer and report it was started for.
#[derive(Debug)]
pub(crate) struct Resolved {
    pub(crate) layer: String,
    pub(crate) report: LayerSourceReport,
    /// The timeline frame the report was looked up at.
    pub(crate) timeline_frame: i64,
    pub(crate) outcome: Result<ElementTargets, String>,
}

/// The values own writes put into rows; `None` removed the prop.
type WrittenValues = Vec<(PropKey, Option<EditValue>)>;

/// The selected layer's targets as the view keeps them. Resolution runs off
/// the UI thread; a result for an older request or another layer is
/// dropped.
#[derive(Debug, Default)]
pub(crate) struct ElementEdit {
    request: u64,
    /// The layer, report and lookup frame the newest resolution was started for.
    wanted: Option<(String, LayerSourceReport, i64)>,
    /// A resolution is running.
    pending: bool,
    resolved: Option<Resolved>,
    /// A write changed the file since `resolved` was read. Only `reset`, or a
    /// resolution started after `expire` (the reload that reads the write),
    /// clears it: a lookup the old code answered after the write may still
    /// resolve meanwhile.
    stale: bool,
    /// The code reloaded after the write: the next resolution reads the
    /// written file, so it is current again.
    expired: bool,
    /// The values the editor's own writes put into the file (`None` for a
    /// removed prop), and the layer they belong to: shown in the rows while
    /// `resolved` is stale, so a field does not show the old value until the
    /// reload resolves.
    written: Option<(String, WrittenValues)>,
    /// The last commit's error and the layer it belongs to.
    error: Option<(String, String)>,
    /// The code failed to reload since the last successful reload (or
    /// `reset`).
    reload_failed: bool,
    /// The preview worker stopped: no lookup answers again, whatever
    /// reloads or is selected.
    preview_stopped: bool,
}

impl ElementEdit {
    /// Starts resolving `report` for `layer`, returning the request number,
    /// unless that is already resolved (or resolving) and still current.
    pub(crate) fn begin(
        &mut self,
        layer: &str,
        report: &LayerSourceReport,
        timeline_frame: i64,
    ) -> Option<u64> {
        if self.wanted.as_ref().is_some_and(|(wanted, was, frame)| {
            wanted == layer && was == report && *frame == timeline_frame
        }) {
            return None;
        }
        if self
            .resolved
            .as_ref()
            .is_some_and(|resolved| resolved.layer != layer)
        {
            self.resolved = None;
        }
        self.request += 1;
        self.wanted = Some((layer.to_owned(), report.clone(), timeline_frame));
        self.pending = true;
        Some(self.request)
    }

    /// Keeps the outcome of resolution `request` when it is the newest one and
    /// `selected` is still the layer it was for. True when kept.
    pub(crate) fn finish(
        &mut self,
        request: u64,
        selected: Option<&str>,
        outcome: Result<ElementTargets, String>,
    ) -> bool {
        let Some((layer, report, timeline_frame)) = self.wanted.as_ref() else {
            return false;
        };
        if request != self.request || selected != Some(layer.as_str()) {
            return false;
        }
        self.pending = false;
        if std::mem::take(&mut self.expired) {
            self.stale = false;
            self.written = None;
        }
        self.resolved = Some(Resolved {
            layer: layer.clone(),
            report: report.clone(),
            timeline_frame: *timeline_frame,
            outcome,
        });
        true
    }

    /// The editor wrote the file: the targets are unknown until the layer is
    /// looked up again in the reloaded code and resolved.
    pub(crate) fn invalidate(&mut self) {
        self.stale = true;
        self.expired = false;
        self.pending = false;
        self.request += 1;
    }

    /// The code reloaded after an own write: keeps showing `resolved` (not
    /// current), and resolves the next lookup answer even when it equals the
    /// last. The view starts no resolution until the reloaded code answers,
    /// so the next one reads the written file against the new answer. An old
    /// commit error is gone, as after `reset`.
    pub(crate) fn expire(&mut self) {
        self.stale = true;
        self.expired = true;
        self.wanted = None;
        self.pending = false;
        self.request += 1;
        self.error = None;
        self.reload_failed = false;
    }

    /// Forgets everything (selection cleared, or the code reloaded): the next
    /// lookup answer is resolved again even when it equals the last one, and
    /// an old commit error is gone.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            request: self.request + 1,
            preview_stopped: self.preview_stopped,
            ..Self::default()
        };
    }

    /// An own write for `layer` put `values` into the file (after
    /// `invalidate`; `None` removed the prop). Kept until the reloaded code
    /// resolves; values for a row written again replace the earlier ones.
    pub(crate) fn record_written(
        &mut self,
        layer: Option<&str>,
        values: &[(PropKey, Option<EditValue>)],
    ) {
        let Some(layer) = layer.filter(|_| self.stale) else {
            return;
        };
        if self.written.as_ref().is_none_or(|(was, _)| was != layer) {
            self.written = Some((layer.to_owned(), Vec::new()));
        }
        let Some((_, written)) = &mut self.written else {
            return;
        };
        for (key, value) in values {
            written.retain(|(other, _)| other != key);
            written.push((key.clone(), value.clone()));
        }
    }

    /// The value an own write put into `key` of `selected` (`None` when it
    /// removed the prop), while the rows still show the targets resolved
    /// before that write.
    pub(crate) fn written_value(
        &self,
        selected: Option<&str>,
        key: &PropKey,
    ) -> Option<&Option<EditValue>> {
        let (layer, written) = self.written.as_ref().filter(|_| self.stale)?;
        if Some(layer.as_str()) != selected {
            return None;
        }
        written
            .iter()
            .find(|(other, _)| other == key)
            .map(|(_, value)| value)
    }

    /// The timeline frame of the targets the rows of `selected` show: the
    /// newest resolution's, even while a newer lookup (after a seek, say)
    /// is being resolved.
    pub(crate) fn shown_frame(&self, selected: Option<&str>) -> Option<i64> {
        self.resolved(selected)
            .map(|(resolved, _)| resolved.timeline_frame)
    }

    /// The newest resolution for `selected` failed and no write changed the
    /// file since: a commit has nothing to wait for. Anything else (being
    /// read, or an own write reloading) becomes current again.
    /// Also after the code failed to reload: nothing reads the file again
    /// until the next reload; and once the preview worker stopped.
    pub(crate) fn failed(&self, selected: Option<&str>) -> bool {
        self.reload_failed
            || self.preview_stopped
            || self.resolved(selected).is_some_and(|(resolved, current)| {
                current && !self.pending && resolved.outcome.is_err()
            })
    }

    /// The code failed to reload after an own write: the targets stay
    /// stale until the next reload, and a commit fails instead of waiting.
    pub(crate) fn reload_failed(&mut self) {
        self.reload_failed = true;
    }

    /// The preview worker stopped: the targets never become current again,
    /// and a commit fails instead of waiting, for the rest of the session.
    pub(crate) fn preview_stopped(&mut self) {
        self.preview_stopped = true;
    }

    pub(crate) fn preview_has_stopped(&self) -> bool {
        self.preview_stopped
    }

    /// Why a commit for `selected` has no targets to write to.
    pub(crate) fn unavailable(&self, selected: Option<&str>) -> String {
        match self.resolved(selected) {
            _ if self.preview_stopped => PREVIEW_STOPPED.to_owned(),
            _ if self.reload_failed => RELOAD_FAILED.to_owned(),
            _ if self.pending => STILL_READING.to_owned(),
            None => STILL_READING.to_owned(),
            Some((resolved, _)) if resolved.outcome.is_err() => {
                resolved.outcome.as_ref().err().cloned().unwrap_or_default()
            }
            Some(_) => "Select the layer again".to_owned(),
        }
    }

    /// The newest resolution for `selected`, and whether it may be committed
    /// against (no write changed the file since).
    pub(crate) fn resolved(&self, selected: Option<&str>) -> Option<(&Resolved, bool)> {
        self.resolved
            .as_ref()
            .filter(|resolved| Some(resolved.layer.as_str()) == selected)
            .map(|resolved| (resolved, !self.stale))
    }

    /// The targets a commit for `selected` writes to, with the report they
    /// were resolved for. `lookup` is the view's current lookup answer and
    /// the timeline frame it was looked up at: targets resolved for another
    /// answer (before a seek, say, while the new one resolves) are not
    /// committed to.
    pub(crate) fn targets(
        &self,
        selected: Option<&str>,
        lookup: Option<(&LayerSourceReport, i64)>,
    ) -> Option<(&ElementTargets, &LayerSourceReport)> {
        let (resolved, current) = self.resolved(selected)?;
        let (report, frame) = lookup?;
        if !current || *report != resolved.report || frame != resolved.timeline_frame {
            return None;
        }
        let targets = resolved.outcome.as_ref().ok()?;
        Some((targets, &resolved.report))
    }

    pub(crate) fn error(&self, selected: Option<&str>) -> Option<&str> {
        self.error
            .as_ref()
            .filter(|(layer, _)| Some(layer.as_str()) == selected)
            .map(|(_, message)| message.as_str())
    }

    pub(crate) fn set_error(&mut self, layer: Option<&str>, message: Option<String>) {
        self.error = layer
            .zip(message)
            .map(|(layer, message)| (layer.to_owned(), message));
    }
}

const STILL_READING: &str = "The source is still being read";

/// Why a commit made, or held, after the code failed to reload is not
/// written.
pub(crate) const RELOAD_FAILED: &str = "Not saved: the code failed to reload";

/// Why a commit made, or held, after the preview worker stopped is not
/// written.
pub(crate) const PREVIEW_STOPPED: &str = "Not saved: the preview stopped unexpectedly";

/// The timeline frame of the layer's local `keyframe`, given the timeline
/// frame the layer was looked up at and its local frame there.
pub(crate) fn keyframe_timeline_frame(keyframe: f64, lookup_frame: i64, local_frame: f64) -> i64 {
    (keyframe + lookup_frame as f64 - local_frame).round() as i64
}

/// The timeline frames of the keyframes before and after a target between
/// keyframes, converted with the candidate frame the resolver found them
/// from (not the report's `local_frame`, which may be another frame).
pub(crate) fn keyframe_jump_frames(target: &EditTarget, lookup_frame: i64) -> Option<(i64, i64)> {
    let EditTarget::Locked(LockReason::BetweenKeyframes {
        previous,
        next,
        frame,
    }) = target
    else {
        return None;
    };
    Some((
        keyframe_timeline_frame(*previous, lookup_frame, *frame),
        keyframe_timeline_frame(*next, lookup_frame, *frame),
    ))
}

impl EditorView {
    /// Starts resolving the selected layer's targets when its source lookup
    /// has an answer the targets were not resolved for. Reads the files and
    /// parses them on the background executor.
    pub(crate) fn sync_element_targets(&mut self, cx: &mut Context<Self>) {
        // A stale answer describes the code before the reload; resolving it
        // against the written file would pair the wrong report with it.
        if !self.is_react_preview() || self.source_lookup_stale {
            return;
        }
        let (Some(layer), Some(SourceLookup::Found(report))) =
            (self.selected_layer.as_deref(), self.source_lookup.as_ref())
        else {
            return;
        };
        let frame = self.source_lookup_frame;
        let Some(request) = self.element_edit.begin(layer, report, frame) else {
            return;
        };
        // The rows the runtime declares for the primitive; the transform
        // props alone without a table.
        let table = report
            .chain
            .first()
            .and_then(|element| self.primitive_table(&element.name))
            .cloned();
        // What `registerComponent()` declared types the Component section:
        // the composition's own, else the JSON project's entry's.
        let schemas = match &self.react_preview {
            Some(react) => react.component_schemas.clone(),
            None => self.component_schemas.clone(),
        };
        let report = report.clone();
        let entry_dir = self
            .react_entry()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        cx.spawn(async move |view, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let read = |path: &Path| {
                        fs::read_to_string(path)
                            .map_err(|error| format!("Couldn't read {}: {error}", path.display()))
                    };
                    let mut targets = resolve_targets(read, &report, table.as_ref())?;
                    type_component_props(&mut targets, &schemas, read);
                    let real = |path: &Path| fs::canonicalize(path).unwrap_or(path.to_path_buf());
                    let entry_dir = entry_dir.map(|dir| real(&dir));
                    lock_outside_project(&mut targets, |file| {
                        entry_dir
                            .as_ref()
                            .is_none_or(|dir| !inside_project(&real(file), dir))
                    });
                    Ok(targets)
                })
                .await;
            view.update(cx, |this, cx| {
                let selected = this.selected_layer.clone();
                if this
                    .element_edit
                    .finish(request, selected.as_deref(), outcome)
                {
                    // Commits made while the targets were read go now.
                    this.drain_held_rows(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The selected layer's targets and the report they were resolved for,
    /// when they can be committed to.
    // The canvas drag reads these too.
    pub(crate) fn element_targets(&self) -> Option<(&ElementTargets, &LayerSourceReport)> {
        let lookup = match &self.source_lookup {
            Some(SourceLookup::Found(report)) => Some((report, self.source_lookup_frame)),
            _ => None,
        };
        self.element_edit
            .targets(self.selected_layer.as_deref(), lookup)
    }

    /// The scene calls of the selected element read for the index in use
    /// and the element's call `S` (see [`current_calls`]); `None` until
    /// read, or once the index or the chain moved on.
    fn current_scene_calls(
        &self,
        targets: &ElementTargets,
        report: &LayerSourceReport,
    ) -> Option<&SceneCalls> {
        let element = self.indexed_element(report)?;
        let call = &targets.component.as_ref()?.call_site;
        current_calls(
            self.scene_calls.as_ref(),
            self.occurrence_generation,
            report,
            element,
            call,
        )
    }

    /// The occurrences of the element `report` describes, in the index in
    /// use (a refreshing one too).
    fn indexed_element(&self, report: &LayerSourceReport) -> Option<&ElementOccurrences> {
        let index = match self.occurrences.as_ref()? {
            (_, OccurrenceState::Ready(index) | OccurrenceState::Refreshing(index)) => index,
            (_, OccurrenceState::Scanning | OccurrenceState::Failed(_)) => return None,
        };
        index.instance_for(report).map(|(element, _)| element)
    }

    /// Starts reading the scene calls of the selected element (see
    /// [`read_scene_calls`]) when the index in use has it and the calls
    /// held are not for that index and chain. A sweep that lands starts it
    /// again.
    pub(crate) fn sync_scene_calls(&mut self, cx: &mut Context<Self>) {
        let Some((targets, report)) = self.element_targets() else {
            return;
        };
        let Some(call) = targets
            .component
            .as_ref()
            .map(|component| component.call_site.clone())
        else {
            return;
        };
        let Some(element) = self.indexed_element(report) else {
            return;
        };
        if self.current_scene_calls(targets, report).is_some() {
            return;
        }
        let generation = self.occurrence_generation;
        let at = calls_of(report, element, &call);
        let position = element.element.clone();
        let wanted = (position.clone(), generation, at.clone());
        if self.scene_calls_wanted.as_ref() == Some(&wanted) {
            return;
        }
        self.scene_calls_wanted = Some(wanted.clone());
        cx.spawn(async move |view, cx| {
            let calls = cx
                .background_executor()
                .spawn(async move {
                    let read = |path: &Path| fs::read_to_string(path).map_err(|e| e.to_string());
                    read_scene_calls(read, position, generation, at)
                })
                .await;
            view.update(cx, |this, cx| {
                if this.scene_calls_wanted.as_ref() == Some(&wanted) {
                    this.scene_calls = Some(calls);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The first lookup answer after a kept reload: the scope choice made
    /// for the element before it moves to where the answer puts it.
    pub(crate) fn carry_scope_choice(&mut self) {
        let Some(from) = self.scope_carry_from.take() else {
            return;
        };
        if let Some(SourceLookup::Found(report)) = &self.source_lookup
            && let Some(element) = report.chain.first()
        {
            self.scope_choices.carry(&from, &SourcePos::from(element));
        }
    }

    /// The scope choice remembered for the selected element.
    pub(crate) fn remembered_scope(&self) -> Option<Scope> {
        let (_, report) = self.element_targets()?;
        let element = report.chain.first()?;
        self.scope_choices.get(&SourcePos::from(element))
    }

    /// The selected element's targets, its scope state with the playhead
    /// where it is now (as the impact line reads it), and the scene calls
    /// that count; `None` without targets to commit to.
    fn scope_context(&self) -> Option<(&ElementTargets, ScopeState, Option<&SceneCalls>)> {
        let (targets, report) = self.element_targets()?;
        let calls = self.current_scene_calls(targets, report);
        let state = scope_state(
            report,
            targets,
            self.occurrences.as_ref().map(|(_, state)| state),
            calls,
            self.react_chapters().unwrap_or_default(),
            u64::try_from(self.clock.frame()).unwrap_or(0),
        );
        Some((targets, state, calls))
    }

    /// The gate for committing `keys` of the selected element with the
    /// choice `remembered` (see [`commit_gate`]); without targets, it goes
    /// (and fails as [`Self::commit_element`] says).
    pub(crate) fn commit_gate_for(
        &self,
        remembered: Option<Scope>,
        keys: &[PropKey],
    ) -> CommitGate {
        match self.scope_context() {
            Some((targets, state, _)) => commit_gate(&state, remembered, targets, keys),
            None => CommitGate::Go(None),
        }
    }

    /// The scope an edit of the selected element uses now, when its options
    /// are known and allow one.
    fn scope_in_effect(&self, state: &ScopeState) -> Option<Scope> {
        match state {
            ScopeState::Options(options) => chosen_scope(self.remembered_scope(), options).ok(),
            ScopeState::NotShared | ScopeState::Unknown { .. } => None,
        }
    }

    /// Why a drag, a scrub or an inline text edit can't write `key` of the
    /// selected element under the scope in effect (see [`gesture_lock`]).
    pub(crate) fn gesture_scope_lock(&self, key: &PropKey) -> Option<ScopeLock> {
        self.gesture_scope_locks(std::slice::from_ref(key))
            .pop()
            .flatten()
    }

    /// [`Self::gesture_scope_lock`] of each of `keys`, in order, the scope
    /// state read once.
    pub(crate) fn gesture_scope_locks(&self, keys: &[PropKey]) -> Vec<Option<ScopeLock>> {
        let Some((targets, state, calls)) = self.scope_context() else {
            return vec![None; keys.len()];
        };
        let scope = self.scope_in_effect(&state);
        keys.iter()
            .map(|key| gesture_lock(scope, targets, key, calls))
            .collect()
    }

    /// Whether the row `key` (`target` in `targets`) offers its removal:
    /// under the scope in effect, the removal [`scoped_removable`] allows.
    pub(crate) fn row_removable(
        &self,
        targets: &ElementTargets,
        key: &PropKey,
        target: &Target,
    ) -> bool {
        // Targets that are not current (being read again) have no scope.
        let scoped = self
            .scope_context()
            .filter(|(current, ..)| std::ptr::eq(*current, targets))
            .and_then(|(_, state, calls)| Some((self.scope_in_effect(&state)?, calls)));
        match scoped {
            Some((scope, calls)) => scoped_removable(scope, targets, key, calls),
            None => removable(targets, key, target),
        }
    }

    /// Writes `values` for the selected layer into its source as one undo
    /// entry (`None` removes the prop), as resolved (`scope: None`) or
    /// under `scope`: the call site's file too when a value passes through
    /// or is lifted. Then reloads keeping the selection. `done` gets the
    /// outcome (also shown in the Layer section when it failed).
    pub(crate) fn commit_element(
        &mut self,
        values: Vec<(PropKey, Option<EditValue>)>,
        scope: Option<Scope>,
        done: impl FnOnce(&mut Self, Result<(), String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let layer = self.selected_layer.clone();
        let Some((targets, report)) = self.element_targets() else {
            let message = self.element_edit.unavailable(layer.as_deref());
            self.element_edit
                .set_error(layer.as_deref(), Some(message.clone()));
            return done(self, Err(message), cx);
        };
        let calls = self.current_scene_calls(targets, report).cloned();
        let (targets, report) = (targets.clone(), report.clone());
        let written = values.clone();
        self.element_edit.set_error(layer.as_deref(), None);
        self.record_own_writes(
            targets.files.clone(),
            move |texts| {
                commit_files(texts, &targets, &report, scope, calls.as_ref(), &values)
                    .map_err(|error| error.to_string())
            },
            move |this, result, cx| {
                // A success invalidated the targets (`apply_own_write_sample`);
                // the rows show the written values until the reload resolves.
                match &result {
                    Ok(()) => this.element_edit.record_written(layer.as_deref(), &written),
                    Err(message) => this
                        .element_edit
                        .set_error(layer.as_deref(), Some(message.clone())),
                }
                done(this, result, cx);
            },
            cx,
        );
    }
}

/// Whether the editor may write `file`: it is under the React entry's
/// directory `entry_dir` and not inside any `node_modules`. Compares paths
/// lexically after resolving `.` and `..`; callers pass real paths.
pub(crate) fn inside_project(file: &Path, entry_dir: &Path) -> bool {
    let file = normalize(file);
    file.starts_with(normalize(entry_dir))
        && !file
            .components()
            .any(|component| component.as_os_str() == "node_modules")
}

fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

/// Locks every target, row, input, or component prop, in a file `outside`
/// says is outside the project (the element's, or a call site's), and
/// keeps those of the files read in [`ElementTargets::outside`].
pub(crate) fn lock_outside_project(targets: &mut ElementTargets, outside: impl Fn(&Path) -> bool) {
    targets.outside = targets
        .files
        .iter()
        .filter(|file| outside(file))
        .cloned()
        .collect();
    let lock = |target: &mut Target| {
        if outside(&target.file) {
            target.target = EditTarget::Locked(LockReason::OutsideProject);
        }
    };
    for (_, target) in &mut targets.targets {
        lock(target);
        for (_, input) in &mut target.inputs {
            lock(input);
        }
    }
    for prop in targets
        .component
        .iter_mut()
        .flat_map(|component| &mut component.props)
    {
        lock(&mut prop.target);
    }
}
