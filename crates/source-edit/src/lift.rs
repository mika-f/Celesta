//! Lifting literals written at an element into new props of the component
//! that encloses it, so each call site can set its own value.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingIdentifier, BindingPattern, Expression, FormalParameters,
    Function, IdentifierReference, ImportDeclaration, JSXOpeningElement, TSInterfaceDeclaration,
    TSSignature, TSType, TSTypeAliasDeclaration, TSTypeName, VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;

use crate::apply::{Edit, expression, write_value};
use crate::locate::ElementRef;
use crate::parse_checked;
use crate::pass_through::{component_function, is_component_name, wrapper};
use crate::resolve::{EditTarget, EditValue, LiteralForm, PropPath, Span};
use crate::scope::Scopes;
use crate::shape::span_of;

/// A prop of the element to lift.
#[derive(Debug, Clone, Copy)]
pub struct LiftProp<'a> {
    /// The prop, as [`resolve`](crate::resolve) takes it.
    pub prop: &'a str,
    /// Where the prop is written at the element: `Literal` or `Missing`.
    pub target: &'a EditTarget,
    /// The value about to be written; types the props type's field when
    /// the target holds no value (`Missing` without a default).
    pub value: &'a EditValue,
}

/// The edits that lift props into the component enclosing the element.
#[derive(Debug, Clone, PartialEq)]
pub struct Lift {
    /// The new parameters, one per lifted prop, in order.
    pub params: Vec<String>,
    /// The edits to the component's file.
    pub edits: Vec<Edit>,
}

/// Why props can't be lifted.
#[derive(Debug, Clone, PartialEq)]
pub enum LiftLock {
    /// The element is in no component a call site can name.
    NoComponent,
    /// The component's first parameter is not an object pattern.
    NotObjectPattern { component: String },
    /// The component's props type is imported from another file.
    ImportedPropsType { component: String },
    /// The component's props type can't take the field `param`.
    PropsTypeNotEditable { component: String, param: String },
    /// A prop is not a literal written in the component: a keyframe, a
    /// `style` property, a value from elsewhere.
    NotALiteral,
}

/// The new prop's name: `tag` in lower camel case and `prop` with a
/// capital (`rectWidth`), with 2, 3, … appended while the name is `taken`.
pub fn lift_name(tag: &str, prop: &str, taken: &[String]) -> String {
    let base = format!("{}{}", first_lower(&camel(tag)), first_upper(&camel(prop)));
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|name| !taken.contains(name))
        .expect("some suffix is free")
}

