//! `defineScenes([...])` lists and how a `<Composition>` writes its length.
//! Neither is JSX the runtime reports a position for, so they are found by
//! reading the files.

use std::path::Path;

use oxc_ast::ast::{
    CallExpression, Declaration, ExportDefaultDeclarationKind, Expression, JSXAttributeItem,
    JSXAttributeName, JSXAttributeValue, JSXOpeningElement, ModuleExportName, ObjectExpression,
    ObjectPropertyKind, Program, PropertyKind, Statement, VariableDeclaration,
    VariableDeclarationKind,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;

use crate::imports::{Module, PACKAGE};
use crate::inputs::inputs_at;
use crate::resolve::{EditTarget, LockReason, Span, resolve_value_at};
use crate::shape::{span_of, string_of};
use crate::{source_type_of, with_program_as};

/// A `defineScenes(…)` call.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneList {
    /// `X` of `const X = defineScenes(…)`.
    pub binding: Option<String>,
    /// The names the file exports the list under: `X` for `export const X`
    /// and `export { X }`, `Y` for `export { X as Y }`, and `default` for
    /// `export default X` or `export default defineScenes(…)`.
    pub exports: Vec<String>,
    /// The entries read, in list order: object literals with a
    /// string-literal id.
    pub entries: Vec<SceneItem>,
    /// Where the list holds something else (a spread, a `.map()`, a hole,
    /// an entry with a spread, a computed key or another id), as the
    /// number of read entries before it; one `0` when the argument is not
    /// an array literal. Empty when every entry was read.
    pub unread: Vec<usize>,
}

/// An entry of a [`SceneList`].
#[derive(Debug, Clone, PartialEq)]
pub struct SceneItem {
    pub id: String,
    /// The entry's `durationInFrames`, by the rules of a prop value (a
    /// `const` of the file read too); `Exported` for a `const` the file
    /// exports, `NoSceneDuration` when it is not written.
    pub duration: EditTarget,
    /// When `duration` is an expression, the identifiers it reads.
    pub inputs: Vec<(String, EditTarget)>,
}

