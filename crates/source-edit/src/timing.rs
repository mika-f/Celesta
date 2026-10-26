//! Timing wrappers (`Sequence`, `Series.Sequence`) told apart through the
//! file's `@celesta/react` import, and the `Series` items after one.

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingIdentifier, ImportDeclarationSpecifier, JSXChild, JSXElement, JSXElementName,
    JSXMemberExpression, JSXMemberExpressionObject, ModuleExportName, Program, Statement,
};
use oxc_ast_visit::Visit;

use crate::locate::ElementRef;
use crate::parse_checked;

/// The package whose components the editor knows.
const PACKAGE: &str = "@celesta/react";

/// Which timing wrapper an element is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingKind {
    /// `<Sequence from durationInFrames>`.
    Sequence,
    /// `<Series.Sequence durationInFrames offset>`, an item of a `<Series>`.
    SeriesSequence,
}

/// The timing wrapper `element` is, through the file's `@celesta/react`
/// import (aliases and `import * as C` included); `None` for anything
/// else, or when the text does not parse.
pub fn timing_kind(text: &str, element: &ElementRef) -> Option<TimingKind> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return None;
    }
    let module = Module::of(&program);
    let mut find = FindKind {
        module: &module,
        start: element.start,
        found: None,
    };
    find.visit_program(&program);
    find.found.flatten()
}

/// The `<Series.Sequence>` siblings after `element` in its `<Series>`, as
/// 1-based line and UTF-16 column (what `element_at` takes). Only the
/// `Series`' own children count; empty when `element` is not one of them.
pub fn later_series_items(text: &str, element: &ElementRef) -> Vec<(u32, u32)> {
    let alloc = Allocator::default();
    let (program, failed) = parse_checked(text, &alloc);
    if failed {
        return Vec::new();
    }
    let module = Module::of(&program);
    let mut find = FindSiblings {
        module: &module,
        start: element.start,
        found: None,
    };
    find.visit_program(&program);
    find.found
        .unwrap_or_default()
        .into_iter()
        .map(|offset| position_of(text, offset))
        .collect()
}

/// The 1-based line and UTF-16 column of byte `offset`, as esbuild's
/// `jsxDEV` reports an element (only `\n` ends a line).
fn position_of(text: &str, offset: u32) -> (u32, u32) {
    let before = &text[..offset as usize];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count() as u32 + 1;
    let column = before[line_start..].encode_utf16().count() as u32 + 1;
    (line, column)
}

/// A value import: `import { name as local }`, `import local`, or
/// `import * as local` from `source`.
#[derive(Debug)]
pub(crate) struct Import {
    pub(crate) local: String,
    pub(crate) source: String,
    /// The imported name; `default` for a default import, `None` for a
    /// namespace.
    pub(crate) imported: Option<String>,
}

/// What a file imports, and how often it declares each name.
pub(crate) struct Module {
    imports: Vec<Import>,
    /// Every name the file declares (imports, variables, functions,
    /// parameters, in any scope), with how many times.
    declared: HashMap<String, usize>,
}

impl Module {
    pub(crate) fn of(program: &Program<'_>) -> Self {
        let mut imports = Vec::new();
        for stmt in &program.body {
            let Statement::ImportDeclaration(import) = stmt else {
                continue;
            };
            if import.import_kind.is_type() {
                continue;
            }
            for specifier in import.specifiers.iter().flatten() {
                let (local, imported) = match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(s) if !s.import_kind.is_type() => {
                        (&s.local, Some(export_name(&s.imported)))
                    }
                    ImportDeclarationSpecifier::ImportSpecifier(_) => continue,
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        (&s.local, Some("default".to_string()))
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => (&s.local, None),
                };
                imports.push(Import {
                    local: local.name.to_string(),
                    source: import.source.value.to_string(),
                    imported,
                });
            }
        }
        let mut declarations = Declarations::default();
        declarations.visit_program(program);
        Self {
            imports,
            declared: declarations.0,
        }
    }

    /// The import that binds `local`, when nothing else in the file
    /// declares the name (an inner declaration may shadow it).
    pub(crate) fn import(&self, local: &str) -> Option<&Import> {
        if self.declared(local) != 1 {
            return None;
        }
        self.imports.iter().find(|import| import.local == local)
    }

    /// How many times the file declares `name`.
    pub(crate) fn declared(&self, name: &str) -> usize {
        self.declared.get(name).copied().unwrap_or(0)
    }

    /// The `@celesta/react` export a JSX tag names, as a path:
    /// `<Series.Sequence>`, `<S.Sequence>` with `Series as S` and
    /// `<C.Series.Sequence>` with `import * as C` are all
    /// `["Series", "Sequence"]`. `None` when the tag's root is not bound by
    /// that import.
    pub(crate) fn export_of(&self, name: &JSXElementName<'_>) -> Option<Vec<String>> {
        let mut path = tag_path(name)?;
        let import = self.import(&path[0])?;
        if import.source != PACKAGE {
            return None;
        }
        match &import.imported {
            Some(imported) => path[0] = imported.clone(),
            None => {
                path.remove(0);
            }
        }
        (!path.is_empty()).then_some(path)
    }

    /// [`Self::export_of`] as a timing wrapper.
    pub(crate) fn kind_of(&self, name: &JSXElementName<'_>) -> Option<TimingKind> {
        let path = self.export_of(name)?;
        match path.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["Sequence"] => Some(TimingKind::Sequence),
            ["Series", "Sequence"] => Some(TimingKind::SeriesSequence),
            _ => None,
        }
    }
}

