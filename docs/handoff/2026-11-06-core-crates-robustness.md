# Core crates: robustness fixes and cleanups (2026-11-06)

Small fixes across `celesta-react-bridge`, `celesta-remote`,
`celesta-source-edit`, `celesta-composition`, and the FFmpeg codec plugin
(mika-f/Celesta#707). None was reachable through a common path; each was a
trap for the next caller or made an error harder to read. The codec-plugin
move (#681) had already landed, so the media items apply to
`plugins/ffmpeg`, and `MediaBackend` already refused more than two channels
on the host side.

## Robustness

- **Bridge message length** (`bridge.rs`, `read_message`). The runtime
  frames each message as a little-endian `u32` length and a JSON object.
  A direct `process.stdout.write()` from an entry or a dependency reaches
  the same pipe (only `console.*` is sent to stderr), and its first bytes
  read as a length: `hell` is about 1.8 GB, and the bridge used to wait
  forever for it. Now a length over 1 GiB (`MAX_MESSAGE_LENGTH`), an empty
  message, or a payload whose first byte is not `{` fails as
  `ReactBridgeError::Protocol` with "…; something other than the Celesta
  runtime wrote to its stdout". The first byte is checked before the
  buffer grows, and the buffer grows with `try_reserve`. `read_message`
  takes any `Read`, so it is tested on a byte slice.
- **Media import copy** (`props_edit.rs`, `copy_into_assets`). It used to
  read the chosen file whole, and each same-named candidate whole, so a
  multi-GB video from outside the project peaked at about twice its size in
  memory. Now a candidate is compared only when its length matches, by
  streaming both files through `BufReader`s, and a new copy is streamed into
  a `StagedFile` beside its name and published without overwriting
  (`publish(&candidate, false)`), so a half-written copy never shows and an
  existing file is never replaced. A name taken between the check and the
  publish is compared like any other candidate.
- **Ready message errors** (`protocol.rs`). `ReadyMessage` was
  `#[serde(untagged)]`, so a runtime of another version or a malformed
  `componentSchemas` entry failed the spawn with "data did not match any
  variant of untagged enum ReadyMessage". It is now read through `RawReady`,
  whose hand-written visitor reads each key once and prefixes a value's
  error with its key (`` `componentSchemas`: unknown variant `strng`… ``),
  then picks the variant in the old order (`config`, `probeMedia`,
  `measureText`, `decodeAudio`, `invalidProperties`, `error`).
  `propertySchema` is read into an ordered list in the same pass, so
  `ReadyMessage::Ready` carries `project_property_order` and the second parse
  (`ready_property_order`) is gone. A repeated key keeps its first position.
  A `derive` with field context would need `serde_path_to_error`, which is
  not a dependency; the top-level key is what a version mismatch needs.
- **Local stylesheet `url()`s** (`celesta-remote`, `resolve_reference`). In
  a stylesheet read from a file, a reference is now read as a URL path, as
  a browser reads it: after the query and fragment are cut, it is
  percent-decoded (`percent-encoding`, now a direct dependency), so
  `url(My%20Font.woff2)` finds `My Font.woff2` and `url(100%25.woff2#x)`
  finds `100%.woff2`. A `file:` reference becomes a path through
  `Url::to_file_path`; one that names no local path (a host, on Unix) is the
  new `RemoteAssetError::NotAFilePath`. Follows #600.

## Latent traps

- **`format_number`** (`celesta-source-edit`). `inf` parsed as an identifier
  and threw `ReferenceError` at render time, and `1e19` saturated to
  `i64::MAX`. `apply` now refuses an edit that writes a non-finite number
  with `ApplyError::NonFinite`, before writing anything. `format_number`
  stays total because the editor also uses it for display: it spells
  non-finite values the JavaScript way (`Infinity`, `-Infinity`, `NaN`),
  and from 2^53 on, where every `f64` is an integer, it writes `{}` (the
  shortest round trip, in full digits).
- **`Time::default()`** is `Time::ZERO` (`0/1`), not the invalid `0/0`. The
  type's doc says `==` is structural (`1/2 != 2/4`) and points to
  `cmp_exact` and `reduced`.
- **`decode_audio` channels** (`plugins/ffmpeg`, `decoder.rs`). The plugin
  decoded mono for 1 channel and stereo otherwise while reporting the
  channels asked for. It now refuses anything but 1 or 2 with
  `MediaError::InvalidAudioFormat`, before looking at the file.
- **`serde_json` key order.** The workspace still does not enable
  `preserve_order`: the property panel's design
  (`docs/superpowers/specs/2026-10-11-property-panel-design.md`) rules it
  out because it would change map order across the workspace. The issue's
  other option is taken instead: `OrderedObject`'s doc now says that only the
  top level keeps its order by itself, and that a nested object's keys are
  sorted on write except in the editor, which gets the feature through GPUI.
- **`relative_path`** treated `..intro.mp4` as a parent path. It now checks
  `joined == ".." || joined.starts_with("../")`.
- **`stream_duration`** (`plugins/ffmpeg`, `probe.rs`) is exact:
  `Time::new(duration * num, den).reduced()`, with a checked multiply, not a
  truncating conversion to microseconds. `None` on overflow. `probe_path`
  still opens the container twice (`find_all_stream_infos` and
  `container_info::get_duration_us`): ez-ffmpeg 0.19 exposes no shared
  format context, and probes are memoized per path, so it stays.

## Duplication

- The handshake's three replies use `write_line`. The remote-or-local path
  in the handshake's `probeMedia` and in `decodeAudio` is
  `celesta_remote::local_asset_path`. `resolve_components`' five
  `UnexpectedResponse` arms are one `_`.
- `DecodedAudioFiles` holds `tempfile::TempPath`s, which remove their files
  when dropped; its `Drop` and `keep()` are gone. The test helper `Scratch`
  is a `tempfile` folder (its first field is still the path, so callers are
  unchanged). This alone does not fix #654.
- `Time::from_seconds(seconds, timescale)` and `Time::MICROSECONDS` replace
  the hand-written conversions in `composition/src/duck.rs`,
  `media/src/duck.rs`, `react-bridge/src/audio.rs` (its own
  `REACT_AUDIO_SECONDS_TIMESCALE` is gone), `plugins/ffmpeg`'s
  `seconds_to_us`, and `plugins/vrm`'s `track_time`. NaN gives 0 and
  out-of-range values saturate, as the `as` casts did.

## User-facing

- The fonts guide (en/ja) and the Agent Skill (`react-core.md`) say how a
  stylesheet's `url()` resolves, including percent escapes and `file:`.
- The troubleshooting page (en/ja) and the Agent Skill say that writing to
  `process.stdout` breaks the runtime's connection, with the error's
  wording.

## Tests

- `react-bridge`: `reads_framed_messages_one_at_a_time`,
  `a_stray_write_before_a_message_is_a_protocol_error` (`hello\n`, a short
  write, an empty message), the live
  `a_stray_stdout_write_is_a_protocol_error_when_node_is_available`,
  `a_ready_message_that_fails_to_parse_names_the_field`, the reworked
  `ready_message_keeps_the_declared_property_order`,
  `content_longer_than_a_buffer_is_compared_to_the_last_byte`, and
  `a_name_starting_with_two_dots_is_not_a_parent_folder`.
- `remote`: `local_references_are_percent_decoded`,
  `local_stylesheets_read_file_urls_as_paths`.
- `source-edit`: `format_number_rules` (2^53, `±1e19`, infinities, NaN),
  `apply_refuses_numbers_that_are_not_finite`.
- `composition`: `the_default_time_is_a_valid_zero`,
  `converts_seconds_to_the_nearest_tick`.
- `codec-ffmpeg`: `stream_durations_are_exact_in_the_stream_time_base`,
  `refuses_to_decode_more_than_two_channels`.

## Checks

See the pull request for the commands run and their results.
