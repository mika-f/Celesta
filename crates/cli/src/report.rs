//! What `celesta` says on stderr: `celesta: warning: …` and
//! `celesta: error: …`. Results go to stdout.

use std::fmt::Display;

use celesta_registry_client::ResolveWarning;

pub fn warn(text: impl Display) {
    eprintln!("celesta: warning: {text}");
}

pub fn error(text: impl Display) {
    eprintln!("celesta: error: {text}");
}

/// Warns about each of `warnings`.
pub fn resolve_warnings(warnings: &[ResolveWarning]) {
    for warning in warnings {
        warn(warning);
    }
}