/// Every call of `@celesta.studio/react`'s `defineScenes(…)` in `text`
/// (parsed as `path`'s type; a function of another module or of the file's
/// own is not it), in source order. The entries that are object literals
/// with a string-literal id are read; where the list holds anything else
/// is in [`SceneList::unread`].
pub fn scene_lists(text: &str, path: &Path) -> Vec<SceneList> {
    with_program_as(text, source_type_of(path), |program| {
        let module = Module::of(program);
        let mut calls = Calls {
            module: &module,
            lists: Vec::new(),
            bindings: Vec::new(),
        };
        calls.visit_program(program);
        let Calls {
            lists, bindings, ..
        } = calls;
        let exports = local_exports(program);
        let exported: Vec<String> = exports
            .iter()
            .filter_map(|export| match export {
                LocalExport::Name { local, .. } => Some(local.clone()),
                LocalExport::DefaultCall(_) => None,
            })
            .collect();
        lists
            .into_iter()
            .map(|(start, read)| {
                let binding = bindings
                    .iter()
                    .find(|(at, _)| *at == start)
                    .map(|(_, name)| name.clone());
                SceneList {
                    exports: exports
                        .iter()
                        .filter_map(|export| match export {
                            LocalExport::Name { local, exported } => {
                                (binding.as_ref() == Some(local)).then(|| exported.clone())
                            }
                            LocalExport::DefaultCall(at) => {
                                (*at == start).then(|| "default".to_owned())
                            }
                        })
                        .collect(),
                    binding,
                    entries: read
                        .entries
                        .into_iter()
                        .map(|(id, duration)| item(program, text, &exported, id, duration))
                        .collect(),
                    unread: read.unread,
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

/// How `<Composition durationInFrames>` is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionLength {
    /// `X.durationInFrames`, `X` a scene list's binding.
    ListDuration {
        /// The import specifier `X` comes from; `None`: this file.
        module: Option<String>,
        /// The name: the export's in that module (`default` for a default
        /// import; see [`SceneList::exports`]), the binding's in this file.
        name: String,
    },
    /// A literal, any other expression, or none.
    Other,
}

/// How `<Composition durationInFrames>` is written in `text` (parsed as
/// `path`'s type); `None` without a `<Composition>` (from
/// `@celesta.studio/react`), or when the text does not parse. The first
/// `<Composition>` counts.
pub fn composition_length(text: &str, path: &Path) -> Option<CompositionLength> {
    with_program_as(text, source_type_of(path), |program| {
        let module = Module::of(program);
        let mut find = FindComposition {
            module: &module,
            found: None,
        };
        find.visit_program(program);
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
        let local =
            module.declared(&list) == 1 && top_level_consts(program).any(|name| name == list);
        Some(if local {
            CompositionLength::ListDuration {
                module: None,
                name: list,
            }
        } else {
            CompositionLength::Other
        })
    })
    .flatten()
}

/// An entry's `durationInFrames` as written.
enum Duration {
    /// The value expression, and the name it reads when it is a bare
    /// identifier.
    Value(Span, Option<String>),
    /// A method or accessor: its source.
    Other(Span),
    /// Not written (`defineScenes` throws on it).
    Absent,
}

/// An entry of a list: its duration resolved as a prop value, except that
/// a `const` the file exports is locked (writing it would change the files
/// importing it too), and a missing one is locked.
fn item(
    program: &Program<'_>,
    text: &str,
    exported: &[String],
    id: String,
    duration: Duration,
) -> SceneItem {
    let (duration, inputs) = match duration {
        Duration::Value(span, name) => {
            // An `export { X }` reads `X` too: exported, it is not shared
            // within the file only.
            let exported = name.filter(|name| exported.contains(name));
            let duration = match (resolve_value_at(program, text, span), exported) {
                (
                    EditTarget::Locked(LockReason::SharedBinding { .. })
                    | EditTarget::Literal { .. },
                    Some(name),
                ) => EditTarget::Locked(LockReason::Exported { name }),
                (duration, _) => duration,
            };
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
        Duration::Absent => (
            EditTarget::Locked(LockReason::NoSceneDuration { id: id.clone() }),
            Vec::new(),
        ),
    };
    SceneItem {
        id,
        duration,
        inputs,
    }
}

/// The `defineScenes` calls (of `@celesta.studio/react`'s, through the file's
/// import), by the offset of the call, and the `const`s that hold a call,
/// by the same offset.
struct Calls<'m> {
    module: &'m Module,
    lists: Vec<(u32, ListRead)>,
    bindings: Vec<(u32, String)>,
}

/// What a `defineScenes(…)` argument holds that can be read.
struct ListRead {
    entries: Vec<(String, Duration)>,
    /// See [`SceneList::unread`].
    unread: Vec<usize>,
}

impl Calls<'_> {
    /// Whether `callee` is `@celesta.studio/react`'s `defineScenes`: imported by
    /// name (under any name), or through `import * as C`.
    fn define_scenes(&self, callee: &Expression<'_>) -> bool {
        let import = match callee.without_parentheses() {
            Expression::Identifier(id) => self
                .module
                .import(&id.name)
                .filter(|import| import.imported.as_deref() == Some("defineScenes")),
            Expression::StaticMemberExpression(member)
                if member.property.name == "defineScenes" =>
            {
                match member.object.without_parentheses() {
                    Expression::Identifier(id) => self
                        .module
                        .import(&id.name)
                        .filter(|import| import.imported.is_none()),
                    _ => None,
                }
            }
            _ => None,
        };
        import.is_some_and(|import| import.source == PACKAGE)
    }
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
        if self.define_scenes(&it.callee) {
            self.lists.push((it.span.start, list_of(it)));
        }
        oxc_ast_visit::walk::walk_call_expression(self, it);
    }
}

/// The entries of a `defineScenes(…)` call that are object literals with
/// string-literal ids, and where it holds anything else.
fn list_of(call: &CallExpression<'_>) -> ListRead {
    let array = call
        .arguments
        .first()
        .and_then(|argument| argument.as_expression())
        .map(Expression::without_parentheses);
    let Some(Expression::ArrayExpression(array)) = array else {
        return ListRead {
            entries: Vec::new(),
            unread: vec![0],
        };
    };
    let mut read = ListRead {
        entries: Vec::new(),
        unread: Vec::new(),
    };
    for element in &array.elements {
        let entry = match element.as_expression().map(Expression::without_parentheses) {
            Some(Expression::ObjectExpression(object)) => entry_of(object),
            _ => None,
        };
        match entry {
            Some(entry) => read.entries.push(entry),
            None => read.unread.push(read.entries.len()),
        }
    }
    read
}

/// An entry's id and `durationInFrames` (a later key wins, as at runtime),
/// or `None` when a spread or a computed key may set either, or the id is
/// not a string literal.
fn entry_of(object: &ObjectExpression<'_>) -> Option<(String, Duration)> {
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
        match p
            .key
            .static_name()
            .and_then(|name| name.into_cow_str())
            .as_deref()
        {
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
                    let name = match p.value.without_parentheses() {
                        Expression::Identifier(name) => Some(name.name.to_string()),
                        _ => None,
                    };
                    Duration::Value(span_of(p.value.span()), name)
                } else {
                    Duration::Other(span_of(p.span))
                });
            }
            _ => {}
        }
    }
    Some((id?, duration.unwrap_or(Duration::Absent)))
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

/// What the file exports of its own.
enum LocalExport {
    /// A local name, and the name it is exported under: `export const X`,
    /// `export { X }`, `export { X as Y }`, `export default X`.
    Name { local: String, exported: String },
    /// `export default <call>`: the offset of the call.
    DefaultCall(u32),
}

/// What the file exports of its own. `export { X } from './y'` (an
/// `ExportFromDeclaration`) exports another module's `X`, not a local one,
/// and is left out, as is `export type { X }`.
fn local_exports(program: &Program<'_>) -> Vec<LocalExport> {
    let mut exports = Vec::new();
    for stmt in &program.body {
        match stmt {
            Statement::ExportDeclaration(export) => {
                if let Declaration::VariableDeclaration(decl) = &export.declaration {
                    exports.extend(
                        decl.declarations
                            .iter()
                            .filter_map(|d| d.id.get_binding_identifier())
                            .map(|id| LocalExport::Name {
                                local: id.name.to_string(),
                                exported: id.name.to_string(),
                            }),
                    );
                }
            }
            Statement::ExportNamedDeclaration(export) if !export.export_kind.is_type() => {
                exports.extend(export.specifiers.iter().filter_map(|specifier| {
                    match &specifier.local {
                        ModuleExportName::IdentifierReference(id) => Some(LocalExport::Name {
                            local: id.name.to_string(),
                            exported: specifier.exported.name().to_string(),
                        }),
                        _ => None,
                    }
                }));
            }
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::Identifier(id) => exports.push(LocalExport::Name {
                    local: id.name.to_string(),
                    exported: "default".to_owned(),
                }),
                declaration => {
                    if let Some(Expression::CallExpression(call)) = declaration
                        .as_expression()
                        .map(Expression::get_inner_expression)
                    {
                        exports.push(LocalExport::DefaultCall(call.span.start));
                    }
                }
            },
            _ => {}
        }
    }
    exports
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