/// Plans lifting `props` of `element` into new props of the component that
/// encloses it: each literal at the element becomes the new parameter, with
/// the literal (or an absent prop's known default) as its default, and the
/// props type gains an optional field when the component has one in this
/// file. The names are chosen together, avoiding every name the file
/// declares or reads, the pattern's keys and the props type's members.
pub fn plan_lift(
    text: &str,
    element: &ElementRef,
    props: &[LiftProp<'_>],
) -> Result<Lift, LiftLock> {
    plan_lift_avoiding(text, element, props, &[])
}

/// [`plan_lift`], the names also avoiding `also_taken`: the attributes the
/// call that will pass the new props already writes, which a `...rest`
/// may forward to the element.
pub fn plan_lift_avoiding(
    text: &str,
    element: &ElementRef,
    props: &[LiftProp<'_>],
    also_taken: &[String],
) -> Result<Lift, LiftLock> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Err(LiftLock::NoComponent);
    }
    let mut finder = Finder::new(text, element.start);
    finder.visit_program(&program);
    let (Some(tag), Some(component)) = (finder.tag.as_deref(), &finder.component) else {
        return Err(LiftLock::NoComponent);
    };
    let Some(pattern) = &component.pattern else {
        return Err(LiftLock::NotObjectPattern {
            component: component.name.clone(),
        });
    };
    let body = component
        .props_type
        .as_ref()
        .map(|props_type| finder.resolve(props_type))
        .transpose();

    let mut taken: Vec<String> = finder.scopes.names().map(str::to_string).collect();
    taken.extend(also_taken.iter().cloned());
    if let Pattern::Object { keys, .. } = pattern {
        taken.extend(keys.iter().cloned());
    }
    if let Ok(Some(body)) = &body {
        taken.extend(finder.keys(body));
    }
    let mut params = Vec::with_capacity(props.len());
    for prop in props {
        let name = lift_name(tag, prop.prop, &taken);
        taken.push(name.clone());
        params.push(name);
    }

    let body = match body {
        Ok(body) => body,
        Err(TypeLock::Imported) => {
            return Err(LiftLock::ImportedPropsType {
                component: component.name.clone(),
            });
        }
        Err(TypeLock::NotEditable) => {
            return Err(LiftLock::PropsTypeNotEditable {
                component: component.name.clone(),
                param: params.first().cloned().unwrap_or_default(),
            });
        }
    };
    if props.is_empty() {
        return Ok(Lift {
            params,
            edits: Vec::new(),
        });
    }

    // A bare attribute's `={name}` goes right after its name, which may be
    // where the attributes end: before any prop inserted there (`apply`
    // keeps the order of edits at one offset).
    let mut attached = Vec::new();
    let mut edits = Vec::new();
    let mut entries = Vec::with_capacity(props.len());
    let mut fields = Vec::with_capacity(props.len());
    for (prop, name) in props.iter().zip(&params) {
        let path = PropPath::parse(prop.prop);
        if matches!(path, PropPath::Style(_)) {
            return Err(LiftLock::NotALiteral);
        }
        let (default, value) = match prop.target {
            EditTarget::Literal { span, value, form } if within(*span, component.span) => {
                let edit = Edit::Splice {
                    start: span.start,
                    end: span.end,
                    text: at_element(name, *form),
                };
                if *form == LiteralForm::BareTrue {
                    attached.push(edit);
                } else {
                    edits.push(edit);
                }
                (Some(literal_default(text, *span, value, *form)), value)
            }
            EditTarget::Missing { insert_at, default } => {
                let written = match path {
                    PropPath::Children => format!("{{{name}}}"),
                    _ => format!(" {}={{{name}}}", prop.prop),
                };
                edits.push(Edit::Splice {
                    start: *insert_at,
                    end: *insert_at,
                    text: written,
                });
                (
                    default.as_ref().map(expression),
                    default.as_ref().unwrap_or(prop.value),
                )
            }
            _ => return Err(LiftLock::NotALiteral),
        };
        entries.push(match default {
            Some(default) => format!("{name} = {default}"),
            None => name.clone(),
        });
        fields.push(format!("{name}?: {}", type_of(value)));
    }
    attached.append(&mut edits);
    let mut edits = attached;
    edits.push(pattern_edit(text, pattern, &entries));
    if let Some(body) = body {
        edits.push(field_edit(text, body, &fields));
    }
    Ok(Lift { params, edits })
}

/// Whether `inner` lies in `outer`: a literal outside the component can't
/// read its parameter.
fn within(inner: Span, outer: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// What replaces the literal at the element: the bare identifier, in a
/// container where the literal had none.
fn at_element(name: &str, form: LiteralForm) -> String {
    match form {
        LiteralForm::Quoted(_) | LiteralForm::JsxText | LiteralForm::ChildContainer(_) => {
            format!("{{{name}}}")
        }
        LiteralForm::BareTrue => format!("={{{name}}}"),
        LiteralForm::Number
        | LiteralForm::Bool
        | LiteralForm::ContainerQuoted(_)
        | LiteralForm::Template => name.to_string(),
    }
}

/// The literal as an expression in the style it had: as written when it
/// already is one, otherwise a string in its quotes (`"` for JSX text).
fn literal_default(text: &str, span: Span, value: &EditValue, form: LiteralForm) -> String {
    match form {
        LiteralForm::Quoted(quote) | LiteralForm::ChildContainer(quote) => {
            write_value(value, LiteralForm::ContainerQuoted(quote))
        }
        LiteralForm::JsxText => write_value(value, LiteralForm::ContainerQuoted('"')),
        LiteralForm::BareTrue => "true".to_string(),
        LiteralForm::Number
        | LiteralForm::Bool
        | LiteralForm::ContainerQuoted(_)
        | LiteralForm::Template => text[span.start as usize..span.end as usize].to_string(),
    }
}

fn type_of(value: &EditValue) -> &'static str {
    match value {
        EditValue::Number(_) => "number",
        EditValue::String(_) => "string",
        EditValue::Bool(_) => "boolean",
    }
}

