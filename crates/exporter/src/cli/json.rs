//! `--json`: one machine-readable result document on stdout in place of
//! progress output, for scripts and AI agents. Nothing is written to stderr.
//! `--json-progress` adds a line for each progress event before it.

use std::io::{self, Write};
use std::path::Path;

use crate::report::{self, Events, HintStyle};
use crate::{ExportError, ExportProgress};
use celesta_react_bridge::PropsFileChoice;

/// Runs `export`, then prints its report; with `progress`, each event is
/// printed as it happens (`--json-progress`); without `react` (`--react`),
/// a React entry read as a project hints at `--react`. Returns whether it
/// succeeded.
pub fn run(
    source: &Path,
    props_file: Option<PropsFileChoice>,
    progress: bool,
    react: bool,
    export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
) -> bool {
    let mut events = Events::default();
    let (mut report, ok) = report::build(source, props_file, HintStyle::Cli, |on_progress| {
        export(&mut |event| {
            if progress && let Some(line) = events.event(&event) {
                print(&line);
            }
            on_progress(event);
        })
    });
    if !react {
        report::add_react_hint(&mut report, source);
    }
    print(&report);
    ok
}

/// Prints the report for arguments rejected before anything ran.
pub fn usage_error(message: &str) {
    print(&report::usage(message));
}

/// Prints the report of an error outside an export, such as `--describe`'s.
pub fn failure(error: &ExportError) {
    print(&report::failure(error, HintStyle::Cli));
}

fn print(line: &impl serde::Serialize) {
    let mut stdout = io::stdout().lock();
    // A closed stdout leaves nobody to report to.
    let _ = serde_json::to_writer(&mut stdout, line);
    let _ = writeln!(stdout);
}
