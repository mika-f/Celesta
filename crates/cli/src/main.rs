use std::process::ExitCode;

use celesta_cli::args::Cli;
use clap::Parser;

fn main() -> ExitCode {
    celesta_cli::run(Cli::parse())
}