/// Adds `entries` to the component's first parameter: a new pattern when it
/// has none, after the last property, or before a rest element.
fn pattern_edit(text: &str, pattern: &Pattern, entries: &[String]) -> Edit {
    let list = entries.join(", ");
    let insert = |at: u32, text: String| Edit::Splice {
        start: at,
        end: at,
        text,
    };
    match pattern {
        Pattern::Absent { at } => insert(*at, format!("{{ {list} }}")),
        Pattern::Object {
            last_end: Some(end),
            ..
        } => insert(*end, format!(", {list}")),
        Pattern::Object {
            rest_start: Some(start),
            ..
        } => insert(*start, format!("{list}, ")),
        Pattern::Object { span, .. } => inside_braces(text, *span, format!(" {list} ")),
    }
}

/// Adds `fields` after the last member of `body`, separated and indented as
/// that member is: with its trailing `;` or `,` (else an earlier member's,
/// else `;`), on a line of its own when it starts one.
fn field_edit(text: &str, body: &Body, fields: &[String]) -> Edit {
    let Some(last) = body.members.last() else {
        return inside_braces(text, body.span, format!(" {} ", fields.join("; ")));
    };
    let trailing = separator(text, last.span);
    let sep = trailing
        .or_else(|| {
            body.members
                .iter()
                .rev()
                .find_map(|m| separator(text, m.span))
        })
        .unwrap_or(';');
    let before = &text[..last.span.start as usize];
    let space = &before[before.trim_end().len()..];
    let space = match space.rfind('\n') {
        Some(line) if space[..line].ends_with('\r') => &space[line - 1..],
        Some(line) => &space[line..],
        None => " ",
    };
    let written = fields
        .iter()
        .map(|field| match trailing {
            Some(_) => format!("{space}{field}{sep}"),
            None => format!("{sep}{space}{field}"),
        })
        .collect();
    Edit::Splice {
        start: last.span.end,
        end: last.span.end,
        text: written,
    }
}

/// The `;` or `,` a type member's span ends with.
fn separator(text: &str, span: Span) -> Option<char> {
    text[..span.end as usize]
        .chars()
        .next_back()
        .filter(|c| matches!(c, ';' | ','))
}

/// Writes `filling` between the braces at `span`, replacing them when
/// only whitespace is between, otherwise right after the `{`.
fn inside_braces(text: &str, span: Span, filling: String) -> Edit {
    let (open, close) = (span.start + 1, span.end - 1);
    let blank = text[open as usize..close as usize].trim().is_empty();
    Edit::Splice {
        start: open,
        end: if blank { close } else { open },
        text: filling,
    }
}

/// `name` with each run of characters an identifier can't hold dropped and
/// the letter after it capitalized (`aria-label` → `ariaLabel`).
fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut capital = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            if capital {
                out.push(c.to_ascii_uppercase());
            } else {
                out.push(c);
            }
            capital = false;
        } else {
            capital = !out.is_empty();
        }
    }
    out
}

fn first_lower(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_lowercase().chain(chars).collect())
}

