//! The `celesta` command's library: editing a project's settings and managing
//! its registry plugins.

pub mod advisories;
pub mod args;
pub mod config;
pub mod report;
pub mod session;

use std::process::ExitCode;

use args::{Cli, Command, PluginCommand};
use session::{NO_KEY_MESSAGE, Session};

/// Exit status for a mistake in how `celesta` was invoked.
pub const USAGE: u8 = 2;

/// Runs `cli` and returns its exit status: 0 on success, 1 on failure, 2
/// for a usage error.
pub fn run(cli: Cli) -> ExitCode {
    let Command::Plugin(args) = cli.command;
    let session = match Session::from_env() {
        Ok(session) => session,
        Err(error) => {
            report::error(&error);
            return if error.is_usage() {
                ExitCode::from(USAGE)
            } else {
                ExitCode::FAILURE
            };
        }
    };
    if needs_registry(&args.command) && !session.has_key {
        report::error(NO_KEY_MESSAGE);
        return ExitCode::FAILURE;
    }
    match args.command {
        PluginCommand::Search { .. }
        | PluginCommand::Info { .. }
        | PluginCommand::Add { .. }
        | PluginCommand::Remove { .. }
        | PluginCommand::Install { .. }
        | PluginCommand::Update { .. }
        | PluginCommand::List
        | PluginCommand::Audit
        | PluginCommand::Gc { .. } => {
            report::error("not implemented");
            ExitCode::FAILURE
        }
    }
}

/// Whether `command` cannot run without reaching the registry.
fn needs_registry(command: &PluginCommand) -> bool {
    match command {
        PluginCommand::Search { .. }
        | PluginCommand::Info { .. }
        | PluginCommand::Add { .. }
        | PluginCommand::Update { .. } => true,
        PluginCommand::Install { global, .. } => global.is_some(),
        PluginCommand::Remove { .. }
        | PluginCommand::List
        | PluginCommand::Audit
        | PluginCommand::Gc { .. } => false,
    }
}
