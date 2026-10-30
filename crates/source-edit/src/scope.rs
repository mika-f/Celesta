//! The functions enclosing an element and the names they bind: function-level
//! `const`s, a component's parameters, and everything else as not editable.

use std::collections::HashMap;

use oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, BindingIdentifier, BindingPattern, Declaration,
    Expression, FormalParameters, Function, FunctionBody, IdentifierReference, Program, Statement,
    VariableDeclaration, VariableDeclarationKind, VariableDeclarator,
};

use crate::pass_through::{component_function, is_component_name};
use crate::resolve::{EditValue, LiteralForm, Span};
use crate::shape::{Shape, shape_of};

/// A parameter's default value.
#[derive(Debug, Clone)]
pub(crate) enum ParamDefault {
    /// None is written: an absent prop leaves the parameter `undefined`.
    /// ` = …` would go at `binding_end`, right after the bound name.
    None { binding_end: u32 },
    /// A literal written as `form` at `span`.
    Literal {
        value: EditValue,
        span: Span,
        form: LiteralForm,
    },
    /// An expression, whose value is not known here.
    Other,
    /// Read as `props.<key>`: no pattern holds a default.
    NoPattern,
}

impl ParamDefault {
    /// The literal default's value, if any.
    pub(crate) fn value(&self) -> Option<&EditValue> {
        match self {
            Self::Literal { value, .. } => Some(value),
            Self::None { .. } | Self::Other | Self::NoPattern => None,
        }
    }
}

/// A prop value that is a parameter of a component enclosing the element.
#[derive(Debug)]
pub(crate) struct ParamInfo {
    /// The component, as a call site names it.
    pub(crate) component: String,
    /// The prop the call site passes.
    pub(crate) param: String,
    pub(crate) default: ParamDefault,
}

#[derive(Debug)]
pub(crate) enum Binding {
    Const(Shape),
    /// A property of a component's first parameter (`{ label }`,
    /// `{ label = 'Sale' }`, `{ label: text }`): the call site's prop
    /// `param`.
    Parameter {
        component: String,
        param: String,
        default: ParamDefault,
    },
    /// A component's first parameter named `props`: `props.<key>` is the
    /// call site's prop `<key>`.
    Props {
        component: String,
    },
    /// `let`, `var`, other parameters, destructuring: never editable.
    Other,
}

struct Scope {
    /// The function's name when a call site can name it.
    component: Option<String>,
    bindings: Vec<(String, Binding)>,
}

/// What the file binds around the element, filled in by the visitor that
/// finds the element.
pub(crate) struct Scopes {
    element_start: u32,
    /// Functions enclosing the element, outermost first.
    stack: Vec<Scope>,
    /// The outermost function enclosing the element.
    outer: Option<oxc_span::Span>,
    /// Every binding the file declares (variables, parameters, catch
    /// parameters, destructured names, functions, imports), with its offset.
    declarations: Vec<(String, u32)>,
    /// The offsets of every read of a name in the file.
    refs: HashMap<String, Vec<u32>>,
    /// Functions a `const` names (`const PriceTag = …`), by their offset.
    names: Vec<(u32, String)>,
}

impl Scopes {
    pub(crate) fn new(element_start: u32) -> Self {
        Self {
            element_start,
            stack: Vec::new(),
            outer: None,
            declarations: Vec::new(),
            refs: HashMap::new(),
            names: Vec::new(),
        }
    }

    /// The innermost function enclosing the element that a call site can
    /// name.
    pub(crate) fn component(&self) -> Option<&str> {
        self.stack.iter().rev().find_map(|s| s.component.as_deref())
    }

    /// The parameter `shape` reads: an identifier bound by the first
    /// parameter of [`Self::component`], or `props.<key>`. A parameter of an
    /// outer component is not followed: the call site found is the inner
    /// component's, which never passes it.
    pub(crate) fn parameter(&self, shape: &Shape) -> Option<ParamInfo> {
        let (name, member) = match shape {
            Shape::Ident(name) => (name, None),
            Shape::Member {
                object, property, ..
            } => (object, Some(property)),
            _ => return None,
        };
        if self.declared_more_than_once(name) {
            return None;
        }
        let found = match (self.lookup(name)?, member) {
            (
                Binding::Parameter {
                    component,
                    param,
                    default,
                },
                None,
            ) => Some(ParamInfo {
                component: component.clone(),
                param: param.clone(),
                default: default.clone(),
            }),
            (Binding::Props { component }, Some(property)) => Some(ParamInfo {
                component: component.clone(),
                param: property.clone(),
                default: ParamDefault::NoPattern,
            }),
            _ => None,
        }?;
        (Some(found.component.as_str()) == self.component()).then_some(found)
    }

