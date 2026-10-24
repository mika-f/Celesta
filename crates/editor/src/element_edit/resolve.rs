//! Which source span holds each row of the selected element: its table
//! rows (or the transform props without a table), the inputs of an
//! expression row, and the props written at its component's call, each
//! typed by its field, its schema, or its literal.

use super::{
    ComponentProp, ComponentTargets, ElementTargets, PropKey, Target, TransformProp, reason_text,
};
use crate::element_rows::control_kind;
use crate::layer_select::owner_call_site;
use crate::property_controls::ControlKind;
use celesta_react_bridge::{
    ComponentPropertyField, ComponentPropertySchema, ElementKind, LayerSource, LayerSourceReport,
    PrimitivePropertyField, PrimitivePropertyTable, ReceivedValue, SourcePos,
};
use celesta_source_edit::{
    EditTarget, EditValue, ElementRef, LiteralForm, LockReason, ResolveContext, element_at,
    enclosing_component, has_attribute, parameter_of, resolve_call_site_prop, resolve_inputs,
    resolve_through, text_style_path, written_attributes,
};
use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The control kind of a literal's value; `String` for anything else (a
/// locked prop, whose row is disabled anyway).
fn literal_kind(target: &EditTarget) -> ControlKind {
    match target {
        EditTarget::Literal { value, .. } => match value {
            EditValue::Number(_) => ControlKind::Number,
            EditValue::String(_) => ControlKind::String,
            EditValue::Bool(_) => ControlKind::Boolean,
        },
        _ => ControlKind::String,
    }
}

/// A registered component field as a table field, so its row is typed,
/// hinted, checked, and locked as a table row is.
pub(crate) fn schema_field(field: &ComponentPropertyField) -> PrimitivePropertyField {
    match field {
        ComponentPropertyField::String { default_value, .. } => PrimitivePropertyField::String {
            default_value: Some(default_value.clone()),
        },
        ComponentPropertyField::Number {
            default_value,
            min,
            max,
            step,
            integer,
            ..
        } => PrimitivePropertyField::Number {
            default_value: Some(*default_value),
            min: *min,
            max: *max,
            step: *step,
            integer: *integer,
        },
        ComponentPropertyField::Boolean { default_value, .. } => PrimitivePropertyField::Boolean {
            default_value: Some(*default_value),
        },
        ComponentPropertyField::Color { default_value, .. } => PrimitivePropertyField::Color {
            default_value: Some(default_value.clone()),
        },
        ComponentPropertyField::Select {
            default_value,
            options,
            ..
        } => PrimitivePropertyField::Select {
            options: options.clone(),
            default_value: Some(default_value.clone()),
        },
        ComponentPropertyField::Path { .. } => PrimitivePropertyField::Path,
    }
}

/// Types the Component section's props from the schema `registerComponent()`
/// registered for the component's name, reading the call site's file with
/// `read`; see [`apply_component_schema`].
pub(crate) fn type_component_props(
    targets: &mut ElementTargets,
    schemas: &BTreeMap<String, ComponentPropertySchema>,
    read: impl Fn(&Path) -> Result<String, String>,
) {
    let Some(schema) = targets
        .component
        .as_ref()
        .and_then(|component| schemas.get(&component.name))
        .cloned()
    else {
        return;
    };
    apply_component_schema(targets, schema, read);
}

/// Types each component prop `schema` declares by its field, and locks a
/// literal of another kind (`label="Sale"` for a number) as the expression
/// it is in the source, as table rows are. The others keep their literal's
/// kind. Without the call site's text, nothing is typed.
pub(super) fn apply_component_schema(
    targets: &mut ElementTargets,
    schema: ComponentPropertySchema,
    read: impl Fn(&Path) -> Result<String, String>,
) {
    let Some(component) = &mut targets.component else {
        return;
    };
    let Ok(text) = read(&component.call_site.file) else {
        return;
    };
    for prop in &mut component.props {
        let Some(field) = schema.get(&prop.name).map(schema_field) else {
            continue;
        };
        let target = std::mem::replace(
            &mut prop.target.target,
            EditTarget::Locked(LockReason::ElementNotFound),
        );
        prop.target.target = fit_kind(target, &field, &text, None, &prop.name);
        prop.kind = control_kind(&field);
        prop.field = Some(field);
    }
    component.schema = Some(schema);
}

/// The table key a row belongs to: `scaleX` and `scaleY` are `scale`'s.
pub(crate) fn table_key(row: &str) -> &str {
    match row {
        "scaleX" | "scaleY" => "scale",
        other => other,
    }
}

/// The table field of a row: `scaleX` and `scaleY` take `scale`'s.
pub(crate) fn field_of<'t>(
    table: &'t PrimitivePropertyTable,
    row: &str,
) -> Option<&'t PrimitivePropertyField> {
    let key = table_key(row);
    table
        .groups
        .iter()
        .flat_map(|group| &group.fields)
        .find(|field| field.key == key)
        .map(|field| &field.field)
}

