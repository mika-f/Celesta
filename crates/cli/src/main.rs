use std::process::ExitCode;

use celesta_cli::args::Cli;
use celesta_exporter::cli as exporter;
use clap::{Command, CommandFactory, FromArgMatches};

fn main() -> ExitCode {
    let command = command();
    match command.try_get_matches() {
        Ok(matches) => match matches.subcommand() {
            Some(("export", args)) => {
                exporter::run_export(args, exporter::export_command().bin_name("celesta export"))
            }
            Some(("info", args)) => exporter::run_info(args),
            _ => match Cli::from_arg_matches(&matches) {
                Ok(cli) => celesta_cli::run(cli),
                Err(error) => parse_error(error),
            },
        },
        Err(error)
            if std::env::args_os()
                .nth(1)
                .is_some_and(|arg| arg == "export" || arg == "info") =>
        {
            exporter::parse_failed(error)
        }
        Err(error) => parse_error(error),
    }
}

fn command() -> Command {
    Cli::command()
        .subcommand(exporter::export_command())
        .subcommand(exporter::info_command())
}

fn parse_error(error: clap::Error) -> ExitCode {
    match error {
        // Help and the version go to stdout and are not errors.
        error if !error.use_stderr() => {
            let _ = error.print();
            ExitCode::SUCCESS
        }
        error => {
            let rendered = error.render().to_string();
            match rendered.strip_prefix("error: ") {
                Some(message) => celesta_cli::report::error(message.trim_end()),
                // The help shown for a missing subcommand is not an error
                // message of its own.
                None => eprint!("{rendered}"),
            }
            ExitCode::from(celesta_cli::USAGE)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_arguments_are_well_formed() {
        super::command().debug_assert();
    }
}
