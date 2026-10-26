//! Writing committed values into the source: the edits for each row, as
//! resolved or under an edit scope, checked against the spans the rows
//! were resolved from, and the rows resolved again against the written
//! text.

use super::resolve::{
    RowRead, WindowTargets, apply_component_schema, editable, resolve_rows, resolve_window,
};
use super::{ElementTargets, PropKey, Target, lock_outside_project};
use crate::edit_history::Texts;
use crate::element_rows::removable;
use crate::scope::{SceneCalls, Scope, ScopeLock, ScopedRow, lift_scope_lock, scoped_row};
use celesta_react_bridge::LayerSourceReport;
use celesta_source_edit::{
    ApplyError, DefaultTarget, Edit, EditTarget, EditValue, ElementRef, LiftProp, LockReason,
    apply, expression, parses, plan_lift_avoiding, removal, written_attributes,
};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommitError {
    /// The file no longer has the targets the edit started from.
    SourceChanged,
    /// The row (by its label) is locked or not a target of this element.
    NotEditable(String),
    /// The row (by its label) has no written value it can do without: the
    /// prop is required, absent, locked, or animated.
    NotRemovable(String),
    /// The value is NaN or infinite.
    NotANumber,
    Apply(ApplyError),
    /// The edited text has syntax errors; nothing is written.
    Unparseable,
    /// The row can't be written under the chosen scope.
    Scope(ScopeLock),
}

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceChanged => f.write_str("The source changed while editing"),
            Self::NotEditable(label) => write!(f, "{label} can't be edited here"),
            Self::NotRemovable(label) => write!(f, "{label} can't be removed here"),
            Self::NotANumber => f.write_str("Enter a number"),
            Self::Apply(error) => write!(f, "Couldn't edit the source: {error}"),
            Self::Unparseable => f.write_str("Couldn't edit the source: the result does not parse"),
            Self::Scope(lock) => f.write_str(&lock.text()),
        }
    }
}

/// The selected element's commit: [`commit_scoped`], resolving the rows
/// `expected` has again for `report` (for the element as one instance, the
/// report [`super::commit_report`] gives), typed by the schema they were
/// typed with.
pub(crate) fn commit_files(
    texts: &Texts,
    expected: &ElementTargets,
    report: &LayerSourceReport,
    scope: Option<Scope>,
    calls: Option<&SceneCalls>,
    values: &[(PropKey, Option<EditValue>)],
) -> Result<Vec<(PathBuf, String)>, CommitError> {
    let resolve_again = |texts: &Texts| {
        let read = |path: &Path| {
            texts
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{} was not read", path.display()))
        };
        let rows = |_: &str, _: &ElementRef| {
            expected
                .targets
                .iter()
                .filter_map(|(key, _)| match key {
                    PropKey::Prop(name) => Some(name.clone()),
                    PropKey::Input { .. } | PropKey::CallSite(_) => None,
                })
                .collect()
        };
        let mut fresh = resolve_rows(
            read,
            report,
            expected.table.as_ref(),
            rows,
            RowRead::Received,
        )
        .map_err(|_| CommitError::SourceChanged)?;
        // Typed by the schema the targets were typed with, as when resolved.
        if let Some(schema) = expected
            .component
            .as_ref()
            .and_then(|component| component.schema.clone())
        {
            apply_component_schema(&mut fresh, schema, read);
        }
        Ok(fresh)
    };
    commit_scoped(texts, expected, resolve_again, scope, calls, values)
}

/// [`commit_scoped`] for the timing window `window` around the element
/// `report` describes: its rows resolved again with [`resolve_window`].
// Called by the band drag, which is not drawn yet.
#[cfg_attr(not(test), expect(dead_code))]
pub(crate) fn window_files(
    texts: &Texts,
    window: &WindowTargets,
    report: &LayerSourceReport,
    scope: Option<Scope>,
    calls: Option<&SceneCalls>,
    values: &[(PropKey, Option<EditValue>)],
) -> Result<Vec<(PathBuf, String)>, CommitError> {
    let resolve_again = |texts: &Texts| {
        let read = |path: &Path| {
            texts
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{} was not read", path.display()))
        };
        resolve_window(read, report, &window.source)
            .map(|fresh| fresh.targets)
            .map_err(|_| CommitError::SourceChanged)
    };
    commit_scoped(texts, &window.targets, resolve_again, scope, calls, values)
}