/// Whether the props type requires the prop of a row (`scaleX` and
/// `scaleY` as `scale`), so it is never removed.
pub(crate) fn required_of(table: &PrimitivePropertyTable, row: &str) -> bool {
    let key = table_key(row);
    table
        .groups
        .iter()
        .flat_map(|group| &group.fields)
        .any(|field| field.key == key && field.required)
}

/// The declared `defaultValue` of `field`, as the placeholder of an absent
/// prop shows it.
pub(crate) fn table_default(field: &PrimitivePropertyField) -> Option<EditValue> {
    match field {
        PrimitivePropertyField::Number { default_value, .. } => {
            default_value.map(EditValue::Number)
        }
        PrimitivePropertyField::String { default_value }
        | PrimitivePropertyField::Text { default_value }
        | PrimitivePropertyField::Color { default_value }
        | PrimitivePropertyField::Select { default_value, .. } => {
            default_value.clone().map(EditValue::String)
        }
        PrimitivePropertyField::Boolean { default_value } => default_value.map(EditValue::Bool),
        PrimitivePropertyField::Path => None,
    }
}

/// Whether a literal `value` is of the kind `field` edits.
fn fits(field: &PrimitivePropertyField, value: &EditValue) -> bool {
    match field {
        PrimitivePropertyField::Number { .. } => matches!(value, EditValue::Number(_)),
        PrimitivePropertyField::Boolean { .. } => matches!(value, EditValue::Bool(_)),
        PrimitivePropertyField::String { .. }
        | PrimitivePropertyField::Text { .. }
        | PrimitivePropertyField::Color { .. }
        | PrimitivePropertyField::Select { .. }
        | PrimitivePropertyField::Path => matches!(value, EditValue::String(_)),
    }
}

/// Locks a literal of another kind than `field` edits (`anchorY="baseline"`
/// on a number row) as the expression it is in the source: `text` holds the
/// element's literal, `call_site_text` a pass-through's. `prop` names a
/// bare attribute (`loop`), whose source is its name.
fn fit_kind(
    target: EditTarget,
    field: &PrimitivePropertyField,
    text: &str,
    call_site_text: Option<&str>,
    prop: &str,
) -> EditTarget {
    match target {
        EditTarget::Literal {
            span, value, form, ..
        } if !fits(field, &value) => {
            let source = match form {
                LiteralForm::BareTrue => prop.to_owned(),
                _ => text
                    .get(span.start as usize..span.end as usize)
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            };
            EditTarget::Locked(LockReason::Expression { source })
        }
        EditTarget::PassThrough {
            call_site,
            param,
            target,
        } => {
            let target = match call_site_text {
                Some(call_site_text) => fit_kind(*target, field, call_site_text, None, &param),
                None => *target,
            };
            EditTarget::PassThrough {
                call_site,
                param,
                target: Box::new(target),
            }
        }
        other => other,
    }
}

/// The target a value is shown from and written to: for a `PassThrough`,
/// the one at the call site.
pub(crate) fn inner_target(target: &EditTarget) -> &EditTarget {
    match target {
        EditTarget::PassThrough { target, .. } => inner_target(target),
        other => other,
    }
}

/// Whether a value can be written to `target`.
pub(crate) fn editable(target: &EditTarget) -> bool {
    matches!(
        inner_target(target),
        EditTarget::Literal { .. } | EditTarget::Keyframe { .. } | EditTarget::Missing { .. }
    )
}

/// Every row of `table` (or the transform props when `table` is `None`),
/// then their inputs; the call site is read once, when any row or input
/// passes through.
///
/// Resolves `report`'s innermost element, reading files with `read`: the
/// element's, and, only when a row or an input is a parameter of the
/// component enclosing the element, the file of that component's call site,
/// where such a value resolves. Scale is per axis when `scaleX` or `scaleY`
/// is written as an attribute of the element, and uniform otherwise (also
/// when only a parent sets them). A `Text`'s font and color rows edit the
/// `style` property when the `style` object sets it, and a literal of
/// another kind than its row edits is locked as an expression.
pub(crate) fn resolve_targets(
    read: impl Fn(&Path) -> Result<String, String>,
    report: &LayerSourceReport,
    table: Option<&PrimitivePropertyTable>,
) -> Result<ElementTargets, String> {
    resolve_rows(read, report, table, |text, element| {
        let per_axis = [TransformProp::ScaleX, TransformProp::ScaleY]
            .iter()
            .any(|prop| has_attribute(text, element, prop.name()));
        let scale = |rows: &mut Vec<String>| {
            if per_axis {
                rows.extend(["scaleX".to_owned(), "scaleY".to_owned()]);
            } else {
                rows.push("scale".to_owned());
            }
        };
        let mut rows = Vec::new();
        match table {
            Some(table) => {
                for field in table.groups.iter().flat_map(|group| &group.fields) {
                    if field.key == "scale" {
                        scale(&mut rows);
                    } else {
                        rows.push(field.key.clone());
                    }
                }
            }
            None => {
                rows.extend(["x", "y", "rotation"].map(str::to_owned));
                scale(&mut rows);
            }
        }
        rows
    })
}

