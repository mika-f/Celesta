//! The `celesta` command's arguments.

use std::path::PathBuf;

use celesta_registry_format::is_registry_id;
use clap::{Args, Parser, Subcommand};
use semver::VersionReq;

#[derive(Debug, Parser)]
#[command(
    name = "celesta",
    version,
    about = "Manages a Celesta project's plugins."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add, install, and inspect registry plugins.
    Plugin(PluginArgs),
}

#[derive(Debug, Args)]
pub struct PluginArgs {
    /// The project folder, or a folder inside it; the current folder by
    /// default.
    #[arg(long, global = true)]
    pub project: Option<PathBuf>,
    #[command(subcommand)]
    pub command: PluginCommand,
}

#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// Find registry plugins whose id or description contains the text.
    Search { text: String },
    /// Show a registry plugin's versions and where this project gets it.
    Info { id: String },
    /// Add a plugin to this project and lock it.
    Add {
        /// The plugin id, with an optional version requirement: `id[@req]`.
        spec: String,
        /// Lock a media plugin to this project instead of every project.
        #[arg(long)]
        pin: bool,
    },
    /// Remove a plugin from this project.
    Remove { id: String },
    /// Install the plugins this project's celesta.lock names.
    Install {
        /// Install exactly what celesta.lock names, never resolving again.
        #[arg(long)]
        locked: bool,
        /// Use only what is already downloaded.
        #[arg(long, conflicts_with = "global")]
        offline: bool,
        /// Install a media plugin for every project instead.
        #[arg(long, value_name = "ID[@REQ]", conflicts_with = "locked")]
        global: Option<String>,
    },
    /// Move this project's plugins, or one of them, to the newest versions
    /// their requirements allow.
    Update { id: Option<String> },
    /// Show the registry plugins this project and every project use.
    List,
    /// Check the plugins in use against the security advisories.
    Audit,
    /// Remove downloaded versions no project uses.
    Gc {
        /// Say what would be removed without removing it.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Splits `id[@req]` into the plugin id and its version requirement, with
/// Cargo's meaning (`0.2` is `^0.2`).
pub fn parse_spec(spec: &str) -> Result<(String, Option<VersionReq>), String> {
    let (id, requirement) = match spec.split_once('@') {
        Some((id, requirement)) => (id, Some(requirement)),
        None => (spec, None),
    };
    if !is_registry_id(id) {
        return Err(format!(
            "{spec:?} does not start with a registry plugin id such as \"acme.charts\""
        ));
    }
    let requirement = requirement
        .map(|text| {
            VersionReq::parse(text)
                .map_err(|error| format!("{text:?} is not a version requirement: {error}"))
        })
        .transpose()?;
    Ok((id.to_owned(), requirement))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_spec_splits_id_and_requirement() {
        assert_eq!(
            parse_spec("acme.charts@0.2"),
            Ok((
                "acme.charts".to_owned(),
                Some(VersionReq::parse("^0.2").unwrap())
            ))
        );
        assert_eq!(
            parse_spec("acme.charts"),
            Ok(("acme.charts".to_owned(), None))
        );
        assert!(parse_spec("@1").is_err());
        assert!(parse_spec("acme.charts@").is_err());
        assert!(parse_spec("acme.charts@nope").is_err());
    }

    #[test]
    fn arguments_are_well_formed() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
