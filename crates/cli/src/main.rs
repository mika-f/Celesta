use std::process::ExitCode;

use celesta_cli::args::Cli;
use clap::Parser;

fn main() -> ExitCode {
    match Cli::try_parse() {
        Ok(cli) => celesta_cli::run(cli),
        // Help and the version go to stdout and are not errors.
        Err(error) if !error.use_stderr() => {
            let _ = error.print();
            ExitCode::SUCCESS
        }
        Err(error) => {
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
