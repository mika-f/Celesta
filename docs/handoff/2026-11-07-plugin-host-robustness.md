# Plugin host robustness: the rest of #689

Issue #689 listed gaps in `crates/plugin-host` and `crates/plugin`. Several
landed with `celesta_plugin_host::PluginChild`, the helper content plugins
and the codec plugin both run their processes through (see
`2026-11-05-codec-plugin.md`): a plugin's stderr is piped, passed through,
and its tail ends the error of a process that went away; each answer must
be of the kind its request expects, and `hello` must name protocol 1, so a
plugin out of step is restarted; dropping it closes stdin and kills only
after 500 ms; and an asset path that is not UTF-8 is a `PluginError::Failed`
instead of a panic. This note covers the rest.

## Specification

- **Manifest `protocol` at discovery.** A plugin whose `protocol` is not
  `celesta_plugin::PROTOCOL_VERSION` is skipped as `DiscoveryError::Invalid`
  ("it speaks plugin protocol N, and this Celesta speaks protocol 1"), so it
  is not offered for approval, and a module-only plugin is checked too. The
  check comes after the `celesta` range: a plugin made for another Celesta is
  reported as `Incompatible`, which says what to do about it. It applies to
  module-only plugins too, as #689 asked: `protocol` is a required field
  every manifest declares, and one this Celesta does not speak marks a
  plugin built for something else.
- **stdout on Windows.** `serve_requests` (and so `serve`) keeps stdout for
  the protocol on Windows as on Unix: it answers through a duplicate of the
  stdout handle, points the C runtime's descriptor 1 at stderr with `_dup2`,
  and sets `STD_OUTPUT_HANDLE` to descriptor 1's new handle, which Rust's
  `io::stdout()` reads at every write. `println!` and a native library's
  `printf` then land in Celesta's log. Other targets keep plain stdout.
- **Framing limits.** A header is at most 16 MiB (`MAX_HEADER`) and a payload
  at most 1 GiB (`MAX_PAYLOAD`), on both sides. A part's buffer starts at no
  more than 1 MiB and grows as its bytes arrive, so a bogus length (such as
  `"OK: "`, about 540 MB, from text printed into the stream) allocates no
  more than the bytes really sent; such a header length is refused at once.
  A stream that ends inside a part, header included, is an `UnexpectedEof`
  error naming how far it got; only an end before a message's first length
  is `Ok(None)`. Each part starts from a buffer of at most 1 MiB rather than
  none, so a large payload (textures) is not regrown from a few bytes; that
  1 MiB is all a bogus length can make it reserve.
- **Unknown models.** `PluginHost::frame` for a model never opened is the new
  `PluginError::UnknownModel(ModelId)` ("no open model 7") rather than a
  `Failed` naming plugin `""`; `ContentProvider::frame`'s docs name it.
  The sample providers keep answering `Failed` inside their processes,
  where any error reaches Celesta as its message.
- **Light limit.** `MAX_DIRECTIONAL_LIGHTS` (4) is `pub` in
  `celesta_plugin::scene`, beside the other scene limits; the GPU renderer
  and `Lights::directional`'s docs use it.
- `celesta-plugin`'s docs, `HANDOFF.md`, and `2026-11-01-plugin-system.md`
  list its dependencies as serde, serde_json, thiserror, and libc (now on
  Unix and Windows).

The open question, whether an answer should echo something of its request so
that a stale answer of the right kind (a second `frame`) is caught, is left
open: it changes the protocol.

## Implementation

- `crates/plugin/src/protocol.rs`: `MAX_HEADER`, `MAX_PAYLOAD`,
  `PREALLOCATED`; `read_part` takes its limit and reads through `take`;
  `write_message` checks the header against `MAX_HEADER`; the `cfg(windows)`
  `protocol_output` (declares `SetStdHandle` from kernel32 itself rather
  than adding a Windows bindings crate).
- `crates/plugin/Cargo.toml`: `libc` for `cfg(any(unix, windows))`; the
  harness-less `stdout` test.
- `crates/plugin-host/src/manifest.rs`: the protocol check in
  `read_manifest`.
- `crates/plugin/src/provider.rs`: `PluginError::UnknownModel`.

## Checks

- `crates/plugin/tests/stdout.rs` runs itself as a plugin that prints with
  `println!` and with the C runtime's `write(1, …)` while answering, and
  checks that stdout carries only the answers and stderr the text. It has no
  test harness, which would print on stdout first. It runs on every CI OS;
  the Windows path was only cross-checked here
  (`cargo clippy -p celesta-plugin --all-targets --target x86_64-pc-windows-msvc`).
- `protocol::tests`: `printed_text_is_refused_as_a_header_at_once`,
  `a_part_longer_than_the_stream_ends_it`,
  `a_stream_that_ends_inside_a_header_is_an_error`,
  `a_header_too_large_is_not_sent`.
- `host::tests::a_plugin_that_answers_twice_is_restarted_back_in_step`: a
  shell-script plugin whose first process answers `open` twice; the next
  `frame` restarts it and succeeds. `an_unknown_model_names_no_plugin`.
- `manifest::tests::skips_plugins_of_another_protocol`.
