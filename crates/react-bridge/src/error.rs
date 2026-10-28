use crate::properties::PropertyIssue;
use celesta_composition::DuckError;
use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ReactBridgeError {
    #[error("could not run {executable}: {source}")]
    Executable {
        executable: PathBuf,
        source: io::Error,
    },
    #[error("the Node.js React runtime did not provide its stdio pipes")]
    MissingPipe,
    #[error("could not communicate with the Node.js React runtime: {0}")]
    Io(#[source] io::Error),
    #[error("the Node.js React runtime sent an unexpected response: {0}")]
    Protocol(#[source] serde_json::Error),
    /// The runtime closed its stdout. `stderr` holds the last lines it wrote
    /// there, if any, which usually say why.
    #[error("the Node.js React runtime exited unexpectedly{}", last_output(.stderr))]
    UnexpectedExit { stderr: String },
    #[error("the Node.js React runtime answered with the wrong kind of response")]
    UnexpectedResponse,
    #[error("could not load the React composition: {0}")]
    EntryFailed(String),
    /// The entry rejected project property values before running `prepare()`.
    #[error("invalid project properties:{}", list_issues(.0))]
    InvalidProperties(Vec<PropertyIssue>),
    #[error("could not render the composition: {0}")]
    Render(String),
    #[error("invalid composition time: {0}")]
    Time(#[source] celesta_composition::TimeError),
    /// The runtime ignored a request for a frame past the end: it is older
    /// than this bridge.
    #[error(
        "the Node.js React runtime cannot evaluate frames past the end; it is older than this exporter"
    )]
    OpenEndUnsupported,
    /// A frame referred to a `<Static>` subtree this bridge was never sent
    /// or has dropped.
    #[error("the React runtime referred to static subtree {0}, which this bridge does not have")]
    MissingStatic(u32),
    /// An audio clip's `duckGroup` or `duck` breaks a rule.
    #[error("audio `{src}`: {} {error}", duck_field(.error))]
    InvalidDuck { src: String, error: DuckError },
}

/// `stderr`'s lines, indented under a heading, or nothing when it is empty.
fn last_output(stderr: &str) -> String {
    if stderr.is_empty() {
        return String::new();
    }
    let mut text = String::from("\nIts last output:");
    for line in stderr.lines() {
        text.push('\n');
        if !line.is_empty() {
            text.push_str("  ");
            text.push_str(line);
        }
    }
    text
}

fn list_issues(issues: &[PropertyIssue]) -> String {
    issues
        .iter()
        .map(|issue| format!("\n  - {issue}"))
        .collect()
}

/// The authored field `error` is about: `duckGroup`, or `duck.<field>`.
fn duck_field(error: &DuckError) -> String {
    match error {
        DuckError::Group => error.field().to_owned(),
        _ => format!("duck.{}", error.field()),
    }
}
