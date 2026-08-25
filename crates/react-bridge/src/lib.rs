//! Spawns the `@mikan/react` Node.js runtime and evaluates a React
//! composition into the shared `mikan_composition::Scene` model.
//!
//! A single Node process is kept alive for the lifetime of a [`ReactBridge`]
//! and answers one JSON request per requested frame over its stdin/stdout
//! pipe, following the same "one long-lived process instead of one process
//! per frame" shape as `mikan-media`'s sequential video decoding session.

use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use mikan_composition::{Rational, Scene, Time};
use serde::{Deserialize, Serialize};

/// Static composition facts read once from the entry's `<Composition>` root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactCompositionMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub duration_in_frames: u64,
}

/// A live connection to the `@mikan/react` CLI evaluating one entry module.
pub struct ReactBridge {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    metadata: ReactCompositionMetadata,
}

impl Drop for ReactBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ReactBridge {
    /// Spawns `node <cli_script> <entry>` and reads its startup configuration.
    pub fn spawn(
        node: impl AsRef<Path>,
        cli_script: impl AsRef<Path>,
        entry: impl AsRef<Path>,
    ) -> Result<Self, ReactBridgeError> {
        let node = node.as_ref();
        let mut child = Command::new(node)
            .arg(cli_script.as_ref())
            .arg(entry.as_ref())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|source| ReactBridgeError::Executable {
                executable: node.to_owned(),
                source,
            })?;
        let stdin = child.stdin.take().ok_or(ReactBridgeError::MissingPipe)?;
        let stdout = child.stdout.take().ok_or(ReactBridgeError::MissingPipe)?;
        let mut stdout = BufReader::new(stdout);

        let mut line = String::new();
        let read = stdout.read_line(&mut line).map_err(ReactBridgeError::Io)?;
        if read == 0 {
            let _ = child.wait();
            return Err(ReactBridgeError::UnexpectedExit);
        }
        let ready: ReadyMessage =
            serde_json::from_str(line.trim()).map_err(ReactBridgeError::Protocol)?;
        let metadata = match ready {
            ReadyMessage::Ready { config } => ReactCompositionMetadata {
                width: config.width,
                height: config.height,
                frame_rate: config.frame_rate,
                duration_in_frames: config.duration_in_frames,
            },
            ReadyMessage::Error { error } => return Err(ReactBridgeError::EntryFailed(error)),
        };

        Ok(Self {
            child,
            stdin,
            stdout,
            metadata,
        })
    }

    pub const fn metadata(&self) -> &ReactCompositionMetadata {
        &self.metadata
    }

    /// Requests the evaluated `Scene` at an exact composition time.
    pub fn scene_at(&mut self, time: Time) -> Result<Scene, ReactBridgeError> {
        let payload =
            serde_json::to_string(&Request { time }).map_err(ReactBridgeError::Protocol)?;
        writeln!(self.stdin, "{payload}").map_err(ReactBridgeError::Io)?;
        self.stdin.flush().map_err(ReactBridgeError::Io)?;

        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .map_err(ReactBridgeError::Io)?;
        if read == 0 {
            return Err(ReactBridgeError::UnexpectedExit);
        }
        let response: Response =
            serde_json::from_str(line.trim()).map_err(ReactBridgeError::Protocol)?;
        match response {
            Response::Ok { scene } => Ok(scene),
            Response::Err { error } => Err(ReactBridgeError::Render(error)),
        }
    }
}

#[derive(Serialize)]
struct Request {
    time: Time,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Response {
    Ok { scene: Scene },
    Err { error: String },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ReadyMessage {
    Ready { config: ReactCompositionConfig },
    Error { error: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReactCompositionConfig {
    width: u32,
    height: u32,
    frame_rate: Rational,
    duration_in_frames: u64,
}

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
    EntryFailed(String),
    Render(String),
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
            Self::EntryFailed(error) => {
                write!(formatter, "could not load the React composition: {error}")
            }
            Self::Render(error) => write!(formatter, "could not render the composition: {error}"),
        }
    }
}

impl Error for ReactBridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Executable { source, .. } | Self::Io(source) => Some(source),
            Self::Protocol(error) => Some(error),
            Self::MissingPipe | Self::UnexpectedExit | Self::EntryFailed(_) | Self::Render(_) => {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_missing_node_executable() {
        let result = ReactBridge::spawn(
            "/definitely-not-installed/mikan-node",
            "cli.js",
            "entry.tsx",
        );
        assert!(matches!(result, Err(ReactBridgeError::Executable { .. })));
    }
}
