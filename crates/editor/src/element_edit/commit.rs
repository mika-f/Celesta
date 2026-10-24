//! Writing committed values into the source: the edits for each row,
//! checked against the spans the rows were resolved from, and the rows
//! resolved again against the written text.

use super::resolve::{apply_component_schema, editable, resolve_rows};
use super::{ElementTargets, PropKey, lock_outside_project};
use crate::edit_history::Texts;
use crate::element_rows::removable;
use celesta_react_bridge::LayerSourceReport;
use celesta_source_edit::{
    ApplyError, Edit, EditTarget, EditValue, ElementRef, LockReason, apply, parses, removal,
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
        }
    }
}

/// The new text of every file `values` touch, in path order, after checking
/// `texts` (each of `expected.files`) still have the targets of `expected`:
/// its rows and their inputs, resolved again, are the same kinds at the
/// same spans, in the same files. An `Input` value is written to its
/// input's target. A `None` value removes the prop (see [`removable`]).
/// Nothing is returned for a file no value is written to.
pub(crate) fn commit_files(
    texts: &Texts,
    expected: &ElementTargets,
    report: &LayerSourceReport,
    values: &[(PropKey, Option<EditValue>)],
) -> Result<Vec<(PathBuf, String)>, CommitError> {
    // A prop locked when the edit started (outside the project, say) stays
    // locked whatever the file holds now.
    for (key, value) in values {
        let entry = expected.entry(key);
        match value {
            Some(_) if !entry.is_some_and(|target| editable(&target.target)) => {
                return Err(CommitError::NotEditable(key.label()));
            }
            None if !entry.is_some_and(|target| removable(expected, key, target)) => {
                return Err(CommitError::NotRemovable(key.label()));
            }
            _ => {}
        }
    }
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
    let mut fresh = resolve_rows(read, report, expected.table.as_ref(), rows)
        .map_err(|_| CommitError::SourceChanged)?;
    // Typed by the schema the targets were typed with, as when resolved.
    if let Some(schema) = expected
        .component
        .as_ref()
        .and_then(|component| component.schema.clone())
    {
        apply_component_schema(&mut fresh, schema, read);
    }
    let was = expected.entries();
    let outside: Vec<&Path> = was
        .iter()
        .filter(|(_, was)| was.target == EditTarget::Locked(LockReason::OutsideProject))
        .map(|(_, was)| was.file.as_path())
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
    let mut edits: BTreeMap<&Path, Vec<Edit>> = BTreeMap::new();
    for (key, value) in values {
        let edit = match value {
            Some(value) => {
                if value.as_number().is_some_and(|value| !value.is_finite()) {
                    return Err(CommitError::NotANumber);
                }
                // `None` for a keyframe given anything but a number, too.
                fresh
                    .entry(key)
                    .and_then(|target| target.target.to_edit(&target.path, value))
                    .ok_or_else(|| CommitError::NotEditable(key.label()))?
            }
            None => removal_of(&fresh, key, texts)
                .ok_or_else(|| CommitError::NotRemovable(key.label()))?,
        };
        edits.entry(fresh.file_of(key)).or_default().push(edit);
    }
    edits
        .into_iter()
        .map(|(file, edits)| {
            // Resolving read every file a target is in.
            let text = texts.get(file).ok_or(CommitError::SourceChanged)?;
            let written = apply_checked(text, &edits)?;
            Ok((file.to_path_buf(), written))
        })
        .collect()
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
