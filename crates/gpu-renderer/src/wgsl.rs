//! Checks shared by the WGSL Celesta assembles around someone else's code:
//! custom shaders (`shader.rs`) and scene materials (`scene_material.rs`).
//! Each is one module of Celesta's own declarations, the author's code at
//! a known byte range, and Celesta's entry points after it.

use std::fmt::Write as _;
use std::ops::Range;

/// `message`, located at byte `at` of `code`: as a line and column of the
/// author's code (the `author` range) when it lies there, followed by
/// `before` when it lies in Celesta's declarations before that code, or by
/// `after` when it lies in the entry points after it. An empty hint adds
/// nothing.
pub(crate) fn locate(
    code: &str,
    author: &Range<usize>,
    at: Option<usize>,
    message: String,
    before: &str,
    after: &str,
) -> String {
    let hinted = |hint: &str| {
        if hint.is_empty() {
            message.clone()
        } else {
            format!("{message} {hint}")
        }
    };
    match at {
        Some(at) if author.contains(&at) || (at == author.end && !author.is_empty()) => {
            let before = &code[author.start..at];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |newline| newline + 1) + 1;
            format!("{line}:{column}: {message}")
        }
        Some(at) if at < author.start => hinted(before),
        Some(at) if at > author.end => hinted(after),
        _ => message,
    }
}

/// `error` and the errors that caused it, outermost first.
pub(crate) fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        write!(message, ": {error}").expect("writing to a String cannot fail");
        cause = error.source();
    }
    message
}

/// Rejects entry points and resources the author declared, and names
/// starting with `celesta` there: Celesta's declarations bind everything
/// the author's code gets. `prelude_resource` says which resource names
/// Celesta declares, `entry_points` which entry points are its own, and
/// `what` names the author's code in messages ("a custom shader").
pub(crate) fn check_declarations(
    module: &naga::Module,
    code: &str,
    author: &Range<usize>,
    prelude_resource: &dyn Fn(&str) -> bool,
    entry_points: &[&str],
    what: &str,
) -> Result<(), String> {
    let start = |span: naga::Span| span.to_range().map(|range| range.start);
    let fail = |at: Option<usize>, message: String| Err(locate(code, author, at, message, "", ""));
    for entry in &module.entry_points {
        if !entry_points.contains(&entry.name.as_str()) {
            // naga keeps no span for an entry point; its `fn` is in the
            // author's code.
            let at = code[author.clone()]
                .find(&format!("fn {}", entry.name))
                .map(|offset| author.start + offset);
            return fail(
                at,
                format!(
                    "declares the entry point `{}`; {what} cannot declare entry points of its own",
                    entry.name
                ),
            );
        }
    }
    for (handle, global) in module.global_variables.iter() {
        let name = global.name.as_deref().unwrap_or("");
        let at = start(module.global_variables.get_span(handle));
        // Told apart by where they are declared, not by name: the author
        // could otherwise bind a resource of a prelude's name that this
        // module does not declare.
        let authored = at.is_none_or(|at| author.contains(&at));
        if global.binding.is_some() && (authored || !prelude_resource(name)) {
            return fail(
                at,
                format!("declares the resource `{name}`; {what} cannot bind resources of its own"),
            );
        }
    }
    // Names starting with `celesta` belong to Celesta, whose own
    // declarations lie outside the author's code.
    let declared = module
        .functions
        .iter()
        .map(|(handle, function)| (function.name.as_deref(), module.functions.get_span(handle)))
        .chain(module.global_variables.iter().map(|(handle, global)| {
            (
                global.name.as_deref(),
                module.global_variables.get_span(handle),
            )
        }))
        .chain(module.constants.iter().map(|(handle, constant)| {
            (constant.name.as_deref(), module.constants.get_span(handle))
        }))
        .chain(
            module
                .types
                .iter()
                .map(|(handle, ty)| (ty.name.as_deref(), module.types.get_span(handle))),
        );
    for (name, span) in declared {
        let at = start(span);
        if let Some(name) = name
            && name.starts_with("celesta")
            && at.is_some_and(|at| author.contains(&at))
        {
            return fail(
                at,
                format!("declares `{name}`; names starting with `celesta` belong to Celesta"),
            );
        }
    }
    Ok(())
}

/// The byte where `error`, from `naga::front::wgsl::parse_str`, points.
pub(crate) fn parse_error_at(error: &naga::front::wgsl::ParseError) -> Option<usize> {
    error
        .labels()
        .next()
        .and_then(|(span, _)| span.to_range())
        .map(|range| range.start)
}

/// The byte where `error`, from naga's validator, points.
pub(crate) fn validation_error_at(
    error: &naga::WithSpan<naga::valid::ValidationError>,
) -> Option<usize> {
    error
        .spans()
        .next()
        .and_then(|(span, _)| span.to_range())
        .map(|range| range.start)
}

#[cfg(test)]
mod tests {
    use super::locate;

    #[test]
    fn locate_places_errors_by_where_they_lie() {
        let code = "head\nab\ncd\ntail";
        let author = 5..10;
        let at = |at| locate(code, &author, at, "oops".to_owned(), "(before)", "(after)");
        assert_eq!(at(Some(5)), "1:1: oops");
        assert_eq!(at(Some(9)), "2:2: oops");
        assert_eq!(at(Some(10)), "2:3: oops");
        assert_eq!(at(Some(2)), "oops (before)");
        assert_eq!(at(Some(12)), "oops (after)");
        assert_eq!(at(None), "oops");
        assert_eq!(
            locate(code, &author, Some(2), "oops".to_owned(), "", "(after)"),
            "oops"
        );
    }
}
