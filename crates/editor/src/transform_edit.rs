//! Editing the selected React layer's transform in its JSX source: which
//! source spans hold `x`, `y`, `rotation` and the scale props, the state the
//! Layer section and the canvas read, and the commit that writes new values.

use crate::layer_select::SourceLookup;
use crate::view::EditorView;
use celesta_react_bridge::{ElementKind, LayerSourceReport, ReceivedProps};
use celesta_source_edit::{
    ApplyError, EditTarget, ElementRef, LockReason, ResolveContext, apply, element_at,
    format_number, has_attribute, resolve,
};
use gpui_kit::Context;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

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

    /// The Layer section's row label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Rotation => "Rotation",
            Self::Scale => "Scale",
            Self::ScaleX => "Scale X",
            Self::ScaleY => "Scale Y",
        }
    }

    /// The value the runtime passed for this prop.
    pub(crate) fn received(self, props: &ReceivedProps) -> Option<f64> {
        match self {
            Self::X => props.x,
            Self::Y => props.y,
            Self::Rotation => props.rotation,
            Self::Scale => props.scale,
            Self::ScaleX => props.scale_x,
            Self::ScaleY => props.scale_y,
        }
    }
}

/// Where each transform prop of one element lives in its file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransformTargets {
    pub(crate) file: PathBuf,
    pub(crate) element: ElementRef,
    /// `X`, `Y`, `Rotation`, then `Scale` or `ScaleX`, `ScaleY`.
    pub(crate) targets: Vec<(TransformProp, EditTarget)>,
}

impl TransformTargets {
    pub(crate) fn target(&self, prop: TransformProp) -> Option<&EditTarget> {
        self.targets
            .iter()
            .find(|(candidate, _)| *candidate == prop)
            .map(|(_, target)| target)
    }
}

/// Resolves the transform targets of `report`'s innermost element in `text`
/// (that element's file). Scale is per axis when `scaleX` or `scaleY` is
/// written as an attribute of the element, and uniform otherwise (also when
/// only a parent sets them).
pub(crate) fn resolve_targets(
    text: &str,
    report: &LayerSourceReport,
) -> Result<TransformTargets, String> {
    let source = report
        .chain
        .first()
        .ok_or_else(|| "No source position".to_owned())?;
    let element = element_at(text, source.line, source.column)
        .ok_or_else(|| reason_text(&LockReason::ElementNotFound))?;
    let target = |prop: TransformProp| {
        let ctx = ResolveContext {
            primitive: source.kind == ElementKind::Primitive,
            candidate_frames: candidate_frames(report),
            received: prop.received(&report.props),
            render_count: report.render_count,
        };
        (prop, resolve(text, &element, prop.name(), &ctx))
    };
    let mut targets = vec![
        target(TransformProp::X),
        target(TransformProp::Y),
        target(TransformProp::Rotation),
    ];
    let per_axis = [TransformProp::ScaleX, TransformProp::ScaleY]
        .iter()
        .any(|prop| has_attribute(text, &element, prop.name()));
    if per_axis {
        targets.extend([target(TransformProp::ScaleX), target(TransformProp::ScaleY)]);
    } else {
        targets.push(target(TransformProp::Scale));
    }
    Ok(TransformTargets {
        file: source.file.clone(),
        element,
        targets,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommitError {
    /// The file no longer has the targets the edit started from.
    SourceChanged,
    /// The prop is locked or not a target of this element.
    NotEditable(TransformProp),
    /// The value is NaN or infinite.
    NotANumber,
    Apply(ApplyError),
}

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceChanged => f.write_str("The source changed during the edit"),
            Self::NotEditable(prop) => write!(f, "{} can't be edited here", prop.label()),
            Self::NotANumber => f.write_str("Enter a number"),
            Self::Apply(error) => write!(f, "Couldn't edit the source: {error}"),
        }
    }
}