fn first_upper(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The component's first parameter, when new entries can go into it.
enum Pattern {
    /// No parameter: a pattern goes at `at`, inside the parentheses.
    Absent { at: u32 },
    /// An object pattern at `span`.
    Object {
        span: Span,
        /// Its keys, as a call site passes them.
        keys: Vec<String>,
        /// The end of its last property.
        last_end: Option<u32>,
        /// The start of its rest element.
        rest_start: Option<u32>,
    },
}

/// A props type, as an annotation or a declaration writes it.
#[derive(Clone)]
enum PropsType {
    /// A type literal, or the first type literal of an intersection.
    Body(Body),
    /// A reference to a type by name.
    Named(String),
    NotEditable,
}

/// The `{ … }` of a type literal or an interface.
#[derive(Clone)]
struct Body {
    span: Span,
    members: Vec<Member>,
    /// The types it takes members from too: an interface's `extends`, the
    /// other references of an intersection.
    bases: Vec<String>,
    /// The keys of the intersection's other type literals.
    other_keys: Vec<String>,
}

#[derive(Clone)]
struct Member {
    /// Includes the member's trailing separator.
    span: Span,
    key: Option<String>,
}

enum TypeLock {
    Imported,
    NotEditable,
}

/// The function enclosing the element that a call site names.
struct Component {
    name: String,
    span: Span,
    /// `None` when the first parameter is not an object pattern.
    pattern: Option<Pattern>,
    /// From the parameter's annotation, else the declarator's `FC<P>`, else
    /// the wrapper call's type argument; `None` when nothing annotates it.
    props_type: Option<PropsType>,
}

/// A function a `const` names, with the props type its declarator or its
/// wrapper call gives.
struct Named {
    start: u32,
    name: String,
    props_type: Option<PropsType>,
}

struct Finder<'t> {
    text: &'t str,
    element_start: u32,
    /// The element's tag, its last `.` segment.
    tag: Option<String>,
    /// The innermost enclosing function a call site can name.
    component: Option<Component>,
    named: Vec<Named>,
    /// Interfaces and type aliases, by name.
    types: Vec<(String, PropsType)>,
    /// Names imports bind.
    imports: Vec<String>,
    scopes: Scopes,
}

impl<'t> Finder<'t> {
    fn new(text: &'t str, element_start: u32) -> Self {
        Self {
            text,
            element_start,
            tag: None,
            component: None,
            named: Vec::new(),
            types: Vec::new(),
            imports: Vec::new(),
            scopes: Scopes::new(element_start),
        }
    }

    fn named_at(&self, start: u32) -> Option<&Named> {
        self.named.iter().find(|named| named.start == start)
    }

    /// A function, before its body is visited: the component when it
    /// encloses the element and a call site can name it. Functions are
    /// visited outermost first, so the innermost such one is kept.
    fn enter(&mut self, name: Option<String>, span: oxc_span::Span, params: &FormalParameters<'_>) {
        if !(span.start <= self.element_start && self.element_start < span.end) {
            return;
        }
        let named = self.named_at(span.start);
        let Some(name) = name
            .or_else(|| named.map(|named| named.name.clone()))
            .filter(|name| is_component_name(name))
        else {
            return;
        };
        let annotation = params
            .items
            .first()
            .and_then(|param| param.type_annotation.as_ref());
        let props_type = match annotation {
            Some(annotation) => Some(props_type(&annotation.type_annotation, true)),
            None => named.and_then(|named| named.props_type.clone()),
        };
        self.component = Some(Component {
            name,
            span: span_of(span),
            pattern: pattern_of(params),
            props_type,
        });
    }

    /// The body to add fields to: a type literal, or a same-file interface
    /// or type alias that is (or intersects) one.
    fn resolve<'s>(&'s self, props_type: &'s PropsType) -> Result<&'s Body, TypeLock> {
        let name = match props_type {
            PropsType::Body(body) => return Ok(body),
            PropsType::NotEditable => return Err(TypeLock::NotEditable),
            PropsType::Named(name) => name,
        };
        match self.types.iter().find(|(declared, _)| declared == name) {
            Some((_, PropsType::Body(body))) => Ok(body),
            Some(_) => Err(TypeLock::NotEditable),
            None if self.imports.contains(name) => Err(TypeLock::Imported),
            None => Err(TypeLock::NotEditable),
        }
    }

