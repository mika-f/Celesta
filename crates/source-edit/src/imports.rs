//! What a file imports, and the edit that imports more from a module.

use std::collections::{BTreeMap, HashMap};

use oxc_ast::ast::{
    BindingIdentifier, Directive, ImportDeclarationSpecifier, JSXAttribute, JSXAttributeValue,
    JSXElementName, JSXMemberExpression, JSXMemberExpressionObject, ModuleExportName, Program,
    Statement, StringLiteral,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;

use crate::apply::Edit;
use crate::resolve::Span;
use crate::{line_break, with_program};

/// The package whose components the editor knows.
pub const PACKAGE: &str = "@celesta.studio/react";

/// The scope the Celesta packages had before `@celesta.studio`, which the
/// runtime still resolves.
const LEGACY_SCOPE: &str = "@celesta/";

/// An import's source with [`LEGACY_SCOPE`] renamed, so a file importing
/// from `@celesta/react` imports [`PACKAGE`].
fn current_source(source: String) -> String {
    match source.strip_prefix(LEGACY_SCOPE) {
        Some(name) => format!("@celesta.studio/{name}"),
        None => source,
    }
}

/// How to write a name imported from a module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportName {
    /// The local name to write: `Sequence`, or `Seq` for `Sequence as Seq`.
    Named(String),
    /// The member to write: `C.Sequence` under `import * as C`.
    Namespace(String),
}

/// Why a name can't be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportLock {
    /// The file binds `name` otherwise: a local declaration, a type-only
    /// import, an import from another module, or a declaration shadowing
    /// the module's import.
    Clash { name: String },
    /// The text does not parse.
    SyntaxError,
}

impl ImportLock {
    /// The lock as the editor shows it.
    pub fn text(&self) -> String {
        match self {
            Self::Clash { name } => format!("`{name}` here is not Celesta’s"),
            Self::SyntaxError => SYNTAX_ERROR.to_owned(),
        }
    }
}

/// Why nothing is written into a file with syntax errors.
pub(crate) const SYNTAX_ERROR: &str = "The source has syntax errors";

/// The one edit (if any) that makes every name of `names` from `module`
/// usable in `text`, and how to write each, in `names` order.
///
/// A name already imported from `module` writes its local name (the alias,
/// if any); under `import * as C` it writes `C.name`. The others go into
/// the first value import of `module` with a `{ … }` list (a sorted list,
/// compared ignoring case, stays sorted; any other list gets them at its
/// end), else into a new import line after the last import (after a
/// comment ending its line too), sorted, with that import's quotes and
/// semicolon. In a file without imports the line goes first, quoted as the
/// file's first string outside JSX and ended with `;` when its first
/// statement is (`"` and `;` when it has neither). `import type { … }` is never
/// extended. Several names into a list are one splice from the first
/// insertion point to the last, the text between kept verbatim.
pub fn ensure_imports(
    text: &str,
    module: &str,
    names: &[&str],
) -> Result<(Option<Edit>, Vec<ImportName>), ImportLock> {
    if names.is_empty() {
        return Ok((None, Vec::new()));
    }
    with_program(text, |program| {
        ensure_imports_in(text, program, module, names)
    })
    .unwrap_or(Err(ImportLock::SyntaxError))
}

/// [`ensure_imports`] in `text` already parsed (without error) as
/// `program`.
pub(crate) fn ensure_imports_in(
    text: &str,
    program: &Program<'_>,
    module: &str,
    names: &[&str],
) -> Result<(Option<Edit>, Vec<ImportName>), ImportLock> {
    let file = Module::of(program);
    let mut written = Vec::with_capacity(names.len());
    let mut missing: Vec<&str> = Vec::new();
    for &name in names {
        if let Some(usable) = file.usable(module, name) {
            written.push(usable);
        } else if file.declared(name) > 0 {
            return Err(ImportLock::Clash {
                name: name.to_string(),
            });
        } else {
            if !missing.contains(&name) {
                missing.push(name);
            }
            written.push(ImportName::Named(name.to_string()));
        }
    }
    if missing.is_empty() {
        return Ok((None, written));
    }
    let edit = file
        .extend(text, module, &missing)
        .unwrap_or_else(|| file.new_line(text, program, module, &missing));
    Ok((Some(edit), written))
}

/// [`ensure_imports`] for one name.
pub fn ensure_import(
    text: &str,
    module: &str,
    name: &str,
) -> Result<(Option<Edit>, ImportName), ImportLock> {
    let (edit, mut written) = ensure_imports(text, module, &[name])?;
    Ok((edit, written.remove(0)))
}

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
    /// A `{ … }` member, rather than a default or namespace import.
    member: bool,
    /// The specifier's bytes (`name as local`).
    span: Span,
    /// The index of the statement holding it in [`Module::statements`].
    statement: usize,
}

