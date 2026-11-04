# The codec plugin (2026-11-05)

The goal, for v0.10.0: Celesta's executables (the editor, `celesta-exporter`,
`celesta-mcp`) link no FFmpeg, so they can be distributed under MIT OR
Apache-2.0 alone. Probing, decoding, encoding, and muxing media move into a
codec plugin, `celesta-codec-ffmpeg`: a separate program Celesta starts and
talks to over the plugin protocol (`2026-11-01-plugin-system.md`). Only that
program links FFmpeg, and with it the GPL libx264 of the official packages.

## Specification

- **Interface.** `celesta_media::CodecProvider`: `probe`, `decode_video`
  (an exact-time request, or a `Sequential` one naming a session and the
  frame rate expected until the session shows its own), `decode_audio`
  (interleaved `f32` at a given rate and 1 or 2 channels), `check_encoder`,
  `open_encoder` / `encode_frame` / `finish_encoder` / `abort_encoder`, and
  `mux_audio`. Encoding is described by meaning, not FFmpeg options:
  `Container` (`mp4`, `mov`, `webm`), the input `PixelFormat` (`rgba8`,
  `rgba16`, `yuv420p`, `yuv420p10`), and for MP4 the libx264 preset name,
  CRF, and bit depth. Which codecs and filters realize a container is the
  plugin's choice; `celesta-codec-ffmpeg` keeps exactly the previous FFmpeg
  settings (`crates/codec-ffmpeg/src/encoder.rs`).
- **`MediaBackend`** replaces `FfmpegBackend` on Celesta's side, with the
  same shape (`new`, `with_sequential_video`, `probe` cached by path, and the
  `VideoFrameDecoder` and `AudioDecoder` traits) plus the encoder calls. It
  opens its codec on first use, from the factory `celesta_media::set_codec`
  chose at that moment (so a backend made before the choice, such as the
  editor's long-lived audio worker, still gets it), or from the one given to
  `MediaBackend::with_codec`. A request that finds the codec gone
  (`MediaError::CodecUnavailable`) opens a new one and is sent once more;
  encoder requests are not retried, since the encoder lived in the old one.
  With no codec, requests fail with `MediaError::NoCodec`.
- **Process.** `CodecProcess` starts the program in its plugin folder, checks
  that `hello`'s `roles` include `codec`, and makes every path absolute
  against Celesta's working directory before sending it (the renderer passes
  paths such as `./assets/clip.mp4`); `NotFound` and `NoVideoStream` errors
  name the path as the caller gave it. One process per backend: the
  preview's renderer, the editor's audio mix, each React runtime's probes, and
  each export's encoder each have their own, as each had its own decoder
  state before. Dropping the backend stops its process.
- **Protocol** (version 1, the framing of `celesta_plugin::protocol`; a
  codec plugin's main loop is `celesta_media::codec::protocol::serve`):

  | Request | Answer |
  | --- | --- |
  | `{"type":"hello","protocol":1}` | `{"type":"hello",…,"roles":["codec"]}` |
  | `{"type":"probe","path"}` | `{"type":"probed","probe":{duration,video,audio}}` (`MediaProbe` in camelCase; `Time` as `{value,timescale}`, `Rational` as `{numerator,denominator}`) |
  | `{"type":"decodeVideo","path","time","sequential":{"session","frameRate"}\|null}` | `{"type":"videoFrame","width","height"}` + RGBA8 payload |
  | `{"type":"decodeAudio","path","sampleRate","channels"}` | `{"type":"audio","sampleRate","channels"}` + `f32` LE payload |
  | `{"type":"checkEncoder","container"}` | `{"type":"encoderReady"}` |
  | `{"type":"openEncoder","settings":{output,width,height,frameRate,input,container,preset,crf,bitDepth}}` | `{"type":"encoderOpened","encoder":n}` |
  | `{"type":"encodeFrame","encoder":n}` + one frame of the input format | `{"type":"frameEncoded"}` |
  | `{"type":"finishEncoder","encoder":n}` / `{"type":"abortEncoder","encoder":n}` | `{"type":"encoderFinished"}` / `{"type":"encoderAborted"}` |
  | `{"type":"muxAudio","request":{video,audio,sampleRate,channels,output,container,leadInFrames}}` | `{"type":"muxed"}` |
  | any | `{"type":"error","message", "code"?, …}` |

  An error's `code` (`notFound`, `noVideoStream`, `invalidTimestamp`,
  `frameTooLarge`, `unexpectedFrameSize`, `invalidAudioFormat`,
  `invalidAudioSampleCount`, `encoderUnavailable`) and its fields bring the
  matching `MediaError` back; any other error is `MediaError::Codec` with the
  message. `muxAudio`'s `audio` is the raw `f32le` PCM the exporter stages
  beside the video, as before; `leadInFrames` is the looping MP4's lead-in,
  which the plugin times before 0.
