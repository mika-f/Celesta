//! Node's stderr: passed through to this process's stderr as it arrives,
//! with its last lines kept to explain an unexpected exit.

use crate::error::ReactBridgeError;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::process::{Child, ChildStderr};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// How many of the runtime's last stderr lines an exit error keeps.
pub(crate) const MAX_LINES: usize = 40;
/// How many bytes of those lines it keeps, at most.
pub(crate) const MAX_BYTES: usize = 8 * 1024;
/// How long an exit error waits for the runtime to exit and its stderr to
/// close. A grandchild can hold the pipe open past the runtime's exit.
const EXIT_WAIT: Duration = Duration::from_millis(500);

/// The last lines of a stream, within [`MAX_LINES`] and [`MAX_BYTES`];
/// the oldest go first. Lines are kept without ANSI escape sequences.
#[derive(Default)]
pub(crate) struct StderrTail {
    lines: VecDeque<String>,
    bytes: usize,
    /// The line written so far, up to [`MAX_BYTES`] of it.
    pending: Vec<u8>,
}

impl StderrTail {
    /// Adds raw output, which may end partway through a line.
    pub(crate) fn write(&mut self, mut bytes: &[u8]) {
        while let Some(end) = bytes.iter().position(|&byte| byte == b'\n') {
            self.extend_pending(&bytes[..end]);
            let line = std::mem::take(&mut self.pending);
            self.push(&line_text(&line));
            bytes = &bytes[end + 1..];
        }
        self.extend_pending(bytes);
    }

    /// Bytes past [`MAX_BYTES`] are dropped until the line ends.
    fn extend_pending(&mut self, bytes: &[u8]) {
        let room = MAX_BYTES.saturating_sub(self.pending.len());
        self.pending
            .extend_from_slice(&bytes[..bytes.len().min(room)]);
    }

    /// Adds one line, without its line ending. A line longer than
    /// [`MAX_BYTES`] keeps only its start.
    pub(crate) fn push(&mut self, line: &str) {
        let line = clean(line);
        self.bytes += line.len();
        self.lines.push_back(line);
        while self.lines.len() > MAX_LINES || self.bytes > MAX_BYTES {
            let Some(oldest) = self.lines.pop_front() else {
                break;
            };
            self.bytes -= oldest.len();
        }
    }

    /// The kept lines, and the unfinished one, joined with `\n`, without
    /// trailing blank lines.
    pub(crate) fn text(&self) -> String {
        let pending = (!self.pending.is_empty()).then(|| clean(&line_text(&self.pending)));
        let lines: Vec<&str> = self
            .lines
            .iter()
            .map(String::as_str)
            .chain(pending.as_deref())
            .collect();
        let kept = lines
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .map_or(0, |last| last + 1);
        lines[..kept].join("\n")
    }
}

/// A line's bytes as text, without a trailing `\r`.
fn line_text(line: &[u8]) -> String {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    String::from_utf8_lossy(line).into_owned()
}

/// `line` without ANSI escape sequences (colors, with `FORCE_COLOR`), cut to
/// at most [`MAX_BYTES`] at a character boundary.
fn clean(line: &str) -> String {
    let mut cleaned = strip_ansi_escapes::strip_str(line);
    if cleaned.len() > MAX_BYTES {
        let end = (0..=MAX_BYTES)
            .rev()
            .find(|&end| cleaned.is_char_boundary(end))
            .unwrap_or(0);
        cleaned.truncate(end);
    }
    cleaned
}

/// Copies `reader` to `forward` as it arrives, unchanged, and keeps its
/// lines in `tail`. Stops at the end of `reader` or on a read error. A
/// failed write is ignored and reading goes on; a write blocks while
/// `forward` is full, as it would for Node writing there itself.
pub(crate) fn pump(mut reader: impl Read, mut forward: impl Write, tail: &Mutex<StderrTail>) {
    let mut buffer = [0; 8 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        let chunk = &buffer[..read];
        let _ = forward.write_all(chunk).and_then(|()| forward.flush());
        tail.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write(chunk);
    }
}

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

#[cfg(test)]
mod tests {
    use super::{MAX_BYTES, MAX_LINES, StderrTail, pump};
    use std::cell::RefCell;
    use std::io::{self, Cursor, Read, Write};
    use std::rc::Rc;
    use std::sync::Mutex;

