//! The Properties tab's element rows: one row per prop of the selected
//! layer's property table (the transform props without one), each with a
//! control by type that writes the prop into the JSX source, and under an
//! expression row the inputs it reads. This module holds the pure helpers,
//! the row state, and the commit flow; `element_rows_view.rs` renders them.

use crate::element_edit::{
    ComponentTargets, ElementTargets, PREVIEW_STOPPED, PropKey, Target, edit_value, editable,
    field_of, inner_target, required_of, table_default, text_style_key,
};
use crate::inline_text_view::EditEvent;
use crate::layer_select::{SourceLookup, display_path};
use crate::property_controls::{
    Control, ControlEvent, ControlKind, build_control, parse_color, parse_color_input,
    parse_number_input, take_picked_color,
};
use crate::property_panel::field_label;
use crate::scope::{CommitGate, Scope, ScopedRow, scoped_row};
use crate::transform_drag::override_sites;
use crate::view::EditorView;
use celesta_react_bridge::{
    LayerSourceReport, PrimitivePropertyField, PropOverride, ReceivedProps, SourcePos, import_path,
};
use celesta_source_edit::{EditTarget, EditValue, LockReason, default_for, format_number};
use gpui_kit::{Context, MouseMoveEvent, PathPromptOptions, Subscription, Window};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// The control kind of a table field.
pub(crate) fn control_kind(field: &PrimitivePropertyField) -> ControlKind {
    match field {
        PrimitivePropertyField::Number { .. } => ControlKind::Number,
        PrimitivePropertyField::String { .. } => ControlKind::String,
        PrimitivePropertyField::Text { .. } => ControlKind::Text,
        PrimitivePropertyField::Boolean { .. } => ControlKind::Boolean,
        PrimitivePropertyField::Color { .. } => ControlKind::Color,
        PrimitivePropertyField::Select { options, .. } => ControlKind::Select(options.clone()),
        PrimitivePropertyField::Path => ControlKind::Path,
    }
}

