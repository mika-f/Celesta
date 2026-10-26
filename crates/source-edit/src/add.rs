//! Adding an element as a parent's last child.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, Expression, Function, JSXChild, JSXElement, Program,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;

use crate::apply::{Edit, write_value};
use crate::imports::{ImportLock, ImportName, PACKAGE, ensure_imports_in};
use crate::insert::indent_unit;
use crate::locate::ElementRef;
use crate::resolve::{EditValue, LiteralForm, Span};
use crate::{line_break, parse_checked};

/// The package of `Ellipse` and `Circle`.
const SHAPES: &str = "@celesta/shapes";

/// The `fill` or `color` every added element starts with.
const COLOR: &str = "#4F7CFF";

/// A kind of element that can be added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewKind {
    Rect,
    Ellipse,
    Circle,
    Text,
    Image,
    Video,
}

impl NewKind {
    /// The element's tag, as its module exports it.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Rect => "Rect",
            Self::Ellipse => "Ellipse",
            Self::Circle => "Circle",
            Self::Text => "Text",
            Self::Image => "Image",
            Self::Video => "Video",
        }
    }

    /// The module that exports [`Self::tag`]: `@celesta/react` or
    /// `@celesta/shapes`.
    pub fn module(self) -> &'static str {
        match self {
            Self::Ellipse | Self::Circle => SHAPES,
            Self::Rect | Self::Text | Self::Image | Self::Video => PACKAGE,
        }
    }
}

/// An element to add, in the parent's units.
#[derive(Clone, Debug, PartialEq)]
pub enum NewElement {
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Ellipse {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// Centered on `x`/`y`.
    Circle {
        x: f64,
        y: f64,
        radius: f64,
    },
    Text {
        x: f64,
        y: f64,
    },
    Image {
        src: String,
        x: f64,
        y: f64,
        width: f64,
    },
    Video {
        src: String,
        x: f64,
        y: f64,
        width: f64,
    },
}

impl NewElement {
    pub fn kind(&self) -> NewKind {
        match self {
            Self::Rect { .. } => NewKind::Rect,
            Self::Ellipse { .. } => NewKind::Ellipse,
            Self::Circle { .. } => NewKind::Circle,
            Self::Text { .. } => NewKind::Text,
            Self::Image { .. } => NewKind::Image,
            Self::Video { .. } => NewKind::Video,
        }
    }

    /// The attributes written, in order.
    fn attributes(&self) -> Vec<(&'static str, EditValue)> {
        let number = EditValue::Number;
        let color = || EditValue::String(COLOR.to_string());
        match self {
            Self::Rect {
                x,
                y,
                width,
                height,
            }
            | Self::Ellipse {
                x,
                y,
                width,
                height,
            } => vec![
                ("x", number(*x)),
                ("y", number(*y)),
                ("width", number(*width)),
                ("height", number(*height)),
                ("fill", color()),
            ],
            Self::Circle { x, y, radius } => vec![
                ("x", number(*x)),
                ("y", number(*y)),
                ("anchorX", number(0.5)),
                ("anchorY", number(0.5)),
                ("radius", number(*radius)),
                ("fill", color()),
            ],
            Self::Text { x, y } => vec![
                ("x", number(*x)),
                ("y", number(*y)),
                ("fontSize", number(48.0)),
                ("color", color()),
            ],
            Self::Image { src, x, y, width } | Self::Video { src, x, y, width } => vec![
                ("src", EditValue::String(src.clone())),
                ("x", number(*x)),
                ("y", number(*y)),
                ("width", number(*width)),
            ],
        }
    }

    /// The element on one line, its tag written `tag`.
    fn written(&self, tag: &str) -> String {
        let attributes = attributes(&self.attributes());
        match self {
            Self::Text { .. } => format!("<{tag}{attributes}>Text</{tag}>"),
            _ => format!("<{tag}{attributes} />"),
        }
    }
}

/// The wrapper's `from`/`durationInFrames`; an addition without one is unwrapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub from: i64,
    pub duration: i64,
}

/// Why an element can't be added under a parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddLock {
    /// The parent's only child is a prop (`{children}`, `{props.children}`,
    /// a destructured prop). Also when the text does not parse.
    NoChildrenRegion,
    /// A tag the addition writes can't be imported.
    Import(ImportLock),
}

impl AddLock {
    pub fn text(&self) -> String {
        match self {
            Self::NoChildrenRegion => "Its children come from a prop".to_string(),
            Self::Import(ImportLock::Clash { name }) => format!("`{name}` here is not Celesta's"),
        }
    }
}

/// Whether `element` has a children region an element can be added to.
///
/// A self-closing element has one (it is opened on insert), and so has any
/// element with a closing tag, unless its only child is an expression that
/// reads a parameter of an enclosing function: `{children}`,
/// `{props.children}`, a destructured prop. `{items.map(…)}` is a region.
pub fn accepts_children(text: &str, element: &ElementRef) -> bool {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    !failed && has_region(&program, element)
}