/// An `import … from source` statement.
#[derive(Debug)]
struct ImportStatement {
    /// The whole statement, its semicolon included.
    span: Span,
    /// Where the source string's opening quote is.
    source_start: u32,
    source: String,
    /// `import type …`.
    type_only: bool,
}

/// What a file imports, and how often it declares each name.
pub(crate) struct Module {
    imports: Vec<Import>,
    statements: Vec<ImportStatement>,
    /// Every name the file declares (imports, variables, functions,
    /// parameters, in any scope), with how many times.
    declared: HashMap<String, usize>,
}

impl Module {
    pub(crate) fn of(program: &Program<'_>) -> Self {
        let mut imports = Vec::new();
        let mut statements = Vec::new();
        for stmt in &program.body {
            let Statement::ImportDeclaration(import) = stmt else {
                continue;
            };
            let statement = statements.len();
            let type_only = import.import_kind.is_type();
            for specifier in import.specifiers.iter().flatten() {
                let (local, imported, member_type, member) = match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => (
                        &s.local,
                        Some(export_name(&s.imported)),
                        s.import_kind.is_type(),
                        true,
                    ),
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        (&s.local, Some("default".to_string()), false, false)
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                        (&s.local, None, false, false)
                    }
                };
                // A lone surrogate is written escaped (`\ud800`), so such a source or
                // name never equals `PACKAGE` or an export name.
                let span = specifier.span();
                imports.push(Import {
                    local: local.name.to_string(),
                    source: current_source(import.source.value.display().to_string()),
                    imported,
                    type_only: type_only || member_type,
                    member,
                    span: Span {
                        start: span.start,
                        end: span.end,
                    },
                    statement,
                });
            }
            statements.push(ImportStatement {
                span: Span {
                    start: import.span.start,
                    end: import.span.end,
                },
                source_start: import.source.span.start,
                source: current_source(import.source.value.display().to_string()),
                type_only,
            });
        }
        let mut declarations = Declarations::default();
        declarations.visit_program(program);
        Self {
            imports,
            statements,
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

    /// The `@celesta.studio/react` export a JSX tag names, as a path:
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

    /// How `name` from `module` is written already: through a value import
    /// of it, else through a namespace import, either bound only there.
    fn usable(&self, module: &str, name: &str) -> Option<ImportName> {
        let mut values = self.imports.iter().filter(|import| {
            import.source == module && !import.type_only && self.declared(&import.local) == 1
        });
        if let Some(import) = values
            .clone()
            .find(|import| import.member && import.imported.as_deref() == Some(name))
        {
            return Some(ImportName::Named(import.local.clone()));
        }
        values
            .find(|import| import.imported.is_none())
            .map(|import| ImportName::Namespace(format!("{}.{name}", import.local)))
    }

    /// `names` added to the `{ … }` list of the first value import of
    /// `module` that has one.
    fn extend(&self, text: &str, module: &str, names: &[&str]) -> Option<Edit> {
        let statement = self.statements.iter().enumerate().position(|(i, s)| {
            s.source == module
                && !s.type_only
                && self.imports.iter().any(|m| m.statement == i && m.member)
        })?;
        let members: Vec<&Import> = self
            .imports
            .iter()
            .filter(|m| m.statement == statement && m.member)
            .collect();
        let key = |m: &Import| m.imported.as_deref().unwrap_or_default().to_lowercase();
        let last = members.last()?.span.end;
        let separator = separator(text, self.statements[statement].span.start, &members);

        // What goes at each offset: before a member, `name, `; after the
        // last one, `, name`.
        let mut inserts: BTreeMap<u32, String> = BTreeMap::new();
        if members.windows(2).all(|w| key(w[0]) <= key(w[1])) {
            let mut sorted = names.to_vec();
            sorted.sort_by_key(|name| name.to_lowercase());
            for name in sorted {
                let lower = name.to_lowercase();
                match members.iter().find(|m| key(m) > lower) {
                    Some(next) => {
                        let insert = inserts.entry(next.span.start).or_default();
                        insert.push_str(name);
                        insert.push_str(&separator);
                    }
                    None => {
                        let insert = inserts.entry(last).or_default();
                        insert.push_str(&separator);
                        insert.push_str(name);
                    }
                }
            }
        } else {
            let insert = inserts.entry(last).or_default();
            for name in names {
                insert.push_str(&separator);
                insert.push_str(name);
            }
        }

        let (&start, _) = inserts.first_key_value()?;
        let (&end, _) = inserts.last_key_value()?;
        let mut spliced = String::new();
        let mut cursor = start;
        for (at, insert) in inserts {
            spliced.push_str(&text[cursor as usize..at as usize]);
            spliced.push_str(&insert);
            cursor = at;
        }
        Some(Edit::Splice {
            start,
            end,
            text: spliced,
        })
    }

    /// A new `import { names } from module` line after the last import,
    /// or first in a file without imports.
    fn new_line(&self, text: &str, program: &Program<'_>, module: &str, names: &[&str]) -> Edit {
        let mut sorted = names.to_vec();
        sorted.sort_by_key(|name| name.to_lowercase());
        let list = sorted.join(", ");
        let newline = line_break(text);
        let Some(last) = self.statements.last() else {
            let (quote, semicolon) = file_style(text, program);
            let line = format!("import {{ {list} }} from {quote}{module}{quote}{semicolon}");
            // After a hashbang or the directives (`"use client"`), else
            // at the top.
            let after = program
                .directives
                .last()
                .map(|d| d.span.end)
                .or(program.hashbang.as_ref().map(|h| h.span.end));
            return match after {
                Some(at) => Edit::Splice {
                    start: at,
                    end: at,
                    text: format!("{newline}{line}"),
                },
                None => Edit::Splice {
                    start: 0,
                    end: 0,
                    text: format!("{line}{newline}"),
                },
            };
        };
        let quote = text[last.source_start as usize..]
            .chars()
            .next()
            .unwrap_or('"');
        let semicolon = if text[..last.span.end as usize].ends_with(';') {
            ";"
        } else {
            ""
        };
        let at = after_trailing_comment(text, last.span.end);
        Edit::Splice {
            start: at,
            end: at,
            text: format!("{newline}import {{ {list} }} from {quote}{module}{quote}{semicolon}"),
        }
    }
}