/// The `style` key each `Text` shorthand row edits when the `style` object
/// sets it (`themedTextStyle` fills `fill` from `color`).
pub(crate) fn text_style_key(row: &str) -> Option<&'static str> {
    match row {
        "fontFamily" => Some("fontFamily"),
        "fontSize" => Some("fontSize"),
        "fontWeight" => Some("fontWeight"),
        "color" => Some("fill"),
        _ => None,
    }
}

/// A call site read for a pass-through: its chain entry, text, and element.
struct CallSite<'r> {
    entry: &'r LayerSource,
    text: String,
    call: ElementRef,
}

/// The text/element pair `resolve_through` and `resolve_inputs` take.
fn through<'s>(site: Option<&'s CallSite<'_>>) -> Option<(&'s str, &'s ElementRef)> {
    site.map(|site| (site.text.as_str(), &site.call))
}

/// Resolves the rows `rows` names (table keys) from the text and element of
/// `report`'s innermost element, with the fields of `table`; see
/// [`resolve_targets`]. A commit resolves the rows it started from again
/// this way.
pub(super) fn resolve_rows(
    read: impl Fn(&Path) -> Result<String, String>,
    report: &LayerSourceReport,
    table: Option<&PrimitivePropertyTable>,
    rows: impl FnOnce(&str, &ElementRef) -> Vec<String>,
) -> Result<ElementTargets, String> {
    let source = report
        .chain
        .first()
        .ok_or_else(|| "No source position".to_owned())?;
    let text = read(&source.file)?;
    let element = element_at(&text, source.line, source.column)
        .ok_or_else(|| reason_text(&LockReason::ElementNotFound))?;
    // Read on first need: the file read (also when the call is not found
    // there), and the call. A call site that can't be read or found locks
    // only the values passed through (`CallSiteNotFound`); the element's own
    // stay editable.
    let loaded: OnceCell<(Option<PathBuf>, Option<CallSite>)> = OnceCell::new();
    let call_site = || {
        loaded
            .get_or_init(|| {
                let Some((component, entry)) = caller(&text, &element, report) else {
                    return (None, None);
                };
                let Ok(call_site_text) = read(&entry.file) else {
                    return (None, None);
                };
                let call = element_at(&call_site_text, entry.line, entry.column);
                // An anonymous entry may be any `memo` component above the
                // element, one it is only passed to as children among them.
                let call = call.filter(|call| {
                    entry.name == component || tag_name(&call_site_text, call) == component
                });
                let found = call.map(|call| CallSite {
                    entry,
                    text: call_site_text,
                    call,
                });
                (Some(entry.file.clone()), found)
            })
            .1
            .as_ref()
    };
    // Where a resolved value is written: at the call site for a
    // pass-through, on the element otherwise.
    let place = |path: String, target: EditTarget, site: Option<&CallSite>| match (&target, site) {
        (EditTarget::PassThrough { .. }, Some(site)) => Target {
            file: site.entry.file.clone(),
            path,
            target,
            call_site: Some(SourcePos::from(site.entry)),
            inputs: Vec::new(),
        },
        _ => Target {
            file: source.file.clone(),
            path,
            target,
            call_site: None,
            inputs: Vec::new(),
        },
    };
    let ctx = |path: &str| ResolveContext {
        primitive: source.kind == ElementKind::Primitive,
        component_props: false,
        candidate_frames: candidate_frames(report),
        received: report.props.get(path).map(edit_value),
        render_count: report.render_count,
    };
    let inputs = |path: &str, ctx: &ResolveContext| {
        let site = loaded.get().and_then(|(_, site)| site.as_ref());
        let mut found = resolve_inputs(&text, &element, path, ctx, through(site));
        // A parameter input needs the call site, not read yet.
        let wants_call_site = found.iter().any(|(_, target)| {
            matches!(
                target,
                EditTarget::Locked(LockReason::CallSiteNotFound { .. })
            )
        });
        let site = match site {
            None if loaded.get().is_none() && wants_call_site => {
                let site = call_site();
                if site.is_some() {
                    found = resolve_inputs(&text, &element, path, ctx, through(site));
                }
                site
            }
            site => site,
        };
        found
            .into_iter()
            .map(|(name, target)| (name.clone(), place(name, target, site)))
            .collect()
    };
    let row = |name: String| {
        let style_key = if source.name == "Text" {
            text_style_key(&name)
        } else {
            None
        };
        let path = match style_key {
            Some(style_key) => text_style_path(&text, &element, &name, style_key),
            None => Ok(name.clone()),
        };
        let path = match path {
            Ok(path) => path,
            Err(reason) => {
                let target = place(name.clone(), EditTarget::Locked(reason), None);
                return (PropKey::Prop(name), target);
            }
        };
        let ctx = ctx(&path);
        let site = match loaded.get() {
            Some((_, site)) => site.as_ref(),
            None if parameter_of(&text, &element, &path).is_some() => call_site(),
            None => None,
        };
        let mut target = resolve_through(&text, &element, &path, &ctx, through(site));
        if let Some(field) = table.and_then(|table| field_of(table, &name)) {
            let call_site_text = site.map(|site| site.text.as_str());
            target = fit_kind(target, field, &text, call_site_text, &path);
        }
        let expression = matches!(target, EditTarget::Locked(LockReason::Expression { .. }));
        let mut target = place(path, target, site);
        if expression {
            target.inputs = inputs(&target.path, &ctx);
        }
        (PropKey::Prop(name), target)
    };
    let targets: Vec<(PropKey, Target)> = rows(&text, &element).into_iter().map(row).collect();
    // The props written at the component's call, read now when no row
    // needed it. A prop a row or an input passes through to is shown there.
    let component = call_site().map(|site| {
        let passed: Vec<&str> = targets
            .iter()
            .flat_map(|(_, row)| {
                std::iter::once(row).chain(row.inputs.iter().map(|(_, input)| input))
            })
            .filter_map(|target| match &target.target {
                EditTarget::PassThrough { param, .. } => Some(param.as_str()),
                _ => None,
            })
            .collect();
        let ctx = ResolveContext {
            primitive: false,
            component_props: true,
            candidate_frames: candidate_frames(report),
            received: None,
            render_count: 1,
        };
        // The call's children are not listed: they are the component's
        // content, not a value (a `children` attribute among them too).
        let props = written_attributes(&site.text, &site.call)
            .into_iter()
            .filter(|name| name != "children" && !passed.contains(&name.as_str()))
            .map(|name| {
                let target = resolve_call_site_prop(&site.text, &site.call, &name, &ctx);
                let kind = literal_kind(&target);
                let target = Target {
                    file: site.entry.file.clone(),
                    path: name.clone(),
                    target,
                    call_site: None,
                    inputs: Vec::new(),
                };
                ComponentProp {
                    name,
                    target,
                    kind,
                    field: None,
                }
            })
            .collect();
        ComponentTargets {
            name: tag_name(&site.text, &site.call).to_owned(),
            call_site: SourcePos::from(site.entry),
            call: site.call,
            props,
            schema: None,
        }
    });
    let mut files = vec![source.file.clone()];
    if let Some((Some(file), _)) = loaded.get()
        && !files.contains(file)
    {
        files.push(file.clone());
    }
    Ok(ElementTargets {
        file: source.file.clone(),
        files,
        element,
        targets,
        table: table.cloned(),
        component,
    })
}

