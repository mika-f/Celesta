//! A plugin's process, as Celesta runs every plugin: content plugins for the
//! renderer ([`crate::PluginHost`]) and media plugins for media
//! (`celesta_media::MediaProvider`).
//!
//! - On Windows it starts without a console window of its own, which a
//!   console program started by the GUI editor would otherwise get.
//! - Requests are written and answers read on threads of their own, so every
//!   request has a deadline, even one whose payload a hung plugin would never
//!   read. A plugin that misses it is killed.
//! - Another thread can kill it through a [`PluginKillHandle`] (or a
//!   [`crate::PluginKillSwitch`] it is registered with): every request in
//!   flight fails at once.
//! - Its stderr is passed through to Celesta's and its last lines kept, so an
//!   error for a process that went away says what it last printed.
//! - An answer must be of the kind its request expects: a plugin that falls
//!   out of step (answering one request twice) is treated as gone, so the
//!   caller starts it again rather than reading each answer as the next
//!   request's.

use crate::console::without_console_window;
use crate::kill::{PluginKillHandle, SharedChild, lock, register_everywhere};
use crate::stderr::{StderrTail, pump};
use celesta_plugin::protocol::{
    Message, PROTOCOL_VERSION, error_message, read_message, write_message,
};
use celesta_plugin::shared::SharedMemory;
use serde_json::{Map, Value, json};
use std::collections::VecDeque;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// How long `hello` may take: the process starting and loading its
/// libraries.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a stopping plugin has to exit once its stdin closes, and an
/// error waits for a plugin that went away to finish its stderr.
const EXIT_WAIT: Duration = Duration::from_millis(500);

/// How a request went wrong: the process cannot be talked to (it exited or
/// fell out of step); it was stopped on purpose (it missed its deadline, or
/// was killed through a [`PluginKillHandle`]), so sending the same request to
/// a new process would not help; or it refused with an `error` answer, kept
/// whole for callers whose errors carry more than a message.
#[derive(Debug, Clone, PartialEq)]
pub enum RequestError {
    Unavailable(String),
    Stopped(String),
    Refused(Message),
}

impl RequestError {
    /// What went wrong, as text: an `error` answer's `message`.
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable(message) | Self::Stopped(message) => message.clone(),
            Self::Refused(answer) => answer.header["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_owned(),
        }
    }
}

/// Where a request's deadline starts counting: when it was sent, or (for a
/// request queued behind others) when the request ahead of it was answered,
/// whichever is later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadlineFrom {
    Sent,
    PreviousAnswer,
}

/// A request sent and not yet answered.
struct Pending {
    kind: String,
    expected: String,
    timeout: Duration,
    from: DeadlineFrom,
    sent: Instant,
}

/// A running plugin process. Requests may be sent several at a time
/// ([`Self::send`]); the plugin answers them in order ([`Self::receive`]).
/// Dropping it closes the plugin's stdin, which asks it to exit, and kills it
/// if it has not within half a second.
pub struct PluginChild {
    child: Arc<Mutex<SharedChild>>,
    /// What [`Self::request`] writes; dropping it ends the writer thread,
    /// which closes the plugin's stdin.
    requests: Option<Sender<Message>>,
    /// The reader thread's answers; an `Err` once the stream ends.
    answers: Receiver<Result<Message, String>>,
    stderr: Arc<Mutex<StderrTail>>,
    /// Disconnects once the plugin's stderr has been read to its end.
    stderr_closed: Receiver<()>,
    name: String,
    /// The requests sent and not yet answered, oldest first.
    pending: VecDeque<Pending>,
    /// When the last answer arrived (the process's start before any).
    last_answered: Instant,
}

impl PluginChild {
    /// Starts `executable` in `directory`, the plugin's own folder.
    pub fn start(executable: &Path, directory: &Path) -> Result<Self, String> {
        Self::start_sharing(executable, directory, None).map(|(child, _)| child)
    }