/// [`accepts_children`] in a program that parsed without error.
fn has_region(program: &Program<'_>, element: &ElementRef) -> bool {
    let mut find = FindParent::new(element.start);
    find.visit_program(program);
    find.found.is_some_and(|parent| !parent.prop_only)
}

/// What would refuse adding a `kind` (wrapped or not) under `parent`, checked when the menu opens.
pub fn add_lock(text: &str, parent: &ElementRef, kind: NewKind, wrapped: bool) -> Option<AddLock> {
    match add_locks(text, parent, &[kind], wrapped) {
        Some(mut locks) => locks.pop().and_then(|(_, lock)| lock),
        None => Some(AddLock::NoChildrenRegion),
    }
}

/// [`add_lock`] of each of `kinds`, in order, the text parsed once; `None`
/// when `parent` has no children region (see [`accepts_children`]), which
/// locks every kind with [`AddLock::NoChildrenRegion`].
pub fn add_locks(
    text: &str,
    parent: &ElementRef,
    kinds: &[NewKind],
    wrapped: bool,
) -> Option<Vec<(NewKind, Option<AddLock>)>> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed || !has_region(&program, parent) {
        return None;
    }
    let lock = |kind| {
        tags(text, &program, kind, wrapped)
            .err()
            .map(AddLock::Import)
    };
    Some(kinds.iter().map(|&kind| (kind, lock(kind))).collect())
}

/// The edits and where the new element's `<` is in the edited text (1-based line and column, as the runtime reports positions).
#[derive(Clone, Debug, PartialEq)]
pub struct Added {
    pub edits: Vec<Edit>,
    pub line: u32,
    pub column: u32,
}

/// Adds `new` as `parent`'s last child, wrapped in `<Sequence from
/// durationInFrames>` when `timing` is given, importing the tags it writes.
///
/// The new lines are indented one [`indent_unit`] deeper than the line of
/// the parent's opening tag. The element goes after the last child that is
/// not whitespace, else between the tags; a self-closing parent is opened.
/// The import edits come first: one per module, as
/// [`ensure_imports`](crate::ensure_imports) writes them.
pub fn add_child(
    text: &str,
    parent: &ElementRef,
    new: &NewElement,
    timing: Option<&Timing>,
) -> Result<Added, AddLock> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Err(AddLock::NoChildrenRegion);
    }
    let mut find = FindParent::new(parent.start);
    find.visit_program(&program);
    let found = find
        .found
        .filter(|found| !found.prop_only)
        .ok_or(AddLock::NoChildrenRegion)?;
    let tags = tags(text, &program, new.kind(), timing.is_some()).map_err(AddLock::Import)?;

    let start = parent.start as usize;
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..];
    let indent = &line[..line.len() - line.trim_start().len()];
    let unit = indent_unit(text, parent.start);
    let newline = line_break(text);

    // The element, in its wrapper when timed, and what goes before its `<`.
    let element = new.written(&tags.tag);
    let lead = format!("{newline}{indent}{unit}");
    let (block, before_element) = match (timing, tags.sequence) {
        (Some(timing), Some(sequence)) => {
            let open = format!(
                "<{sequence}{}>",
                attributes(&[
                    ("from", EditValue::Number(timing.from as f64)),
                    (
                        "durationInFrames",
                        EditValue::Number(timing.duration as f64)
                    ),
                ])
            );
            let before = format!("{open}{lead}{unit}");
            (
                format!("{before}{element}{lead}</{sequence}>"),
                before.len(),
            )
        }
        _ => (element, 0),
    };

    // The bytes replaced, what replaces them, and how much of it comes
    // before the block.
    let (start, end, inserted, before_block) = match found.closing_start {
        None => {
            let name = &text[found.name.start as usize..found.name.end as usize];
            let opened = format!(">{lead}{block}{newline}{indent}</{name}>");
            (parent.end_of_attributes, found.opening_end, opened, 1)
        }
        Some(closing) => {
            let children = &text[found.opening_end as usize..closing as usize];
            let after = found.opening_end + children.trim_end().len() as u32;
            if after == found.opening_end {
                let filled = format!("{lead}{block}{newline}{indent}");
                (found.opening_end, closing, filled, 0)
            } else {
                let rest = &text[after as usize..closing as usize];
                let close = if rest.contains('\n') {
                    String::new()
                } else {
                    format!("{newline}{indent}")
                };
                (after, after, format!("{lead}{block}{close}"), 0)
            }
        }
    };

    // The lines before the insertion in the edited text, and the inserted
    // text up to the `<`, whose line starts inside it.
    let mut line = 1 + text[..start as usize].matches('\n').count();
    for edit in &tags.edits {
        if let Edit::Splice {
            start: at,
            end,
            text: written,
        } = edit
            && *at < start
        {
            line += written.matches('\n').count();
            line -= text[*at as usize..*end as usize].matches('\n').count();
        }
    }
    let prefix = &inserted[..before_block + lead.len() + before_element];
    line += prefix.matches('\n').count();
    let column = prefix[prefix.rfind('\n').map_or(0, |i| i + 1)..]
        .encode_utf16()
        .count()
        + 1;

    let mut edits = tags.edits;
    edits.push(Edit::Splice {
        start,
        end,
        text: inserted,
    });
    Ok(Added {
        edits,
        line: line as u32,
        column: column as u32,
    })
}