/// The new text of every file `values` touch, in path order, for the
/// element `expected` describes. Each value is checked against `expected`
/// (editable, removable, or allowed under `scope`); `resolve_again` resolves
/// the element against `texts` (each of `expected.files`), and the commit is
/// refused unless every target (rows, inputs, component props) is the same
/// kind at the same span in the same file. Then the values are written as
/// resolved (`scope: None`, see [`resolved_edit`]) or under `scope` (see
/// [`scoped_edits`]). Nothing is returned for a file no value is written to.
pub(crate) fn commit_scoped(
    texts: &Texts,
    expected: &ElementTargets,
    resolve_again: impl FnOnce(&Texts) -> Result<ElementTargets, CommitError>,
    scope: Option<Scope>,
    calls: Option<&SceneCalls>,
    values: &[(PropKey, Option<EditValue>)],
) -> Result<Vec<(PathBuf, String)>, CommitError> {
    // A prop locked when the edit started (outside the project, say) stays
    // locked whatever the file holds now.
    for (key, value) in values {
        check(expected, key, value.as_ref(), scope, calls)?;
    }
    let mut fresh = resolve_again(texts)?;
    let was = expected.entries();
    let outside: Vec<&Path> = was
        .iter()
        .filter(|(_, was)| was.target == EditTarget::Locked(LockReason::OutsideProject))
        .map(|(_, was)| was.file.as_path())
        .chain(expected.outside.iter().map(PathBuf::as_path))
        .collect();
    lock_outside_project(&mut fresh, |file| outside.contains(&file));
    let now = fresh.entries();
    let unchanged = fresh.element == expected.element
        && now.len() == was.len()
        && now.iter().zip(&was).all(|((key, now), (was_key, was))| {
            key == was_key
                && now.file == was.file
                && now.path == was.path
                && same_place(&now.target, &was.target)
        });
    if !unchanged {
        return Err(CommitError::SourceChanged);
    }
    let edits = match scope {
        None => {
            let mut edits: BTreeMap<PathBuf, Vec<Edit>> = BTreeMap::new();
            for (key, value) in values {
                let edit = resolved_edit(texts, &fresh, key, value.as_ref())?;
                edits
                    .entry(fresh.file_of(key).to_path_buf())
                    .or_default()
                    .push(edit);
            }
            edits
        }
        Some(scope) => scoped_edits(texts, &fresh, scope, calls, values)?,
    };
    edits
        .into_iter()
        .map(|(file, edits)| {
            // Resolving read every file a target is in.
            let text = texts.get(&file).ok_or(CommitError::SourceChanged)?;
            let written = apply_checked(text, &edits)?;
            Ok((file, written))
        })
        .collect()
}

/// Whether `value` (`None` removes the prop) may be committed to `key` of
/// `targets`, as resolved or under `scope`: a lifted or a default's value
/// is written (even when the value at `S` can't be), never removed; a
/// locked row refuses a value with its lock, and a removal too unless it
/// removes as resolved (the value this scene sets itself, at `S`).
fn check(
    targets: &ElementTargets,
    key: &PropKey,
    value: Option<&EditValue>,
    scope: Option<Scope>,
    calls: Option<&SceneCalls>,
) -> Result<(), CommitError> {
    let row = scope.map(|scope| scoped_row(scope, targets, key, calls));
    if let Some(locked @ ScopedRow::Locked(lock)) = &row
        && (value.is_some() || !locked.removes_as_resolved())
    {
        return Err(CommitError::Scope(lock.clone()));
    }
    let entry = targets.entry(key);
    match (value, &row) {
        (Some(_), _) => {
            let written = matches!(row, Some(ScopedRow::Lift | ScopedRow::Default { .. }))
                || entry.is_some_and(|target| editable(&target.target));
            if !written {
                return Err(CommitError::NotEditable(key.label()));
            }
        }
        // `scoped_removable`, with the row already read.
        (None, Some(row)) if !row.removes_as_resolved() => {
            return Err(CommitError::NotRemovable(key.label()));
        }
        (None, _) if !entry.is_some_and(|target| removable(targets, key, target)) => {
            return Err(CommitError::NotRemovable(key.label()));
        }
        (None, _) => {}
    }
    Ok(())
}

