//! The `celesta` command's library.

pub mod advisories;
pub mod args;
pub mod commands;
pub mod json;
pub mod report;
pub mod session;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use args::{Cli, Command, PluginArgs, PluginCommand};
use celesta_plugin_host::project_root_of_dir;
use commands::{init, project, registry, store};
use json::{Error, Output, Scope};
use session::{Session, SessionError};

/// Exit status for a mistake in how `celesta` was invoked.
pub const USAGE: u8 = 2;

/// Runs `cli` and returns its exit status: 0 on success, 1 on failure, 2
/// for a usage error.
pub fn run(cli: Cli) -> ExitCode {
    match cli.command {
        Command::Init {
            directory,
            template,
        } => init::initialize(&directory, template.as_deref()),
        Command::Templates => init::list(),
        Command::Plugin(args) => run_plugin(args),
    }
}

fn run_plugin(args: PluginArgs) -> ExitCode {
    let PluginArgs {
        project: folder,
        command,
    } = args;
    let (json, name, scope) = match &command {
        PluginCommand::Install { json, global, .. } => (
            *json,
            "install",
            Some(if global.is_some() {
                Scope::Global
            } else {
                Scope::Project
            }),
        ),
        PluginCommand::List { json } => (*json, "list", None),
        PluginCommand::Audit { json } => (*json, "audit", None),
        _ => (false, "", None),
    };
    let output = Output {
        json,
        command: name,
        scope,
    };
    let session = match Session::from_env() {
        Ok(session) => session,
        Err(error) => {
            return output.fail(match &error {
                SessionError::Selection(_) => Error::usage("registry_selection", &error),
                SessionError::NoUserFolder => Error::new("user_folder", &error),
                SessionError::CacheFolder { .. } => Error::new("cache_folder", &error),
            });
        }
    };
    match command {
        PluginCommand::Add { spec, pin } => in_project(folder, output, |root| {
            project::add(&session, root, &spec, pin)
        }),
        PluginCommand::Remove { id } => {
            in_project(folder, output, |root| project::remove(root, &id))
        }
        PluginCommand::Install {
            locked,
            offline,
            global: None,
            ..
        } => in_project(folder, output, |root| {
            project::install(&session, root, locked, offline, output)
        }),
        PluginCommand::Update { id } => in_project(folder, output, |root| {
            project::update(&session, root, id.as_deref())
        }),
        PluginCommand::Install {
            global: Some(spec), ..
        } => store::install_global_plugin(&session, &spec, output),
        PluginCommand::Search { text } => registry::search(&session, &text),
        PluginCommand::Info { id } => {
            in_project(folder, output, |root| registry::info(&session, root, &id))
        }
        PluginCommand::List { .. } => {
            in_project(folder, output, |root| store::list(&session, root, output))
        }
        PluginCommand::Audit { .. } => in_project(folder, output, |root| {
            registry::audit(&session, root, output)
        }),
        PluginCommand::Gc { dry_run } => {
            in_project(folder, output, |root| store::gc(&session, root, dry_run))
        }
    }
}

/// Runs `command` in the project `folder` (the current folder by default)
/// is in.
fn in_project(
    folder: Option<PathBuf>,
    output: Output,
    command: impl FnOnce(&Path) -> ExitCode,
) -> ExitCode {
    let folder = match folder {
        Some(folder) => folder,
        None => match std::env::current_dir() {
            Ok(folder) => folder,
            Err(error) => {
                return output.fail(Error::new(
                    "current_directory",
                    format_args!("could not find the current folder: {error}"),
                ));
            }
        },
    };
    command(&project_root_of_dir(&folder))
}