/// The end of a comment that ends the line at `end` (`// …`, or `/* … */`
/// closing on it), else `end`: a new line goes after the comment, which
/// belongs to the code before it.
fn after_trailing_comment(text: &str, end: u32) -> u32 {
    let rest = &text[end as usize..];
    let line = rest[..rest.find('\n').unwrap_or(rest.len())].trim_end_matches('\r');
    let comment = line.trim_start();
    let ends_line = comment.starts_with("//")
        || (comment.starts_with("/*")
            && comment
                .find("*/")
                .is_some_and(|close| close + 2 == comment.len()));
    if ends_line {
        end + line.len() as u32
    } else {
        end
    }
}

/// How a file without imports quotes and ends a line: the quote of its
/// first string outside JSX attributes (`"` without one), and `;` unless
/// its first statement or directive ends without one.
fn file_style(text: &str, program: &Program<'_>) -> (char, &'static str) {
    let mut first = FirstString(None);
    first.visit_program(program);
    let quote = first
        .0
        .and_then(|at| text[at as usize..].chars().next())
        .unwrap_or('"');
    let first_end = program
        .directives
        .first()
        .map(|d| d.span.end)
        .or_else(|| program.body.first().map(|s| s.span().end));
    let semicolon = match first_end {
        Some(end) if !text[..end as usize].ends_with(';') => "",
        _ => ";",
    };
    (quote, semicolon)
}

/// Where the first string literal outside JSX attributes starts (prettier
/// quotes those apart, with `"` by default). Directives count.
struct FirstString(Option<u32>);

impl<'a> Visit<'a> for FirstString {
    fn visit_directive(&mut self, it: &Directive<'a>) {
        if self.0.is_none() {
            self.0 = Some(it.expression.span.start);
        }
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        if self.0.is_none() {
            self.0 = Some(it.span.start);
        }
    }

    fn visit_jsx_attribute(&mut self, it: &JSXAttribute<'a>) {
        // The value is a JSX string; a `{…}` value holds code.
        if let Some(JSXAttributeValue::ExpressionContainer(container)) = &it.value {
            self.visit_jsx_expression_container(container);
        }
    }
}

/// What separates the members of a `{ … }` list: the text between the
/// first two when it is a comma and whitespace; else `,` and the line
/// break before the first member, for a list on several lines; else `, `.
fn separator(text: &str, statement_start: u32, members: &[&Import]) -> String {
    if let [first, second, ..] = members {
        let between = &text[first.span.end as usize..second.span.start as usize];
        if between
            .strip_prefix(',')
            .is_some_and(|rest| rest.chars().all(char::is_whitespace))
        {
            return between.to_string();
        }
    }
    let before = &text[statement_start as usize..members[0].span.start as usize];
    let space = &before[before.trim_end().len()..];
    if space.contains('\n') {
        format!(",{space}")
    } else {
        ", ".to_string()
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
        ModuleExportName::StringLiteral(s) => s.value.display().to_string(),
    }
}

#[derive(Default)]
struct Declarations(HashMap<String, usize>);

impl<'a> Visit<'a> for Declarations {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        *self.0.entry(it.name.to_string()).or_default() += 1;
    }
}