/// The new text of `text` with `values` written, after checking `text` still
/// has the targets of `expected` (same kinds at the same spans).
pub(crate) fn commit_text(
    text: &str,
    expected: &TransformTargets,
    report: &LayerSourceReport,
    values: &[(TransformProp, f64)],
) -> Result<String, CommitError> {
    // A prop locked when the edit started (outside the project, say) stays
    // locked whatever the file holds now.
    if let Some(&(prop, _)) = values.iter().find(|(prop, _)| {
        !matches!(
            expected.target(*prop),
            Some(
                EditTarget::Literal { .. }
                    | EditTarget::Keyframe { .. }
                    | EditTarget::Missing { .. }
            )
        )
    }) {
        return Err(CommitError::NotEditable(prop));
    }
    let fresh = resolve_targets(text, report).map_err(|_| CommitError::SourceChanged)?;
    let unchanged = fresh.element == expected.element
        && fresh.targets.len() == expected.targets.len()
        && fresh
            .targets
            .iter()
            .zip(&expected.targets)
            .all(|((prop, now), (was_prop, was))| prop == was_prop && same_place(now, was));
    if !unchanged {
        return Err(CommitError::SourceChanged);
    }
    let edits = values
        .iter()
        .map(|&(prop, value)| {
            if !value.is_finite() {
                return Err(CommitError::NotANumber);
            }
            fresh
                .target(prop)
                .and_then(|target| target.to_edit(prop.name(), value))
                .ok_or(CommitError::NotEditable(prop))
        })
        .collect::<Result<Vec<_>, _>>()?;
    apply(text, &edits).map_err(CommitError::Apply)
}

/// Same kind at the same span; locked targets compare by reason kind.
fn same_place(a: &EditTarget, b: &EditTarget) -> bool {
    match (a, b) {
        (EditTarget::Literal { span: a, .. }, EditTarget::Literal { span: b, .. })
        | (EditTarget::Keyframe { span: a, .. }, EditTarget::Keyframe { span: b, .. }) => a == b,
        (EditTarget::Missing { insert_at: a, .. }, EditTarget::Missing { insert_at: b, .. }) => {
            a == b
        }
        (EditTarget::Locked(a), EditTarget::Locked(b)) => {
            std::mem::discriminant(a) == std::mem::discriminant(b)
        }
        _ => false,
    }
}

/// The Layer section's line for a locked target.
pub(crate) fn reason_text(reason: &LockReason) -> String {
    match reason {
        LockReason::SyntaxError => "The source has syntax errors".to_owned(),
        LockReason::ElementNotFound => "Couldn't find this element in the source".to_owned(),
        LockReason::NotPrimitive => "Set by a component, not written on this element".to_owned(),
        LockReason::Expression { source } => format!("Written as an expression: {source}"),
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
    }
}

/// A finished resolution, for the layer and report it was started for.
#[derive(Debug)]
pub(crate) struct Resolved {
    pub(crate) layer: String,
    pub(crate) report: LayerSourceReport,
    /// The timeline frame the report was looked up at.
    pub(crate) timeline_frame: i64,
    pub(crate) outcome: Result<TransformTargets, String>,
}

/// The selected layer's transform targets as the view keeps them. Resolution
/// runs off the UI thread; a result for an older request or another layer is
/// dropped.
#[derive(Debug, Default)]
pub(crate) struct TransformEdit {
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
    /// The values the editor's own writes put into the file, and the layer
    /// they belong to: shown in the rows while `resolved` is stale, so a
    /// field does not show the old value until the reload resolves.
    written: Option<(String, Vec<(TransformProp, f64)>)>,
    /// The last commit's error and the layer it belongs to.
    error: Option<(String, String)>,
}