/// Refuses a value that is NaN or infinite.
fn finite(value: &EditValue) -> Result<&EditValue, CommitError> {
    if value.as_number().is_some_and(|value| !value.is_finite()) {
        return Err(CommitError::NotANumber);
    }
    Ok(value)
}

/// The edit that writes `value` to `key` where it resolved in `targets`
/// (`None` removes the prop, see [`removal_of`]). An `Input` value is
/// written to its input's target.
fn resolved_edit(
    texts: &Texts,
    targets: &ElementTargets,
    key: &PropKey,
    value: Option<&EditValue>,
) -> Result<Edit, CommitError> {
    match value {
        Some(value) => {
            let value = finite(value)?;
            // `None` for a keyframe given anything but a number, too.
            targets
                .entry(key)
                .and_then(|target| target.target.to_edit(&target.path, value))
                .ok_or_else(|| CommitError::NotEditable(key.label()))
        }
        None => {
            removal_of(targets, key, texts).ok_or_else(|| CommitError::NotRemovable(key.label()))
        }
    }
}

/// Per file, the edits for `values` under `scope` on the freshly resolved
/// targets `fresh` (see [`scoped_row`]): as resolved; for every row lifted,
/// one [`plan_lift`] of them all in `C`'s file and an attribute per new
/// parameter at `S`; the parameter's default for a "Shared value" of a
/// pass-through (written after the bound name when it has none). A locked
/// row refuses the commit with its lock, except the removal of a value this
/// scene sets itself, which goes at `S` as resolved.
pub(crate) fn scoped_edits(
    texts: &Texts,
    fresh: &ElementTargets,
    scope: Scope,
    calls: Option<&SceneCalls>,
    values: &[(PropKey, Option<EditValue>)],
) -> Result<BTreeMap<PathBuf, Vec<Edit>>, CommitError> {
    let mut edits: BTreeMap<PathBuf, Vec<Edit>> = BTreeMap::new();
    let mut lifted: Vec<(&Target, &EditValue)> = Vec::new();
    for (key, value) in values {
        let not_removable = || CommitError::NotRemovable(key.label());
        match scoped_row(scope, fresh, key, calls) {
            ScopedRow::AsResolved => {
                let edit = resolved_edit(texts, fresh, key, value.as_ref())?;
                edits
                    .entry(fresh.file_of(key).to_path_buf())
                    .or_default()
                    .push(edit);
            }
            // A value this scene sets itself: only its removal, at `S`.
            row @ ScopedRow::Locked(_) if value.is_none() && row.removes_as_resolved() => {
                let edit = resolved_edit(texts, fresh, key, None)?;
                edits
                    .entry(fresh.file_of(key).to_path_buf())
                    .or_default()
                    .push(edit);
            }
            ScopedRow::Lift => {
                let value = finite(value.as_ref().ok_or_else(not_removable)?)?;
                let target = fresh
                    .entry(key)
                    .ok_or_else(|| CommitError::NotEditable(key.label()))?;
                lifted.push((target, value));
            }
            ScopedRow::Default { .. } => {
                let value = finite(value.as_ref().ok_or_else(not_removable)?)?;
                let edit = default_edit(fresh, key, value)
                    .ok_or_else(|| CommitError::NotEditable(key.label()))?;
                edits.entry(fresh.file.clone()).or_default().push(edit);
            }
            ScopedRow::Locked(lock) => return Err(CommitError::Scope(lock)),
        }
    }
    if !lifted.is_empty() {
        lift_edits(texts, fresh, &lifted, &mut edits)?;
    }
    Ok(edits)
}