    /// The member keys of `body`, with those of the types it extends or
    /// intersects that this file declares (theirs too, each once).
    fn keys(&self, body: &Body) -> Vec<String> {
        let mut keys: Vec<String> = body.members.iter().filter_map(|m| m.key.clone()).collect();
        keys.extend(body.other_keys.iter().cloned());
        let mut seen: Vec<&str> = Vec::new();
        let mut bases: Vec<&str> = body.bases.iter().map(String::as_str).collect();
        while let Some(name) = bases.pop() {
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            let declared = self.types.iter().filter(|(declared, _)| declared == name);
            for (_, props_type) in declared {
                match props_type {
                    PropsType::Body(base) => {
                        keys.extend(base.members.iter().filter_map(|m| m.key.clone()));
                        keys.extend(base.other_keys.iter().cloned());
                        bases.extend(base.bases.iter().map(String::as_str));
                    }
                    PropsType::Named(other) => bases.push(other),
                    PropsType::NotEditable => {}
                }
            }
        }
        keys
    }
}

fn pattern_of(params: &FormalParameters<'_>) -> Option<Pattern> {
    match (params.items.first(), &params.rest) {
        (None, None) => Some(Pattern::Absent {
            at: params.span.start + 1,
        }),
        (Some(first), _) => match &first.pattern {
            BindingPattern::ObjectPattern(object) => Some(Pattern::Object {
                span: span_of(object.span),
                keys: object
                    .properties
                    .iter()
                    .filter(|p| !p.computed)
                    .filter_map(|p| p.key.static_name().map(|key| key.to_string()))
                    .collect(),
                last_end: object.properties.last().map(|p| p.span.end),
                rest_start: object.rest.as_ref().map(|rest| rest.span.start),
            }),
            _ => None,
        },
        (None, Some(_)) => None,
    }
}

/// The props type an annotation writes. A reference is followed by name
/// only when `follow` (a type alias's own reference is not).
fn props_type(ty: &TSType<'_>, follow: bool) -> PropsType {
    match ty {
        TSType::TSTypeLiteral(literal) => PropsType::Body(Body {
            span: span_of(literal.span),
            members: members(&literal.members),
            bases: Vec::new(),
            other_keys: Vec::new(),
        }),
        TSType::TSIntersectionType(intersection) => {
            let mut body: Option<Body> = None;
            let (mut bases, mut other_keys) = (Vec::new(), Vec::new());
            for ty in &intersection.types {
                // The first type literal takes the fields; the rest only
                // name members.
                match props_type(ty, true) {
                    PropsType::Body(found) if body.is_none() => body = Some(found),
                    PropsType::Body(other) => {
                        other_keys.extend(other.members.into_iter().filter_map(|m| m.key));
                        other_keys.extend(other.other_keys);
                        bases.extend(other.bases);
                    }
                    PropsType::Named(name) => bases.push(name),
                    PropsType::NotEditable => {}
                }
            }
            match body {
                Some(mut body) => {
                    body.bases.extend(bases);
                    body.other_keys.extend(other_keys);
                    PropsType::Body(body)
                }
                None => PropsType::NotEditable,
            }
        }
        TSType::TSParenthesizedType(inner) => props_type(&inner.type_annotation, follow),
        TSType::TSTypeReference(reference) if follow => match &reference.type_name {
            TSTypeName::IdentifierReference(id) => PropsType::Named(id.name.to_string()),
            _ => PropsType::NotEditable,
        },
        _ => PropsType::NotEditable,
    }
}