impl TransformEdit {
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
        outcome: Result<TransformTargets, String>,
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
    }

    /// Forgets everything (selection cleared, or the code reloaded): the next
    /// lookup answer is resolved again even when it equals the last one, and
    /// an old commit error is gone.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            request: self.request + 1,
            ..Self::default()
        };
    }

    /// An own write for `layer` put `values` into the file (after
    /// `invalidate`). Kept until the reloaded code resolves; values for a
    /// prop written again replace the earlier ones.
    pub(crate) fn record_written(&mut self, layer: Option<&str>, values: &[(TransformProp, f64)]) {
        let Some(layer) = layer.filter(|_| self.stale) else {
            return;
        };
        if self.written.as_ref().is_none_or(|(was, _)| was != layer) {
            self.written = Some((layer.to_owned(), Vec::new()));
        }
        let Some((_, written)) = &mut self.written else {
            return;
        };
        for &(prop, value) in values {
            written.retain(|(other, _)| *other != prop);
            written.push((prop, value));
        }
    }

    /// The value an own write put into `prop` of `selected`, while the rows
    /// still show the targets resolved before that write.
    pub(crate) fn written_value(&self, selected: Option<&str>, prop: TransformProp) -> Option<f64> {
        let (layer, written) = self.written.as_ref().filter(|_| self.stale)?;
        if Some(layer.as_str()) != selected {
            return None;
        }
        written
            .iter()
            .find(|(other, _)| *other == prop)
            .map(|(_, value)| *value)
    }

    /// Why a commit for `selected` has no targets to write to.
    pub(crate) fn unavailable(&self, selected: Option<&str>) -> String {
        match self.resolved(selected) {
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
    ) -> Option<(&TransformTargets, &LayerSourceReport)> {
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

/// The frames an `interpolate(frame, …)` written for `report`'s element could
/// read; an older runtime that reports none reads its `local_frame`.
fn candidate_frames(report: &LayerSourceReport) -> Vec<f64> {
    if report.candidate_frames.is_empty() {
        vec![report.local_frame]
    } else {
        report.candidate_frames.clone()
    }
}

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
    /// Starts resolving the selected layer's transform targets when its
    /// source lookup has an answer the targets were not resolved for. Reads
    /// the file and parses it on the background executor.
    pub(crate) fn sync_transform_targets(&mut self, cx: &mut Context<Self>) {
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
        let Some(request) = self.transform.begin(layer, report, frame) else {
            return;
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
                    let file = report
                        .chain
                        .first()
                        .map(|source| source.file.clone())
                        .ok_or_else(|| "No source position".to_owned())?;
                    let text = fs::read_to_string(&file)
                        .map_err(|error| format!("Couldn't read {}: {error}", file.display()))?;
                    let mut targets = resolve_targets(&text, &report)?;
                    let real = |path: &Path| fs::canonicalize(path).unwrap_or(path.to_path_buf());
                    if entry_dir.is_none_or(|dir| !inside_project(&real(&file), &real(&dir))) {
                        lock_outside_project(&mut targets);
                    }
                    Ok(targets)
                })
                .await;
            view.update(cx, |this, cx| {
                let selected = this.selected_layer.clone();
                if this.transform.finish(request, selected.as_deref(), outcome) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The selected layer's transform targets and the report they were
    /// resolved for, when they can be committed to.
    // The canvas drag reads these too.
    pub(crate) fn transform_targets(&self) -> Option<(&TransformTargets, &LayerSourceReport)> {
        let lookup = match &self.source_lookup {
            Some(SourceLookup::Found(report)) => Some((report, self.source_lookup_frame)),
            _ => None,
        };
        self.transform
            .targets(self.selected_layer.as_deref(), lookup)
    }

    /// Writes `values` for the selected layer into its source as one undo
    /// entry, then reloads keeping the selection. `done` gets the outcome
    /// (also shown in the Layer section when it failed).
    pub(crate) fn commit_transform(
        &mut self,
        values: Vec<(TransformProp, f64)>,
        done: impl FnOnce(&mut Self, Result<(), String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let layer = self.selected_layer.clone();
        let Some((targets, report)) = self.transform_targets() else {
            let message = self.transform.unavailable(layer.as_deref());
            self.transform
                .set_error(layer.as_deref(), Some(message.clone()));
            return done(self, Err(message), cx);
        };
        let (targets, report) = (targets.clone(), report.clone());
        let written = values.clone();
        self.transform.set_error(layer.as_deref(), None);
        self.record_own_write(
            targets.file.clone(),
            move |text| {
                commit_text(text, &targets, &report, &values).map_err(|error| error.to_string())
            },
            move |this, result, cx| {
                // A success invalidated the targets (`apply_own_write_sample`);
                // the rows show the written values until the reload resolves.
                match &result {
                    Ok(()) => this.transform.record_written(layer.as_deref(), &written),
                    Err(message) => this
                        .transform
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

/// Locks every target: the element's file is outside the project.
pub(crate) fn lock_outside_project(targets: &mut TransformTargets) {
    for (_, target) in &mut targets.targets {
        *target = EditTarget::Locked(LockReason::OutsideProject);
    }
}
