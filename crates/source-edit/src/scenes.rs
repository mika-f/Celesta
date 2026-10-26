//! `defineScenes([...])` lists and how a `<Composition>` writes its length.
//! Neither is JSX the runtime reports a position for, so they are found by
//! reading the files.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, Declaration, Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue,
    JSXOpeningElement, ObjectExpression, ObjectPropertyKind, Program, PropertyKind, Statement,
    VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;

use crate::imports::Module;
use crate::inputs::inputs_at;
use crate::resolve::{EditTarget, LockReason, Span, property_insert_at, resolve_value_at};
use crate::shape::{span_of, string_of};
use crate::{parse_checked_as, source_type_of};

/// A `defineScenes([...])` call.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneList {
    /// `X` of `const X = defineScenes(…)`.
    pub binding: Option<String>,
    /// The entries, in list order.
    pub entries: Vec<SceneItem>,
}

/// An entry of a [`SceneList`].
#[derive(Debug, Clone, PartialEq)]
pub struct SceneItem {
    pub id: String,
    /// The entry's `durationInFrames`, by the rules of a prop value (a
    /// `const` of the file read too); `Missing` after the last property
    /// when it is not written.
    pub duration: EditTarget,
    /// When `duration` is an expression, the identifiers it reads.
    pub inputs: Vec<(String, EditTarget)>,
}

/// Every `defineScenes([...])` call in `text` (parsed as `path`'s type)
/// whose argument is an array literal of object literals with
/// string-literal ids, in source order. A list holding anything else (a
/// spread, a hole, an entry with a spread or a computed key) is skipped.
pub fn scene_lists(text: &str, path: &Path) -> Vec<SceneList> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked_as(text, source_type_of(path), &alloc);
    if failed {
        return Vec::new();
    }
    let mut calls = Calls {
        text,
        lists: Vec::new(),
        bindings: Vec::new(),
    };
    calls.visit_program(&program);
    let Calls {
        lists, bindings, ..
    } = calls;
    lists
        .into_iter()
        .map(|(start, entries)| SceneList {
            binding: bindings
                .iter()
                .find(|(at, _)| *at == start)
                .map(|(_, name)| name.clone()),
            entries: entries
                .into_iter()
                .map(|(id, duration)| item(&program, text, id, duration))
                .collect(),
        })
        .collect()
}

/// How `<Composition durationInFrames>` is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionLength {
    /// `X.durationInFrames`, `X` a scene list's binding.
    ListDuration {
        /// The import specifier `X` comes from; `None`: this file.
        module: Option<String>,
        /// The binding's name in that module.
        name: String,
    },
    /// A literal, any other expression, or none.
    Other,
}

/// How `<Composition durationInFrames>` is written in `text` (parsed as
/// `path`'s type); `None` without a `<Composition>` (from
/// `@celesta/react`), or when the text does not parse. The first
/// `<Composition>` counts.
pub fn composition_length(text: &str, path: &Path) -> Option<CompositionLength> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked_as(text, source_type_of(path), &alloc);
    if failed {
        return None;
    }
    let module = Module::of(&program);
    let mut find = FindComposition {
        module: &module,
        found: None,
    };
    find.visit_program(&program);
    let list = find.found?;
    let Some(list) = list else {
        return Some(CompositionLength::Other);
    };
    if let Some(import) = module.import(&list) {
        return Some(match &import.imported {
            Some(name) => CompositionLength::ListDuration {
                module: Some(import.source.clone()),
                name: name.clone(),
            },
            None => CompositionLength::Other,
        });
    }
    let local = module.declared(&list) == 1 && top_level_consts(&program).any(|name| name == list);
    Some(if local {
        CompositionLength::ListDuration {
            module: None,
            name: list,
        }
    } else {
        CompositionLength::Other
    })
}

/// An entry's `durationInFrames` as written.
enum Duration {
    /// The value expression.
    Value(Span),
    /// A method or accessor: its source.
    Other(Span),
    /// Not written: properties go at `insert_at`.
    Absent { insert_at: u32 },
}

fn item(program: &Program<'_>, text: &str, id: String, duration: Duration) -> SceneItem {
    let (duration, inputs) = match duration {
        Duration::Value(span) => {
            let duration = resolve_value_at(program, text, span);
            let inputs = match duration {
                EditTarget::Locked(LockReason::Expression { .. }) => inputs_at(program, text, span),
                _ => Vec::new(),
            };
            (duration, inputs)
        }
        Duration::Other(span) => (
            EditTarget::Locked(LockReason::Expression {
                source: text[span.start as usize..span.end as usize].to_string(),
            }),
            Vec::new(),
        ),
        Duration::Absent { insert_at } => (
            EditTarget::Missing {
                insert_at,
                default: None,
            },
            Vec::new(),
        ),
    };
    SceneItem {
        id,
        duration,
        inputs,
    }
}

/// The `defineScenes` calls, by the offset of the call, and the `const`s
/// that hold a call, by the same offset.
struct Calls<'t> {
    text: &'t str,
    lists: Vec<(u32, Vec<(String, Duration)>)>,
    bindings: Vec<(u32, String)>,
}

