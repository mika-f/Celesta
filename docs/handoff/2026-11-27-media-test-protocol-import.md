# celesta-media test build fix

## Spec

`main` failed `cargo build --workspace --all-targets --features runtime-tests --locked`
on the Linux and macOS test jobs:

```text
error[E0425]: cannot find value `MEDIA_PROTOCOL_VERSION` in this scope
  --> crates/media/src/process.rs:990:46
```

Commit `7b380a701` (audit handshake validation) moved the media `hello` into
`PluginChild::hello_media` and dropped the module-level
`use celesta_plugin::MEDIA_PROTOCOL_VERSION;` from `process.rs`. Commit
`6cce4f8d9`, merged alongside it in #909, added the test
`an_rgba16_frame_is_sent_as_a_slot_without_a_pipe_payload`, which still names
the constant. The test is `#[cfg(unix)]`, so Windows never compiled it.

## Implementation

Import `celesta_plugin::MEDIA_PROTOCOL_VERSION` inside that test, next to its
other test-local imports. The library code no longer uses the constant, so it
stays out of the module-level imports.

## Checks

- Before the fix, `cargo build -p celesta-media --all-targets --locked`
  reproduced the error above; after it, the build passed.
- `cargo test -p celesta-media --locked --lib an_rgba16_frame_is_sent_as_a_slot_without_a_pipe_payload`
  passed, and `cargo test -p celesta-media --locked` passed (152 tests).
- `cargo fmt --all --check` and `cargo clippy -p celesta-media --all-targets --locked`
  reported nothing.
- `cargo build --workspace --all-targets --features runtime-tests --locked`
  (the CI command) passed on macOS arm64.