    /// [`Self::start`], passing `shared` on to the process, and the handle
    /// the process maps it by (for its `hello`): `None` without `shared`,
    /// and the error when it could not be passed on (the process then runs
    /// without it).
    pub fn start_sharing(
        executable: &Path,
        directory: &Path,
        shared: Option<&SharedMemory>,
    ) -> Result<(Self, Option<io::Result<u64>>), String> {
        let name = executable.display().to_string();
        let mut command = Command::new(executable);
        command
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        without_console_window(&mut command);
        if let Some(shared) = shared {
            shared.prepare(&mut command);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start {name}: {error}"))?;
        let shared_handle = shared.map(|shared| shared.handle_for(&child));
        let started = (|| -> io::Result<_> {
            let mut input = BufWriter::new(child.stdin.take().expect("piped"));
            let mut output = BufReader::new(child.stdout.take().expect("piped"));
            let stderr_pipe = child.stderr.take().expect("piped");

            let (requests, outgoing) = mpsc::channel::<Message>();
            thread::Builder::new()
                .name("celesta-plugin-requests".into())
                .spawn(move || {
                    for message in outgoing {
                        if write_message(&mut input, &message).is_err() {
                            break;
                        }
                    }
                })?;
            let (answer, answers) = mpsc::channel();
            thread::Builder::new()
                .name("celesta-plugin-answers".into())
                .spawn(move || {
                    loop {
                        let read = match read_message(&mut output) {
                            Ok(Some(message)) => Ok(message),
                            Ok(None) => Err("its process exited".to_owned()),
                            Err(error) => Err(error.to_string()),
                        };
                        let ended = read.is_err();
                        if answer.send(read).is_err() || ended {
                            break;
                        }
                    }
                })?;
            let stderr = Arc::new(Mutex::new(StderrTail::default()));
            let (closed, stderr_closed) = mpsc::channel::<()>();
            let tail = Arc::clone(&stderr);
            thread::Builder::new()
                .name("celesta-plugin-stderr".into())
                .spawn(move || {
                    let _closed = closed;
                    pump(stderr_pipe, io::stderr(), &tail);
                })?;
            Ok((requests, answers, stderr, stderr_closed))
        })();
        let (requests, answers, stderr, stderr_closed) = match started {
            Ok(started) => started,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("could not start {name}: {error}"));
            }
        };
        let child = Arc::new(Mutex::new(SharedChild {
            child,
            killed: false,
        }));
        register_everywhere(PluginKillHandle(Arc::downgrade(&child)));
        Ok((
            Self {
                child,
                requests: Some(requests),
                answers,
                stderr,
                stderr_closed,
                name,
                pending: VecDeque::new(),
                last_answered: Instant::now(),
            },
            shared_handle,
        ))
    }

    /// Kills the process from any thread (see [`crate::PluginKillSwitch`]).
    pub fn kill_handle(&self) -> PluginKillHandle {
        PluginKillHandle(Arc::downgrade(&self.child))
    }

    /// Says `hello` and checks the answer speaks this protocol; the caller
    /// checks the plugin and roles it names.
    pub fn hello(&mut self) -> Result<Message, RequestError> {
        self.hello_with(Map::new())
    }

    /// [`Self::hello`], the request also carrying the fields of `offer`
    /// (what a role offers the plugin, such as a codec's shared memory),
    /// which cannot replace `type` or `protocol`. A content plugin speaks
    /// protocol 1 (see [`Self::hello_as`]).
    pub fn hello_with(&mut self, offer: Map<String, Value>) -> Result<Message, RequestError> {
        self.hello_as(PROTOCOL_VERSION, offer)
    }

    /// [`Self::hello_with`] in `protocol`, which the answer must speak too:
    /// a media plugin speaks [`celesta_plugin::MEDIA_PROTOCOL_VERSION`].
    pub fn hello_as(
        &mut self,
        protocol: u32,
        mut offer: Map<String, Value>,
    ) -> Result<Message, RequestError> {
        offer.insert("type".to_owned(), json!("hello"));
        offer.insert("protocol".to_owned(), json!(protocol));
        let hello = self.request(Message::new(Value::Object(offer)), "hello", HELLO_TIMEOUT)?;
        if hello.header["protocol"].as_u64() != Some(u64::from(protocol)) {
            return Err(RequestError::Refused(error_message(format!(
                "{} speaks protocol {}, not {protocol}",
                self.name, hello.header["protocol"]
            ))));
        }
        Ok(hello)
    }

    /// Sends `request` and waits up to `timeout` for its answer, which must
    /// be of the kind `expected` or an `error`. A plugin that misses the
    /// deadline, answers with another kind, or whose stream fails is killed;
    /// an `error` answer leaves it running. One killed through its
    /// [`PluginKillHandle`] while this waits makes it return at once.
    ///
    /// Only with nothing in flight ([`Self::in_flight`] is 0): otherwise
    /// the answer taken would be the oldest request's, not this one's.
    pub fn request(
        &mut self,
        request: Message,
        expected: &str,
        timeout: Duration,
    ) -> Result<Message, RequestError> {
        debug_assert!(
            self.pending.is_empty(),
            "`{}` was asked with {} requests in flight",
            request.kind(),
            self.pending.len()
        );
        self.send(request, expected, timeout, DeadlineFrom::Sent)?;
        self.receive()
    }

    /// Sends `request` without waiting; its answer (of the kind `expected`,
    /// or an `error`) is taken by [`Self::receive`], in the order requests
    /// were sent. `timeout` counts from `from`.
    pub fn send(
        &mut self,
        request: Message,
        expected: &str,
        timeout: Duration,
        from: DeadlineFrom,
    ) -> Result<(), RequestError> {
        let kind = request.kind().to_owned();
        let sent = self
            .requests
            .as_ref()
            .is_some_and(|requests| requests.send(request).is_ok());
        if !sent {
            return Err(self.gone(&kind, "it stopped reading requests"));
        }
        self.pending.push_back(Pending {
            kind,
            expected: expected.to_owned(),
            timeout,
            from,
            sent: Instant::now(),
        });
        Ok(())
    }

    /// How many requests are sent and not yet answered.
    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// Waits for the answer to the oldest request in flight, with the rules
    /// of [`Self::request`]. Once the process is stopped, each request still
    /// in flight fails without waiting.
    pub fn receive(&mut self) -> Result<Message, RequestError> {
        let Some(Pending {
            kind,
            expected,
            timeout,
            from,
            sent,
        }) = self.pending.pop_front()
        else {
            return Err(RequestError::Unavailable(format!(
                "{} is unavailable: no request is in flight",
                self.name
            )));
        };
        if self.requests.is_none() {
            let reason = format!("it was stopped before it answered `{kind}`");
            let message = self.explain(&reason);
            return Err(if lock(&self.child).killed {
                RequestError::Stopped(message)
            } else {
                RequestError::Unavailable(message)
            });
        }
        let start = match from {
            DeadlineFrom::Sent => sent,
            DeadlineFrom::PreviousAnswer => sent.max(self.last_answered),
        };
        let wait = (start + timeout).saturating_duration_since(Instant::now());
        let answer = match self.answers.recv_timeout(wait) {
            Ok(Ok(answer)) => answer,
            Ok(Err(reason)) => return Err(self.gone(&kind, &reason)),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(self.gone(&kind, "its process exited"));
            }
            Err(RecvTimeoutError::Timeout) => {
                // Killed at once: it had its whole deadline.
                self.stop();
                return Err(RequestError::Stopped(self.explain(&format!(
                    "it did not answer `{kind}` within {} s, so it was stopped",
                    timeout.as_secs_f64()
                ))));
            }
        };
        self.last_answered = Instant::now();
        match answer.kind() {
            "error" => Err(RequestError::Refused(answer)),
            found if found == expected => Ok(answer),
            found => {
                self.stop();
                Err(RequestError::Unavailable(self.explain(&format!(
                    "it answered `{kind}` with `{found}`, not `{expected}`, so it was stopped"
                ))))
            }
        }
    }

    /// The process's stream failed for `reason` while it worked on `kind`:
    /// it is given a moment to exit on its own (so its stderr is complete),
    /// then stopped, since a process whose stream failed cannot be talked to
    /// again. A process killed through its [`PluginKillHandle`] is stopped.
    fn gone(&mut self, kind: &str, reason: &str) -> RequestError {
        let deadline = Instant::now() + EXIT_WAIT;
        while matches!(lock(&self.child).child.try_wait(), Ok(None)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        self.stop();
        if lock(&self.child).killed {
            RequestError::Stopped(
                self.explain(&format!("it was stopped while it worked on `{kind}`")),
            )
        } else {
            RequestError::Unavailable(self.explain(reason))
        }
    }

    /// Why the stopped process cannot be talked to: `reason`, and the last
    /// lines it printed on stderr, once they have all been read.
    fn explain(&mut self, reason: &str) -> String {
        let _ = self.stderr_closed.recv_timeout(EXIT_WAIT);
        let tail = self
            .stderr
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .text();
        if tail.is_empty() {
            format!("{} is unavailable: {reason}", self.name)
        } else {
            format!(
                "{} is unavailable: {reason}; it last printed:\n{tail}",
                self.name
            )
        }
    }

    /// Kills the process, so the threads that wait on its pipes end.
    fn stop(&mut self) {
        self.requests = None;
        let mut child = lock(&self.child);
        let _ = child.child.kill();
        let _ = child.child.wait();
    }
}

