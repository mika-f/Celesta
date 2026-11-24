use std::process::ExitCode;

fn main() -> ExitCode {
    celesta_exporter::cli::standalone()
}
