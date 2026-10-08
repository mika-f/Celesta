use crate::properties::PropertyIssue;
use std::error::Error;
use std::path::PathBuf;
use std::{fmt, io};

#[derive(Debug)]
pub enum ReactBridgeError {
    Executable {
        executable: PathBuf,
        source: io::Error,
    },
    MissingPipe,
    Io(io::Error),
    Protocol(serde_json::Error),
    UnexpectedExit,
    UnexpectedResponse,
    EntryFailed(String),
    /// The entry rejected project property values before running `prepare()`.
    InvalidProperties(Vec<PropertyIssue>),
    Render(String),
    Time(celesta_composition::TimeError),
}

impl fmt::Display for ReactBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Executable { executable, source } => write!(
                formatter,
                "could not run {}: {source}",
                executable.display()
            ),
            Self::MissingPipe => {
                formatter.write_str("the Node.js React runtime did not provide its stdio pipes")
            }
            Self::Io(error) => write!(
                formatter,
                "could not communicate with the Node.js React runtime: {error}"
            ),
            Self::Protocol(error) => write!(
                formatter,
                "the Node.js React runtime sent an unexpected response: {error}"
            ),
            Self::UnexpectedExit => {
                formatter.write_str("the Node.js React runtime exited unexpectedly")
            }
            Self::UnexpectedResponse => formatter
                .write_str("the Node.js React runtime answered with the wrong kind of response"),
            Self::EntryFailed(error) => {
                write!(formatter, "could not load the React composition: {error}")
            }
            Self::InvalidProperties(issues) => {
                formatter.write_str("invalid project properties:")?;
                for issue in issues {
                    write!(formatter, "\n  - {issue}")?;
                }
                Ok(())
            }
            Self::Render(error) => write!(formatter, "could not render the composition: {error}"),
            Self::Time(error) => write!(formatter, "invalid composition time: {error}"),
        }
    }
}

impl Error for ReactBridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Executable { source, .. } | Self::Io(source) => Some(source),
            Self::Protocol(error) => Some(error),
            Self::MissingPipe
            | Self::UnexpectedExit
            | Self::UnexpectedResponse
            | Self::EntryFailed(_)
            | Self::InvalidProperties(_)
            | Self::Render(_) => None,
            Self::Time(error) => Some(error),
        }
    }
}