/// The row label: the table key in words (`cornerRadius` → "Corner radius",
/// `children` → "Text", `scaleX` → "Scale X", `x` → "X", `anchorX` →
/// "Anchor X").
pub(crate) fn row_label(key: &str) -> String {
    match key {
        "children" => return "Text".to_owned(),
        "src" => return "Source".to_owned(),
        _ => {}
    }
    let mut words: Vec<String> = Vec::new();
    for c in key.chars() {
        match words.last_mut() {
            Some(word) if !c.is_uppercase() => word.push(c),
            _ => words.push(c.to_string()),
        }
    }
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let mut chars = word.chars();
            match chars.next() {
                // A one-letter word is an axis: "Scale X".
                Some(first) if index == 0 || word.len() == 1 => {
                    first.to_uppercase().chain(chars).collect()
                }
                _ => word.to_lowercase(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// The label of the Component-section row for the prop `name`: the `label`
/// its `registerComponent()` schema gives it, else the name as written
/// (the author's identifier, not put into words as [`row_label`] does).
pub(crate) fn component_row_label(component: &ComponentTargets, name: &str) -> String {
    component
        .schema
        .as_ref()
        .and_then(|schema| schema.get(name))
        .map_or(name, |entry| field_label(name, &entry.field))
        .to_owned()
}

/// How a value shows in a row's text: numbers as written to the source,
/// strings as they are, booleans as `true`/`false`.
pub(crate) fn value_text(value: &EditValue) -> String {
    match value {
        EditValue::Number(value) => format_number(*value),
        EditValue::String(value) => value.clone(),
        EditValue::Bool(value) => value.to_string(),
    }
}

/// What a row shows for `target`: the value, and the dimmed placeholder of
/// an absent prop (Rulings: source default, received, table default).
pub(crate) fn row_text(
    key: &str,
    target: &Target,
    field: Option<&PrimitivePropertyField>,
    report: &LayerSourceReport,
) -> (String, String) {
    shown_text(
        key,
        &target.path,
        &target.target,
        field,
        Some(&report.props),
    )
}

/// [`row_text`] for `target` at `path`; `props` are the element's received
/// props, `None` for a value the element does not receive itself (an
/// input, a component's prop).
pub(crate) fn shown_text(
    key: &str,
    path: &str,
    target: &EditTarget,
    field: Option<&PrimitivePropertyField>,
    props: Option<&ReceivedProps>,
) -> (String, String) {
    let (value, placeholder) = shown_value(key, path, target, field, props);
    let text = value.as_ref().map(value_text).unwrap_or_default();
    if placeholder {
        (String::new(), text)
    } else {
        (text, String::new())
    }
}

/// The value [`shown_text`] shows, unrounded, and whether it shows as the
/// dimmed placeholder of an absent prop.
fn shown_value(
    key: &str,
    path: &str,
    target: &EditTarget,
    field: Option<&PrimitivePropertyField>,
    props: Option<&ReceivedProps>,
) -> (Option<EditValue>, bool) {
    let received = |path: &str| props.and_then(|props| props.get(path)).map(edit_value);
    // A scale axis absent from the element is drawn at the `scale` it
    // received (`<Rect scale={2} scaleX={3} />` draws Y at 2).
    let axis = || {
        matches!(key, "scaleX" | "scaleY")
            .then(|| received("scale"))
            .flatten()
    };
    // What an absent prop is drawn with when the source sets no default; a
    // primitive's own defaults only for the element's props.
    let absent = || {
        axis()
            .or_else(|| field.and_then(table_default))
            .or_else(|| props.and_then(|_| default_for(key)))
    };
    let placeholder = |value: Option<EditValue>| (value, true);
    match target {
        EditTarget::Literal { value, .. } => (Some(value.clone()), false),
        EditTarget::Keyframe { value, .. } => (Some(EditValue::Number(*value)), false),
        EditTarget::Missing {
            default: Some(default),
            ..
        } => placeholder(axis().or_else(|| Some(default.clone()))),
        // A parameter without a literal default: the element draws what it
        // received (`{ scale = base * 2 }`), or its own default.
        EditTarget::Missing { default: None, .. } => placeholder(received(path).or_else(absent)),
        EditTarget::PassThrough { target, .. } => match inner_target(target) {
            // The parameter's default is what the element is drawn with.
            EditTarget::Missing {
                default: Some(default),
                ..
            } => placeholder(Some(default.clone())),
            inner => shown_value(key, path, inner, field, props),
        },
        // A `Text` font or color row `style` may set shows what `style`
        // gave it, which is what is drawn; the shorthand's when none.
        EditTarget::Locked(LockReason::StyleMaySet) => {
            let style = text_style_key(key).and_then(|style| received(&format!("style.{style}")));
            match style.or_else(|| received(path)) {
                Some(value) => (Some(value), false),
                None => placeholder(absent()),
            }
        }
        EditTarget::Locked(_) => match received(path) {
            Some(value) => (Some(value), false),
            None => placeholder(absent()),
        },
    }
}

const COLOR_MESSAGE: &str = "Enter a color: #RRGGBB, #RRGGBBAA, or a theme color name";

/// Why a held commit to a keyframe was not written after a seek.
const FRAME_MOVED: &str = "Not saved: the playhead moved off this keyframe";

/// Why held commits were not written when the layer's source was not
/// found again (not in the frame, or no source position).
const SOURCE_LOST: &str = "Not saved: the layer's source was not found again";

/// A `<Theme>` color name: a letter, then letters, digits, `_` or `-`.
fn theme_color_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// Parses a typed number for a number `field`, inside its range and, with
/// `integer`, whole; the error says what to enter.
pub(crate) fn parse_field_number(
    field: Option<&PrimitivePropertyField>,
    text: &str,
) -> Result<f64, String> {
    let value = parse_number_input(text)?;
    let (min, max, integer) = match field {
        Some(PrimitivePropertyField::Number {
            min, max, integer, ..
        }) => (*min, *max, integer.unwrap_or(false)),
        _ => (None, None, false),
    };
    let inside = min.is_none_or(|min| value >= min)
        && max.is_none_or(|max| value <= max)
        && (!integer || value.fract() == 0.0);
    let what = if integer {
        "a whole number"
    } else {
        "a number"
    };
    match (min, max) {
        _ if inside => Ok(value),
        (Some(min), Some(max)) => Err(format!(
            "Enter {what} from {} to {}",
            format_number(min),
            format_number(max)
        )),
        (Some(min), None) => Err(format!("Enter {what} of at least {}", format_number(min))),
        (None, Some(max)) => Err(format!("Enter {what} of at most {}", format_number(max))),
        (None, None) => Err(format!("Enter {what}")),
    }
}

/// Parses typed text for `kind`; the error is the row's message.
pub(crate) fn parse_row_value(
    kind: &ControlKind,
    field: Option<&PrimitivePropertyField>,
    text: &str,
) -> Result<EditValue, String> {
    match kind {
        ControlKind::Number => parse_field_number(field, text).map(EditValue::Number),
        ControlKind::String | ControlKind::Text | ControlKind::Path => {
            Ok(EditValue::String(text.to_owned()))
        }
        ControlKind::Color => {
            let text = text.trim();
            if parse_color_input(text).is_ok() || theme_color_name(text) {
                Ok(EditValue::String(text.to_owned()))
            } else {
                Err(COLOR_MESSAGE.to_owned())
            }
        }
        ControlKind::Select(options) => {
            let text = text.trim();
            if options.iter().any(|option| option == text) {
                Ok(EditValue::String(text.to_owned()))
            } else {
                Err(format!("Choose one of {}", options.join(", ")))
            }
        }
        ControlKind::Boolean => match text.trim() {
            "true" => Ok(EditValue::Bool(true)),
            "false" => Ok(EditValue::Bool(false)),
            _ => Err("Enter true or false".to_owned()),
        },
    }
}

/// A pass-through row's line naming the call site its value is written at,
/// the file relative to the entry's directory as [`display_path`] shows it.
pub(crate) fn set_at_text(call_site: &SourcePos, entry_dir: &Path) -> String {
    format!(
        "Set at {}:{}",
        display_path(&call_site.file, entry_dir),
        call_site.line
    )
}

/// The field of a scale row without a table (the transform rows alone): a
/// ratio around 1, scrubbed as the tables declare `scale`.
static SCALE_WITHOUT_TABLE: PrimitivePropertyField = PrimitivePropertyField::Number {
    default_value: None,
    min: None,
    max: None,
    step: Some(0.01),
    integer: None,
};

/// The table field of the row `key`, or a component prop's registered
/// field; inputs have none. Without a table, only a scale row has one.
pub(crate) fn row_field<'t>(
    targets: &'t ElementTargets,
    key: &PropKey,
) -> Option<&'t PrimitivePropertyField> {
    match key {
        PropKey::Prop(name) => match &targets.table {
            Some(table) => field_of(table, name),
            None => matches!(name.as_str(), "scale" | "scaleX" | "scaleY")
                .then_some(&SCALE_WITHOUT_TABLE),
        },
        PropKey::Input { .. } => None,
        PropKey::CallSite(name) => targets
            .component
            .as_ref()?
            .props
            .iter()
            .find(|prop| prop.name == *name)?
            .field
            .as_ref(),
    }
}

/// Whether the row `key` can remove its prop, so the default applies again:
/// its value is written (on the element, in its `style` object, or at the
/// call a pass-through reads it from), and the props type does not require
/// it. Never an input (a value an expression reads, not a prop), nor a
/// `<Text>`'s children, an absent, locked, or animated prop. A component
/// prop, or a pass-through removed at the call, is optional unless the
/// component's `registerComponent()` schema declares it `required`; without
/// a schema, the component's own default applies.
pub(crate) fn removable(targets: &ElementTargets, key: &PropKey, target: &Target) -> bool {
    let optional = match key {
        PropKey::Prop(name) => {
            name != "children"
                && !targets
                    .table
                    .as_ref()
                    .is_some_and(|table| required_of(table, name))
                && !matches!(&target.target, EditTarget::PassThrough { param, .. }
                    if component_requires(targets, param))
        }
        PropKey::CallSite(name) => !component_requires(targets, name),
        PropKey::Input { .. } => false,
    };
    optional && matches!(inner_target(&target.target), EditTarget::Literal { .. })
}

/// Whether the schema registered for the call's component declares the prop
/// `name` required.
fn component_requires(targets: &ElementTargets, name: &str) -> bool {
    targets
        .component
        .as_ref()
        .and_then(|component| component.schema.as_ref())
        .and_then(|schema| schema.get(name))
        .is_some_and(|field| field.required)
}

/// What committing typed text in a row does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TypedText {
    /// Write the value; `None` removes the prop.
    Write(Option<EditValue>),
    /// Show the source's value again: an emptied row that can't remove its
    /// prop.
    Restore,
    /// The text does not parse; the row's message.
    Invalid(String),
}

/// Typed `text` in a row of `kind`: an emptied field removes the prop when
/// the row is `removable`, and otherwise writes nothing; anything else is
/// parsed by [`parse_row_value`].
pub(crate) fn typed_text(
    kind: &ControlKind,
    field: Option<&PrimitivePropertyField>,
    text: &str,
    removable: bool,
) -> TypedText {
    if text.trim().is_empty() {
        return if removable {
            TypedText::Write(None)
        } else {
            TypedText::Restore
        };
    }
    match parse_row_value(kind, field, text) {
        Ok(value) => TypedText::Write(Some(value)),
        Err(message) => TypedText::Invalid(message),
    }
}

/// What a row shows for a value it wrote, until the reload resolves: the
/// value, or for a removed prop the dimmed default that applies now (the
/// table's, else a transform prop's own; nothing when unknown).
pub(crate) fn written_text(
    key: &PropKey,
    field: Option<&PrimitivePropertyField>,
    written: Option<&EditValue>,
) -> (String, String) {
    match written {
        Some(value) => (value_text(value), String::new()),
        None => {
            let default = field.and_then(table_default).or_else(|| match key {
                PropKey::Prop(name) => default_for(name),
                PropKey::Input { .. } | PropKey::CallSite(_) => None,
            });
            (
                String::new(),
                default.as_ref().map(value_text).unwrap_or_default(),
            )
        }
    }
}

/// What a row shows when it writes a parameter's default (see
/// [`ScopedRow::Default`]): that default, which is the value it edits
/// whatever this scene passes; with none written yet, what the element
/// draws without one, dimmed (see [`written_text`]). `None` for any other
/// row, which shows as resolved.
pub(crate) fn default_row_text(
    row: &ScopedRow,
    key: &PropKey,
    field: Option<&PrimitivePropertyField>,
) -> Option<(String, String)> {
    match row {
        ScopedRow::Default { shown, .. } => Some(match shown {
            Some(value) => (value_text(value), String::new()),
            None => written_text(key, field, None),
        }),
        ScopedRow::AsResolved | ScopedRow::Lift | ScopedRow::Locked(_) => None,
    }
}