impl Drop for PluginChild {
    fn drop(&mut self) {
        // The writer thread ends and closes the plugin's stdin, which asks it
        // to exit; it is killed if it has not in time.
        self.requests = None;
        let deadline = Instant::now() + EXIT_WAIT;
        while matches!(lock(&self.child).child.try_wait(), Ok(None)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        self.stop();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A plugin made of a shell script.
    fn script(directory: &Path, body: &str) -> std::path::PathBuf {
        // A sibling test forking while the file is open for writing here
        // inherits the descriptor, and exec of the script then fails with
        // ETXTBSY until that child execs or exits. Writing under another
        // name and renaming does not prevent it (the rename keeps the
        // inode the forked child still holds open for writing); the tests
        // start the script through `start_retrying`, which waits it out.
        let path = directory.join("plugin.sh");
        let staging = directory.join("plugin.sh.new");
        std::fs::write(&staging, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::rename(&staging, &path).unwrap();
        path
    }

    /// [`PluginChild::start`], tried again while the script is still open
    /// for writing in a sibling test's forked child (ETXTBSY).
    fn start_retrying(executable: &Path, directory: &Path) -> Result<PluginChild, String> {
        let mut attempts = 0;
        loop {
            match PluginChild::start(executable, directory) {
                Err(error) if attempts < 20 && error.contains("busy") => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(50));
                }
                other => return other,
            }
        }
    }

    /// Waits until the script has run as far as touching `marker`, as
    /// starting a process can take a while on a loaded machine.
    fn wait_for(marker: &Path) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "the script did not start");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn exited(child: &PluginChild) -> bool {
        lock(&child.child).child.try_wait().unwrap().is_some()
    }

