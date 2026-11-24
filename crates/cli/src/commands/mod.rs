//! What each `celesta` subcommand does.

use std::fmt::Display;
use std::process::ExitCode;

use crate::report;

pub mod init;
pub mod project;
pub mod registry;
pub mod store;

/// Why a command failed, said as `celesta: error: …`.
struct Failure(String);

impl<E: Display> From<E> for Failure {
    fn from(error: E) -> Self {
        Self(error.to_string())
    }
}

/// The exit status of a command that ended with `result`, saying why it
/// failed.
fn finish(result: Result<(), Failure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure(message)) => {
            report::error(message);
            ExitCode::FAILURE
        }
    }
}