/// What "None" on a path row writes: an empty path over a written one, and
/// nothing for an absent prop (no `src=""` is inserted).
pub(crate) fn cleared_path(target: &EditTarget) -> Option<EditValue> {
    match inner_target(target) {
        EditTarget::Missing { .. } => None,
        _ => Some(EditValue::String(String::new())),
    }
}

/// The control a row edits its value with: its table field's, a number for
/// a transform prop without a table, the kind of an input's literal (or of
/// the parameter default it takes; a number otherwise), and the Component
/// section's kind for a component prop.
pub(crate) fn row_kind(targets: &ElementTargets, key: &PropKey, target: &Target) -> ControlKind {
    match key {
        PropKey::Prop(_) => row_field(targets, key).map_or(ControlKind::Number, control_kind),
        PropKey::Input { .. } => match inner_target(&target.target) {
            EditTarget::Literal { value, .. }
            | EditTarget::Missing {
                default: Some(value),
                ..
            } => match value {
                EditValue::Number(_) => ControlKind::Number,
                EditValue::String(_) => ControlKind::String,
                EditValue::Bool(_) => ControlKind::Boolean,
            },
            _ => ControlKind::Number,
        },
        PropKey::CallSite(name) => targets
            .component
            .iter()
            .flat_map(|component| &component.props)
            .find(|prop| prop.name == *name)
            .map_or(ControlKind::String, |prop| prop.kind.clone()),
    }
}

/// The value a scrub `dx` pixels from its start gives: `start + dx * step`,
/// `step` the field's declared step per pixel, else 0.01 for a field with
/// `max <= 1` (opacity, ratios), else 1, rounded for a whole-number field
/// (a start that is not whole too), clamped to `min`/`max`; Shift
/// multiplies the step by 10.
pub(crate) fn scrub_value(
    start: f64,
    dx: f32,
    field: Option<&PrimitivePropertyField>,
    shift: bool,
) -> f64 {
    let (min, max, step, integer) = match field {
        Some(PrimitivePropertyField::Number {
            min,
            max,
            step,
            integer,
            ..
        }) => (*min, *max, *step, integer.unwrap_or(false)),
        _ => (None, None, None, false),
    };
    let step = step.unwrap_or(if max.is_some_and(|max| max <= 1.0) {
        0.01
    } else {
        1.0
    });
    let step = if shift { step * 10.0 } else { step };
    let value = start + f64::from(dx) * step;
    let value = if integer { value.round() } else { value };
    let value = min.map_or(value, |min| value.max(min));
    max.map_or(value, |max| value.min(max))
}

/// Whether dragging the label of the row `key` scrubs its value: a number
/// the editor can write as one value. Not an input (the runtime overrides
/// props, and an input is a value an expression reads), nor a keyframe (no
/// single prop value to override); a locked row, between keyframes too,
/// can't be written at all.
pub(crate) fn scrubs(targets: &ElementTargets, key: &PropKey, target: &Target) -> bool {
    !matches!(key, PropKey::Input { .. })
        && row_kind(targets, key, target) == ControlKind::Number
        && editable(&target.target)
        && !matches!(inner_target(&target.target), EditTarget::Keyframe { .. })
}

/// Whether a scrub of the row `scrubbed` takes the focus to the editor, so
/// its keys reach it again (Cmd+Z undoes the scrub's write; a row's text
/// field takes Cmd+Z as its own undo). Not while another row holds typed
/// text (`edited`): that text stays in its focused field, neither lost nor
/// submitted. The scrubbed row's own typed text gives way to the scrub.
pub(crate) fn scrub_takes_focus<'k>(
    edited: impl IntoIterator<Item = &'k PropKey>,
    scrubbed: &PropKey,
) -> bool {
    edited.into_iter().all(|key| key == scrubbed)
}

/// A scrub in progress on one number row.
#[derive(Clone, Debug)]
pub(crate) struct Scrub {
    key: PropKey,
    /// Where the label was pressed, in window pixels.
    start_x: f32,
    start_value: f64,
    value: f64,
    /// The row's override, its position fixed for the scrub: the runtime
    /// remounts the entry when the positions change.
    sites: Vec<PropOverride>,
    /// The row's field (its range), kept from the start: the targets are
    /// gone while the write runs.
    field: Option<PrimitivePropertyField>,
    /// The pointer changed the value at least once.
    moved: bool,
    /// Released; the override stays while the write runs and, after a
    /// successful one, until its reload, as a released canvas drag's does.
    released: bool,
    /// The edit scope the control showed when the scrub started: it is
    /// written under it or refused (see `EditorView::commit_gesture_row`).
    scope: Option<Scope>,
}

impl Scrub {
    /// A scrub of the row `key` pressed at `start_x`, starting from the
    /// value the row shows (0 when it shows none); `None` when the row does
    /// not scrub.
    pub(crate) fn start(
        targets: &ElementTargets,
        report: &LayerSourceReport,
        key: &PropKey,
        start_x: f32,
    ) -> Option<Self> {
        let target = targets.entry(key)?;
        if !scrubs(targets, key, target) {
            return None;
        }
        let (_, site) = override_sites(targets, report)
            .into_iter()
            .find(|(candidate, _)| candidate == key)?;
        // The value or the placeholder the row shows, as the value it is
        // (not its text), moved inside the field's range and, for a
        // whole-number field, rounded: a pointer back at the start pixel
        // then gives the start value, which writes nothing.
        let field = row_field(targets, key).cloned();
        let shown = match entry_value(targets, key, target, report) {
            Some(EditValue::Number(value)) => value,
            _ => 0.0,
        };
        let start_value = scrub_value(shown, 0.0, field.as_ref(), false);
        Some(Self {
            key: key.clone(),
            start_x,
            start_value,
            value: start_value,
            sites: vec![site.with(start_value)],
            field,
            moved: false,
            released: false,
            scope: None,
        })
    }

    /// Takes the pointer at `x` (window pixels); true when the value
    /// changed.
    pub(crate) fn follow(&mut self, x: f32, shift: bool) -> bool {
        let dx = (x - self.start_x).round();
        let value = scrub_value(self.start_value, dx, self.field.as_ref(), shift);
        if value == self.value {
            return false;
        }
        self.value = value;
        self.moved = true;
        true
    }

    /// Whether there is a value to write: the pointer left the start value.
    pub(crate) fn changed(&self) -> bool {
        self.value != self.start_value
    }

    /// The prop overrides the preview evaluates the scene with: none before
    /// the value first changes, nor while `playing`; from then on the row's
    /// prop at the same position, also back at the start value.
    pub(crate) fn overrides(&self, playing: bool) -> Vec<PropOverride> {
        if playing || !self.moved {
            return Vec::new();
        }
        self.sites
            .iter()
            .map(|site| PropOverride {
                value: self.value,
                ..site.clone()
            })
            .collect()
    }

    /// The value the row `key` shows while this scrub changes it.
    fn shown(&self, key: &PropKey) -> Option<f64> {
        (self.moved && self.key == *key).then_some(self.value)
    }
}