    /// The default of the call site's prop `param` in the first parameter
    /// of [`Self::component`]: `NoPattern` when that parameter is `props`,
    /// `None` when the pattern has no such key.
    pub(crate) fn parameter_default(&self, param: &str) -> Option<ParamDefault> {
        let scope = self.stack.iter().rev().find(|s| s.component.is_some())?;
        scope
            .bindings
            .iter()
            .find_map(|(_, binding)| match binding {
                Binding::Parameter {
                    param: key,
                    default,
                    ..
                } if key == param => Some(default.clone()),
                Binding::Props { .. } => Some(ParamDefault::NoPattern),
                _ => None,
            })
    }

    /// Every name the file declares or reads.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.declarations
            .iter()
            .map(|(name, _)| name.as_str())
            .chain(self.refs.keys().map(String::as_str))
    }

    /// Innermost binding of `name` among the enclosing functions.
    pub(crate) fn lookup(&self, name: &str) -> Option<&Binding> {
        self.stack
            .iter()
            .rev()
            .find_map(|s| s.bindings.iter().find(|(n, _)| n == name).map(|(_, b)| b))
    }

    /// Whether `name` is declared more than once in the outermost function
    /// enclosing the element (anywhere in the file when there is none). Only
    /// function-level `const`s are modelled, so a second declaration in a
    /// block, loop, `catch` or nested function could be the one in effect.
    /// A module-level declaration is never resolved, and one inside the
    /// function shadows it, so those are not counted.
    pub(crate) fn declared_more_than_once(&self, name: &str) -> bool {
        self.declarations
            .iter()
            .filter(|(declared, at)| {
                declared == name
                    && self
                        .outer
                        .is_none_or(|outer| outer.start <= *at && *at < outer.end)
            })
            .count()
            > 1
    }

    /// How many times the file reads `name` (an expression read as
    /// [`Self::one_place`] counting once).
    pub(crate) fn uses(&self, name: &str) -> usize {
        self.refs.get(name).map_or(0, Vec::len)
    }

    /// Reads the expression at `span` as one place, as its inputs are: the
    /// names it declares itself (an arrow's parameters) bind only inside
    /// it, so they are dropped, and its reads of a name count as one use.
    pub(crate) fn one_place(&mut self, span: Span) {
        let inside = |at: u32| span.start <= at && at < span.end;
        self.declarations.retain(|(_, at)| !inside(*at));
        for reads in self.refs.values_mut() {
            let mut seen = false;
            reads.retain(|&at| !inside(at) || !std::mem::replace(&mut seen, true));
        }
    }

    /// A function, before its body is visited. Scopes stay pushed: only
    /// functions enclosing the element get one, and those are visited
    /// outermost first.
    pub(crate) fn enter_function(&mut self, it: &Function<'_>) {
        if !self.encloses_element(it.span) {
            return;
        }
        if self.outer.is_none() {
            self.outer = Some(it.span);
        }
        let name = it
            .id
            .as_ref()
            .map(|id| id.name.to_string())
            .or_else(|| self.named(it.span.start).map(str::to_string));
        self.push_scope(name, &it.params, it.body.as_deref());
    }

    /// An arrow function, before its body is visited.
    pub(crate) fn enter_arrow_function(&mut self, it: &ArrowFunctionExpression<'_>) {
        if !self.encloses_element(it.span) {
            return;
        }
        if self.outer.is_none() {
            self.outer = Some(it.span);
        }
        let body = match &it.body {
            ArrowFunctionBody::FunctionBody(b) => Some(&**b),
            _ => None,
        };
        let name = self.named(it.span.start).map(str::to_string);
        self.push_scope(name, &it.params, body);
    }

    /// A variable declarator, before its initializer is visited, so the
    /// function it names is named before it is entered.
    pub(crate) fn declarator(&mut self, it: &VariableDeclarator<'_>) {
        if let (Some(id), Some(init)) = (it.id.get_binding_identifier(), &it.init)
            && let Some(start) = component_function(init)
        {
            self.names.push((start, id.name.to_string()));
        }
    }

    pub(crate) fn declare(&mut self, it: &BindingIdentifier<'_>) {
        self.declarations.push((it.name.to_string(), it.span.start));
    }

    pub(crate) fn reference(&mut self, it: &IdentifierReference<'_>) {
        self.refs
            .entry(it.name.to_string())
            .or_default()
            .push(it.span.start);
    }

    /// The name a `const` gives the function starting at `start`.
    fn named(&self, start: u32) -> Option<&str> {
        self.names
            .iter()
            .find(|(at, _)| *at == start)
            .map(|(_, name)| name.as_str())
    }

    fn encloses_element(&self, span: oxc_span::Span) -> bool {
        span.start <= self.element_start && self.element_start < span.end
    }

    /// Pushes the scope of a function enclosing the element, named
    /// `name` (by its declaration or the `const` that holds it).
    fn push_scope(
        &mut self,
        name: Option<String>,
        params: &FormalParameters<'_>,
        body: Option<&FunctionBody<'_>>,
    ) {
        let component = name.filter(|name| is_component_name(name));
        let mut bindings = Vec::new();
        for (index, param) in params.items.iter().enumerate() {
            match &component {
                Some(component) if index == 0 => {
                    parameter_bindings(&param.pattern, component, &mut bindings);
                }
                _ => other_bindings(&param.pattern, &mut bindings),
            }
        }
        if let Some(rest) = &params.rest {
            other_bindings(&rest.rest.argument, &mut bindings);
        }
        for stmt in body.iter().flat_map(|b| &b.statements) {
            if let Statement::VariableDeclaration(decl) = stmt {
                declaration_bindings(decl, &mut bindings);
            }
        }
        self.stack.push(Scope {
            component,
            bindings,
        });
    }

    /// The module's scope, when no function encloses the anchor: its
    /// top-level declarations, `export const` included, read as a
    /// function's are. Called after the visit, for a value anchored at an
    /// expression only; an element's module-level names stay unresolved.
    pub(crate) fn enter_module(&mut self, program: &Program<'_>) {
        if !self.stack.is_empty() {
            return;
        }
        let mut bindings = Vec::new();
        for stmt in &program.body {
            match stmt {
                Statement::VariableDeclaration(decl) => declaration_bindings(decl, &mut bindings),
                Statement::ExportDeclaration(export) => {
                    if let Declaration::VariableDeclaration(decl) = &export.declaration {
                        declaration_bindings(decl, &mut bindings);
                    }
                }
                _ => {}
            }
        }
        self.stack.push(Scope {
            component: None,
            bindings,
        });
    }
}