/// The component that encloses `element` in `text`, and the chain entry
/// calling it.
fn caller<'r>(
    text: &str,
    element: &ElementRef,
    report: &'r LayerSourceReport,
) -> Option<(String, &'r LayerSource)> {
    let component = enclosing_component(text, element)?;
    let entry = owner_call_site(report, &component)?;
    Some((component, entry))
}

/// The last `.` segment of the tag name of `element` in `text`: `PriceTag`
/// for `<PriceTag …>` and `<ui.PriceTag …>`.
fn tag_name<'t>(text: &'t str, element: &ElementRef) -> &'t str {
    let name = text
        .get(element.start as usize + 1..)
        .unwrap_or_default()
        .trim_start();
    let end = name
        .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | '-' | ':')))
        .unwrap_or(name.len());
    name[..end].rsplit('.').next().unwrap_or_default()
}

/// A value the runtime passed, as the editor writes values.
pub(crate) fn edit_value(value: ReceivedValue) -> EditValue {
    match value {
        ReceivedValue::Number(value) => EditValue::Number(value),
        ReceivedValue::String(value) => EditValue::String(value),
        ReceivedValue::Bool(value) => EditValue::Bool(value),
    }
}

/// The frames an `interpolate(frame, …)` written for `report`'s element could
/// read; an older runtime that reports none reads its `local_frame`.
fn candidate_frames(report: &LayerSourceReport) -> Vec<f64> {
    if report.candidate_frames.is_empty() {
        vec![report.local_frame]
    } else {
        report.candidate_frames.clone()
    }
}