/// The names of a tag: `<A.B.C>` is `["A", "B", "C"]`. `None` for
/// `<this.X>` and `<a:b>`.
fn tag_path(name: &JSXElementName<'_>) -> Option<Vec<String>> {
    match name {
        JSXElementName::Identifier(id) => Some(vec![id.name.to_string()]),
        JSXElementName::IdentifierReference(id) => Some(vec![id.name.to_string()]),
        JSXElementName::MemberExpression(member) => member_path(member),
        JSXElementName::NamespacedName(_) | JSXElementName::ThisExpression(_) => None,
    }
}

fn member_path(member: &JSXMemberExpression<'_>) -> Option<Vec<String>> {
    let mut path = match &member.object {
        JSXMemberExpressionObject::IdentifierReference(id) => vec![id.name.to_string()],
        JSXMemberExpressionObject::MemberExpression(inner) => member_path(inner)?,
        JSXMemberExpressionObject::ThisExpression(_) => return None,
    };
    path.push(member.property.name.to_string());
    Some(path)
}

fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(id) => id.name.to_string(),
        ModuleExportName::IdentifierReference(id) => id.name.to_string(),
        ModuleExportName::StringLiteral(s) => s.value.to_string(),
    }
}

#[derive(Default)]
struct Declarations(HashMap<String, usize>);

impl<'a> Visit<'a> for Declarations {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        *self.0.entry(it.name.to_string()).or_default() += 1;
    }
}

/// The kind of the element whose `<` is at `start`.
struct FindKind<'m> {
    module: &'m Module,
    start: u32,
    found: Option<Option<TimingKind>>,
}

impl<'a> Visit<'a> for FindKind<'_> {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if self.found.is_some() {
            return;
        }
        let opening = &it.opening_element;
        if opening.span.start == self.start {
            self.found = Some(self.module.kind_of(&opening.name));
            return;
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }
}

/// The offsets of the `Series` items after the child whose `<` is at
/// `start`, once its `<Series>` is found.
struct FindSiblings<'m> {
    module: &'m Module,
    start: u32,
    found: Option<Vec<u32>>,
}

impl<'a> Visit<'a> for FindSiblings<'_> {
    fn visit_jsx_element(&mut self, it: &JSXElement<'a>) {
        if self.found.is_some() {
            return;
        }
        let series = self
            .module
            .export_of(&it.opening_element.name)
            .is_some_and(|path| path == ["Series"]);
        let at = it.children.iter().position(
            |child| matches!(child, JSXChild::Element(e) if e.opening_element.span.start == self.start),
        );
        if let (true, Some(at)) = (series, at) {
            self.found = Some(
                it.children[at + 1..]
                    .iter()
                    .filter_map(|child| match child {
                        JSXChild::Element(e)
                            if self.module.kind_of(&e.opening_element.name)
                                == Some(TimingKind::SeriesSequence) =>
                        {
                            Some(e.opening_element.span.start)
                        }
                        _ => None,
                    })
                    .collect(),
            );
            return;
        }
        oxc_ast_visit::walk::walk_jsx_element(self, it);
    }
}
