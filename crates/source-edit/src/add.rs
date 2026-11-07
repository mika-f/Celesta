//! Adding an element as a parent's last child.

use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, ExportDefaultDeclaration,
    ExportDefaultDeclarationKind, Expression, FormalParameters, Function, IdentifierReference,
    JSXChild, JSXElement, ModuleExportName, Program, Statement, VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;
use oxc_syntax::symbol::SymbolId;

use crate::apply::{Edit, write_value};
use crate::imports::{ImportLock, ImportName, PACKAGE, ensure_imports_in};
use crate::insert::indent_unit;
use crate::locate::{ElementRef, element_in};
use crate::pass_through::{component_function, component_in, is_component_name};
use crate::resolve::{EditValue, LiteralForm, Span};
use crate::{line_break, with_program};

/// The package of `Ellipse` and `Circle`.
const SHAPES: &str = "@celesta.studio/shapes";

/// The `fill` an added Rect, Ellipse or Circle starts with.
const SHAPE_COLOR: &str = "#4F7CFF";

/// The `color` an added Text starts with, so it reads on an added shape.
const TEXT_COLOR: &str = "#FFFFFF";

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

    /// The module that exports [`Self::tag`]: `@celesta.studio/react` or
    /// `@celesta.studio/shapes`.
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
    /// `height` is written only when given: alone, `width` keeps the
    /// file's aspect.
    Image {
        src: String,
        x: f64,
        y: f64,
        width: f64,
        height: Option<f64>,
    },
    /// As [`Self::Image`].
    Video {
        src: String,
        x: f64,
        y: f64,
        width: f64,
        height: Option<f64>,
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
        let fill = || EditValue::String(SHAPE_COLOR.to_string());
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
                ("fill", fill()),
            ],
            Self::Circle { x, y, radius } => vec![
                ("x", number(*x)),
                ("y", number(*y)),
                ("anchorX", number(0.5)),
                ("anchorY", number(0.5)),
                ("radius", number(*radius)),
                ("fill", fill()),
            ],
            Self::Text { x, y } => vec![
                ("x", number(*x)),
                ("y", number(*y)),
                ("fontSize", number(48.0)),
                ("color", EditValue::String(TEXT_COLOR.to_string())),
            ],
            Self::Image {
                src,
                x,
                y,
                width,
                height,
            }
            | Self::Video {
                src,
                x,
                y,
                width,
                height,
            } => {
                let mut written = vec![
                    ("src", EditValue::String(src.clone())),
                    ("x", number(*x)),
                    ("y", number(*y)),
                    ("width", number(*width)),
                ];
                written.extend(height.map(|height| ("height", number(height))));
                written
            }
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
            Self::Import(lock) => lock.text(),
        }
    }
}

/// Whether `element` has a children region an element can be added to.
///
/// A self-closing element has one (it is opened on insert), and so has any
/// element with a closing tag, unless its only child is an expression that
/// reads a parameter of an enclosing component: `{children}`,
/// `{props.children}`, `{props['children']}`, a destructured prop: a name
/// a component's first parameter binds. A component is a function a call
/// site can name (`function Card`, `const Card = …`, through
/// `memo`/`forwardRef`) or the module's default export, named or not,
/// also through the name it is bound to (`export default box`). A callback's or a lowercase helper's parameter, and a
/// local that shadows a prop, are not props: `{items.map(…)}` and
/// `{item}` inside it are regions. A text that does not parse has none.
pub fn accepts_children(text: &str, element: &ElementRef) -> bool {
    with_program(text, |program| has_region(program, element)).unwrap_or(false)
}

/// [`accepts_children`] in a program that parsed without error.
fn has_region(program: &Program<'_>, element: &ElementRef) -> bool {
    find_parent(program, element.start).is_some_and(|parent| !parent.prop_only)
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
    with_program(text, |program| {
        locks_in(text, program, parent, kinds, wrapped)
    })
    .flatten()
}

