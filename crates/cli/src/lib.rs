//! The `celesta` command's library: editing a project's settings and managing
//! its registry plugins.

pub mod advisories;
pub mod args;
pub mod commands;
pub mod config;
pub mod report;
pub mod session;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use args::{Cli, Command, PluginArgs, PluginCommand};
use celesta_plugin_host::project_root_of_dir;
use commands::{project, registry, store};
use session::{NO_KEY_MESSAGE, Session};

/// Exit status for a mistake in how `celesta` was invoked.
pub const USAGE: u8 = 2;

/// Runs `cli` and returns its exit status: 0 on success, 1 on failure, 2
/// for a usage error.
pub fn run(cli: Cli) -> ExitCode {
    let Command::Plugin(PluginArgs {
        project: folder,
        command,
    }) = cli.command;
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
    if needs_registry(&command) && !session.has_key {
        report::error(NO_KEY_MESSAGE);
        return ExitCode::FAILURE;
    }
    match command {
        PluginCommand::Add { spec, pin } => {
            in_project(folder, |root| project::add(&session, root, &spec, pin))
        }
        PluginCommand::Remove { id } => in_project(folder, |root| project::remove(root, &id)),
        PluginCommand::Install {
            locked,
            offline,
            global: None,
        } => in_project(folder, |root| {
            project::install(&session, root, locked, offline)
        }),
        PluginCommand::Update { id } => in_project(folder, |root| {
            project::update(&session, root, id.as_deref())
        }),
        PluginCommand::Install {
            global: Some(spec), ..
        } => store::install_global_plugin(&session, &spec),
        PluginCommand::Search { text } => registry::search(&session, &text),
        PluginCommand::Info { id } => {
            in_project(folder, |root| registry::info(&session, root, &id))
        }
        PluginCommand::List => in_project(folder, |root| store::list(&session, root)),
        PluginCommand::Audit => in_project(folder, |root| registry::audit(&session, root)),
        PluginCommand::Gc { dry_run } => {
            in_project(folder, |root| store::gc(&session, root, dry_run))
        }
    }
}

/// Runs `command` in the project `folder` (the current folder by default)
/// is in.
fn in_project(folder: Option<PathBuf>, command: impl FnOnce(&Path) -> ExitCode) -> ExitCode {
    let folder = match folder {
        Some(folder) => folder,
        None => match std::env::current_dir() {
            Ok(folder) => folder,
            Err(error) => {
                report::error(format_args!("could not find the current folder: {error}"));
                return ExitCode::FAILURE;
            }
        },
    };
    command(&project_root_of_dir(&folder))
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