/// A `const name = …` is a `Const`; any other declared name is never
/// editable.
fn declaration_bindings(decl: &VariableDeclaration<'_>, bindings: &mut Vec<(String, Binding)>) {
    for d in &decl.declarations {
        let simple = d.id.get_binding_identifier();
        match (decl.kind, simple, &d.init) {
            (VariableDeclarationKind::Const, Some(id), Some(init)) => {
                bindings.push((id.name.to_string(), Binding::Const(shape_of(init))));
            }
            _ => other_bindings(&d.id, bindings),
        }
    }
}

/// The bindings of a component's first parameter. A property of an object
/// pattern bound to a plain name (with or without a default) reads the
/// call site's prop of that key, and so does `props.<key>` for a parameter
/// named `props`; nested patterns and rest elements are never editable.
fn parameter_bindings(
    pattern: &BindingPattern<'_>,
    component: &str,
    bindings: &mut Vec<(String, Binding)>,
) {
    match pattern {
        BindingPattern::BindingIdentifier(id) if id.name == "props" => {
            bindings.push((
                id.name.to_string(),
                Binding::Props {
                    component: component.to_string(),
                },
            ));
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                let key = if property.computed {
                    None
                } else {
                    property
                        .key
                        .static_name()
                        .and_then(|name| name.into_cow_str())
                };
                let (id, default) = match &property.value {
                    BindingPattern::BindingIdentifier(id) => (
                        Some(id),
                        ParamDefault::None {
                            binding_end: id.span.end,
                        },
                    ),
                    BindingPattern::AssignmentPattern(assign) => match &assign.left {
                        BindingPattern::BindingIdentifier(id) => {
                            (Some(id), default_of(&assign.right))
                        }
                        _ => (None, ParamDefault::Other),
                    },
                    _ => (None, ParamDefault::Other),
                };
                match (key, id) {
                    (Some(key), Some(id)) => bindings.push((
                        id.name.to_string(),
                        Binding::Parameter {
                            component: component.to_string(),
                            param: key.to_string(),
                            default,
                        },
                    )),
                    _ => other_bindings(&property.value, bindings),
                }
            }
            if let Some(rest) = &object.rest {
                other_bindings(&rest.argument, bindings);
            }
        }
        _ => other_bindings(pattern, bindings),
    }
}

fn other_bindings(pattern: &BindingPattern<'_>, bindings: &mut Vec<(String, Binding)>) {
    for id in pattern.get_binding_identifiers() {
        bindings.push((id.name.to_string(), Binding::Other));
    }
}

fn default_of(expr: &Expression<'_>) -> ParamDefault {
    let (value, span, form) = match shape_of(expr) {
        Shape::Number(span, value) => (EditValue::Number(value), span, LiteralForm::Number),
        Shape::String(span, value, form) => (EditValue::String(value), span, form),
        Shape::Bool(span, value, form) => (EditValue::Bool(value), span, form),
        _ => return ParamDefault::Other,
    };
    ParamDefault::Literal { value, span, form }
}
