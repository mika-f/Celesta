//! `serve_requests` keeps stdout for the protocol: what the plugin prints
//! there, through Rust's `println!` or the C runtime, goes to stderr.
//!
//! This program is the test and, started again with `PLUGIN_ENV` set, the
//! plugin it talks to. It has no test harness, since one would print on
//! stdout before the plugin could take it over.

use celesta_plugin::PROTOCOL_VERSION;
use celesta_plugin::protocol::{Message, read_message, serve_requests, write_message};
use serde_json::json;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

const PLUGIN_ENV: &str = "CELESTA_PLUGIN_STDOUT_TEST_PLUGIN";

fn main() {
    if std::env::var_os(PLUGIN_ENV).is_some() {
        plugin();
    } else {
        test();
        println!("test stdout ... ok");
    }
}

/// Answers `ping` with `pong`, printing on stdout on the way.
fn plugin() {
    serve_requests("stdout-test", "1", &["content"], |request| {
        assert_eq!(request.kind(), "ping");
        println!("printed by Rust");
        std::io::stdout().flush().unwrap();
        c_print(b"printed by C\n");
        Message::new(json!({ "type": "pong" }))
    })
    .unwrap();
}

/// Writes `text` to descriptor 1 as C code would.
fn c_print(text: &[u8]) {
    // SAFETY: writes from a live buffer of the length given.
    #[cfg(unix)]
    let written = unsafe { libc::write(1, text.as_ptr().cast(), text.len()) };
    #[cfg(windows)]
    let written = unsafe { libc::write(1, text.as_ptr().cast(), text.len() as u32) };
    assert_eq!(usize::try_from(written).ok(), Some(text.len()));
}

fn test() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .env(PLUGIN_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    let mut request = |message: serde_json::Value| {
        write_message(&mut input, &Message::new(message)).unwrap();
        read_message(&mut output).unwrap().expect("an answer")
    };
    let hello = request(json!({ "type": "hello", "protocol": PROTOCOL_VERSION }));
    assert_eq!(hello.kind(), "hello", "{:?}", hello.header);
    let pong = request(json!({ "type": "ping" }));
    assert_eq!(pong.kind(), "pong", "{:?}", pong.header);
    drop(input);

    let mut rest = Vec::new();
    output.read_to_end(&mut rest).unwrap();
    assert!(
        rest.is_empty(),
        "stdout had more: {:?}",
        String::from_utf8_lossy(&rest)
    );
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(child.wait().unwrap().success(), "{stderr}");
    assert!(stderr.contains("printed by Rust"), "{stderr}");
    assert!(stderr.contains("printed by C"), "{stderr}");
}