    fn hello() -> Message {
        Message::new(json!({ "type": "hello", "protocol": PROTOCOL_VERSION }))
    }

    #[test]
    fn hello_as_checks_the_answered_protocol() {
        let directory = tempfile::tempdir().unwrap();
        // A plugin that answers `hello` with protocol 1 whatever it is asked.
        let answer = directory.path().join("answer.bin");
        let mut bytes = Vec::new();
        write_message(&mut bytes, &hello()).unwrap();
        std::fs::write(&answer, bytes).unwrap();
        let program = script(
            directory.path(),
            &format!("cat '{}'\nexec sleep 30", answer.display()),
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let error = child.hello_as(2, Map::new()).unwrap_err();
        let RequestError::Refused(refusal) = error else {
            panic!("{error:?}");
        };
        let message = refusal.header["message"].as_str().unwrap();
        assert!(message.contains("speaks protocol 1, not 2"), "{message}");
    }

    #[test]
    fn a_plugin_that_does_not_answer_in_time_is_stopped() {
        let directory = tempfile::tempdir().unwrap();
        let started = directory.path().join("started");
        let program = script(
            directory.path(),
            &format!(
                "echo 'loading the model' >&2\ntouch '{}'\nexec sleep 30",
                started.display()
            ),
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        wait_for(&started);
        let started = Instant::now();
        let error = child
            .request(hello(), "hello", Duration::from_millis(200))
            .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(5));
        let RequestError::Stopped(message) = error else {
            panic!("{error:?}");
        };
        assert!(message.contains("did not answer `hello`"), "{message}");
        assert!(message.contains("loading the model"), "{message}");
        assert!(exited(&child), "it was killed");
        // Later requests fail at once rather than wait again.
        assert!(matches!(
            child.request(hello(), "hello", Duration::from_secs(30)),
            Err(RequestError::Unavailable(_))
        ));
    }

    #[test]
    fn killing_it_from_another_thread_ends_the_request_it_waits_on() {
        let directory = tempfile::tempdir().unwrap();
        let started = directory.path().join("started");
        let program = script(
            directory.path(),
            &format!(
                "echo 'drawing the frame' >&2\ntouch '{}'\nexec sleep 30",
                started.display()
            ),
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let switch = crate::PluginKillSwitch::new();
        switch.register(child.kill_handle());
        wait_for(&started);
        let killer = {
            let switch = switch.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(200));
                switch.kill();
            })
        };
        let begun = Instant::now();
        let error = child
            .request(hello(), "hello", Duration::from_secs(30))
            .unwrap_err();
        killer.join().unwrap();
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "it returned at once"
        );
        let RequestError::Stopped(message) = error else {
            panic!("{error:?}");
        };
        assert!(
            message.contains("stopped while it worked on `hello`"),
            "{message}"
        );
        assert!(message.contains("drawing the frame"), "{message}");
        assert!(exited(&child), "it was killed");
    }

    #[test]
    fn a_switch_killed_already_kills_what_is_registered_later() {
        let directory = tempfile::tempdir().unwrap();
        let program = script(directory.path(), "exec sleep 30");
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let switch = crate::PluginKillSwitch::new();
        switch.kill();
        assert!(switch.is_killed());
        switch.register(child.kill_handle());
        assert!(matches!(
            child.request(hello(), "hello", Duration::from_secs(30)),
            Err(RequestError::Stopped(_))
        ));
        assert!(exited(&child));
    }

    #[test]
    fn a_plugin_that_exits_says_what_it_printed() {
        let directory = tempfile::tempdir().unwrap();
        let program = script(directory.path(), "echo 'boom: no libavcodec' >&2\nexit 3");
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let error = child
            .request(hello(), "hello", Duration::from_secs(5))
            .unwrap_err();
        let RequestError::Unavailable(message) = error else {
            panic!("{error:?}");
        };
        assert!(message.contains("its process exited"), "{message}");
        assert!(message.contains("boom: no libavcodec"), "{message}");
    }

    #[test]
    fn an_answer_of_another_kind_stops_the_plugin() {
        let directory = tempfile::tempdir().unwrap();
        // `{"type":"x"}` (12 bytes) with an empty payload, then nothing.
        let program = script(
            directory.path(),
            r#"printf '\014\000\000\000{"type":"x"}\000\000\000\000'
exec cat >/dev/null"#,
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let error = child
            .request(hello(), "hello", Duration::from_secs(5))
            .unwrap_err();
        let RequestError::Unavailable(message) = error else {
            panic!("{error:?}");
        };
        assert!(message.contains("with `x`, not `hello`"), "{message}");
        assert!(exited(&child), "it was killed");
    }

    #[test]
    fn a_plugin_whose_stream_breaks_is_stopped() {
        let directory = tempfile::tempdir().unwrap();
        // A 4-byte header that is not JSON, then the process lingers.
        let program = script(
            directory.path(),
            "printf '\\004\\000\\000\\000nope'\nexec sleep 30",
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let error = child
            .request(hello(), "hello", Duration::from_secs(5))
            .unwrap_err();
        assert!(matches!(error, RequestError::Unavailable(_)), "{error:?}");
        assert!(exited(&child), "it was killed");
    }

    #[test]
    fn dropping_it_closes_stdin_before_killing() {
        let directory = tempfile::tempdir().unwrap();
        let started = directory.path().join("started");
        let marker = directory.path().join("finished");
        let program = script(
            directory.path(),
            &format!(
                "touch '{}'\ncat >/dev/null\ntouch '{}'",
                started.display(),
                marker.display()
            ),
        );
        let child = start_retrying(&program, directory.path()).unwrap();
        wait_for(&started);
        drop(child);
        assert!(
            marker.exists(),
            "the plugin saw its stdin close and finished"
        );
    }

    fn kind(kind: &str) -> Message {
        Message::new(json!({ "type": kind }))
    }

    /// The bytes of `messages` as a plugin would write them.
    fn canned(directory: &Path, name: &str, messages: &[Message]) -> std::path::PathBuf {
        let path = directory.join(name);
        let mut bytes = Vec::new();
        for message in messages {
            write_message(&mut bytes, message).unwrap();
        }
        std::fs::write(&path, bytes).unwrap();
        path
    }

    const LONG: Duration = Duration::from_secs(5);

    #[test]
    fn answers_come_back_in_order_through_the_window() {
        let directory = tempfile::tempdir().unwrap();
        let answers = canned(
            directory.path(),
            "answers.bin",
            &[kind("x"), kind("y"), kind("z")],
        );
        let program = script(
            directory.path(),
            &format!("cat '{}'\nexec cat >/dev/null", answers.display()),
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        for (request, expected) in [("a", "x"), ("b", "y"), ("c", "z")] {
            child
                .send(kind(request), expected, LONG, DeadlineFrom::Sent)
                .unwrap();
        }
        assert_eq!(child.in_flight(), 3);
        for expected in ["x", "y", "z"] {
            assert_eq!(child.receive().unwrap().kind(), expected);
        }
        assert_eq!(child.in_flight(), 0);
    }

    #[test]
    fn a_wrong_kind_in_the_window_stops_the_plugin() {
        let directory = tempfile::tempdir().unwrap();
        let answers = canned(directory.path(), "answers.bin", &[kind("x"), kind("z")]);
        let program = script(
            directory.path(),
            &format!("cat '{}'\nexec cat >/dev/null", answers.display()),
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        for (request, expected) in [("a", "x"), ("b", "y"), ("c", "w")] {
            child
                .send(kind(request), expected, LONG, DeadlineFrom::Sent)
                .unwrap();
        }
        assert_eq!(child.receive().unwrap().kind(), "x");
        let RequestError::Unavailable(message) = child.receive().unwrap_err() else {
            panic!("expected Unavailable");
        };
        assert!(message.contains("with `z`, not `y`"), "{message}");
        assert!(exited(&child), "it was killed");
        let RequestError::Unavailable(message) = child.receive().unwrap_err() else {
            panic!("expected Unavailable");
        };
        assert!(
            message.contains("stopped before it answered `c`"),
            "{message}"
        );
    }

    /// `sleep 1; x; sleep 0.5; y`, the second answer 1.5 s after the start.
    fn slow_pair(directory: &Path) -> std::path::PathBuf {
        let x = canned(directory, "x.bin", &[kind("x")]);
        let y = canned(directory, "y.bin", &[kind("y")]);
        script(
            directory,
            &format!(
                "sleep 1\ncat '{}'\nsleep 0.5\ncat '{}'\nexec sleep 30",
                x.display(),
                y.display()
            ),
        )
    }

    #[test]
    fn a_deadline_from_the_previous_answer_waits_for_the_queue() {
        let directory = tempfile::tempdir().unwrap();
        let program = slow_pair(directory.path());
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let begun = Instant::now();
        child
            .send(kind("a"), "x", LONG, DeadlineFrom::Sent)
            .unwrap();
        child
            .send(
                kind("b"),
                "y",
                Duration::from_millis(800),
                DeadlineFrom::PreviousAnswer,
            )
            .unwrap();
        assert_eq!(child.receive().unwrap().kind(), "x");
        assert_eq!(child.receive().unwrap().kind(), "y");
        assert!(
            begun.elapsed() > Duration::from_millis(1200),
            "a deadline from the send would have passed"
        );
    }

    #[test]
    fn a_deadline_from_send_counts_the_queue() {
        let directory = tempfile::tempdir().unwrap();
        let program = slow_pair(directory.path());
        let mut child = start_retrying(&program, directory.path()).unwrap();
        child
            .send(kind("a"), "x", LONG, DeadlineFrom::Sent)
            .unwrap();
        child
            .send(
                kind("b"),
                "y",
                Duration::from_millis(800),
                DeadlineFrom::Sent,
            )
            .unwrap();
        assert_eq!(child.receive().unwrap().kind(), "x");
        let RequestError::Stopped(message) = child.receive().unwrap_err() else {
            panic!("expected Stopped");
        };
        assert!(message.contains("did not answer `b`"), "{message}");
        assert!(exited(&child), "it was killed");
    }

    #[test]
    fn an_error_answer_leaves_the_window_running() {
        let directory = tempfile::tempdir().unwrap();
        let answers = canned(
            directory.path(),
            "answers.bin",
            &[error_message("no"), kind("y")],
        );
        let program = script(
            directory.path(),
            &format!("cat '{}'\nexec cat >/dev/null", answers.display()),
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        child
            .send(kind("a"), "x", LONG, DeadlineFrom::Sent)
            .unwrap();
        child
            .send(kind("b"), "y", LONG, DeadlineFrom::Sent)
            .unwrap();
        let error = child.receive().unwrap_err();
        let RequestError::Refused(_) = &error else {
            panic!("{error:?}");
        };
        assert_eq!(error.message(), "no");
        assert_eq!(child.receive().unwrap().kind(), "y");
        assert!(!exited(&child));
    }

    #[test]
    fn receive_without_a_request_in_flight_is_an_error_not_a_kill() {
        let directory = tempfile::tempdir().unwrap();
        let program = script(directory.path(), "exec cat >/dev/null");
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let RequestError::Unavailable(message) = child.receive().unwrap_err() else {
            panic!("expected Unavailable");
        };
        assert!(message.contains("no request is in flight"), "{message}");
        assert!(!exited(&child));
    }

    #[test]
    fn killing_the_switch_fails_every_request_in_flight() {
        let directory = tempfile::tempdir().unwrap();
        let started = directory.path().join("started");
        let program = script(
            directory.path(),
            &format!("touch '{}'\nexec sleep 30", started.display()),
        );
        let mut child = start_retrying(&program, directory.path()).unwrap();
        let switch = crate::PluginKillSwitch::new();
        switch.register(child.kill_handle());
        wait_for(&started);
        child
            .send(kind("a"), "x", Duration::from_secs(30), DeadlineFrom::Sent)
            .unwrap();
        child
            .send(kind("b"), "y", Duration::from_secs(30), DeadlineFrom::Sent)
            .unwrap();
        let killer = {
            let switch = switch.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(200));
                switch.kill();
            })
        };
        let begun = Instant::now();
        let first = child.receive().unwrap_err();
        let second = child.receive().unwrap_err();
        killer.join().unwrap();
        assert!(begun.elapsed() < Duration::from_secs(10));
        assert!(matches!(first, RequestError::Stopped(_)), "{first:?}");
        assert!(matches!(second, RequestError::Stopped(_)), "{second:?}");
    }
}