/// What a row shows for `key`, as [`row_text`] does for a row, and without
/// received props for an input or a component prop (the element did not
/// receive those).
fn entry_text(
    targets: &ElementTargets,
    key: &PropKey,
    target: &Target,
    report: &LayerSourceReport,
) -> (String, String) {
    match key {
        PropKey::Prop(name) => row_text(name, target, row_field(targets, key), report),
        PropKey::Input { name, .. } | PropKey::CallSite(name) => {
            shown_text(name, &target.path, &target.target, None, None)
        }
    }
}

/// The value [`entry_text`] shows, unrounded: the row's value, else its
/// placeholder (source default, else received, else table default).
fn entry_value(
    targets: &ElementTargets,
    key: &PropKey,
    target: &Target,
    report: &LayerSourceReport,
) -> Option<EditValue> {
    let (name, field, props) = match key {
        PropKey::Prop(name) => (name, row_field(targets, key), Some(&report.props)),
        PropKey::Input { name, .. } | PropKey::CallSite(name) => (name, None, None),
    };
    shown_value(name, &target.path, &target.target, field, props).0
}

/// One row's control and what it shows.
pub(crate) struct ElementRow {
    pub(crate) kind: ControlKind,
    pub(crate) control: Control,
    /// Typed since the control last showed the source's value.
    edited: bool,
    /// The text and placeholder last put into the control, and the layer
    /// they belong to.
    pub(crate) shown: Option<(String, String)>,
    layer: Option<String>,
    /// Why the typed text was not written.
    pub(crate) invalid: Option<String>,
    /// A color was picked since the picker last closed.
    picked: bool,
    /// A value this row committed whose write has not finished (`None`
    /// removes the prop).
    pending: Option<Option<EditValue>>,
    /// Why the row's held commit waits, when the occurrence index has yet
    /// to answer ("Scanning scenes…"); shown instead of "Saving…".
    pub(crate) held_note: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl ElementRow {
    /// The switch state: the shown value, else the placeholder.
    pub(crate) fn shown_bool(&self) -> bool {
        self.shown.as_ref().is_some_and(|(text, placeholder)| {
            let text = if text.is_empty() { placeholder } else { text };
            text == "true"
        })
    }

    /// A removal of the row's prop is being written.
    pub(crate) fn removing(&self) -> bool {
        matches!(self.pending, Some(None))
    }
}

/// Row commits made while the targets could not be written to (an own
/// write's reload, the source being read, another write running), for one
/// layer. Each is written once the targets are current again, one at a
/// time in the order made; a row committed again keeps its place with the
/// newer value.
#[derive(Debug, Default)]
pub(crate) struct HeldCommits {
    commits: VecDeque<HeldCommit>,
}

/// One row commit waiting its turn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HeldCommit {
    pub(crate) layer: String,
    pub(crate) key: PropKey,
    /// `None` removes the prop.
    pub(crate) value: Option<EditValue>,
    /// The timeline frame the row's targets were looked up at.
    pub(crate) frame: i64,
    /// The scope choice the commit was made with; it is written with it.
    pub(crate) scope: Option<Scope>,
}

impl HeldCommits {
    /// Holds `commit`; one for the same row of the same layer is replaced
    /// in its place.
    pub(crate) fn push(&mut self, commit: HeldCommit) {
        match self
            .commits
            .iter_mut()
            .find(|held| held.layer == commit.layer && held.key == commit.key)
        {
            Some(held) => *held = commit,
            None => self.commits.push_back(commit),
        }
    }

    /// The oldest commit held for `layer`, left held.
    pub(crate) fn first(&self, layer: Option<&str>) -> Option<&HeldCommit> {
        self.commits
            .iter()
            .find(|held| Some(held.layer.as_str()) == layer)
    }

    /// The oldest commit held for `layer`.
    pub(crate) fn pop(&mut self, layer: Option<&str>) -> Option<HeldCommit> {
        let index = self
            .commits
            .iter()
            .position(|held| Some(held.layer.as_str()) == layer)?;
        self.commits.remove(index)
    }

    pub(crate) fn contains(&self, layer: &str, key: &PropKey) -> bool {
        self.commits
            .iter()
            .any(|held| held.layer == layer && held.key == *key)
    }

    /// Whether any commit is held for `layer`.
    pub(crate) fn any(&self, layer: Option<&str>) -> bool {
        self.commits
            .iter()
            .any(|held| Some(held.layer.as_str()) == layer)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    /// Every held commit, oldest first, none held after.
    pub(crate) fn take_all(&mut self) -> Vec<HeldCommit> {
        self.commits.drain(..).collect()
    }
}

/// A selection change asked for while commits are held for the selected
/// layer: it waits until they are written (or dropped), and a newer request
/// replaces it.
#[derive(Debug, Default)]
pub(crate) struct SelectionGate {
    /// The selection to make; `Some(None)` clears it.
    waiting: Option<Option<String>>,
}

impl SelectionGate {
    /// A request to select `layer` (`None` clears the selection) while
    /// `current` is selected, `held` saying commits wait for it. The
    /// selection to make now; `None` when the request waits, or is the
    /// current selection (which cancels a waiting one).
    pub(crate) fn request(
        &mut self,
        layer: Option<String>,
        current: Option<&str>,
        held: bool,
    ) -> Option<Option<String>> {
        if layer.as_deref() == current {
            self.waiting = None;
            return None;
        }
        if held {
            self.waiting = Some(layer);
            return None;
        }
        self.waiting = None;
        Some(layer)
    }

    /// The waiting selection, once no commit is `held` for the current one.
    pub(crate) fn release(&mut self, held: bool) -> Option<Option<String>> {
        if held { None } else { self.waiting.take() }
    }

    pub(crate) fn waiting(&self) -> bool {
        self.waiting.is_some()
    }

    pub(crate) fn clear(&mut self) {
        self.waiting = None;
    }
}

/// What toggling the interactive mode does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InteractiveToggle {
    Enter,
    /// Leave now: the selection is cleared.
    Leave,
    /// Leave once the selected layer's held commits are written: until
    /// then the mode stays, so frames keep looking the source up and the
    /// commits can drain.
    LeaveAfterCommits,
    /// Toggled again while waiting to leave: stay, keeping the selection.
    StayInteractive,
}

/// The toggle's outcome, `interactive` the mode now, `leaving` whether a
/// leave already waits, and `selection_waits` whether clearing the
/// selection (asked for before deciding) waits for held commits.
pub(crate) fn interactive_toggle(
    interactive: bool,
    leaving: bool,
    selection_waits: bool,
) -> InteractiveToggle {
    match (interactive, leaving, selection_waits) {
        (false, ..) => InteractiveToggle::Enter,
        (true, true, _) => InteractiveToggle::StayInteractive,
        (true, false, true) => InteractiveToggle::LeaveAfterCommits,
        (true, false, false) => InteractiveToggle::Leave,
    }
}

/// The keys of the rows typed in for `selected` and not committed, from
/// `(key, edited, layer)`: what leaving the layer submits.
pub(crate) fn edited_rows<'k>(
    rows: impl IntoIterator<Item = (&'k PropKey, bool, Option<&'k str>)>,
    selected: Option<&str>,
) -> Vec<PropKey> {
    rows.into_iter()
        .filter(|(_, edited, layer)| *edited && selected.is_some() && *layer == selected)
        .map(|(key, ..)| key.clone())
        .collect()
}

