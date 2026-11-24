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
            Some(("doctor", args)) => exporter::run_doctor(args),
            _ => match Cli::from_arg_matches(&matches) {
                Ok(cli) => celesta_cli::run(cli),
                Err(error) => parse_error(error),
            },
        },
        Err(error) => match std::env::args_os()
            .nth(1)
            .as_deref()
            .and_then(|arg| arg.to_str())
        {
            Some("export" | "info") => exporter::parse_failed(error),
            Some("doctor") => exporter::doctor_parse_failed(error),
            _ => parse_error(error),
        },
    }
}

fn command() -> Command {
    Cli::command()
        .subcommand(exporter::export_command())
        .subcommand(exporter::info_command())
        .subcommand(exporter::doctor_command())
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
            if wants_json_usage_error() {
                return celesta_cli::json::usage_error(rendered.trim_end());
            }
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

fn wants_json_usage_error() -> bool {
    let mut args = std::env::args_os().skip(1);
    if !args.next().is_some_and(|arg| arg == "plugin") {
        return false;
    }
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_encoded_bytes() {
            b"--" => break,
            b"--project" => {
                args.next();
            }
            b"--json" => json = true,
            b"list" | b"install" | b"audit" => {
                return json
                    || args
                        .take_while(|arg| arg != "--")
                        .any(|arg| arg == "--json");
            }
            option if option.starts_with(b"-") => {}
            _ => break,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_arguments_are_well_formed() {
        super::command().debug_assert();
    }
}