/// The tags an addition writes, and the import edits that make them usable.
struct Tags {
    tag: String,
    /// `Sequence`, when wrapped.
    sequence: Option<String>,
    edits: Vec<Edit>,
}

/// [`Tags`] for a `kind` in `text`, parsed without error as `program`: one
/// [`ensure_imports`](crate::ensure_imports) per module, so the names a
/// module needs land in one edit.
fn tags(
    text: &str,
    program: &Program<'_>,
    kind: NewKind,
    wrapped: bool,
) -> Result<Tags, ImportLock> {
    let shapes = kind.module() == SHAPES;
    let react: Vec<&str> = [
        (!shapes).then_some(kind.tag()),
        wrapped.then_some("Sequence"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let (react_edit, react_names) = ensure_imports_in(text, program, PACKAGE, &react)?;
    let (shapes_edit, shapes_names) = if shapes {
        ensure_imports_in(text, program, SHAPES, &[kind.tag()])?
    } else {
        (None, Vec::new())
    };
    let mut react_names = react_names.into_iter().map(written);
    let tag = if shapes {
        shapes_names.into_iter().map(written).next()
    } else {
        react_names.next()
    };
    Ok(Tags {
        tag: tag.unwrap_or_else(|| kind.tag().to_string()),
        sequence: react_names.next(),
        edits: react_edit.into_iter().chain(shapes_edit).collect(),
    })
}

fn written(name: ImportName) -> String {
    match name {
        ImportName::Named(name) | ImportName::Namespace(name) => name,
    }
}

/// ` name={…}` for each attribute, a string in `"` when it can be.
fn attributes(attrs: &[(&str, EditValue)]) -> String {
    attrs
        .iter()
        .map(|(name, value)| format!(" {name}={}", write_value(value, LiteralForm::Quoted('"'))))
        .collect()
}

/// The parent element, as found in the program.
struct Parent {
    /// The tag's name as written (`Group`, `C.Group`).
    name: Span,
    /// After the opening tag's `>` (or `/>`).
    opening_end: u32,
    /// The closing tag's `<`; `None` when self-closing.
    closing_start: Option<u32>,
    /// Its only child reads a parameter of an enclosing function.
    prop_only: bool,
}

/// The element whose `<` is at `start`, and the names the parameters of
/// the functions around it bind.
struct FindParent {
    start: u32,
    params: Vec<Vec<String>>,
    found: Option<Parent>,
}

impl FindParent {
    fn new(start: u32) -> Self {
        Self {
            start,
            params: Vec::new(),
            found: None,
        }
    }

    fn reads_a_parameter(&self, expr: &Expression<'_>) -> bool {
        root(expr).is_some_and(|name| self.params.iter().flatten().any(|p| p == name))
    }
}

/// The identifier `children` or `props.children` starts from.
fn root<'e>(expr: &'e Expression<'_>) -> Option<&'e str> {
    match expr.without_parentheses() {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => root(&member.object),
        _ => None,
    }
}

impl<'a> Visit<'a> for FindParent {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let mut names = Names::default();
        names.visit_formal_parameters(&it.params);
        self.params.push(names.0);
        oxc_ast_visit::walk::walk_function(self, it, flags);
        self.params.pop();
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        let mut names = Names::default();
        names.visit_formal_parameters(&it.params);
        self.params.push(names.0);
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
        self.params.pop();
    }

    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if self.found.is_some() {
            return;
        }
        let opening = &it.opening_element;
        if opening.span.start != self.start {
            oxc_ast_visit::walk::walk_jsx_element(self, it);
            return;
        }
        let mut children = it
            .children
            .iter()
            .filter(|child| !matches!(child, JSXChild::Text(t) if t.value.trim().is_empty()));
        let only = match (children.next(), children.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        };
        let prop_only = matches!(only, Some(JSXChild::ExpressionContainer(c))
            if c.expression.as_expression().is_some_and(|e| self.reads_a_parameter(e)));
        let name = opening.name.span();
        self.found = Some(Parent {
            name: Span {
                start: name.start,
                end: name.end,
            },
            opening_end: opening.span.end,
            closing_start: it.closing_element.as_ref().map(|c| c.span.start),
            prop_only,
        });
    }
}

/// The names a pattern binds.
#[derive(Default)]
struct Names(Vec<String>);

impl<'a> Visit<'a> for Names {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.0.push(it.name.to_string());
    }
}