/// Whether a commit held at timeline frame `held` would now write another
/// keyframe: the targets were read again at `now`, and the row's is a
/// keyframe, or lies between keyframes (a plain value is the same at every
/// frame).
pub(crate) fn moved_off_keyframe(held: i64, now: i64, target: &EditTarget) -> bool {
    held != now
        && matches!(
            inner_target(target),
            EditTarget::Keyframe { .. } | EditTarget::Locked(LockReason::BetweenKeyframes { .. })
        )
}

/// Whether a finished commit for `commit_layer` may reset the row that now
/// shows `row_layer`: rows are reused across layers by key, so a write of
/// one layer's `x` must leave another layer's `x` row (and its typed text)
/// alone.
pub(crate) fn commit_owns_row(row_layer: Option<&str>, commit_layer: Option<&str>) -> bool {
    row_layer.is_some() && row_layer == commit_layer
}

/// The selected layer's row controls, built on first need (controls need a
/// window) and kept while the rows' keys and kinds stay.
#[derive(Default)]
pub(crate) struct ElementRows {
    rows: HashMap<PropKey, ElementRow>,
    /// The scrub of a row's value by its label, if one is running or being
    /// written. Never together with a canvas drag.
    pub(crate) scrub: Option<Scrub>,
    /// Commits waiting for the targets to be current.
    pub(crate) held: HeldCommits,
    /// A selection change waiting for `held`.
    pub(crate) selection: SelectionGate,
    /// Leaving the interactive mode waits for `selection` (which clears the
    /// selection) to go.
    pub(crate) leaving_interactive: bool,
    /// The playhead moved (or playback started) since the rows last
    /// submitted their typed text.
    pub(crate) submit_requested: bool,
}

impl ElementRows {
    pub(crate) fn get(&self, key: &PropKey) -> Option<&ElementRow> {
        self.rows.get(key)
    }

    /// Whether a commit of the row `key` of `layer` waits its turn to be
    /// written (the row says "Saving…").
    pub(crate) fn saving(&self, layer: Option<&str>, key: &PropKey) -> bool {
        layer.is_some_and(|layer| self.held.contains(layer, key))
    }
}

/// Sends what the person does in a row's control to the row for `key`.
pub(crate) fn element_control_handler(
    key: PropKey,
) -> impl Fn(&mut EditorView, ControlEvent, &mut Window, &mut Context<EditorView>) + Clone + 'static
{
    move |this, event, _, cx| this.element_control_event(&key, event, cx)
}

/// Puts `shown` into `row`'s control.
fn show(
    row: &mut ElementRow,
    shown: &(String, String),
    window: &mut Window,
    cx: &mut Context<EditorView>,
) {
    // Text equal to what the field holds is left alone, so its caret stays
    // (the field holds what was just typed and committed).
    if row
        .control
        .text(cx)
        .is_none_or(|text| text != shown.0.as_str())
    {
        row.control.set_text(shown.0.clone(), window, cx);
    }
    row.control.set_placeholder(shown.1.clone(), window, cx);
    let value = if shown.0.is_empty() {
        &shown.1
    } else {
        &shown.0
    };
    match &row.control {
        // A value none of the options is shows as the select's placeholder.
        Control::Select(select) => {
            let chosen = match &row.kind {
                ControlKind::Select(options) if options.contains(&shown.0) => Some(shown.0.clone()),
                _ => None,
            };
            select.update(cx, |select, cx| match &chosen {
                Some(chosen) => select.set_selected_value(chosen, window, cx),
                None => select.set_selected_index(None, window, cx),
            });
        }
        // A theme color name has no swatch.
        Control::Color { picker, .. } => {
            let color = parse_color(&Value::String(value.clone()));
            picker.update(cx, |picker, cx| match color {
                Some(color) => picker.set_value(color, window, cx),
                None => picker.clear_value(window, cx),
            });
        }
        Control::Text(_) | Control::Textarea(_) | Control::Switch => {}
    }
}