/// [`add_locks`] in `program`, parsed from `text` without error.
fn locks_in(
    text: &str,
    program: &Program<'_>,
    parent: &ElementRef,
    kinds: &[NewKind],
    wrapped: bool,
) -> Option<Vec<(NewKind, Option<AddLock>)>> {
    if !has_region(program, parent) {
        return None;
    }
    let lock = |kind: NewKind| {
        tags(text, program, kind.module(), kind.tag(), wrapped)
            .err()
            .map(AddLock::Import)
    };
    Some(kinds.iter().map(|&kind| (kind, lock(kind))).collect())
}

/// What adding under the element at `line`/`column` of `text` reads, from
/// one parse: the element, the component it is written in (see
/// [`enclosing_component`](crate::enclosing_component)), and
/// [`add_locks`]'s answer for `kinds`.
#[derive(Clone, Debug, PartialEq)]
pub struct ParentReading {
    pub element: ElementRef,
    pub component: Option<String>,
    pub locks: Option<Vec<(NewKind, Option<AddLock>)>>,
}

/// [`ParentReading`] for the element at `line`/`column` (as
/// [`element_at`](crate::element_at) finds it); `None` when there is none
/// or the text does not parse.
pub fn read_parent(
    text: &str,
    line: u32,
    column: u32,
    kinds: &[NewKind],
    wrapped: bool,
) -> Option<ParentReading> {
    with_program(text, |program| {
        let element = element_in(text, program, line, column)?;
        Some(ParentReading {
            element,
            component: component_in(program, &element),
            locks: locks_in(text, program, &element, kinds, wrapped),
        })
    })
    .flatten()
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
    let kind = new.kind();
    add_written(text, parent, kind.module(), kind.tag(), timing, |tag| {
        new.written(tag)
    })
}

/// A component another module exports, such as a content plugin's, to add
/// with these attributes: `<Tag attribute={…} />`.
#[derive(Clone, Debug, PartialEq)]
pub struct NewComponent {
    /// What `tag` is imported from, such as `@celesta-plugin/flag`.
    pub module: String,
    pub tag: String,
    /// Written in order, a string in `"` when it can be.
    pub attributes: Vec<(String, EditValue)>,
}

/// [`add_child`] for a component of another module: the component is
/// imported from its module (a named import, or the name of an existing
/// import of it) and written self-closing with its attributes.
pub fn add_component(
    text: &str,
    parent: &ElementRef,
    new: &NewComponent,
    timing: Option<&Timing>,
) -> Result<Added, AddLock> {
    add_written(text, parent, &new.module, &new.tag, timing, |tag| {
        let attrs: Vec<(&str, EditValue)> = new
            .attributes
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        format!("<{tag}{} />", attributes(&attrs))
    })
}