- **Manifest.** `roles` (`"content"`, `"codec"`; content alone when absent;
  unknown roles are kept as `PluginRole::Unknown` so a newer manifest still
  loads). The codec plugin's:

  ```json
  { "id": "ffmpeg", "version": "0.9.0", "protocol": 1, "roles": ["codec"],
    "executable": { "macos-aarch64": "bin/celesta-codec-ffmpeg" } }
  ```

  `scripts/stage-codec-plugin.mjs <program> <folder>` writes it, copies the
  program to `bin/`, and the GPL text and source offer to `licenses/`.
- **Choosing the codec.** `InstalledPlugins::codec()`: the first plugin that
  may run (the usual search order and approval) with the `codec` role and a
  program for this platform; else `celesta-codec-ffmpeg` beside the running
  executable (`CODEC_PROGRAM`, links resolved), which is where `cargo build`,
  `cargo install`, and the test binaries' `target/debug` have it; else none.
  `InstalledPlugins::host()` gives the renderer only content plugins.
  `celesta_exporter::use_codec(&plugins)` installs the choice;
  `Exporter::new` does so when none was made yet; the exporter's `--inspect`
  and `Celesta-mcp` at start-up (`use_installed_codec`); the editor on every
  discovery (`FoundPlugins::find`: project open and File > Reload Plugins).
- **Without a codec**, the editor shows **No codec plugin is installed** in
  Problems (with the other plugin problems), and exports fail with
  `ExportError::Codec { source: NoCodec }`, reported as `no_codec` with a
  hint. Other codec failures are `codec` (was `ffmpeg`), and
  `encoder_unavailable` names the plugin's missing encoder as before.
- **Packages.** The codec plugin is in the bundled plugin folder:
  `Contents/PlugIns/ffmpeg` in the macOS app, with its Homebrew dylibs in its
  own `Frameworks/` (`@executable_path/../Frameworks/` from its `bin/`);
  `plugins\ffmpeg` beside `Celesta.exe`, FFmpeg static, with its own copies
  of the MSVC runtime DLLs; `/opt/celesta/plugins/ffmpeg` in the Linux image,
  which sets `CELESTA_SYSTEM_PLUGIN_DIR` for that. `package-macos.py` fails
  when Celesta's own executables link FFmpeg, and `test-package.mjs` checks
  the plugin and its GPL text are there before it exports an MP4 through it.

## What moved

| From | To |
| --- | --- |
| `crates/media/src/ffmpeg.rs` (`FfmpegBackend`) | `crates/codec-ffmpeg/src/decoder.rs`, still `FfmpegBackend`, now the plugin's decoder |
| `crates/media/src/{probe,sequential,converted}.rs` | `crates/codec-ffmpeg/src/` |
| `crates/media/src/color.rs` (tone mapping, raw tag names) | `crates/codec-ffmpeg/src/color.rs`; `VideoColor` and `DynamicRange` stay in `celesta-media` |
| `crates/exporter/src/render.rs`'s `VideoWriter`, BT.709 filter, muxing, encoder check | `crates/codec-ffmpeg/src/encoder.rs`; the exporter keeps the PCM staging, the lead-in, and the CRF check |
| `crates/media/tests/ffmpeg_integration.rs`, the decode tests of `media/src/tests.rs`, and the FFmpeg `ebur128` comparison | `crates/codec-ffmpeg/tests/decode.rs`, `src/tests.rs`, `src/tests/loudness.rs` |

