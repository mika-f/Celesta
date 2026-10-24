//! Editing the selected React layer's props in its JSX source: which source
//! spans hold each prop the editor writes, the state the Layer section and
//! the canvas read, and the commit that writes new values.

use crate::edit_history::Texts;
use crate::element_rows::{control_kind, removable, row_label};
use crate::layer_select::{SourceLookup, owner_call_site};
use crate::property_controls::ControlKind;
use crate::view::EditorView;
use celesta_react_bridge::{
    ComponentPropertyField, ComponentPropertySchema, ElementKind, LayerSource, LayerSourceReport,
    PrimitivePropertyField, PrimitivePropertyTable, ReceivedProps, ReceivedValue, SourcePos,
};
use celesta_source_edit::{
    ApplyError, Edit, EditTarget, EditValue, ElementRef, LiteralForm, LockReason, ResolveContext,
    apply, element_at, enclosing_component, format_number, has_attribute, parameter_of, parses,
    removal, resolve_call_site_prop, resolve_inputs, resolve_through, text_style_path,
    written_attributes,
};
use gpui_kit::Context;
use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::fmt;
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
            ..
        } => PrimitivePropertyField::Number {
            default_value: Some(*default_value),
            min: *min,
            max: *max,
            step: *step,
            integer: None,
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
fn apply_component_schema(
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
fn resolve_rows(
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
    /// until the next reload.
    pub(crate) fn failed(&self, selected: Option<&str>) -> bool {
        self.reload_failed
            || self.resolved(selected).is_some_and(|(resolved, current)| {
                current && !self.pending && resolved.outcome.is_err()
            })
    }

    /// The code failed to reload after an own write: the targets stay
    /// stale until the next reload, and a commit fails instead of waiting.
    pub(crate) fn reload_failed(&mut self) {
        self.reload_failed = true;
    }

    /// Why a commit for `selected` has no targets to write to.
    pub(crate) fn unavailable(&self, selected: Option<&str>) -> String {
        match self.resolved(selected) {
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
        // What `registerComponent()` declared types the Component section.
        let schemas = self.component_schemas.clone();
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

    /// Writes `values` for the selected layer into its source as one undo
    /// entry (`None` removes the prop), the call site's file too when a
    /// value passes through, then reloads keeping the selection. `done` gets
    /// the outcome (also shown in the Layer section when it failed).
    pub(crate) fn commit_element(
        &mut self,
        values: Vec<(PropKey, Option<EditValue>)>,
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
        let (targets, report) = (targets.clone(), report.clone());
        let written = values.clone();
        self.element_edit.set_error(layer.as_deref(), None);
        self.record_own_writes(
            targets.files.clone(),
            move |texts| {
                commit_files(texts, &targets, &report, &values).map_err(|error| error.to_string())
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
/// says is outside the project (the element's, or a call site's).
pub(crate) fn lock_outside_project(targets: &mut ElementTargets, outside: impl Fn(&Path) -> bool) {
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