/// The edit that writes `value` as the default of the parameter `key`
/// passes through to: its literal replaced, or ` = value` after the bound
/// name when it has none. `None` for anything else.
fn default_edit(targets: &ElementTargets, key: &PropKey, value: &EditValue) -> Option<Edit> {
    let EditTarget::PassThrough { param, .. } = &targets.entry(key)?.target else {
        return None;
    };
    let (_, default) = targets.defaults.iter().find(|(name, _)| name == param)?;
    match default {
        DefaultTarget::Literal { span, form, .. } => Some(Edit::Replace {
            start: span.start,
            end: span.end,
            value: value.clone(),
            form: *form,
        }),
        DefaultTarget::Absent { insert_at } => Some(Edit::Splice {
            start: *insert_at,
            end: *insert_at,
            text: format!(" = {}", expression(value)),
        }),
        DefaultTarget::Expression | DefaultTarget::NoPattern => None,
    }
}

/// Adds to `edits` lifting every `(target, value)` of `lifted` (rows of
/// `targets`) into new props of `C` together, and passing each new prop's
/// value at `S`; the new names avoid the attributes `S` writes already.
fn lift_edits(
    texts: &Texts,
    targets: &ElementTargets,
    lifted: &[(&Target, &EditValue)],
    edits: &mut BTreeMap<PathBuf, Vec<Edit>>,
) -> Result<(), CommitError> {
    let enclosing = targets.enclosing.as_deref().unwrap_or_default();
    let component = targets.component.as_ref().ok_or_else(|| {
        CommitError::Scope(ScopeLock::NoCall {
            component: enclosing.to_owned(),
        })
    })?;
    let text = texts.get(&targets.file).ok_or(CommitError::SourceChanged)?;
    let props: Vec<LiftProp<'_>> = lifted
        .iter()
        .map(|(target, value)| LiftProp {
            prop: &target.path,
            target: &target.target,
            value,
        })
        .collect();
    let at_s = texts
        .get(&component.call_site.file)
        .ok_or(CommitError::SourceChanged)?;
    let written = written_attributes(at_s, &component.call);
    let lift = plan_lift_avoiding(text, &targets.element, &props, &written)
        .map_err(|lock| CommitError::Scope(lift_scope_lock(&lock, enclosing)))?;
    edits
        .entry(targets.file.clone())
        .or_default()
        .extend(lift.edits);
    let at_call = edits.entry(component.call_site.file.clone()).or_default();
    for (param, (_, value)) in lift.params.into_iter().zip(lifted) {
        at_call.push(Edit::Insert {
            at: component.call.end_of_attributes,
            name: param,
            value: (*value).clone(),
        });
    }
    Ok(())
}

/// `text` with `edits` applied, refused when the result does not parse: a
/// commit never writes a source it broke, whatever the edit.
pub(crate) fn apply_checked(text: &str, edits: &[Edit]) -> Result<String, CommitError> {
    let written = apply(text, edits).map_err(CommitError::Apply)?;
    if parses(&written) {
        Ok(written)
    } else {
        Err(CommitError::Unparseable)
    }
}

/// The edit that removes the prop of `key` from `targets`, in the text of
/// the file it is written in: a pass-through's at its call site, by the
/// parameter's name; a component prop's at the component's call. `None`
/// for an input, or when the text holds nothing [`removal`] can delete.
fn removal_of(targets: &ElementTargets, key: &PropKey, texts: &Texts) -> Option<Edit> {
    let target = targets.entry(key)?;
    let text = texts.get(&target.file)?;
    match (key, &target.target) {
        (PropKey::Input { .. }, _) => None,
        (
            _,
            EditTarget::PassThrough {
                call_site, param, ..
            },
        ) => removal(text, call_site, param),
        (PropKey::CallSite(name), _) => removal(text, &targets.component.as_ref()?.call, name),
        (PropKey::Prop(_), _) => removal(text, &targets.element, &target.path),
    }
}

/// Same kind at the same span; locked targets compare by reason kind, and
/// pass-through targets by call site, parameter and target there.
fn same_place(a: &EditTarget, b: &EditTarget) -> bool {
    match (a, b) {
        (
            EditTarget::PassThrough {
                call_site: a_call,
                param: a_param,
                target: a,
            },
            EditTarget::PassThrough {
                call_site: b_call,
                param: b_param,
                target: b,
            },
        ) => a_call == b_call && a_param == b_param && same_place(a, b),
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