impl EditorView {
    /// Builds a control per row of the selected layer's targets and puts
    /// newly resolved values (or, until the reload after an own write
    /// resolves, the written ones) into those not being typed in. Runs from
    /// `render`, which has the window controls need.
    pub(crate) fn sync_element_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_react_preview() {
            return;
        }
        let Some((resolved, _)) = self.element_edit.resolved(self.selected_layer.as_deref()) else {
            return;
        };
        let Ok(targets) = &resolved.outcome else {
            return;
        };
        let layer = resolved.layer.clone();
        let scope = self.rows_scope(targets);
        type Spec = (
            PropKey,
            ControlKind,
            Option<PrimitivePropertyField>,
            (String, String),
        );
        let specs: Vec<Spec> = targets
            .entries()
            .into_iter()
            .map(|(key, target)| {
                let kind = row_kind(targets, &key, target);
                let field = row_field(targets, &key).cloned();
                // A row writing a parameter's default shows that default.
                let default = || {
                    let (scope, calls) = scope?;
                    let row = scoped_row(scope, targets, &key, calls);
                    default_row_text(&row, &key, field.as_ref())
                };
                let shown = match self.element_edit.written_value(Some(&layer), &key) {
                    Some(written) => written_text(&key, field.as_ref(), written.as_ref()),
                    None => default()
                        .unwrap_or_else(|| entry_text(targets, &key, target, &resolved.report)),
                };
                (key, kind, field, shown)
            })
            .collect();
        let scrub = self.element_rows.scrub.as_ref();
        let rows = &mut self.element_rows.rows;
        rows.retain(|key, _| specs.iter().any(|(wanted, ..)| wanted == key));
        for (key, kind, field, shown) in specs {
            if rows.get(&key).is_none_or(|row| row.kind != kind) {
                let built = build_control(&kind, window, cx, element_control_handler(key.clone()));
                let row = ElementRow {
                    kind,
                    control: built.control,
                    edited: false,
                    shown: None,
                    layer: None,
                    invalid: None,
                    picked: false,
                    pending: None,
                    held_note: None,
                    _subscriptions: built.subscriptions,
                };
                rows.insert(key.clone(), row);
            }
            let Some(row) = rows.get_mut(&key) else {
                continue;
            };
            // Typed text stays until committed, but never under another
            // layer, nor does a value written (or held) for another layer.
            let same_layer = row.layer.as_deref() == Some(layer.as_str());
            if !same_layer {
                row.invalid = None;
                row.pending = None;
                row.held_note = None;
            }
            // A value being scrubbed, then the one being written.
            let scrubbed = scrub.and_then(|scrub| scrub.shown(&key));
            let shown = match (scrubbed, &row.pending) {
                (Some(value), _) => (value_text(&EditValue::Number(value)), String::new()),
                (None, Some(pending)) => written_text(&key, field.as_ref(), pending.as_ref()),
                (None, None) => shown,
            };
            if same_layer && (row.edited || row.shown.as_ref() == Some(&shown)) {
                continue;
            }
            show(row, &shown, window, cx);
            row.shown = Some(shown);
            row.layer = Some(layer.clone());
            row.edited = false;
        }
    }

    /// What the person did in the control of the row `key`.
    fn element_control_event(
        &mut self,
        key: &PropKey,
        event: ControlEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ControlEvent::Edited => {
                if let Some(row) = self.element_rows.rows.get_mut(key) {
                    row.edited = true;
                    row.invalid = None;
                }
            }
            ControlEvent::Submit { blur } => self.submit_element_row(key, blur, cx),
            ControlEvent::Escape => self.restore_element_row(key, true, cx),
            ControlEvent::Toggled(checked) => {
                self.commit_element_row(key, Some(EditValue::Bool(checked)), cx);
            }
            ControlEvent::Chosen(choice) => {
                self.commit_element_row(key, Some(EditValue::String(choice)), cx);
            }
            ControlEvent::ColorPicked => {
                if let Some(row) = self.element_rows.rows.get_mut(key) {
                    row.picked = true;
                }
            }
            ControlEvent::ColorPickerClosed => {
                let Some(row) = self.element_rows.rows.get_mut(key) else {
                    return;
                };
                if let Some(color) = take_picked_color(&row.control, &mut row.picked, cx) {
                    self.commit_element_row(key, Some(EditValue::String(color)), cx);
                }
            }
            ControlEvent::ChoosePath => self.choose_element_path(key, cx),
            ControlEvent::ClearPath => {
                let cleared = self
                    .element_edit
                    .resolved(self.selected_layer.as_deref())
                    .and_then(|(resolved, _)| resolved.outcome.as_ref().ok())
                    .and_then(|targets| targets.entry(key))
                    .and_then(|target| cleared_path(&target.target));
                if let Some(value) = cleared {
                    self.commit_element_row(key, Some(value), cx);
                }
            }
        }
    }

    /// Shows the source's value in the row `key` again on the next render;
    /// `forget` drops its message too.
    fn restore_element_row(&mut self, key: &PropKey, forget: bool, cx: &mut Context<Self>) {
        if let Some(row) = self.element_rows.rows.get_mut(key) {
            row.edited = false;
            row.shown = None;
            if forget {
                row.invalid = None;
            }
        }
        cx.notify();
    }

    /// Enter (`blur` false) or blur: writes typed text that parses for the
    /// row's kind, and removes the prop of an emptied row that can (see
    /// [`typed_text`]). Unedited text is never written; text that does not
    /// parse shows the row's message, and on blur, like an emptied row that
    /// can't remove its prop, the source's value again.
    fn submit_element_row(&mut self, key: &PropKey, blur: bool, cx: &mut Context<Self>) {
        let (field, can_remove) = self
            .element_edit
            .resolved(self.selected_layer.as_deref())
            .and_then(|(resolved, _)| resolved.outcome.as_ref().ok())
            .map_or((None, false), |targets| {
                let can_remove = targets
                    .entry(key)
                    .is_some_and(|target| self.row_removable(targets, key, target));
                (row_field(targets, key).cloned(), can_remove)
            });
        let Some(row) = self.element_rows.rows.get_mut(key) else {
            return;
        };
        if !row.edited {
            return;
        }
        // Clicking another layer blurs the field after the selection moved;
        // the text was typed for the old one.
        if row.layer != self.selected_layer {
            return self.restore_element_row(key, true, cx);
        }
        let Some(text) = row.control.text(cx) else {
            return;
        };
        match typed_text(&row.kind, field.as_ref(), &text, can_remove) {
            TypedText::Write(value) => self.commit_element_row(key, value, cx),
            TypedText::Restore => {
                if blur {
                    self.restore_element_row(key, true, cx);
                }
            }
            TypedText::Invalid(message) => {
                row.invalid = Some(message);
                if blur {
                    self.restore_element_row(key, false, cx);
                }
                cx.notify();
            }
        }
    }

    /// The row's remove button: removes its prop, as emptying the field does.
    pub(crate) fn remove_element_row(&mut self, key: &PropKey, cx: &mut Context<Self>) {
        self.commit_element_row(key, None, cx);
    }

    /// Writes `value` into the row `key` as one history entry (`None`
    /// removes the prop), under the scope the control shows for the
    /// element (kept while the commit is held: see
    /// [`crate::scope::commit_scope`]); the
    /// row shows it until the write finishes, and the written value until
    /// the reload resolves (the source's value again when the write
    /// failed). A text edited on the canvas commits here too.
    pub(crate) fn commit_element_row(
        &mut self,
        key: &PropKey,
        value: Option<EditValue>,
        cx: &mut Context<Self>,
    ) {
        let scope = self.kept_scope();
        self.commit_row_with(key, value, scope, false, cx);
    }

    /// [`Self::commit_element_row`] at the end of a scrub or an inline
    /// text edit that started under `started` (see `kept_scope`): written
    /// under it, or refused, as a drag is (see
    /// [`crate::scope::gesture_gate`]).
    pub(crate) fn commit_gesture_row(
        &mut self,
        key: &PropKey,
        value: Option<EditValue>,
        started: Option<Scope>,
        cx: &mut Context<Self>,
    ) {
        self.commit_row_with(key, value, started, true, cx);
    }

    /// [`Self::commit_element_row`] with the scope `remembered` it keeps
    /// (the one a held commit was made with): written under it, or refused
    /// with its lock, never under another one; one made with no scope shown
    /// is written only for this scene (see [`crate::scope::drain_gate`]). A
    /// `gesture` passes the gesture's gate when it goes at once, which never
    /// holds it for the scan; held for a reload, it drains as a row commit.
    fn commit_row_with(
        &mut self,
        key: &PropKey,
        value: Option<EditValue>,
        remembered: Option<Scope>,
        gesture: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.element_rows.rows.get_mut(key) {
            row.edited = false;
            row.invalid = None;
            row.pending = Some(value.clone());
            row.held_note = None;
        }
        // While the targets are being read again (after an own write, say)
        // or another write runs, the commit waits its turn; it fails at once
        // only when the source can't be read. While the occurrence index
        // can't say where the edit reaches, it waits for the sweep.
        let keys = std::slice::from_ref(key);
        let gate = (!self.row_commit_waits()).then(|| {
            if gesture {
                self.gesture_gate_for(remembered, keys)
            } else {
                self.commit_gate_for(remembered, keys)
            }
        });
        let held = match &gate {
            None => Some(None),
            Some(CommitGate::Hold(note)) => Some(Some(note.clone())),
            Some(CommitGate::Go(_) | CommitGate::Refuse(_)) => None,
        };
        if let Some(layer) = self.selected_layer.clone()
            && let Some(note) = held
        {
            // The frame of the targets the row showed when it was
            // committed: a seek's lookup may have answered since.
            let frame = self
                .element_edit
                .shown_frame(Some(&layer))
                .unwrap_or(self.source_lookup_frame);
            self.element_rows.held.push(HeldCommit {
                layer,
                key: key.clone(),
                value,
                frame,
                scope: remembered,
            });
            if let Some(row) = self.element_rows.rows.get_mut(key) {
                row.held_note = note;
            }
            cx.notify();
            return;
        }
        let written = key.clone();
        let layer = self.selected_layer.clone();
        let refused_layer = layer.clone();
        let done = move |this: &mut Self, result: Result<(), String>, cx: &mut Context<Self>| {
            // A newer commit of the row waiting its turn keeps showing.
            let held = layer
                .as_deref()
                .is_some_and(|layer| this.element_rows.held.contains(layer, &written));
            if let Some(row) = this.element_rows.rows.get_mut(&written)
                && !held
                && commit_owns_row(row.layer.as_deref(), layer.as_deref())
            {
                row.pending = None;
                if result.is_err() {
                    row.edited = false;
                    row.shown = None;
                }
            }
            // A released scrub's override stays until the reload draws
            // the written value; none comes after a failure, nor after
            // a write that left the text as it was (the targets are
            // still current).
            let scrubbed = this
                .element_rows
                .scrub
                .as_ref()
                .is_some_and(|scrub| scrub.released && scrub.key == written);
            if scrubbed && (result.is_err() || this.element_targets().is_some()) {
                this.cancel_row_scrub();
            }
            // A text committed on the canvas: likewise.
            if written == PropKey::Prop("children".to_owned()) {
                this.inline_commit_written(layer.as_deref(), &result);
            }
            cx.notify();
        };
        let refused = match gate {
            Some(CommitGate::Go(scope)) => {
                self.commit_element(vec![(key.clone(), value)], scope, done, cx);
                cx.notify();
                return;
            }
            Some(CommitGate::Refuse(message)) => message,
            // Held above when a layer is selected; never written unscoped.
            None | Some(CommitGate::Hold(_)) => {
                self.element_edit.unavailable(refused_layer.as_deref())
            }
        };
        // Refused as a write that failed at once is.
        self.element_edit
            .set_error(refused_layer.as_deref(), Some(refused.clone()));
        done(self, Err(refused), cx);
        cx.notify();
    }

    /// Whether a row commit made now waits its turn in [`HeldCommits`]:
    /// another write runs, or the targets are not current but will be (the
    /// source is being read, or an own write's reload is coming).
    pub(crate) fn row_commit_waits(&self) -> bool {
        if self.edit_history.is_busy() || self.property_panel.is_writing() {
            return true;
        }
        self.element_targets().is_none()
            && !self.element_edit.failed(self.selected_layer.as_deref())
    }

    /// Writes the held row commits in order once they need not wait: each
    /// write that starts makes the next wait again (one history entry per
    /// commit), and one that fails at once lets the next go. A commit to a
    /// keyframe made before the playhead moved is not written to another
    /// keyframe. One the occurrence index can't answer for yet keeps its
    /// place (and those after it theirs) until the sweep lands; each goes
    /// with the scope choice it was made with (one made with no scope shown
    /// only for this scene). A selection change waiting for them goes once
    /// none is left.
    pub(crate) fn drain_held_rows(&mut self, cx: &mut Context<Self>) {
        while !self.row_commit_waits() {
            let selected = self.selected_layer.clone();
            let scanning = self
                .element_rows
                .held
                .first(selected.as_deref())
                .is_some_and(|held| {
                    let keys = std::slice::from_ref(&held.key);
                    matches!(self.commit_gate_for(held.scope, keys), CommitGate::Hold(_))
                });
            if scanning {
                break;
            }
            let Some(held) = self.element_rows.held.pop(selected.as_deref()) else {
                break;
            };
            let moved = self
                .element_targets()
                .and_then(|(targets, _)| targets.target(&held.key))
                .is_some_and(|target| {
                    moved_off_keyframe(held.frame, self.source_lookup_frame, target)
                });
            if moved {
                self.drop_row_commit(&held.key, FRAME_MOVED);
                continue;
            }
            self.commit_row_with(&held.key, held.value, held.scope, false, cx);
        }
        self.release_selection(cx);
    }

    /// A held commit of the row `key` that will not be written: the row
    /// shows the source's value again and `message`; a text committed on
    /// the canvas is no longer drawn, as after a failed write.
    fn drop_row_commit(&mut self, key: &PropKey, message: &str) {
        if let Some(row) = self.element_rows.rows.get_mut(key) {
            row.pending = None;
            row.edited = false;
            row.shown = None;
            row.invalid = Some(message.to_owned());
        }
        if *key == PropKey::Prop("children".to_owned()) {
            let selected = self.selected_layer.clone();
            self.inline_commit_written(selected.as_deref(), &Err(message.to_owned()));
        }
    }

    /// The held commits can't be written (the code failed to reload, or the
    /// layer's source is no longer found): they are dropped, each row (or,
    /// for a layer no longer shown, the Layer section) saying `message`,
    /// and a waiting selection change goes.
    pub(crate) fn drop_held_rows(&mut self, message: &str, cx: &mut Context<Self>) {
        let selected = self.selected_layer.clone();
        // A text committed on the canvas reverts; the viewer says why (its
        // `drop_row_commit` below no longer finds the override to drop).
        if self.drop_held_commit_override() {
            self.inline_notice = Some(message.to_owned());
            self.refresh_preview();
        }
        for held in self.element_rows.held.take_all() {
            let shown = selected.as_deref() == Some(held.layer.as_str())
                && self.element_rows.rows.contains_key(&held.key);
            if shown {
                self.drop_row_commit(&held.key, message);
            } else {
                self.element_edit
                    .set_error(Some(&held.layer), Some(message.to_owned()));
            }
        }
        for row in self.element_rows.rows.values_mut() {
            if row.pending.take().is_some() {
                row.edited = false;
                row.shown = None;
            }
        }
        self.release_selection(cx);
    }

    /// Selects `layer` (`None` clears the selection) as a click, the layer
    /// list, or leaving the interactive mode asks: the rows' typed text is
    /// submitted first, as a blur would, and while commits are held for the
    /// selected layer the change waits for them (a newer request replaces
    /// it).
    pub(crate) fn select_layer(&mut self, layer: Option<String>, cx: &mut Context<Self>) {
        // Picking a layer while a leave of the interactive mode waits stays
        // in the mode.
        if layer.is_some() {
            self.element_rows.leaving_interactive = false;
        }
        // Why an earlier edit was cancelled, or a commit dropped, goes; a
        // text edited on the canvas is committed first, like the rows.
        if layer != self.selected_layer {
            self.inline_notice = None;
            self.end_inline_text(EditEvent::SelectionChange, cx);
        }
        self.submit_edited_rows(cx);
        let current = self.selected_layer.clone();
        let held = self.element_rows.held.any(current.as_deref());
        if let Some(layer) = self
            .element_rows
            .selection
            .request(layer, current.as_deref(), held)
        {
            self.change_selection(layer, cx);
        }
        cx.notify();
    }

    /// Makes the waiting selection change once no commit is held for the
    /// selected layer.
    fn release_selection(&mut self, cx: &mut Context<Self>) {
        let held = self.element_rows.held.any(self.selected_layer.as_deref());
        if let Some(layer) = self.element_rows.selection.release(held) {
            self.change_selection(layer, cx);
            cx.notify();
        }
    }

    /// Submits the text typed in the selected layer's rows, as a blur
    /// would: a value that parses is committed (or held), anything else is
    /// dropped with its message.
    pub(crate) fn submit_edited_rows(&mut self, cx: &mut Context<Self>) {
        let rows = self
            .element_rows
            .rows
            .iter()
            .map(|(key, row)| (key, row.edited, row.layer.as_deref()));
        for key in edited_rows(rows, self.selected_layer.as_deref()) {
            self.submit_element_row(&key, true, cx);
        }
    }

    /// What happened to the rows since the last render, done now that a
    /// context is at hand: the playhead moved or playback started (a text
    /// edited on the canvas commits first, then the rows submit their typed
    /// text, see [`ElementRows::submit_requested`]), or the held commits can
    /// never be written: the preview worker stopped, or the selected
    /// layer's source lookup answered without a source.
    pub(crate) fn settle_row_requests(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = self.inline_end_due.take() {
            self.end_inline_text(event, cx);
        }
        if std::mem::take(&mut self.element_rows.submit_requested) {
            self.submit_edited_rows(cx);
        }
        // The index answered: commits held until it did go, or are refused.
        if std::mem::take(&mut self.occurrence_landed) {
            self.drain_held_rows(cx);
        }
        // Dropping them releases a selection change (and a leave of the
        // interactive mode) waiting for them.
        if self.element_edit.preview_has_stopped() && !self.element_rows.held.is_empty() {
            self.drop_held_rows(PREVIEW_STOPPED, cx);
        }
        let lost = !matches!(
            self.source_lookup,
            None | Some(SourceLookup::Pending | SourceLookup::Found(_))
        );
        if lost && self.element_rows.held.any(self.selected_layer.as_deref()) {
            self.drop_held_rows(SOURCE_LOST, cx);
        }
    }

    /// Forgets the commits held for the old selection and a waiting
    /// selection change: the selection changed without waiting (the code
    /// reloaded without the layer, say). A text committed on the canvas
    /// whose commit is among them is no longer drawn; the caller asks for
    /// the frame.
    pub(crate) fn forget_held_rows(&mut self) {
        self.drop_held_commit_override();
        self.element_rows.held.take_all();
        self.element_rows.selection.clear();
    }

    /// Whether a scrub is running with the button down (not one released
    /// and being written).
    pub(crate) fn row_scrub_active(&self) -> bool {
        self.element_rows
            .scrub
            .as_ref()
            .is_some_and(|scrub| !scrub.released)
    }

    /// Mouse down on the label of the row `key` at `x` (window pixels):
    /// starts a scrub of its value when the row scrubs, ending a canvas drag
    /// without writing it, and takes the focus to the editor (see
    /// [`scrub_takes_focus`]). Nothing while playing.
    pub(crate) fn begin_row_scrub(
        &mut self,
        key: &PropKey,
        x: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // While a released scrub or drag is being written, none starts: its
        // override stays until the reload draws the written value.
        if self.playing || self.element_rows.scrub.is_some() || self.transform_drag_released() {
            return;
        }
        let Some(mut scrub) = self
            .element_targets()
            .and_then(|(targets, report)| Scrub::start(targets, report, key, x))
        else {
            return;
        };
        scrub.scope = self.kept_scope();
        // A scrub the scope would not show in this scene says why instead.
        if let Some(lock) = self.gesture_scope_lock(key) {
            self.set_row_message(key, lock.text(), cx);
            return;
        }
        if self.transform_drag_active() {
            self.cancel_transform_drag();
        }
        let takes_focus = scrub_takes_focus(
            self.element_rows
                .rows
                .iter()
                .filter(|(_, row)| row.edited)
                .map(|(edited, _)| edited),
            key,
        );
        // Text typed in this row gives way to the scrubbed value.
        match self.element_rows.rows.get_mut(key) {
            Some(row) if row.edited => self.restore_element_row(key, true, cx),
            Some(row) => row.invalid = None,
            None => {}
        }
        // The blur of a field that had the focus then submits nothing: its
        // text was written already, or just gave way.
        if takes_focus && let Some(focus_handle) = &self.focus_handle {
            focus_handle.focus(window, cx);
        }
        self.element_rows.scrub = Some(scrub);
        cx.notify();
    }

    /// Whether a released scrub's write waits its turn in [`HeldCommits`]
    /// (another write ran when it was released): its override stays through
    /// that write's reload.
    pub(crate) fn row_scrub_held(&self) -> bool {
        self.element_rows.scrub.as_ref().is_some_and(|scrub| {
            scrub.released
                && self
                    .selected_layer
                    .as_deref()
                    .is_some_and(|layer| self.element_rows.held.contains(layer, &scrub.key))
        })
    }

    /// Whether a released scrub is being written, or waits for the reload
    /// that draws its value.
    pub(crate) fn row_scrub_released(&self) -> bool {
        self.element_rows
            .scrub
            .as_ref()
            .is_some_and(|scrub| scrub.released)
    }

    /// Follows the pointer: recomputes the value and redraws the frame with
    /// it. Writes nothing.
    pub(crate) fn continue_row_scrub(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.row_scrub_active() {
            return;
        }
        // The button came up where the view did not see it: end there.
        if !event.dragging() {
            if self.end_row_scrub(cx) {
                cx.notify();
            }
            return;
        }
        let Some(scrub) = self.element_rows.scrub.as_mut() else {
            return;
        };
        if scrub.follow(f32::from(event.position.x), event.modifiers.shift) {
            self.refresh_preview();
            cx.notify();
        }
    }

    /// Release: writes the scrubbed value as one history entry, or nothing
    /// when the value is back where it started (or never moved). True when
    /// a scrub was running.
    pub(crate) fn end_row_scrub(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(scrub) = self
            .element_rows
            .scrub
            .as_mut()
            .filter(|scrub| !scrub.released)
        else {
            return false;
        };
        if !scrub.changed() {
            self.cancel_row_scrub();
            return true;
        }
        scrub.released = true;
        let (key, value, scope) = (scrub.key.clone(), scrub.value, scrub.scope);
        // The row shows the value until the write finishes; a failure (the
        // targets went stale, the source changed, the scope it started
        // under is locked by now) drops the override.
        self.commit_gesture_row(&key, Some(EditValue::Number(value)), scope, cx);
        true
    }

    /// Escape during a scrub: drops it and its override, writing nothing.
    /// True when a pressed scrub was cancelled.
    pub(crate) fn escape_row_scrub(&mut self) -> bool {
        if !self.row_scrub_active() {
            return false;
        }
        self.cancel_row_scrub();
        true
    }

    /// Drops the scrub and redraws the frame without its override.
    pub(crate) fn cancel_row_scrub(&mut self) {
        if self.element_rows.scrub.take().is_some() {
            self.refresh_preview();
        }
    }

    /// `Choose…` on a path row: imports the chosen file into the project and
    /// writes the path relative to the entry's directory.
    fn choose_element_path(&mut self, key: &PropKey, cx: &mut Context<Self>) {
        let entry_dir = self.entry_dir();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        let key = key.clone();
        let session = self.session;
        let layer = self.selected_layer.clone();
        cx.spawn(async move |view, cx| {
            let chosen = match selection.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let message = format!("Couldn't show the file dialog: {error}");
                    view.update(cx, |this, cx| this.set_row_message(&key, message, cx))
                        .ok();
                    None
                }
            };
            let Some(chosen) = chosen else {
                return;
            };
            let imported = cx
                .background_executor()
                .spawn(async move { import_path(&entry_dir, &entry_dir, &chosen) })
                .await;
            view.update(cx, |this, cx| {
                // Another file opened or another layer was picked meanwhile.
                if this.session != session || this.selected_layer != layer {
                    return;
                }
                match imported {
                    Ok(path) => this.commit_element_row(&key, Some(EditValue::String(path)), cx),
                    Err(error) => {
                        let message = format!("Couldn't use the chosen file: {error}");
                        this.set_row_message(&key, message, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn set_row_message(&mut self, key: &PropKey, message: String, cx: &mut Context<Self>) {
        if let Some(row) = self.element_rows.rows.get_mut(key) {
            row.invalid = Some(message);
            cx.notify();
        }
    }
}