/// Inserts what `element` writes, given the name `tag` of `module` is
/// imported under, as `parent`'s last child (wrapped when timed), with the
/// imports it needs; shared by [`add_child`] and [`add_component`].
fn add_written(
    text: &str,
    parent: &ElementRef,
    module: &str,
    tag: &str,
    timing: Option<&Timing>,
    element: impl FnOnce(&str) -> String,
) -> Result<Added, AddLock> {
    with_program(text, |program| {
        let found = find_parent(program, parent.start)
            .filter(|found| !found.prop_only)
            .ok_or(AddLock::NoChildrenRegion)?;
        let tags = tags(text, program, module, tag, timing.is_some()).map_err(AddLock::Import)?;

        let start = parent.start as usize;
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let line = &text[line_start..];
        let indent = &line[..line.len() - line.trim_start().len()];
        let unit = indent_unit(text, parent.start);
        let newline = line_break(text);

        // The element, in its wrapper when timed, and what goes before its `<`.
        let element = element(&tags.tag);
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
                // Only the `/>` and the spaces before it on its line: a
                // comment after the last attribute stays, and a `/>` on a
                // line of its own leaves its `>` there.
                let name = &text[found.name.start as usize..found.name.end as usize];
                let opened = format!(">{lead}{block}{newline}{indent}</{name}>");
                let slash = text[..found.opening_end as usize]
                    .rfind('/')
                    .map_or(parent.end_of_attributes, |at| at as u32);
                let kept = text[..slash as usize].trim_end_matches([' ', '\t']);
                let before = if kept.ends_with('\n') {
                    slash
                } else {
                    kept.len() as u32
                };
                (
                    before.clamp(parent.end_of_attributes, slash),
                    found.opening_end,
                    opened,
                    1,
                )
            }
            Some(closing) => {
                let children = &text[found.opening_end as usize..closing as usize];
                let after = found.opening_end + children.trim_end().len() as u32;
                if after == found.opening_end {
                    let filled = format!("{lead}{block}{newline}{indent}");
                    (found.opening_end, closing, filled, 0)
                } else if text[after as usize..closing as usize].contains('\n') {
                    (after, after, format!("{lead}{block}"), 0)
                } else {
                    // One line: the spaces before the closing tag give way
                    // to its own line.
                    let inserted = format!("{lead}{block}{newline}{indent}");
                    (after, closing, inserted, 0)
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
    })
    .unwrap_or(Err(AddLock::NoChildrenRegion))
}

/// The tags an addition writes, and the import edits that make them usable.
struct Tags {
    tag: String,
    /// `Sequence`, when wrapped.
    sequence: Option<String>,
    edits: Vec<Edit>,
}

/// [`Tags`] for `tag` exported by `module`, in `text` parsed without error
/// as `program`: one [`ensure_imports`](crate::ensure_imports) per module,
/// so the names a module needs land in one edit.
fn tags(
    text: &str,
    program: &Program<'_>,
    module: &str,
    tag: &str,
    wrapped: bool,
) -> Result<Tags, ImportLock> {
    let elsewhere = module != PACKAGE;
    // Another module's `Sequence` wrapped in Celesta's: both imports would
    // bind the one name, and each is checked against the unedited text, so
    // neither would see the other.
    if elsewhere && wrapped && tag == "Sequence" {
        return Err(ImportLock::Clash { name: tag.into() });
    }
    let react: Vec<&str> = [(!elsewhere).then_some(tag), wrapped.then_some("Sequence")]
        .into_iter()
        .flatten()
        .collect();
    let (react_edit, react_names) = ensure_imports_in(text, program, PACKAGE, &react)?;
    let (module_edit, module_names) = if elsewhere {
        ensure_imports_in(text, program, module, &[tag])?
    } else {
        (None, Vec::new())
    };
    let mut react_names = react_names.into_iter().map(written);
    let written_tag = if elsewhere {
        module_names.into_iter().map(written).next()
    } else {
        react_names.next()
    };
    Ok(Tags {
        tag: written_tag.unwrap_or_else(|| tag.to_string()),
        sequence: react_names.next(),
        edits: react_edit.into_iter().chain(module_edit).collect(),
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
    /// Its only child reads a parameter of an enclosing component.
    prop_only: bool,
}

/// The parent whose `<` is at `start` in `program`, parsed without error.
fn find_parent(program: &Program<'_>, start: u32) -> Option<Parent> {
    // Binds each read to its declaration, so a local or a callback's
    // parameter that shadows a prop is not taken for it.
    let semantic = SemanticBuilder::new().build(program).semantic;
    let mut find = FindParent {
        start,
        scoping: semantic.scoping(),
        names: Vec::new(),
        default_exports: Vec::new(),
        default_bindings: default_bindings(program, semantic.scoping()),
        props: Vec::new(),
        found: None,
    };
    find.visit_program(program);
    find.found
}

/// The bindings the module exports as its default by name: `export default
/// box` and `export { box as default }`, read before the functions they
/// name are visited.
fn default_bindings(program: &Program<'_>, scoping: &Scoping) -> Vec<SymbolId> {
    let symbol = |id: &IdentifierReference<'_>| {
        id.reference_id
            .get()
            .and_then(|reference| scoping.get_reference(reference).symbol_id())
    };
    let mut bindings = Vec::new();
    for statement in &program.body {
        match statement {
            Statement::ExportDefaultDeclaration(export) => {
                if let Some(Expression::Identifier(id)) = export
                    .declaration
                    .as_expression()
                    .map(Expression::without_parentheses)
                {
                    bindings.extend(symbol(id));
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                for specifier in &export.specifiers {
                    if let (ModuleExportName::IdentifierReference(id), "default") =
                        (&specifier.local, specifier.exported.name().as_str())
                    {
                        bindings.extend(symbol(id));
                    }
                }
            }
            _ => {}
        }
    }
    bindings
}

/// The element whose `<` is at `start`, and the parameters of the
/// components around it.
struct FindParent<'s> {
    start: u32,
    scoping: &'s Scoping,
    /// Functions a `const` names (`const Card = …`), by their offset, with
    /// the binding.
    names: Vec<(u32, String, Option<SymbolId>)>,
    /// The offsets of functions the module exports as its default.
    default_exports: Vec<u32>,
    /// See [`default_bindings`].
    default_bindings: Vec<SymbolId>,
    /// The names every component's parameters bind.
    props: Vec<SymbolId>,
    found: Option<Parent>,
}

impl FindParent<'_> {
    /// A function is a component when a call site can name it (the
    /// `const` that holds it, through `memo`/`forwardRef`, else its own
    /// name), or when it is the module's default export, directly or by
    /// the name it is bound to: not a callback or a lowercase helper, whose
    /// parameters its caller passes. Its props are what its first
    /// parameter binds; a second one (`forwardRef`'s `ref`, a legacy
    /// context) is not a prop.
    fn component_params(
        &mut self,
        start: u32,
        own: Option<&BindingIdentifier<'_>>,
        params: &FormalParameters<'_>,
    ) {
        let held = self.names.iter().find(|(at, ..)| *at == start);
        let name = held
            .map(|(_, name, _)| name.as_str())
            .or_else(|| own.map(|id| id.name.as_str()));
        let binding = held
            .and_then(|(.., symbol)| *symbol)
            .or_else(|| own.and_then(|id| id.symbol_id.get()));
        let component = name.is_some_and(is_component_name)
            || self.default_exports.contains(&start)
            || binding.is_some_and(|symbol| self.default_bindings.contains(&symbol));
        let Some(first) = params.items.first().filter(|_| component) else {
            return;
        };
        let mut names = Names::default();
        names.visit_binding_pattern(&first.pattern);
        self.props.extend(names.0);
    }

    /// Whether `expr` reads a component's parameter: `children`,
    /// `props.children`, `props['children']`, `props?.children`.
    fn reads_a_prop(&self, expr: &Expression<'_>) -> bool {
        root(expr)
            .and_then(|id| id.reference_id.get())
            .and_then(|reference| self.scoping.get_reference(reference).symbol_id())
            .is_some_and(|symbol| self.props.contains(&symbol))
    }
}

/// The identifier `expr` reads a member of, or itself.
fn root<'e, 'a>(expr: &'e Expression<'a>) -> Option<&'e IdentifierReference<'a>> {
    match expr.without_parentheses() {
        Expression::Identifier(id) => Some(id),
        Expression::StaticMemberExpression(member) => root(&member.object),
        Expression::ComputedMemberExpression(member)
            if matches!(member.expression, Expression::StringLiteral(_)) =>
        {
            root(&member.object)
        }
        Expression::ChainExpression(chain) => {
            root(chain.expression.as_member_expression()?.object())
        }
        _ => None,
    }
}

impl<'a> Visit<'a> for FindParent<'_> {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (Some(id), Some(init)) = (it.id.get_binding_identifier(), &it.init)
            && let Some(start) = component_function(init)
        {
            self.names
                .push((start, id.name.to_string(), id.symbol_id.get()));
        }
        oxc_ast_visit::walk::walk_variable_declarator(self, it);
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        let start = match &it.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(f) => Some(f.span.start),
            kind => kind.as_expression().and_then(component_function),
        };
        self.default_exports.extend(start);
        oxc_ast_visit::walk::walk_export_default_declaration(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        self.component_params(it.span.start, it.id.as_ref(), &it.params);
        oxc_ast_visit::walk::walk_function(self, it, flags);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.component_params(it.span.start, None, &it.params);
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
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
            if c.expression.as_expression().is_some_and(|e| self.reads_a_prop(e)));
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
struct Names(Vec<SymbolId>);

impl<'a> Visit<'a> for Names {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.0.extend(it.symbol_id.get());
    }
}