impl<'a> Visit<'a> for Calls<'_> {
    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        if it.kind == VariableDeclarationKind::Const {
            for d in &it.declarations {
                if let (Some(id), Some(Expression::CallExpression(call))) = (
                    d.id.get_binding_identifier(),
                    d.init.as_ref().map(Expression::without_parentheses),
                ) {
                    self.bindings.push((call.span.start, id.name.to_string()));
                }
            }
        }
        oxc_ast_visit::walk::walk_variable_declaration(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if matches!(it.callee.without_parentheses(), Expression::Identifier(id) if id.name == "defineScenes")
            && let Some(entries) = list_of(it, self.text)
        {
            self.lists.push((it.span.start, entries));
        }
        oxc_ast_visit::walk::walk_call_expression(self, it);
    }
}

/// The entries of a `defineScenes([...])` call, or `None` when its
/// argument is anything but an array literal of object literals with
/// string-literal ids.
fn list_of(call: &CallExpression<'_>, text: &str) -> Option<Vec<(String, Duration)>> {
    let Expression::ArrayExpression(array) = call
        .arguments
        .first()?
        .as_expression()?
        .without_parentheses()
    else {
        return None;
    };
    array
        .elements
        .iter()
        .map(
            |element| match element.as_expression()?.without_parentheses() {
                Expression::ObjectExpression(object) => entry_of(object, text),
                _ => None,
            },
        )
        .collect()
}

/// An entry's id and `durationInFrames` (a later key wins, as at runtime),
/// or `None` when a spread or a computed key may set either, or the id is
/// not a string literal.
fn entry_of(object: &ObjectExpression<'_>, text: &str) -> Option<(String, Duration)> {
    let mut id = None;
    let mut duration = None;
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(p) = property else {
            return None;
        };
        if p.computed {
            return None;
        }
        let plain = p.kind == PropertyKind::Init && !p.method;
        match p.key.static_name().as_deref() {
            Some("id") => {
                let Expression::StringLiteral(s) = p.value.without_parentheses() else {
                    return None;
                };
                if !plain {
                    return None;
                }
                id = Some(string_of(s)?.0);
            }
            Some("durationInFrames") => {
                duration = Some(if plain {
                    Duration::Value(span_of(p.value.span()))
                } else {
                    Duration::Other(span_of(p.span))
                });
            }
            _ => {}
        }
    }
    let duration = duration.unwrap_or_else(|| Duration::Absent {
        insert_at: property_insert_at(
            text,
            span_of(object.span),
            object.properties.last().map(|p| p.span().end),
        ),
    });
    Some((id?, duration))
}

/// Finds the first `<Composition>` and the `X` of its
/// `durationInFrames={X.durationInFrames}`: `Some(None)` when it is written
/// otherwise, or not at all.
struct FindComposition<'m> {
    module: &'m Module,
    found: Option<Option<String>>,
}

impl<'a> Visit<'a> for FindComposition<'_> {
    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        if self.found.is_some() {
            return;
        }
        if self
            .module
            .export_of(&it.name)
            .is_some_and(|path| path == ["Composition"])
        {
            self.found = Some(list_duration(it));
            return;
        }
        oxc_ast_visit::walk::walk_jsx_opening_element(self, it);
    }
}

/// `X` of the last `durationInFrames={X.durationInFrames}`, with no spread
/// after it.
fn list_duration(opening: &JSXOpeningElement<'_>) -> Option<String> {
    let mut value = None;
    for item in &opening.attributes {
        match item {
            JSXAttributeItem::SpreadAttribute(_) => value = None,
            JSXAttributeItem::Attribute(attr) => {
                if matches!(&attr.name, JSXAttributeName::Identifier(name) if name.name == "durationInFrames")
                {
                    value = Some(&attr.value);
                }
            }
        }
    }
    let Some(JSXAttributeValue::ExpressionContainer(container)) = value? else {
        return None;
    };
    match container.expression.as_expression()?.without_parentheses() {
        Expression::StaticMemberExpression(member)
            if member.property.name == "durationInFrames" =>
        {
            match member.object.without_parentheses() {
                Expression::Identifier(id) => Some(id.name.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The names the file's top-level `const`s (exported ones too) declare.
fn top_level_consts<'p>(program: &'p Program<'_>) -> impl Iterator<Item = &'p str> {
    program
        .body
        .iter()
        .filter_map(|stmt| match stmt {
            Statement::VariableDeclaration(decl) => Some(&**decl),
            Statement::ExportDeclaration(export) => match &export.declaration {
                Declaration::VariableDeclaration(decl) => Some(&**decl),
                _ => None,
            },
            _ => None,
        })
        .filter(|decl| decl.kind == VariableDeclarationKind::Const)
        .flat_map(|decl| &decl.declarations)
        .filter_map(|d| d.id.get_binding_identifier())
        .map(|id| id.name.as_str())
}
