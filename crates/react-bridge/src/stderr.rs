//! Node's stderr: passed through to this process's stderr as it arrives,
//! with its last lines kept to explain an unexpected exit
//! ([`celesta_plugin_host::stderr`]).

use crate::error::ReactBridgeError;
use celesta_plugin_host::stderr::{StderrTail, pump};
use std::io;
use std::process::{Child, ChildStderr};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// How long an exit error waits for the runtime to exit and its stderr to
/// close. A grandchild can hold the pipe open past the runtime's exit.
const EXIT_WAIT: Duration = Duration::from_millis(500);

/// The runtime's stderr, read on its own thread.
///
/// The thread holds only the pipe and the tail. It ends when the pipe
/// closes, which killing the runtime does, and nothing joins it, so it
/// keeps neither the bridge nor this process alive.
pub(crate) struct StderrCapture {
    tail: Arc<Mutex<StderrTail>>,
    /// Disconnects when the thread has read the pipe to its end.
    closed: Receiver<()>,
}

impl StderrCapture {
    pub(crate) fn start(stderr: ChildStderr) -> io::Result<Self> {
        let tail = Arc::new(Mutex::new(StderrTail::default()));
        let (closed_sender, closed) = mpsc::channel::<()>();
        let thread_tail = Arc::clone(&tail);
        thread::Builder::new()
            .name("celesta-node-stderr".to_owned())
            .spawn(move || {
                let _closed = closed_sender;
                pump(stderr, io::stderr(), &thread_tail);
            })?;
        Ok(Self { tail, closed })
    }

    /// Fills in why the runtime went away. An [`ReactBridgeError::UnexpectedExit`],
    /// or a write the closed pipe refused, becomes an `UnexpectedExit`
    /// carrying the runtime's last stderr lines; other errors pass through.
    ///
    /// It first waits for `child` to exit and then for its stderr to reach
    /// its end, both within [`EXIT_WAIT`], so the lines are complete.
    pub(crate) fn exit_error(
        &self,
        child: &mut Child,
        error: ReactBridgeError,
    ) -> ReactBridgeError {
        match &error {
            ReactBridgeError::UnexpectedExit { .. } => {}
            ReactBridgeError::Io(io) if io.kind() == io::ErrorKind::BrokenPipe => {}
            _ => return error,
        }
        let deadline = Instant::now() + EXIT_WAIT;
        while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let _ = self
            .closed
            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
        let stderr = self
            .tail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .text();
        ReactBridgeError::UnexpectedExit { stderr }
    }
}