    #[test]
    fn keeps_only_the_last_lines() {
        let mut tail = StderrTail::default();
        for line in 0..MAX_LINES + 5 {
            tail.push(&format!("line {line}"));
        }
        let text = tail.text();
        assert_eq!(text.lines().count(), MAX_LINES);
        assert_eq!(text.lines().next(), Some("line 5"));
        assert_eq!(
            text.lines().last(),
            Some(format!("line {}", MAX_LINES + 4).as_str())
        );
    }

    #[test]
    fn drops_the_oldest_lines_past_the_byte_cap() {
        let mut tail = StderrTail::default();
        let line = "x".repeat(MAX_BYTES / 4);
        tail.push("first");
        for _ in 0..4 {
            tail.push(&line);
        }
        let text = tail.text();
        assert!(!text.contains("first"), "the oldest line goes first");
        assert_eq!(text.lines().count(), 4);
        tail.push("last");
        let text = tail.text();
        assert_eq!(text.lines().count(), 4, "one long line made room");
        assert_eq!(text.lines().last(), Some("last"));
    }

    #[test]
    fn truncates_a_line_longer_than_the_byte_cap() {
        let mut tail = StderrTail::default();
        tail.push("before");
        // A multi-byte character straddles the cap.
        tail.push(&format!("{}é{}", "a".repeat(MAX_BYTES - 1), "b".repeat(10)));
        let text = tail.text();
        assert_eq!(text, "a".repeat(MAX_BYTES - 1));
    }

    #[test]
    fn trims_trailing_blank_lines_only() {
        let mut tail = StderrTail::default();
        for line in ["Error: boom", "", "Node.js v24.0.0", "", "  "] {
            tail.push(line);
        }
        assert_eq!(tail.text(), "Error: boom\n\nNode.js v24.0.0");
    }

    #[test]
    fn keeps_only_the_start_of_an_overlong_streamed_line() {
        let mut tail = StderrTail::default();
        let chunk = "a".repeat(MAX_BYTES);
        for _ in 0..3 {
            tail.write(chunk.as_bytes());
            assert!(
                tail.pending.len() <= MAX_BYTES,
                "the pending line is capped"
            );
        }
        tail.write(b"b\r\n");
        assert!(tail.pending.is_empty());
        assert_eq!(tail.text(), chunk);
    }

    #[test]
    fn strips_ansi_escapes_from_kept_lines() {
        let mut tail = StderrTail::default();
        tail.write(b"\x1b[31mError\x1b[39m: \x1b[1;4mboom\x1b[0m\n");
        tail.write(b"\x1b[90m    at render\x1b[39m");
        assert_eq!(tail.text(), "Error: boom\n    at render");
    }

    #[test]
    fn strips_hyperlinks_from_kept_lines() {
        let mut tail = StderrTail::default();
        tail.write(b"see \x1b]8;;https://example.com\x1b\\docs\x1b]8;;\x1b\\\n");
        assert_eq!(tail.text(), "see docs");
    }

    #[test]
    fn forwards_output_unchanged_and_keeps_a_partial_last_line() {
        let tail = Mutex::new(StderrTail::default());
        let mut forwarded = Vec::new();
        let input = b"first\r\nsecond\nno newline";
        pump(Cursor::new(input), &mut forwarded, &tail);
        assert_eq!(forwarded, input);
        assert_eq!(
            tail.into_inner().unwrap().text(),
            "first\nsecond\nno newline"
        );
    }

    /// Hands out one chunk per read, noting what was forwarded before each.
    struct Chunks {
        chunks: Vec<&'static [u8]>,
        forwarded: Rc<RefCell<Vec<u8>>>,
        seen: Vec<Vec<u8>>,
    }

    impl Read for Chunks {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.seen.push(self.forwarded.borrow().clone());
            if self.chunks.is_empty() {
                return Ok(0);
            }
            let chunk = self.chunks.remove(0);
            buffer[..chunk.len()].copy_from_slice(chunk);
            Ok(chunk.len())
        }
    }

    struct Shared(Rc<RefCell<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn forwards_output_before_its_line_ends() {
        let forwarded = Rc::new(RefCell::new(Vec::new()));
        let mut reader = Chunks {
            chunks: vec![b"progress 50%\r", b"progress 100%\n"],
            forwarded: Rc::clone(&forwarded),
            seen: Vec::new(),
        };
        let tail = Mutex::new(StderrTail::default());
        pump(&mut reader, Shared(Rc::clone(&forwarded)), &tail);
        assert_eq!(reader.seen[1], b"progress 50%\r");
        assert_eq!(*forwarded.borrow(), b"progress 50%\rprogress 100%\n");
    }
}