fn members(signatures: &[TSSignature<'_>]) -> Vec<Member> {
    signatures
        .iter()
        .map(|signature| Member {
            span: span_of(signature.span()),
            key: match signature {
                TSSignature::TSPropertySignature(p) if !p.computed => p.key.static_name(),
                TSSignature::TSMethodSignature(m) if !m.computed => m.key.static_name(),
                _ => None,
            }
            .map(|key| key.to_string()),
        })
        .collect()
}

/// The props type a declarator's annotation gives: `P` of `FC<P>` or
/// `FunctionComponent<P>` (also under `React.`); any other annotation
/// can't be extended.
fn declared_props(ty: &TSType<'_>) -> PropsType {
    let TSType::TSTypeReference(reference) = ty else {
        return PropsType::NotEditable;
    };
    let name = match &reference.type_name {
        TSTypeName::IdentifierReference(id) => id.name.as_str(),
        TSTypeName::QualifiedName(q) if matches!(&q.left, TSTypeName::IdentifierReference(id) if id.name == "React") => {
            q.right.name.as_str()
        }
        _ => return PropsType::NotEditable,
    };
    let props = reference
        .type_arguments
        .as_ref()
        .and_then(|args| args.params.first());
    match (name, props) {
        ("FC" | "FunctionComponent", Some(props)) => props_type(props, true),
        _ => PropsType::NotEditable,
    }
}

/// The props type a wrapper call's type argument gives: `P` of `memo<P>`
/// or of `forwardRef<R, P>`, the outermost wrapper that has one.
fn wrapped_props(init: &Expression<'_>) -> Option<PropsType> {
    let Expression::CallExpression(call) = init.without_parentheses() else {
        return None;
    };
    let name = wrapper(&call.callee)?;
    if let Some(args) = &call.type_arguments {
        let index = usize::from(name == "forwardRef");
        return Some(
            args.params
                .get(index)
                .map_or(PropsType::NotEditable, |ty| props_type(ty, true)),
        );
    }
    wrapped_props(call.arguments.first()?.as_expression()?)
}

impl<'a> Visit<'a> for Finder<'_> {
    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let name = it.id.as_ref().map(|id| id.name.to_string());
        self.enter(name, it.span, &it.params);
        oxc_ast_visit::walk::walk_function(self, it, flags);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.enter(None, it.span, &it.params);
        oxc_ast_visit::walk::walk_arrow_function_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let (Some(id), Some(init)) = (it.id.get_binding_identifier(), &it.init)
            && let Some(start) = component_function(init)
        {
            let props_type = match &it.type_annotation {
                Some(annotation) => Some(declared_props(&annotation.type_annotation)),
                None => wrapped_props(init),
            };
            self.named.push(Named {
                start,
                name: id.name.to_string(),
                props_type,
            });
        }
        oxc_ast_visit::walk::walk_variable_declarator(self, it);
    }

    fn visit_ts_interface_declaration(&mut self, it: &TSInterfaceDeclaration<'a>) {
        let bases = it
            .extends
            .iter()
            .filter_map(|heritage| match &heritage.type_name {
                TSTypeName::IdentifierReference(id) => Some(id.name.to_string()),
                _ => None,
            })
            .collect();
        self.types.push((
            it.id.name.to_string(),
            PropsType::Body(Body {
                span: span_of(it.body.span),
                members: members(&it.body.body),
                bases,
                other_keys: Vec::new(),
            }),
        ));
        oxc_ast_visit::walk::walk_ts_interface_declaration(self, it);
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        self.types.push((
            it.id.name.to_string(),
            props_type(&it.type_annotation, false),
        ));
        oxc_ast_visit::walk::walk_ts_type_alias_declaration(self, it);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        for specifier in it.specifiers.iter().flatten() {
            self.imports.push(specifier.local().name.to_string());
        }
        oxc_ast_visit::walk::walk_import_declaration(self, it);
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.scopes.declare(it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.scopes.reference(it);
    }

    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        if it.span.start == self.element_start && self.tag.is_none() {
            let name = it.name.span();
            let name = &self.text[name.start as usize..name.end as usize];
            self.tag = name.rsplit('.').next().map(str::to_string);
        }
        oxc_ast_visit::walk::walk_jsx_opening_element(self, it);
    }
}
