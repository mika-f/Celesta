//! What a file imports.

use std::collections::HashMap;

use oxc_ast::ast::{
    BindingIdentifier, ImportDeclarationSpecifier, JSXElementName, JSXMemberExpression,
    JSXMemberExpressionObject, ModuleExportName, Program, Statement,
};
use oxc_ast_visit::Visit;

/// The package whose components the editor knows.
pub(crate) const PACKAGE: &str = "@celesta/react";

/// An import: `import { name as local }`, `import local`, or
/// `import * as local` from `source`.
#[derive(Debug)]
pub(crate) struct Import {
    pub(crate) local: String,
    pub(crate) source: String,
    /// The imported name; `default` for a default import, `None` for a
    /// namespace.
    pub(crate) imported: Option<String>,
    /// `import type { … }` or `import { type … }`: it binds no value.
    pub(crate) type_only: bool,
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
            let type_only = import.import_kind.is_type();
            for specifier in import.specifiers.iter().flatten() {
                let (local, imported, member_type) = match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => (
                        &s.local,
                        Some(export_name(&s.imported)),
                        s.import_kind.is_type(),
                    ),
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        (&s.local, Some("default".to_string()), false)
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                        (&s.local, None, false)
                    }
                };
                imports.push(Import {
                    local: local.name.to_string(),
                    source: import.source.value.to_string(),
                    imported,
                    type_only: type_only || member_type,
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

    /// The value import that binds `local`, when nothing else in the file
    /// declares the name (an inner declaration may shadow it).
    pub(crate) fn import(&self, local: &str) -> Option<&Import> {
        if self.declared(local) != 1 {
            return None;
        }
        self.imports
            .iter()
            .find(|import| import.local == local && !import.type_only)
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