`celesta-media` now depends on `celesta-plugin` (framing) and serde, and on
no FFmpeg crate. `celesta-codec-ffmpeg` is a workspace member, publishable
like the others; its source is MIT OR Apache-2.0, and a build of it against a
GPL FFmpeg is GPL as distributed. Tests and benches that read media use it in
their own process through dev-dependencies (`celesta_codec_ffmpeg::install()`
or `FfmpegCodec::factory()`), so nothing they ship links it. The VRM plugin's
workspace no longer pulls FFmpeg in.

## Not done

- Frames cross a pipe: a 1080p RGBA frame is 8 MB each way. The exports
  measured below took no longer than in-process decoding, but 4K playback
  in the editor may; shared memory would remove the copy.
- No alternative codec plugin yet (an LGPL-only FFmpeg build, or one on the
  OS's codecs such as VideoToolbox or Media Foundation). With an LGPL-only
  FFmpeg, `celesta-codec-ffmpeg` builds and decodes, but MP4 export fails with
  `encoder_unavailable` (no libx264).
- The editor was not run by hand; its tests (including the missing-codec
  problem) pass, and the preview shares `MediaBackend` with the exporter.
- `celesta-codec-ffmpeg` is not on crates.io yet; `cargo install celesta-editor`
  users need `cargo install celesta-codec-ffmpeg` beside it.

## Checks

- `cargo test --workspace` (2,216 passed, none skipped, macOS arm64, Metal),
  `cargo clippy --workspace --all-targets --locked -- -D warnings`;
  `cargo fmt --all -- --check` reports only files `main` already leaves
  unformatted.
- `cargo tree -e normal,build` of `celesta-editor`, `celesta-exporter`,
  `celesta-mcp`, `celesta-media`, and `celesta-gpu-renderer`: no FFmpeg crate.
  `otool -L target/debug/celesta-exporter`: no FFmpeg library.
- `celesta-media`: the protocol serves every request of a fake codec and
  decodes its answers; errors keep their kind across it; a backend reopens a
  codec that stopped answering once; no codec is `NoCodec`.
- `celesta-codec-ffmpeg/tests/process.rs` drives the built program through
  `CodecProcess`: probes, exact and sequential frames out of order, and audio
  equal the in-process codec's; `NotFound`/`NoVideoStream` keep their paths;
  relative paths resolve against the caller's directory; an H.264 file
  encoded through it, muxed with AAC, reads back; an out-of-range CRF is
  refused.
- `celesta-plugin-host`: roles parse (unknown ones kept), the first codec is
  chosen, the renderer's host leaves codecs out, and the program is found
  beside an executable.
- `cargo test --manifest-path plugins/vrm/Cargo.toml --locked` without
  `PKG_CONFIG_PATH` (372 passed).
- End to end on macOS arm64: the branch's exporter, copied alone into a
  scratch folder, with the codec plugin only in a `CELESTA_PLUGIN_PATH`
  folder staged by `stage-codec-plugin.mjs`, against the exporter built from
  the base commit (`0abc4ac9`, FFmpeg linked in): `examples/greenscreen`'s
  composite as MP4, and a `<Video>` + `<Audio>` entry as MP4 with `--loop`
  and as MOV, came out byte-identical; WebM, whose muxer writes a random
  segment UID, decoded to identical frames and audio. Without a plugin the
  exporter's JSON report is `no_codec` with its hint.
- Not run: the macOS, Windows, and Linux packaging scripts (the macOS
  script's Python parses, `test-package.mjs` passes `node --check`).
