# Media components P2a: encoders and muxers — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Exports choose a muxer, a video encoder, and an audio encoder as
components, drive them through protocol 2's encoding and muxing messages
over the pipe, and write the output once; the FFmpeg plugin and the test
codec serve those components, and protocol 1's encoder messages are gone.

**Architecture:**
- **Components.** The manifest's `media` gains `videoEncoders`,
  `audioEncoders`, and `muxers` (`celesta-plugin-host`). A runtime
  `describe` answers availability and a settings schema, which
  `celesta-media` validates before any `open`. `MediaRegistry` lists every
  component and caches `describe`.
- **Traits and protocol.** `MediaProvider` splits into `Decoder`,
  `VideoEncoder`, `AudioEncoder`, and `Muxer`, each mirroring protocol 2's
  messages one to one. `MediaProcess` is the client of all four;
  `protocol::serve` dispatches to whichever a program registers.
- **The export.** `celesta_exporter::resolve_output` turns an
  `OutputChoice` into components. An `ExportSession` starts one process per
  chosen plugin, opens the video encoder before the first frame, streams
  the mix to the audio encoder once it exists, interleaves packets by
  decode time (memory, then a spill file), opens the muxer once every
  track's config is known, and finishes it. Encoder delay and the loop
  pre-roll are negative timestamps the muxer hides.
- **Plugins.** The FFmpeg plugin encodes with libavcodec and muxes with
  libavformat through `ffmpeg-sys-next`, producing avcC / AudioSpecificConfig /
  OpusHead `TrackConfig`s; ez-ffmpeg stays for decoding. The test codec
  stores packets losslessly in its own file.

**Tech stack:** Rust 2024 (MSRV 1.89), serde/serde_json, `ffmpeg-sys-next`
8.1 (libavcodec, libavformat, libswscale) in `plugins/ffmpeg`, `rubato`
for audio sample-rate conversion in `celesta-media`, Vitest (TypeScript)
for `packages/exporter`.

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`
(Manifest; Protocol 2 → Handshake, Encoding, Muxing, Deadlines, Errors;
Capability discovery; Settings schema; Selection → Resolving an output, CLI,
MCP and `@celesta.studio/exporter`; Exporter API; Registry and routing;
Transport → the interleaver; Audio; Testing; Phases → P2a). Read it with
this plan. P1's plan (`2026-11-08-media-components-p1.md`) and handoff
(`docs/handoff/2026-11-08-media-decoders.md`) describe what this builds on.

## Global constraints

- No Celesta crate except `celesta-codec-ffmpeg` links FFmpeg. The core's
  tests use `celesta-codec-test` (HANDOFF.md).
- Media plugins speak protocol **2**; `hello`, `describe`, and decoding are
  as P1 built them. P2a's video encoders answer `transports: ["pipe"]` and
  Celesta sends one request at a time (`PluginChild::request`): no ring, no
  send window, no `ring` field in `openVideoEncoder`.
- Component references are `<plugin>/<component>`; the plugin ID `builtin`
  stays reserved. Codec families are the part of a codec string before its
  first `.`, with `avc3` read as `avc1` and `hev1` as `hvc1`.
- Times are integers in a track's timescale: video's is the frame rate's
  numerator (a frame lasts the denominator), audio's is the negotiated
  sample rate. `delay` and `leadingAudio` count sample frames.
- Every `openVideoEncoder` and video `Track` carries
  `{"primaries":"bt709","transfer":"bt709","matrix":"bt709","range":"limited"}`.
  P2a's encoders do not act on it (deviation 8): the GPU still converts
  with BT.601 and FFmpeg's encoders keep today's conversion and tags per
  codec, so decoded output can be compared with `main`'s.
- Celesta offers pixel formats best first: `yuv420p`, `yuv420p10` when the
  renderer `supports_yuv420p_readback()` and is not `is_software()` (today's
  `ColorConversion::Auto` rule), then `rgba16` (straight alpha), then
  `rgba8`. `nv12` and `p010` exist in `PixelFormat` but are never offered
  (P2c). An encoder answering `alpha: true` must have chosen `rgba16`.
- Audio is interleaved `f32` LE, 1 or 2 channels, sent in chunks of
  `1024 × 16` sample frames. `LOOP_LEAD_IN_FRAMES = 16 × 1024` is sent only
  to muxers declaring `trimsLeadingAudio`, as `leadingAudio`.
- The interleaver keeps up to **256 MB** of video packets in memory, then
  spills to a temporary file beside the output, which it removes.
- Deadlines (from the spec's table): `describe` 30 s; the three `open`s
  60 s; `encodeVideoFrame`, `encodeAudio`, `writePackets` 60 s;
  `finishVideoEncoder` and `finishAudioEncoder` 10 min; `finishMuxer` 1 h;
  `abortEncoder` and `abortMuxer` 5 s, after which the process is killed.
- Removed from the CLI, MCP, and TS: `--output-format`, `--crf`,
  `--bit-depth`, `--color-conversion`, and `--preset`'s libx264 meaning.
  `--preset <name>` is parsed and fails with `unknown_preset` until P3,
  with the hint `use --video-option preset=<name>` for a libx264 name.
- Stills keep their current path (P4): `--frame`, `--every`, the contact
  sheet, and `StillFormat::from_extension`.
- JS/TS tests use Vitest in TypeScript; Rust tests stay in Rust (AGENTS.md).
- Never amend commits; each task ends in a new commit. Commits are signed
  through 1Password; if signing fails with "failed to fill whole buffer",
  ask the user to unlock 1Password. Each commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Building or testing `plugins/` needs FFmpeg 8:
  `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`
  (Homebrew's default `ffmpeg` is 9 and breaks the build).
- **Builds between tasks.** Task 3 adds `Nv12` and `P010` to
  `PixelFormat`, which `celesta-codec-test` and `celesta-codec-ffmpeg`
  match exhaustively, so those two do not compile from Task 3 until Tasks 8
  and 12 rewrite them. Task 7 removes protocol 1's encoder messages and the
  old types, so `celesta-exporter`, `celesta-editor`, and `celesta-mcp` do
  not compile during Tasks 7–8; Task 9 restores the exporter and the
  editor, Task 10 the CLI, Task 11 `celesta-mcp`. Each task builds and
  tests only the crates it names; from Task 11 on, `cargo test --workspace`
  passes, and from Task 13 on, the plugins workspace too.

## Deviations from the spec (recorded in Task 15's spec edit)

Numbered after P1's seven.

8. **P2a's encoders do not apply `Color`.** `openVideoEncoder` carries the
   spec's BT.709 limited `Color`, but FFmpeg's libx264 converts RGBA input
   with libswscale's default (BT.601, untagged) and its ProRes and VP9 keep
   today's BT.709 conversion and tags, and the GPU's `yuv420p` stays BT.601.
   P2c switches the conversion and tags everywhere at once, so P2a's output
   decodes identically to `main`'s.
9. **Audio sample-rate conversion uses `rubato`.** The spec's "shared
   resampler" (`docs/handoff/2026-11-07-shared-resampling.md`) resamples
   images, not audio. `celesta_media::resample_audio` wraps `rubato`'s
   sinc resampler for the rare mix whose rate an encoder refuses (Opus
   takes 48 kHz only).
10. **A `describe` from the export reuses the export's process.**
    `resolve_output` takes a `describe` callback; the export's
    `ExportSession` answers it from the process it starts for that plugin
    (and keeps), and `MediaRegistry::describe` (start, ask, stop) serves
    the CLI's `--list-components --check`, `--describe`, and MCP. Both
    write the same cache, which lives in the `MediaRegistry` and is dropped
    with it on Reload Plugins.
11. **The mix is encoded in one pass once it is done.** Chunks of
    `1024 × 16` frames go out back to back after the frame during which the
    mix arrived (a project) or after the render (a React entry), rather than
    interleaved with later frames. The muxer opens right after.
12. **Progress labels come from an `ExportProgress::Output` event**, sent
    once `resolve_output` succeeds and before rendering, since the
    dashboard starts before any plugin has been described. `--json-progress`
    gains the event `output`.
13. **`lpcm` codec strings name the sample format** (`lpcm.s24le`,
    `lpcm.f32le`), so a muxer from another plugin knows the layout; the
    family stays `lpcm`.
14. **`MediaError`'s variant names stay** (`Codec`, `CodecUnavailable`,
    `CodecStopped`); only their messages say "media plugin". Renaming them
    would touch every crate for no behavior change.
15. **`InstalledPlugins::media()` stays one `MediaPlugin` per plugin**
    (P1's deviation 3, kept); `MediaRegistry::components()` gives the spec's
    one `MediaComponentInfo` per component.
16. **Export processes are started by the same factory as decoding ones**,
    with a `ProviderUse` telling it the purpose: `Describe` and `Export`
    processes are offered no shared memory in P2a (P2b gives `Export` the
    ring); `Decode` keeps #775's slot.
17. **`--loop` is accepted for every video output.** It is a no-op for a
    muxer without `trimsLeadingAudio` (FFmpeg's WebM), as the spec's Audio
    section says, instead of today's usage error outside MP4.
18. **A software renderer is offered no YUV format.** The spec offers the
    YUV formats whenever `supports_yuv420p_readback()`; P2a keeps today's
    `ColorConversion::Auto` rule and leaves them out when `is_software()`,
    since lavapipe and WARP convert slower than libswscale. That keeps
    `main`'s output on CI's software renderers.

## Decisions the spec left to this plan

- The provider traits' signatures (Task 3), the serve-side dispatch
  (Task 3), the `describe` cache key `(directory, manifest version,
  executable size and mtime)` (Task 4).
- The interleaver's merge rule and spill layout (Task 6): a raw byte file
  with the packet index kept in memory.
- The export loop (Task 9): video opened before the first frame; for a
  project the mix thread's result is polled after each frame; for a React
  entry everything is staged and the muxer opens after the render.
- FFmpeg's `TrackConfig`s (Tasks 12–13): avcC built from libx264's Annex B
  extradata, packets converted to length-prefixed NAL units; AAC's and
  Opus's extradata as the description; VP9's codec string
  `vp09.00.10.08` (libvpx reports no level; the WebM muxer derives vpcC
  itself); `delay` hidden through negative packet times (MP4/MOV edit lists
  from libavformat) and `codecpar->initial_padding` (WebM `CodecDelay`).
- The test codec's components and file format version 2 (Task 8).
- The end-to-end check (Task 15): decoded frames and samples compared with
  `cmp`, expected identical; see that task for where identity is realistic
  and what is recorded otherwise.
- The P2a performance measurement (Task 15): `codec-bench` encode at 1080p
  and 4K and the spec's export set (300 frames at 1080p and 4K with
  `ultrafast` and `medium`, ProRes 4444 at 4K) against `main`, pipe to pipe.

## Review focus

The tests that pin each of these are in the task named.

1. **A plugin whose `describe` disagrees with its manifest**: components
   the answer adds are ignored with one stderr warning; components it
   leaves out are unavailable with "the plugin did not describe it"
   (Task 4, `described_components_follow_the_manifest`).
2. **Cancel while frames are being encoded** leaves no output file and no
   spill file, and does not wait on the plugin beyond the abort deadline
   (Task 9, `cancel_during_encoding_leaves_no_files`).
3. **A React entry with no audio clips exported with `--loop`** gets a
   video-only file and no error: the pre-roll needs a mix (Task 9,
   `a_silent_react_entry_with_loop_has_no_audio_track`).
4. **More video packets than fit in memory** spill to a file beside the
   output, come back byte for byte, and the file is gone afterwards
   (Task 6, `spilled_packets_come_back_identical_and_the_file_goes`).
5. **The mix finishing after the last frame** (a project whose audio is
   slower than its video): every packet is staged, then muxed in decode
   order (Task 9, `frames_before_the_mix_are_staged_then_muxed_in_order`).

---

### Task 1: The manifest's encoder and muxer keys (`celesta-plugin-host`)

**Files:**
- Modify: `crates/plugin-host/src/media.rs` (`MediaManifest`, `parse_media`, `MediaPlugin`, `printed_manifest`, tests)
- Modify: `crates/plugin-host/src/lib.rs` (re-exports)
- Modify: `crates/exporter/src/media.rs:14-18` (`decoders: plugin.media.decoders`, so the exporter keeps compiling)
- Test: `crates/plugin-host/src/media.rs` tests

**Interfaces:**
- Produces, in `celesta_plugin_host`:
  ```rust
  pub struct MediaManifest { pub decoders: Vec<MediaDecoderEntry>, pub video_encoders: Vec<MediaVideoEncoderEntry>,
                             pub audio_encoders: Vec<MediaAudioEncoderEntry>, pub muxers: Vec<MediaMuxerEntry> }
  pub struct MediaVideoEncoderEntry { pub id: String, pub name: String, pub codecs: Vec<String>, pub input_formats: Option<Vec<String>> }
  pub struct MediaAudioEncoderEntry { pub id: String, pub name: String, pub codecs: Vec<String> }
  pub struct MediaMuxerEntry { pub id: String, pub name: String, pub extensions: Vec<String>, pub video: Vec<String>,
                               pub audio: Option<Vec<String>>, pub default_video: Option<String>, pub default_audio: Option<String>,
                               pub trims_leading_audio: bool, pub sequence: bool }
  pub fn codec_family(codec: &str) -> String;   // before the first '.', then avc3→avc1, hev1→hvc1
  pub struct MediaPlugin { pub id, pub version, pub executable, pub directory, pub origin, pub media: MediaManifest }  // `media` replaces `decoders`
  ```
- Reading rules, each a `BadComponent` that skips the one entry: an id that
  fails `is_valid_id` or repeats an earlier id of the same kind; an encoder
  with no `codecs`; a muxer with no `extensions` or no `video`; a
  `defaultVideo` not in `video`, or a `defaultAudio` not in `audio`.
  `codecs`, `video`, and `audio` are stored through `codec_family`;
  extensions lower-cased without a dot, as decoders' are. `inputFormats`
  is kept as the strings given (`celesta-media` maps them to `PixelFormat`
  and ignores unknown names).

- [ ] **Step 1: Write failing tests** in `media.rs`'s tests, on the existing manifest helpers:
  - `reads_encoders_and_muxers`: the spec's Manifest example lists three video encoders, three audio encoders, and three muxers; `muxers[0]` has `extensions == ["mp4","m4v"]`, `default_video == Some("avc1")`, `trims_leading_audio`, and `muxers[2].trims_leading_audio == false`, `sequence == false`.
  - `codec_families_are_normalized`: `"codecs": ["avc3", "hev1.1.6.L93"]` reads as `["avc1", "hvc1"]`; `codec_family("mp4a.40.2") == "mp4a"`.
  - `a_bad_encoder_is_skipped_alone`: `videoEncoders: [{"id":"ok","name":"OK","codecs":["avc1"]}, {"id":"none","name":"No codecs","codecs":[]}]` keeps `ok` and adds one `BadComponent` whose reason contains `"codecs"`.
  - `a_muxer_default_must_be_accepted`: `{"id":"m","name":"M","extensions":["x"],"video":["avc1"],"defaultVideo":"vp09"}` is skipped with a reason containing `"defaultVideo"`.
  - `ids_are_unique_per_kind`: a decoder and a video encoder may share the id `ffmpeg`; two muxers with id `mp4` keep the first.
  - `the_sibling_prints_every_kind`: the fake `--manifest` program's JSON with all four keys lists them on the `MediaPlugin`.
- [ ] **Step 2:** `cargo test -p celesta-plugin-host media` fails.
- [ ] **Step 3: Implement.** `parse_media` reads the three new lists the way it reads `decoders` (each entry from its own `serde_json::Value`), applying the rules above.
- [ ] **Step 4:** `cargo test -p celesta-plugin-host` and `cargo build -p celesta-exporter` pass.
- [ ] **Step 5: Commit:** `feat(plugin-host): encoder and muxer entries in media manifests`.

### Task 2: The settings schema and validation (`celesta-media`)

**Files:**
- Create: `crates/media/src/settings.rs`
- Modify: `crates/media/src/lib.rs` (`pub mod settings;` and re-exports of `SettingValue`, `Settings`, `SettingField`, `SettingKind`, `SelectOption`, `InvalidSetting`)
- Test: `crates/media/src/settings.rs` tests

**Interfaces:**
- Produces:
  ```rust
  #[serde(untagged)] pub enum SettingValue { String(String), Number(f64), Boolean(bool) }   // Display: the string, the number ({} of f64), true/false
  pub type Settings = BTreeMap<String, SettingValue>;
  pub struct SettingField { pub key: String, pub label: String, pub default: SettingValue,
      #[serde(flatten)] pub kind: SettingKind, pub description: Option<String>, pub group: Option<String>,
      #[serde(default)] pub advanced: bool, #[serde(default)] pub visible_when: BTreeMap<String, Vec<SettingValue>> }
  #[serde(tag = "type", rename_all = "camelCase")]
  pub enum SettingKind { Select { options: Vec<SelectOption> },
                         Number { min: Option<f64>, max: Option<f64>, step: Option<f64>, #[serde(default)] integer: bool, unit: Option<String> },
                         Boolean, String }
  pub struct SelectOption { pub value: String, pub label: String }
  pub struct InvalidSetting { pub key: String, pub message: String }
  pub fn validate(schema: &[SettingField], given: &Settings) -> Result<Settings, InvalidSetting>;
  pub fn parse_setting(schema: &[SettingField], key: &str, text: &str) -> Result<SettingValue, InvalidSetting>;  // by the field's type: number/boolean parse, else String
  pub fn is_visible(field: &SettingField, settings: &Settings) -> bool;
  pub fn differences<'a>(schema: &'a [SettingField], settings: &'a Settings) -> Vec<(&'a SettingField, &'a SettingValue)>;  // visible fields whose value is not the default, schema order
  ```
- `visibleWhen` on the wire maps a key to one value or a list; the Rust
  side always holds a list (a custom deserializer). `validate` returns the
  full map: given values checked, missing visible keys filled with their
  defaults, hidden fields left out. Visibility is evaluated against the
  given values and defaults together. Messages: `"unknown setting"`,
  `"must be a string"` / `"a number"` / `"true or false"`,
  `"must be a whole number"`, `"must be at least {min}"` / `"at most {max}"`,
  `"must be one of {values}"` (comma-separated), and
  `"applies only when {key} is {value}"` for a given hidden field.

- [ ] **Step 1: Write failing tests** using the spec's five example fields (`rateControl`, `crf`, `bitrate`, `tune`, `faststart`) as the schema:
  - `the_spec_fields_round_trip_json`: each example JSON deserializes to a `SettingField` and serializes back to the same JSON (`visibleWhen` written as a list).
  - `defaults_fill_missing_keys`: `validate(schema, &{})` gives `rateControl = "crf"`, `crf = 18`, `tune = ""`, `faststart = true`, and no `bitrate`.
  - `a_hidden_field_may_not_be_given`: `{"bitrate": 4000000}` fails with key `bitrate` and the message `"applies only when rateControl is crf"`.
  - `switching_rate_control_shows_bitrate`: `{"rateControl":"vbr"}` gives `bitrate = 8000000` and no `crf`.
  - `numbers_are_ranged_and_whole`: `crf = 52` → `"must be at most 51"`; `crf = 17.5` → `"must be a whole number"`; `crf = "18"` → `"must be a number"`.
  - `selects_take_their_options`: `rateControl = "abr"` → `"must be one of crf, vbr"`.
  - `unknown_keys_fail`: `{"quality": 1}` → key `quality`, `"unknown setting"`.
  - `parse_setting_follows_the_type`: `parse_setting(schema, "crf", "20") == Number(20.0)`, `("faststart", "false") == Boolean(false)`, `("tune", "film") == String("film")`, `("crf", "x")` fails with `"must be a number"`.
  - `differences_list_what_is_not_default`: `{"crf": 20, "faststart": true}` → `[(crf, 20)]`.
- [ ] **Step 2:** `cargo test -p celesta-media settings` fails.
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4:** `cargo test -p celesta-media` passes.
- [ ] **Step 5: Commit:** `feat(media): the component settings schema and its validation`.

### Task 3: Protocol 2's encoding and muxing messages, `describe`, and the four traits (`celesta-media`)

**Files:**
- Rename: `crates/media/src/codec.rs` → `crates/media/src/provider.rs`; `crates/media/src/codec/protocol.rs` → `crates/media/src/protocol.rs`; `crates/media/src/codec/process.rs` → `crates/media/src/process.rs` (`pub mod protocol`, `mod provider`, `mod process`; `celesta_media::protocol::serve` is the program entry point, as the spec names it)
- Modify: `crates/media/src/provider.rs` (traits and types), `crates/media/src/protocol.rs` (dispatch, builders, decoders), `crates/media/src/process.rs` (client impls), `crates/media/src/error.rs`, `crates/media/src/lib.rs`
- Modify: `crates/media/src/tests.rs` and `protocol.rs`'s fakes (add `describe`)
- Test: `crates/media/src/protocol.rs` tests

**Interfaces:**
- Produces, in `celesta_media` (all `Clone, Debug, PartialEq`, serde `camelCase`):
  ```rust
  pub struct SessionId(pub u64);
  pub enum PixelFormat { Rgba8, Rgba16, Yuv420p, Yuv420p10, Nv12, P010 }   // wire: rgba8 rgba16 yuv420p yuv420p10 nv12 p010; FromStr/Display the same
  pub struct Color { pub primaries: String, pub transfer: String, pub matrix: String, pub range: String }
  impl Color { pub fn bt709_limited() -> Self }
  pub struct TrackConfig { pub codec: String, pub description: Option<Vec<u8>> }   // description base64 on the wire
  pub struct SideData { pub id: u32, pub bytes: Vec<u8> }
  pub struct Packet { pub pts: i64, pub dts: i64, pub duration: i64, pub key: bool, pub bytes: Vec<u8>, pub additional: Vec<SideData> }
  pub struct Encoded { pub packets: Vec<Packet>, pub config: Option<TrackConfig> }
  pub struct VideoEncoderRequest<'a> { pub component: &'a str, pub settings: &'a Settings, pub width: u32, pub height: u32,
                                       pub frame_rate: Rational, pub timescale: u32, pub offered: &'a [PixelFormat], pub color: &'a Color }
  pub struct VideoEncoderOpened { pub session: SessionId, pub input: PixelFormat, pub alpha: bool }
  pub struct AudioEncoderRequest<'a> { pub component: &'a str, pub settings: &'a Settings, pub sample_rate: u32, pub channels: u16 }
  pub struct AudioEncoderOpened { pub session: SessionId, pub sample_rate: u32, pub channels: u16, pub delay: u64 }
  #[serde(tag = "kind", rename_all = "camelCase")]
  pub enum Track { Video { config: TrackConfig, timescale: u32, width: u32, height: u32, frame_rate: Rational, color: Color, alpha: bool },
                   Audio { config: TrackConfig, timescale: u32, sample_rate: u32, channels: u16, delay: u64 } }
  pub struct MuxerRequest<'a> { pub component: &'a str, pub settings: &'a Settings, pub output: &'a Path, pub tracks: &'a [Track], pub leading_audio: u64 }
  pub struct TrackPacket { pub track: usize, pub packet: Packet }
  pub enum ComponentKind { Decoder, VideoEncoder, AudioEncoder, Muxer }
  pub enum SampleRates { Any, Only(Vec<u32>) }   // wire: "any" | [48000, …]
  pub struct ComponentDescription { pub kind: ComponentKind, pub id: String, pub available: bool, pub reason: Option<String>,
      pub settings: Vec<SettingField>, pub input_formats: Vec<PixelFormat>, pub transports: Vec<String>,
      pub sample_rates: Option<SampleRates>, pub channels: Vec<u16> }

  pub trait Decoder {   // the three decoding methods, moved here from MediaProvider in Task 7
      fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError>;
      fn decode_video(&mut self, decoder: &str, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError>;
      fn decode_audio(&mut self, decoder: &str, path: &Path, sample_rate: u32, channels: u16) -> Result<AudioBuffer, MediaError>;
  }
  pub trait VideoEncoder {
      fn open_video(&mut self, request: &VideoEncoderRequest<'_>) -> Result<VideoEncoderOpened, MediaError>;
      fn encode_video(&mut self, session: SessionId, pts: i64, frame: Vec<u8>) -> Result<Encoded, MediaError>;
      fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError>;
      fn abort_video(&mut self, session: SessionId);
  }
  pub trait AudioEncoder {
      fn open_audio(&mut self, request: &AudioEncoderRequest<'_>) -> Result<AudioEncoderOpened, MediaError>;
      fn encode_audio(&mut self, session: SessionId, pts: i64, samples: &[f32]) -> Result<Encoded, MediaError>;   // interleaved; frames = len / channels
      fn finish_audio(&mut self, session: SessionId) -> Result<Encoded, MediaError>;
      fn abort_audio(&mut self, session: SessionId);
  }
  pub trait Muxer {
      fn open_muxer(&mut self, request: &MuxerRequest<'_>) -> Result<SessionId, MediaError>;
      fn write_packets(&mut self, session: SessionId, packets: &[TrackPacket]) -> Result<(), MediaError>;
      fn finish_muxer(&mut self, session: SessionId) -> Result<Vec<PathBuf>, MediaError>;
      fn abort_muxer(&mut self, session: SessionId) -> Vec<PathBuf>;
  }
  pub trait MediaProvider: Send {
      fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError>;
      fn decoder(&mut self) -> Option<&mut dyn Decoder> { None }
      fn video_encoder(&mut self) -> Option<&mut dyn VideoEncoder> { None }
      fn audio_encoder(&mut self) -> Option<&mut dyn AudioEncoder> { None }
      fn muxer(&mut self) -> Option<&mut dyn Muxer> { None }
      // until Task 7, unchanged: probe, decode_video, decode_audio, check_encoder, open_encoder, encode_frame, finish_encoder, abort_encoder, mux_audio
  }
  MediaError::InvalidSettings { key: String, message: String }          // code "invalidSettings"; "{key}: {message}"
  MediaError::ComponentUnavailable { component: String, reason: String } // code "componentUnavailable"; "{component} is unavailable: {reason}"
  ```
- `MediaProcess` implements `VideoEncoder`, `AudioEncoder`, `Muxer`, and
  `MediaProvider::describe`; its accessors return `Some(self)`. Deadlines
  are the Global constraints' table (`DESCRIBE_TIMEOUT`, `OPEN_TIMEOUT`,
  `ENCODE_TIMEOUT`, `FINISH_ENCODER_TIMEOUT`, `FINISH_MUXER_TIMEOUT`,
  `ABORT_TIMEOUT`). Paths in `openMuxer` go through `absolute`, as decoding
  paths do.
- On the wire, exactly the spec's tables. Packet bytes: the header lists
  each packet with `bytes` as a length and `additional: [{id, bytes:
  length}]`; the payload is each packet's bytes, then its additional data in
  order, packet after packet (`pack_packets(&[Packet]) -> (Vec<Value>, Vec<u8>)`,
  `unpack_packets(&[Value], &[u8]) -> Result<Vec<Packet>, MediaError>`; a
  payload shorter than the lengths is `malformed`). `encodeAudio` carries
  `frames` and the `f32` LE payload; `samples.len()` must be a whole number
  of frames. `settings` is the `Settings` map as JSON.
- Serve-side dispatch in `protocol::answer`: `describe` →
  `provider.describe()`; the three `open`/`encode`/`finish` groups go to
  `provider.video_encoder()`, `audio_encoder()`, or `muxer()`, and a
  provider without that kind answers `error` with the message
  `"this plugin has no {kind}"`; `abortEncoder` tries the video encoder's
  `abort_video` and the audio encoder's `abort_audio` with the session
  (one of them owns it). The decoding and legacy arms are untouched until
  Task 7.

- [ ] **Step 1: Write failing tests** in `protocol.rs`'s tests, on a fake implementing the four traits that records calls and answers canned values:
  - `describe_round_trips`: a `describe` request served to the fake comes back as the two `ComponentDescription`s it holds (one available video encoder with two settings and `input_formats == [Yuv420p, Rgba8]`, `transports == ["pipe"]`; one unavailable muxer with `reason`), equal after `decode_described`.
  - `video_encoding_round_trips`: `open_video_request` carries `offered == ["yuv420p","rgba8"]` and `color.matrix == "bt709"`; the opened answer decodes to `{session: 7, input: Yuv420p, alpha: false}`; an `encodeVideoFrame` with a 16-byte payload reaches the fake with `pts == 3003` and those bytes, and its answer's two packets (one with `additional: [{id: 1, bytes: 3 bytes}]`) and `config` (`avc1.640028`, a 7-byte description) come back equal; `finishVideoEncoder` returns the fake's last packet.
  - `audio_encoding_round_trips`: `openAudioEncoder{48000, 2}` answers `{sample_rate: 48000, channels: 2, delay: 1024}`; `encodeAudio` with 2048 samples reaches the fake as 1024 frames at `pts == -16384`.
  - `muxing_round_trips`: `openMuxer` with a video and an audio `Track` and `leading_audio == 16384` arrives intact (the audio track's `delay == 1024`); `writePackets` with a `TrackPacket{track: 1, …}` arrives with the bytes; `finishMuxer` answers `files == [output]`; `abortMuxer` answers the fake's `files`.
  - `a_provider_without_the_kind_refuses`: a fake whose `muxer()` is `None` answers `openMuxer` with an `error` whose message is `"this plugin has no muxer"`.
  - `packets_pack_back_to_back`: `pack_packets` of two packets (3 and 2 main bytes, the first with 4 additional bytes) gives a 9-byte payload in the order main₁, additional₁, main₂, and `unpack_packets` inverts it; a payload one byte short is `malformed`.
  - `new_error_codes_round_trip`: `InvalidSettings` and `ComponentUnavailable` survive `decode_error(&encode_error(..))`.
- [ ] **Step 2:** `cargo test -p celesta-media protocol` fails.
- [ ] **Step 3: Implement** the renames, types, traits, dispatch, request builders (`describe_request`, `open_video_request`, `encode_video_request`, `finish_video_request`, `open_audio_request`, `encode_audio_request`, `finish_audio_request`, `abort_encoder_request`, `open_muxer_request`, `write_packets_request`, `finish_muxer_request`, `abort_muxer_request`) and answer decoders, and `MediaProcess`'s impls. Add `describe` to every fake in `crates/media`.
- [ ] **Step 4:** `cargo test -p celesta-media` passes.
- [ ] **Step 5: Commit:** `feat(media): protocol 2 encoding, muxing, and describe`.

### Task 4: Component listing, `ProviderUse`, and the `describe` cache (`celesta-media`)

**Files:**
- Modify: `crates/media/src/registry.rs`, `crates/media/src/provider.rs` (`ProviderFactory`), `crates/media/src/process.rs` (`factory`), `crates/media/src/backend.rs` (`Processes::get` passes `ProviderUse::Decode`), `crates/media/src/lib.rs`
- Modify: `crates/exporter/src/media.rs` (`RegisteredPlugin` construction)
- Test: `crates/media/src/registry.rs` tests, `crates/media/src/tests.rs`

**Interfaces:**
- Consumes: Task 1's manifest entries; Task 3's `ComponentDescription`, `ComponentKind`.
- Produces:
  ```rust
  pub enum ProviderUse { Decode, Describe, Export }
  pub type ProviderFactory = Arc<dyn Fn(&PluginKillSwitch, ProviderUse) -> Result<Box<dyn MediaProvider>, MediaError> + Send + Sync>;
  // MediaProcess::factory: Decode → FrameTransport::from_environment(); Describe and Export → FrameTransport::Pipe
  pub struct RegisteredPlugin { pub id: String, pub version: String, pub origin: MediaOrigin, pub directory: PathBuf,
                                pub executable: Option<PathBuf>, pub media: MediaManifest, pub factory: ProviderFactory }
  pub enum ComponentEntry { Decoder(MediaDecoderEntry), VideoEncoder(MediaVideoEncoderEntry), AudioEncoder(MediaAudioEncoderEntry), Muxer(MediaMuxerEntry) }
  pub struct MediaComponentInfo { pub reference: ComponentRef, pub kind: ComponentKind, pub name: String, pub origin: MediaOrigin,
                                  pub plugin: usize, pub plugin_version: String, pub directory: PathBuf, pub entry: ComponentEntry }
  pub struct PluginDescription { pub components: Vec<ComponentDescription> }   // one per manifest component, manifest order
  impl MediaRegistry {
      pub fn plugins(&self) -> &[RegisteredPlugin];
      pub fn plugin_index(&self, id: &str) -> Option<usize>;
      pub fn components(&self) -> Vec<MediaComponentInfo>;            // search order; within a plugin decoders, video encoders, audio encoders, muxers
      pub fn component(&self, reference: &ComponentRef, kind: ComponentKind) -> Option<MediaComponentInfo>;
      pub fn describe(&self, plugin: usize, kill: &PluginKillSwitch) -> Arc<PluginDescription>;   // cached, else factory(Describe) → describe_with → drop
      pub fn describe_with(&self, plugin: usize, provider: &mut dyn MediaProvider) -> Arc<PluginDescription>;
  }
  ```
- The cache: `Mutex<HashMap<usize, (DescribeKey, Arc<PluginDescription>)>>`
  with `DescribeKey { directory: PathBuf, version: String, executable: Option<(u64, SystemTime)> }`
  read from the plugin and the executable's current metadata at each call;
  a key that differs misses. Only a successful answer is cached. On
  failure every manifest component is `available: false` with
  `reason: error.to_string()`. Merge: an answered component matching a
  manifest `(kind, id)` is taken as answered (its `available`, `reason`,
  `settings`, …); a manifest component not answered is unavailable with
  `reason: "the plugin did not describe it"`; answered components the
  manifest lacks are dropped and named once on stderr
  (`celesta: {plugin} described components its manifest does not declare: {ids}`),
  once per plugin per registry.

- [ ] **Step 1: Write failing tests** in `registry.rs`'s tests, with in-process fakes whose factory counts starts and takes the `ProviderUse`:
  - `components_list_every_kind_in_order`: a plugin with one of each kind lists four `MediaComponentInfo`s in the order decoder, video encoder, audio encoder, muxer, with `reference == "p/<id>"`.
  - `describe_is_cached_until_the_program_changes`: two `describe` calls start the fake once; touching the fake's executable file (a temp file; rewrite its contents so size changes) makes the third call start it again.
  - `a_failed_describe_is_not_cached`: a fake whose `describe` fails twice then succeeds: three calls, three starts, the first two answers unavailable with the error text.
  - `described_components_follow_the_manifest`: the manifest declares video encoders `a` and `b`; the fake describes `a` and `c`; the result has `a` as answered, `b` unavailable with `"the plugin did not describe it"`, and no `c`.
  - `describe_with_fills_the_same_cache`: `describe_with(0, &mut provider)` then `describe(0, kill)` starts nothing.
  - `factories_learn_their_use`: `describe` passes `ProviderUse::Describe`; `MediaBackend::probe` passes `Decode`.
- [ ] **Step 2:** `cargo test -p celesta-media registry` fails.
- [ ] **Step 3: Implement.** Update `exporter/src/media.rs` to fill `version`, `directory`, `executable: Some(plugin.executable)`, and `media: plugin.media`.
- [ ] **Step 4:** `cargo test -p celesta-media` passes and `cargo build -p celesta-exporter` succeeds.
- [ ] **Step 5: Commit:** `feat(media): list components and cache describe`.

### Task 5: `OutputChoice`, the new error codes, and `resolve_output` (`celesta-exporter`)

**Files:**
- Create: `crates/exporter/src/resolve.rs`
- Modify: `crates/exporter/src/options.rs` (add the choice types; the old ones stay until Task 9), `crates/exporter/src/error.rs`, `crates/exporter/src/report.rs` (`classify`), `crates/exporter/src/render.rs:373-388` (`validate_output` constructs the new `UnsupportedOutput` with `extensions` = `VideoFormat::VARIANTS`' names, so the crate keeps compiling until Task 9), `crates/exporter/src/lib.rs`
- Test: `crates/exporter/src/resolve.rs` tests, `crates/exporter/src/report.rs` tests

**Interfaces:**
- Consumes: Task 2's `settings`, Task 4's registry.
- Produces, in `celesta_exporter`:
  ```rust
  pub enum SettingInput { Text(String), Value(SettingValue) }   // Text from the CLI's key=value, parsed by the field's type at resolve
  pub struct EncoderChoice { pub component: Option<ComponentRef>, pub options: BTreeMap<String, SettingInput> }   // Default: none
  pub enum AudioChoice { Encode(EncoderChoice), None }          // Default: Encode(default)
  pub struct OutputChoice { pub preset: Option<String>, pub muxer: Option<ComponentRef>, pub video: EncoderChoice, pub audio: AudioChoice }   // Default
  pub struct ResolvedEncoder { pub plugin: usize, pub reference: ComponentRef, pub name: String, pub codecs: Vec<String>, pub settings: Settings, pub schema: Vec<SettingField> }
  pub struct ResolvedMuxer { pub plugin: usize, pub reference: ComponentRef, pub name: String, pub entry: MediaMuxerEntry, pub settings: Settings, pub schema: Vec<SettingField> }
  pub struct ResolvedOutput { pub muxer: ResolvedMuxer, pub video: ResolvedEncoder, pub audio: Option<ResolvedEncoder> }
  impl ResolvedOutput { pub fn plan(&self) -> OutputPlan }
  pub struct EncoderPlan { pub component: ComponentRef, pub name: String, pub settings: Vec<String> }   // settings: "{Label} {value}[ {unit}]" for numbers and strings, the option label for selects, "{Label}" / "No {Label}" for booleans, from settings::differences
  pub struct OutputPlan { pub muxer: ComponentRef, pub muxer_name: String, pub video: EncoderPlan, pub audio: Option<EncoderPlan> }
  pub fn encoding_label(plan: &EncoderPlan) -> String;   // name and settings joined with " · "
  pub fn resolve_output(choice: &OutputChoice, output: &Path, registry: &MediaRegistry,
                        describe: &mut dyn FnMut(usize) -> Arc<PluginDescription>) -> Result<ResolvedOutput, ExportError>;
  pub const X264_PRESETS: [&str; 10];   // ultrafast … veryslow, placebo
  ExportError::UnknownPreset { name: String }                                        // "unknown preset `{name}`"
  ExportError::UnsupportedOutput { path: PathBuf, extensions: Vec<String> }          // replaces the old variant: "no muxer writes .{ext} files" / "no media plugin offers a muxer"
  ExportError::IncompatibleComponents { component: ComponentRef, muxer: ComponentRef, reason: String }
  ExportError::ComponentUnavailable { component: ComponentRef, reason: String }
  ExportError::InvalidOption { component: ComponentRef, key: String, message: String }   // "{component}: {key} {message}"
  ```
- Report codes: `unknown_preset` (hint `"use --video-option preset=<name>"`
  when `name` is in `X264_PRESETS`, else none; MCP style: `videoEncoder.options.preset`),
  `unsupported_output` (hint names the extensions, or `NO_DECODER_HINT` when
  there are none), `incompatible_components`, `component_unavailable`,
  `invalid_option`. A plugin's own refusal at `open` maps too:
  `ExportError::Codec { source: MediaError::InvalidSettings { .. }, .. }` →
  `invalid_option`, `MediaError::ComponentUnavailable { .. }` →
  `component_unavailable`. `invalid_crf` and `encoder_unavailable` go
  (Task 9 removes the variants; the arms go here).
- Steps, as the spec's Resolving an output: (1) a `preset` is always
  `UnknownPreset` in P2a; (2) muxer by `extensions` (case-insensitive),
  ranked bundled → user by search order, project never by default; an
  explicit muxer may be any installed one; (3) video encoder: explicit, or
  the first available encoder producing `default_video`, ranked muxer's
  plugin → bundled → user; an explicit encoder none of whose `codecs` the
  muxer's `video` accepts is `IncompatibleComponents`; (4) audio likewise
  against `default_audio`, `None` for `AudioChoice::None` or a muxer with
  no `audio`; an explicit audio encoder for such a muxer is
  `IncompatibleComponents`; (5) each chosen component's description must be
  `available` (`describe(plugin)`), else `ComponentUnavailable` with its
  reason; a missing plugin is `"not installed"`; (6) options: `Text` parsed
  with `parse_setting`, `Value` as given, then `validate`; failures are
  `InvalidOption`. Muxer settings come from the muxer's own
  `EncoderChoice`-less options: the muxer takes defaults only in P2a.

- [ ] **Step 1: Write failing tests** in `resolve.rs`, with a registry of in-process fakes: `b` (Bundled: muxer `mp4` taking `avc1`/`mp4a`, encoders `x264` → `avc1` with the spec's five settings, `aac` → `mp4a`), `u` (User: muxer `mp4`, encoder `h264` → `avc1`), `p` (Project: muxer `mp4`, encoders for both). Describes are canned.
  - `defaults_follow_the_extension_and_origin`: `film.MP4` → muxer `b/mp4`, video `b/x264`, audio `b/aac`; `film.mkv` → `UnsupportedOutput { extensions: ["mp4"] }`.
  - `the_muxers_plugin_ranks_first`: with `--muxer u/mp4` and no encoder, video is `u/h264`.
  - `project_components_are_never_defaults`: with only `p` installed, `film.mp4` is `UnsupportedOutput`; naming `p/mp4` and `p/…` explicitly works.
  - `an_incompatible_encoder_is_refused`: `--video-encoder b/aac` → `IncompatibleComponents`.
  - `audio_none_and_muxers_without_audio`: `AudioChoice::None` gives `audio: None`; a muxer without `audio` does too; naming an audio encoder for it is `IncompatibleComponents`.
  - `an_unavailable_component_names_its_reason`: `b/x264` described `available: false, reason: "this FFmpeg build has no libx264"` → `ComponentUnavailable` with that reason.
  - `options_are_parsed_and_validated`: `Text("20")` for `crf` resolves to `Number(20)`; `Text("x")` → `InvalidOption { key: "crf", message: "must be a number" }`; `Value(Number(4000000))` for `bitrate` → the hidden-field message.
  - `an_x264_preset_name_gets_the_video_option_hint`: `preset: Some("ultrafast")` → `UnknownPreset`, and `report::classify` gives `("unknown_preset", Some("use --video-option preset=ultrafast"))`; `Some("YouTube")` gives no hint.
  - `a_plugins_refusal_at_open_is_an_invalid_option` (in `report.rs`'s tests): `ExportError::Codec { stage: "video encoding", source: MediaError::InvalidSettings { key: "alpha".into(), message: "needs rgba16".into() } }` classifies as `invalid_option`, and `MediaError::ComponentUnavailable { .. }` as `component_unavailable`.
  - `the_plan_names_components_and_changed_settings`: with `crf = 20` and `preset = slow`, `encoding_label(&plan.video) == "H.264 (x264) · Slow · CRF 20"` (schema order: preset before crf in the fake's schema).
- [ ] **Step 2:** `cargo test -p celesta-exporter resolve` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-exporter` passes.
- [ ] **Step 5: Commit:** `feat(exporter): resolve an output choice into components`.

### Task 6: The interleaver and its spill (`celesta-exporter`)

**Files:**
- Create: `crates/exporter/src/interleave.rs`
- Modify: `crates/exporter/src/lib.rs` (`mod interleave;`)
- Test: `crates/exporter/src/interleave.rs` tests

**Interfaces:**
- Consumes: Task 3's `Packet`, `TrackPacket`.
- Produces:
  ```rust
  pub(crate) const SPILL_BYTES: usize = 256 << 20;
  pub(crate) const VIDEO_TRACK: usize = 0;  pub(crate) const AUDIO_TRACK: usize = 1;
  pub(crate) struct Interleaver { … }
  impl Interleaver {
      pub(crate) fn new(video_timescale: u32, spill_dir: &Path) -> Self;
      pub(crate) fn with_spill_limit(self, bytes: usize) -> Self;          // tests
      pub(crate) fn set_audio_timescale(&mut self, timescale: u32);
      pub(crate) fn push_video(&mut self, packets: Vec<Packet>) -> Result<(), ExportError>;   // ExportError::Io on a spill failure, operation "stage video packets"
      pub(crate) fn finish_video(&mut self);
      pub(crate) fn push_audio(&mut self, packets: Vec<Packet>);
      pub(crate) fn finish_audio(&mut self);      // also for "there is no audio"
      pub(crate) fn take_ready(&mut self) -> Result<Vec<TrackPacket>, ExportError>;
      pub(crate) fn spilled(&self) -> bool;  pub(crate) fn staged_bytes(&self) -> usize;
  }
  ```
- Merge rule: while both tracks have queued packets, release the one with
  the smaller decode time (`dts₁ × timescale₂` against `dts₂ × timescale₁`
  in `i128`; video first on a tie). A queue that is empty and not finished
  stops the merge; a finished track lets the other flow. Before
  `finish_audio` or any `push_audio`, nothing is released.
- Spill: video packets count toward `staged_bytes` by their byte lengths.
  Once a push would take the memory total over the limit, that packet and
  every later one go to the spill file (`tempfile::NamedTempFile::new_in(spill_dir)`,
  made on first use, deleted on drop): the bytes and additional data are
  appended raw and the queue keeps `{ pts, dts, duration, key, offset, len, additional: Vec<(id, offset, len)> }`.
  `take_ready` reads spilled bytes back by offset. The queue stays in
  order because spilled entries only follow in-memory ones.

- [ ] **Step 1: Write failing tests**:
  - `video_waits_for_audio_up_to_its_time`: video packets at dts 0, 1001, 2002 (timescale 30000) and audio at 0, 1024 (timescale 48000, 1024/48000 ≈ 0.0213 s < 1001/30000 ≈ 0.0334 s): `take_ready` gives `[v0, a0, a1024]`; after `finish_audio`, `[v1001, v2002]`.
  - `no_audio_releases_everything`: `finish_audio` first; three video pushes come back in order with `track == 0`.
  - `nothing_moves_before_the_audio_is_known`: video pushed, no audio call → `take_ready` is empty.
  - `audio_waits_for_video_too`: audio at 0 and 48000, video only at 0 (not finished): `[v0, a0]`; `finish_video` → `[a48000]`.
  - `spilled_packets_come_back_identical_and_the_file_goes`: limit 1024 bytes; six 300-byte packets (the fourth with 8 bytes of additional id 1); `spilled()` after the fourth push; `finish_audio`; `take_ready` returns the six packets equal to those pushed; the spill directory holds no file after the interleaver is dropped.
  - `a_spilled_queue_keeps_decode_order`: after spilling, dts values come back strictly increasing.
- [ ] **Step 2:** `cargo test -p celesta-exporter interleave` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-exporter` passes.
- [ ] **Step 5: Commit:** `feat(exporter): interleave packets by decode time with a spill file`.

### Task 7: Removing protocol 1's encoder messages and the old types; the audio resampler (`celesta-media`)

**Files:**
- Modify: `crates/media/src/provider.rs` (move `probe`, `decode_video`, `decode_audio` into `Decoder`; delete `check_encoder`…`mux_audio`, `EncoderId`, `Container`, the old `EncoderSettings`, `MuxRequest`), `crates/media/src/protocol.rs` (decoding arms through `provider.decoder()`; delete the legacy arms, builders, and their tests), `crates/media/src/process.rs` (`impl Decoder for MediaProcess`; `decoder()` returns `Some(self)`), `crates/media/src/backend.rs` (delete `check_encoder`, `open_encoder`, `encode_frame`, `finish_encoder`, `abort_encoder`, `mux_audio`, `encoder`, `encoding`, `encoder_process`; `decoding` uses `provider.decoder()`), `crates/media/src/registry.rs` (delete `legacy_encoder`), `crates/media/src/error.rs`, `crates/media/src/lib.rs`, `crates/media/Cargo.toml` (`rubato = "0.16"`)
- Create: `crates/media/src/resample.rs`
- Test: `crates/media/src/tests.rs` (delete `encoding_uses_the_first_plugin`; add the two below), `crates/media/src/resample.rs` tests

**Interfaces:**
- Produces:
  - `MediaError::NoCodec` and `EncoderUnavailable` are gone. Messages:
    `CodecUnavailable` → `"the media plugin is unavailable: {0}"`,
    `CodecStopped` → `"the media plugin was stopped: {0}"`,
    `malformed` → `"malformed media message: …"`.
  - A decode request to a provider whose `decoder()` is `None` is
    `MediaError::Codec("{plugin} has no decoder")`.
  - `pub fn resample_audio(audio: &AudioBuffer, sample_rate: u32) -> Result<AudioBuffer, MediaError>`:
    the same channels at `sample_rate`, `rubato::SincFixedIn` with
    `SincInterpolationParameters { sinc_len: 256, f_cutoff: 0.95, oversampling_factor: 256, interpolation: Linear, window: BlackmanHarris2 }`,
    the output trimmed to `round(frames × to / from)` frames (the filter's
    delay compensated). The same rate returns a clone.

- [ ] **Step 1: Write failing tests**:
  - `tests.rs`: `a_provider_without_a_decoder_cannot_route`: a fake whose `decoder()` is `None` makes `probe` fail with `Codec` containing `"has no decoder"`; `the_unavailable_message_names_the_media_plugin`: `CodecUnavailable("x").to_string()` starts with `"the media plugin is unavailable"`.
  - `resample.rs`: `resampling_keeps_length_and_tone`: a 1 s 440 Hz stereo sine at 44 100 Hz resampled to 48 000 has 48 000 frames, both channels equal, and its zero crossings count 880 ± 2; `the_same_rate_is_returned_as_is`.
- [ ] **Step 2:** `cargo test -p celesta-media` fails to compile or fails.
- [ ] **Step 3: Implement** the removals and the resampler.
- [ ] **Step 4:** `cargo test -p celesta-media` passes (`celesta-exporter`, `celesta-mcp`, `celesta-editor`, `celesta-codec-test`, and `celesta-codec-ffmpeg` do not compile until their tasks).
- [ ] **Step 5: Commit:** `feat(media)!: drop protocol 1's encoder messages; resample audio for encoders`.

### Task 8: The test codec's four components (`celesta-codec-test`)

**Files:**
- Modify: `crates/codec-test/src/lib.rs` (whole provider), `crates/codec-test/src/format.rs` (format version 2), `crates/codec-test/src/plugin.rs` (manifest), `crates/codec-test/src/main.rs` (`celesta_media::protocol::serve`)
- Create: `crates/codec-test/src/components.rs` (describe, schemas, ids)
- Test: `crates/codec-test/tests/codec.rs` (rewritten), `crates/codec-test/tests/process.rs`, `crates/codec-test/tests/plugin_folder.rs`

**Interfaces:**
- Consumes: Tasks 3, 4, 7.
- Produces:
  ```rust
  pub const VIDEO_ENCODER_ID: &str = "video";  pub const AUDIO_ENCODER_ID: &str = "pcm";  pub const MUXER_ID: &str = "file";
  pub const VIDEO_CODEC: &str = "ctst";       pub const AUDIO_CODEC: &str = "lpcm.f32le";
  pub struct TestMedia { pub extension: Option<String>, pub video: Option<StoredVideo>, pub audio: Option<StoredAudio> }   // `container` is gone
  pub struct StoredAudio { pub sample_rate: u32, pub channels: u16, pub lead_in_frames: usize, pub samples: Vec<f32> }  // played() skips lead_in_frames
  ```
  - Manifest (`plugin.rs`, `registered()`, and the folder manifest):
    decoder `test` (no extensions); video encoder `video`
    (`codecs: ["ctst"]`, `inputFormats` all six); audio encoder `pcm`
    (`codecs: ["lpcm"]`); muxer `file` (`extensions: ["mp4","mov","webm","ctst"]`,
    `video: ["ctst"]`, `audio: ["lpcm"]`, `defaultVideo: "ctst"`,
    `defaultAudio: "lpcm"`, `trimsLeadingAudio: true`).
  - Settings: `video`: `input` select `auto` (default), `rgba8`, `rgba16`,
    `yuv420p`, `yuv420p10`, `nv12`, `p010` (`auto` takes the first offered);
    `alpha` boolean default `false`, advanced (true needs `rgba16` chosen,
    else `InvalidSettings { key: "alpha" }`; an `input` not offered is
    `InvalidSettings { key: "input" }`). `pcm`: `delay` number integer
    0–65536 default 0, advanced; `sampleRate` select `any` (default),
    `48000`, advanced (the answer's rate; `any` echoes the request).
    `file`: none.
  - Video packets: one per frame, `bytes = lz4_flex::block::compress_prepend_size(frame)`,
    `key: true`, `dts == pts`, `duration == frame_rate.denominator`; the
    first answer carries `TrackConfig { codec: "ctst", description: Some(<format name>.as_bytes()) }`.
    Audio packets: one per chunk, `bytes` = the samples as `f32` LE,
    `pts = given − delay`; with `delay > 0` the first packet holds `delay`
    frames of zeros at `pts = −delay`; config `{ codec: "lpcm.f32le", description: None }`.
  - File format version 2 (`format.rs`): `Header { extension: Option<String>, video: Option<VideoHeader>, audio: Option<AudioHeader> }`
    (`AudioHeader.lead_in_frames = delay + leading_audio`); `V` chunks are
    the video packets' bytes as given (already LZ4), written as they arrive;
    audio packet bytes are buffered in `pts` order and written as one
    compressed `A` chunk at finish; `E` ends the file. `abort_muxer`
    removes the file and returns `[]`; `finish_muxer` returns `[output]`.
    The muxer refuses a packet for an unknown track, a video track whose
    config is not `ctst`, or a frame whose decompressed length is not the
    header's (`MediaError::Codec`).
  - The decoder reads version 2 only (version 1 files are `Unsupported`).
- `TestCodecPlugin::in_profile`'s manifest declares all four components.

- [ ] **Step 1: Rewrite the tests** in `tests/codec.rs` against the traits (helpers `open_video`, `encode_frames`, `mux` through `TestCodec`):
  - `stores_every_pixel_format_byte_for_byte`: for each of the six formats, `input` set to it, 3 frames of the right length round-trip through encoder → muxer → `read`, `video.format` and frames equal.
  - `auto_takes_the_first_offered`: offered `[Yuv420p, Rgba8]` → `input == Yuv420p`; `alpha_needs_rgba16`: `alpha: true` with `[Yuv420p]` offered → `InvalidSettings { key: "alpha" }`; with `[Rgba16]` → `opened.alpha`.
  - `pcm_delay_and_pre_roll_are_hidden`: the encoder opened with `delay = 1024` is fed 2048 pre-roll frames at `pts = -2048` and then 8000 frames at `pts = 0`, and the muxer is opened with `leading_audio = 2048`: the encoder's first packet holds 1024 zero frames at `pts == -3072`; the file's `lead_in_frames == 3072`; `played()` has 8000 frames equal to the input.
  - `a_fixed_sample_rate_is_answered`: `sampleRate = "48000"` with a 44 100 Hz request answers 48 000.
  - `an_aborted_mux_leaves_no_file`.
  - `describe_lists_four_available_components` with the ids above.
  - `reads_the_checked_in_voices` and `names_files_it_cannot_read` stay.
  - `tests/process.rs`: `the_program_describes_and_exports`: through `MediaProcess` (`ProviderUse::Export`), describe, open/encode/finish video, open/encode/finish audio, open/write/finish muxer, then `read` the file.
  - `tests/plugin_folder.rs`: the folder's manifest lists one muxer with `trims_leading_audio`.
- [ ] **Step 2:** `cargo test -p celesta-codec-test` fails to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-codec-test` passes.
- [ ] **Step 5: Commit:** `feat(codec-test): encoders and a muxer for the test codec`.

### Task 9: The export session, the render loops, and the exporter API (`celesta-exporter`, `celesta-gpu-renderer`, `celesta-editor`)

**Files:**
- Create: `crates/exporter/src/encode.rs`
- Modify: `crates/exporter/src/render.rs` (delete `VideoWriter`, `open_video_writer`, `write_frame`, `finish_encode`, `mux_audio`, `ensure_encoders`, `with_readback`; `export_renderer(&self, asset_root, frame_rate, cancellation)`; `validate_output(output, overwrite)` checks `OutputExists` only), `crates/exporter/src/exporter.rs`, `crates/exporter/src/react_entry.rs`, `crates/exporter/src/options.rs` (delete `VideoFormat`, `VideoEncoding`, `EncoderPreset`, `BitDepth`, `ColorConversion`, their `Unknown*`; `ExportOptions.output: OutputChoice` replaces `video_format` and `video`), `crates/exporter/src/control.rs`, `crates/exporter/src/report.rs`, `crates/exporter/src/progress.rs`, `crates/exporter/src/error.rs` (delete `InvalidCrf`, `EncoderUnavailable`), `crates/exporter/src/stills.rs` (the two `export_renderer` calls), `crates/exporter/src/lib.rs`, `crates/exporter/src/tests.rs`, `crates/exporter/tests/integration/{export,stills,cli_formats}.rs`
- Modify: `crates/gpu-renderer/src/renderer.rs` (`pub fn set_background(&mut self, color: Color)`)
- Modify: `crates/editor/src/export_worker.rs` (`ExportRequest.format` removed; `export_options` sets `output: OutputChoice::default()`), `crates/editor/src/helpers.rs` (`export_output(path) -> PathBuf`), `crates/editor/src/export_control.rs` (`start_export(output)`), `crates/editor/src/tests.rs`
- Test: `crates/exporter/src/encode.rs` tests, `crates/exporter/tests/integration/export.rs`

**Interfaces:**
- Consumes: Tasks 4–8.
- Produces:
  ```rust
  // control.rs
  pub struct TrackInfo { pub component: ComponentRef, pub codec: String }
  ExportedFile::Video { path, first_frame, frames, muxer: ComponentRef, video: TrackInfo, audio: Option<TrackInfo>, alpha: bool, files: Vec<PathBuf>, loudness: Option<AudioLoudness> }
  ExportProgress::Output(OutputPlan)           // once resolve_output succeeds, before Rendering
  // encode.rs
  pub(crate) const AUDIO_CHUNK_FRAMES: usize = 1024 * 16;
  pub(crate) struct ExportSession { … }
  impl ExportSession {
      pub(crate) fn start(registry: Arc<MediaRegistry>, cancellation: &ExportCancellation) -> Self;
      pub(crate) fn resolve(&mut self, choice: &OutputChoice, output: &Path) -> Result<OutputPlan, ExportError>;  // describe through this session's processes (ProviderUse::Export)
      pub(crate) fn offered_formats(renderer: &GpuRenderer) -> Vec<PixelFormat>;
      pub(crate) fn open_video(&mut self, width: u32, height: u32, frame_rate: Rational, offered: &[PixelFormat], staged: &Path) -> Result<VideoEncoderOpened, ExportError>;
      pub(crate) fn encode_frame(&mut self, frame: GpuFrame) -> Result<(), ExportError>;   // pts = frames_sent × denominator
      pub(crate) fn encode_audio(&mut self, mix: &AudioBuffer, looping: bool) -> Result<(), ExportError>;
      pub(crate) fn no_audio(&mut self) -> Result<(), ExportError>;
      pub(crate) fn finish(self) -> Result<ExportedTracks, ExportError>;
      pub(crate) fn abort(self);
  }
  pub(crate) struct ExportedTracks { pub muxer: ComponentRef, pub video: TrackInfo, pub audio: Option<TrackInfo>, pub alpha: bool, pub files: Vec<PathBuf> }
  pub(crate) fn readback_of(format: PixelFormat) -> Result<ReadbackFormat, ExportError>;   // Rgba16 → Rgba16Straight; Nv12/P010 → ExportError::Codec "not produced yet"
  ```
- `open_video`: timescale = `frame_rate.numerator`; the answer's `input`
  must be in `offered`, and `alpha` only with `Rgba16`, else
  `ExportError::Codec { stage: "video encoding", source: InvalidSettings { key: "alpha", … } }`.
  `encode_frame` sends `encodeVideoFrame`, gives the packets to the
  interleaver, keeps the first `config`, then `try_open_muxer` and
  `flush`. `try_open_muxer` opens the muxer once the video config is known
  and the audio is decided (no audio, or its config known), with
  `output = staged`, the tracks, `leading_audio`, and the muxer's settings;
  `flush` writes `take_ready()` in one `write_packets`.
- `encode_audio`: `openAudioEncoder` with the mix's rate and channels; a
  different answered rate resamples with `resample_audio`; `looping` and
  `muxer.entry.trims_leading_audio` feed `loop_lead_in` (moved here from
  `render.rs`, applied to the resampled mix) first at `pts = -LOOP_LEAD_IN_FRAMES`
  and set `leading_audio`; chunks of `AUDIO_CHUNK_FRAMES` at increasing
  `pts`; `finish_audio`; packets to the interleaver; `finish_audio` on it.
- `finish`: `finishVideoEncoder` (packets in), `finish_video` on the
  interleaver, `try_open_muxer` (it is an error to have no video config:
  `ExportError::Codec` "the encoder produced no configuration"), `flush`,
  `finishMuxer`; returns the tracks' `TrackInfo`s and `files`.
  `abort` sends `abortEncoder` / `abortMuxer` for open sessions and drops
  the processes.
- `export_project_cancellable`: `validate_output` → `ExportSession::start`
  → `resolve` → `progress(Output(plan))` → the composition checks →
  `export_renderer` → `offered_formats` → `output::stage` → `open_video` →
  `set_readback_format(readback_of(input))` and `set_background(TRANSPARENT if alpha else default)`
  → the mix thread as today → the frame loop, which after each frame polls
  the mix's channel (`mpsc::Receiver::try_recv`) and on a result runs
  `finish_mix` and `encode_audio` → after the loop, if not yet, waits for
  the mix and encodes it → `finish` → `publish` → `Wrote`. The React path
  stages everything: frames, then `build_audio_graph`; clips → mix →
  `encode_audio`, none → `no_audio` (with the loudness warning as today);
  `finish`; `publish`. Any error aborts the session; the staged file and
  spill drop with it.
- `progress.rs`: `Settings` loses `format` and `video`; the Format line is
  `"resolving…"` until `Output`, then `"{muxer_name} · {video label} · {audio label | no audio}"`.
  `report.rs`: `Output::Video { …, muxer: String, video: {component, codec}, audio: Option<{component, codec}>, alpha, files }`,
  `format` gone; the `output` progress event.

- [ ] **Step 1: Write failing tests.**
  - `encode.rs` tests, with an in-process fake registry (a fake plugin with a video encoder that answers canned packets and a config on the first frame, a PCM encoder with `delay = 1024`, and a muxer that records every request):
    - `the_muxer_opens_once_both_configs_are_known`: three frames then `encode_audio` → `openMuxer` is recorded after the audio, with two tracks, `leading_audio == 0`, and the audio track's `delay == 1024`.
    - `frames_before_the_mix_are_staged_then_muxed_in_order`: five frames, then audio, then `finish`: the muxer's packets have dts non-decreasing in seconds across tracks, and the video packets' bytes are the five frames' in order.
    - `no_audio_opens_a_video_only_muxer`: `no_audio()` → one track.
    - `a_looping_export_sends_the_pre_roll_only_to_trimming_muxers`: with `looping` and `trims_leading_audio`, `leading_audio == 16384` and the first audio chunk's `pts == -16384`; with a muxer without the flag, `leading_audio == 0` and the first `pts == 0`.
    - `a_refused_rate_is_resampled`: the PCM fake answers 48 000 to a 44 100 request; the frames it receives number `round(mix × 48000 / 44100)` (± 1).
    - `alpha_needs_rgba16_input`: an encoder answering `alpha: true, input: Yuv420p` fails with `InvalidSettings { key: "alpha" }`.
    - `abort_ends_every_open_session`: after two frames, `abort` records `abortEncoder` for the video session and `abortMuxer` only if opened.
  - `tests/integration/export.rs`, through the test codec (replace the `VideoEncoding`/`ColorConversion` uses with `--video-option input=rgba8` as `OutputChoice`): port `exports_frame_exact_mp4_with_silent_audio` (asserting `media.extension == Some("mp4")` and `Wrote` carries `muxer == "celesta-test-codec/file"`, `video.codec == "ctst"`, `audio.is_some()`, `alpha == false`), `alpha_formats_store_straight_16_bit_frames` (now `alpha=true`), `every_format_stores_the_mix_as_long_as_the_video`, `a_looping_mp4_starts_its_sound_from_its_end` (the stored `lead_in_frames == 16384` and `played()` equals the mix), `exports_a_silent_react_entry_to_webm`, and add `a_silent_react_entry_with_loop_has_no_audio_track` (`looping: true`, `Wrote.audio.is_none()`, no error) and `cancel_during_encoding_leaves_no_files` (cancel at `Rendering { frame: 1 }`; afterwards the output's directory holds no file at all).
  - `report.rs`: `a_video_output_names_its_components`: a `Wrote(ExportedFile::Video { .. })` with `muxer "celesta-test-codec/file"`, `video { "celesta-test-codec/video", "ctst" }`, `audio: None`, `alpha: false`, `files: [path]` serializes to `{"type":"video", …, "muxer":"celesta-test-codec/file", "video":{"component":"celesta-test-codec/video","codec":"ctst"}, "audio":null, "alpha":false, "files":[…]}` with no `format` key; `ExportProgress::Output(plan)` serializes as the `output` progress event with `muxer`, `muxerName`, `video.settings`, and `audio`.
  - `cli_formats.rs`: delete `json_reports_h264_flags_on_mov_as_usage_errors` and `…mismatched_output_format…` (Task 10 adds the new ones).
  - Editor tests: `export_output` returns `a.gif` → `a.mp4`, `a.WebM` unchanged.
- [ ] **Step 2:** `cargo test -p celesta-exporter -p celesta-editor` fails to compile.
- [ ] **Step 3: Implement.** Keep `loop_lead_in` and `LOOP_LEAD_IN_FRAMES` (moved to `encode.rs`, `pub(crate)`); delete the PCM sidecar and the remux.
- [ ] **Step 4:** `cargo test -p celesta-exporter -p celesta-editor -p celesta-gpu-renderer` passes (`celesta-mcp` compiles in Task 11; `cargo test -p celesta-exporter` needs `celesta-codec-test` built).
- [ ] **Step 5: Commit:** `feat(exporter)!: encode and mux through media components`.

### Task 10: The CLI's component flags, `--list-components`, and `--describe` (`celesta-exporter`)

**Files:**
- Create: `crates/exporter/src/components.rs` (shared by the CLI and MCP)
- Modify: `crates/exporter/src/main.rs` (`Cli`, `output_choice`, `subtitles_only_choice`, `job`, the help text, a `components` mode), `crates/exporter/src/lib.rs`
- Test: `crates/exporter/src/main.rs` tests, `crates/exporter/tests/integration/cli_formats.rs`

**Interfaces:**
- Produces, in `celesta_exporter::components`:
  ```rust
  pub struct ListedComponent { pub info: MediaComponentInfo, pub available: Option<bool>, pub reason: Option<String> }
  pub fn list_components(registry: &MediaRegistry, check: bool, kill: &PluginKillSwitch) -> Vec<ListedComponent>;
  pub fn describe_component(registry: &MediaRegistry, reference: &ComponentRef, kill: &PluginKillSwitch) -> Result<(MediaComponentInfo, ComponentDescription), ExportError>;  // ComponentUnavailable "not installed" when unknown; the kind is found by trying the four
  pub fn listing_json(components: &[ListedComponent]) -> Value;   // {"components":[{"id","kind","name","plugin","origin","version", manifest fields…, "available"?, "reason"?}]}
  pub fn description_json(info: &MediaComponentInfo, description: &ComponentDescription) -> Value;   // the listing entry plus "available","reason"?,"settings","inputFormats"?,"transports"?,"sampleRates"?,"channels"?
  ```
- CLI (exactly the spec's block): `--preset <NAME>`, `--preset-file <PATH>`,
  `--muxer <PLUGIN/ID>`, `--video-encoder <PLUGIN/ID>`,
  `--video-option <KEY=VALUE>` (repeatable, `SettingInput::Text`),
  `--audio-encoder <PLUGIN/ID|none>`, `--audio-option <KEY=VALUE>`,
  `--list-components`, `--check` (requires `--list-components`),
  `--describe <PLUGIN/ID>`; `source` is `required_unless_present_any = ["list_components", "describe", "inspect"]`.
  `--json` makes the two listings machine-readable; the plain listing prints
  one line per component, `<id>  <kind>  <name>  (<origin>)`, with
  `--check` appending `available` or `unavailable: <reason>`; the plain
  `--describe` prints the header line, then each setting as
  `<key>  <type>  default <value>  <label>` and its range or options.
  Usage errors: any component flag, `--preset`, or `--loop` with a still
  selection or without an output (as today's H.264 flags were); a
  `--video-option` without `=`; a bad component reference
  (`InvalidComponentRef`'s text); `--preset-file` is parsed and, since P3
  is not here, every `--preset` still fails at resolve.
- `--loop` is accepted for every video output (deviation 17).

- [ ] **Step 1: Write failing tests** in `main.rs`'s tests (the existing `choice`/`error` helpers):
  - `component_flags_build_the_choice`: `["out.mp4","--muxer","ffmpeg/mp4","--video-encoder","ffmpeg/libx264","--video-option","crf=20","--video-option","preset=slow","--audio-encoder","none"]` → `OutputChoice { muxer: ffmpeg/mp4, video.component: ffmpeg/libx264, video.options: {crf: Text("20"), preset: Text("slow")}, audio: None }`.
  - `a_video_option_needs_an_equals_sign`: `"--video-option","crf"` → an error mentioning `KEY=VALUE`.
  - `component_flags_need_a_video_output`: with `--frame 0` and with `--subtitles` alone, `--video-encoder` is an error naming the flag; `--loop` with `--frame 0` stays an error; `--loop` with `out.webm` is accepted.
  - `list_components_needs_no_source`: `["--list-components"]` and `["--describe","ffmpeg/libx264"]` parse; `["--list-components","--check","film.tsx","out.mp4"]` parses too (the listing wins).
  - `cli_formats.rs`: `json_reports_a_bad_component_reference_as_a_usage_error` (`--video-encoder libx264`), `json_lists_the_test_codecs_components` (with `TestCodecPlugin` on `--plugin-dir`: `--list-components --json` has four entries, the muxer with `"trimsLeadingAudio": true`; `--check` adds `"available": true`), `json_describes_the_test_video_encoder` (`--describe celesta-test-codec/video --json` has `settings[0].key == "input"`), `an_x264_preset_fails_with_the_hint` (`--preset ultrafast` → `unknown_preset`, hint contains `--video-option preset=ultrafast`), `a_hidden_option_is_an_invalid_option` (`--audio-option sampleRate=48000` is fine; `--video-option alpha=true` with `input=yuv420p` → `invalid_option` whose message names `alpha`).
- [ ] **Step 2:** `cargo test -p celesta-exporter main cli_formats` fails.
- [ ] **Step 3: Implement.** Remove `OutputFormat`, the `--output-format` checks, and the H.264 flag checks. Rewrite the help text's encoder paragraph: the extension picks the muxer, `--list-components` and `--describe` show what is installed and each component's settings.
- [ ] **Step 4:** `cargo test -p celesta-exporter` passes.
- [ ] **Step 5: Commit:** `feat(exporter): component flags, --list-components, and --describe`.

### Task 11: MCP's `export_video` arguments and the two component tools (`celesta-mcp`)

**Files:**
- Modify: `crates/mcp/src/args.rs` (`ExportVideoArgs`, new arg types, tests), `crates/mcp/src/server.rs` (two tools), `crates/mcp/src/instructions.md` (the tool map line)
- Test: `crates/mcp/src/args.rs` tests

**Interfaces:**
- Produces, as the spec's MCP shape:
  ```rust
  pub struct EncoderArg { id: Option<String>, options: Option<Map<String, Value>> }
  #[serde(untagged)] pub enum AudioEncoderArg { None(NoneWord /* "none" */), Encode(EncoderArg) }
  ExportVideoArgs { …, preset: Option<String>, preset_file: Option<PathBuf>, muxer: Option<String>,
                    video_encoder: Option<EncoderArg>, audio_encoder: Option<AudioEncoderArg> }   // preset's old meaning, crf, bit_depth gone
  pub struct ListComponentsArgs { check: Option<bool> }   // tool list_components → components::listing_json
  pub struct DescribeComponentArgs { component: String }  // tool describe_component → components::description_json, or error_result("component_unavailable", …)
  ```
  Option values: JSON strings, numbers, and booleans become
  `SettingInput::Value`; anything else is a usage error naming the key.
  A bad reference is a usage error with `InvalidComponentRef`'s text. The
  `loop`-is-MP4-only rule goes. `HintStyle::Mcp`'s preset hint is
  `set videoEncoder.options.preset to <name>`.

- [ ] **Step 1: Write failing tests** in `args.rs`:
  - `component_arguments_build_the_choice`: `{"source":"a.tsx","output":"a.mp4","muxer":"ffmpeg/mp4","videoEncoder":{"id":"ffmpeg/libx264","options":{"crf":20,"preset":"slow"}},"audioEncoder":"none"}` → the matching `OutputChoice` with `Value(Number(20))`.
  - `option_values_must_be_scalars`: `"options":{"crf":[1]}` is rejected naming `crf`.
  - `a_bad_reference_is_a_usage_error`: `"muxer":"mp4"` → the error text contains `` `mp4` is not a component reference ``.
  - `loop_applies_to_every_video`: `{"output":"a.webm","loop":true}` is accepted.
  - `old_fields_are_unknown`: `"crf": 20` at the top level is rejected as unknown (the existing `reject_unknown`).
- [ ] **Step 2:** `cargo test -p celesta-mcp args` fails to compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test --workspace` passes.
- [ ] **Step 5: Commit:** `feat(mcp): component arguments and the list/describe tools`.

### Task 12: The FFmpeg plugin's video encoders and `describe` (`plugins/ffmpeg`)

**Files:**
- Create: `plugins/ffmpeg/src/ffi.rs` (RAII `CodecContext`, `Frame`, `PacketBuf`, `Scaler`; `av_error(code: c_int, what: &str) -> MediaError` through `av_strerror`), `plugins/ffmpeg/src/video_encoder.rs`, `plugins/ffmpeg/src/components.rs` (component ids, schemas, `describe`, availability), `plugins/ffmpeg/src/h264.rs` (`avcc_from_annexb`, `annexb_to_avc`, `codec_string`)
- Delete: `plugins/ffmpeg/src/encoder.rs`
- Modify: `plugins/ffmpeg/src/lib.rs` (`FfmpegCodec: MediaProvider + Decoder + VideoEncoder`; `AudioEncoder` and `Muxer` arrive in Task 13), `plugins/ffmpeg/plugin.json` (the spec's `videoEncoders`, `audioEncoders`, `muxers`, with `inputFormats`: libx264 `["yuv420p","yuv420p10","rgba16","rgba8"]`, prores and vp9 `["rgba16"]`)
- Test: `plugins/ffmpeg/tests/encoders.rs` (new), `plugins/ffmpeg/tests/process.rs` (`manifest_flag_prints_the_media_manifest` checks `media.muxers[0].id == "mp4"`)

**Interfaces:**
- Consumes: Tasks 3, 4, 7.
- Produces (component ids `libx264`, `prores`, `vp9`; the audio and muxer ids of Task 13 are declared in `plugin.json` now and described as `available: false, reason: "arrives with the muxers"` until then):
  - `pub(crate) fn availability(encoder: &str) -> Result<(), String>` in `components.rs`: `Ok` when `avcodec_find_encoder_by_name` finds it, else `Err(format!("this FFmpeg build has no {encoder}"))`.
  - `describe`: a video encoder is `available` by `availability` of `libx264` / `prores_ks` / `libvpx-vp9`, with the `Err` as its `reason`; `transports: ["pipe"]`; `input_formats` as the manifest.
  - Schemas. `libx264`: `preset` select of the nine x264 presets (labels capitalized, default `medium`); `rateControl` select `crf` / `vbr` (default `crf`); `crf` number integer 0–51 step 1 default 18, `visibleWhen rateControl=crf`; `bitrate` number integer min 100000 unit `bit/s` default 8000000, `visibleWhen rateControl=vbr`; `bitDepth` select `8` / `10` default `8`; `tune` string default `""` advanced. `prores`: `profile` select `4444` (label `4444`, default) / `hq` (label `422 HQ`). `vp9`: `crf` number integer 0–63 default 20; `alpha` boolean default `true`.
  - Input choice and conversion (`open_video`): `libx264` takes `Yuv420p` (`bitDepth` 8) or `Yuv420p10` (10) when offered, else `Rgba8`/`Rgba16`, converted with `sws_scale` (`SWS_BICUBIC`, default coefficients: today's untagged BT.601) to `yuv420p` / `yuv420p10le`; `alpha: false`. `prores` and `vp9` take `Rgba16` (else `InvalidSettings { key: "input", message: "needs rgba16" }`), converted with BT.709 limited (`sws_setColorspaceDetails` with `sws_getCoefficients(SWS_CS_ITU709)`, dst range 0) and the frames tagged `AVCOL_PRI_BT709`, `AVCOL_TRC_BT709`, `AVCOL_SPC_BT709`, `AVCOL_RANGE_MPEG`, to `yuva444p10le` (4444: `alpha: true`, `alpha_bits=16`, `vendor=apl0`) / `yuv422p10le` (`hq`) / `yuva420p` (vp9 with `alpha`) or `yuv420p`; vp9 options `crf`, `b=0`, `row-mt=1`. Every context: `time_base = 1/timescale`, `framerate = frame_rate`, `AV_CODEC_FLAG_GLOBAL_HEADER`, frame `pts` as given.
  - Packets: `pts`, `dts`, `duration` from the `AVPacket` (its `time_base` is the context's), `key` from `AV_PKT_FLAG_KEY`, side data `AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL` → `SideData { id: u32 from its first 8 bytes big-endian, bytes: the rest }`. H.264 packets are converted `annexb_to_avc` (each 3- or 4-byte start code replaced by the NAL's 4-byte big-endian length).
  - `TrackConfig` with the first answer that has packets: `libx264` → `codec_string(sps) == format!("avc1.{:02X}{:02X}{:02X}", sps[1], sps[2], sps[3])`, `description = avcc_from_annexb(extradata)` = `[1, sps[1], sps[2], sps[3], 0xFF, 0xE1, len(sps) as u16 BE, sps…, 1, len(pps) as u16 BE, pps…]` (the first NAL of type 7 and of type 8); `prores` → `ap4h` / `apch`, no description; `vp9` → `vp09.00.10.08`, no description.
  - `finish_video` drains the encoder (`avcodec_send_frame(NULL)`); `abort_video` frees it.

- [ ] **Step 1: Write failing tests** in `tests/encoders.rs` through `FfmpegCodec` in process (32×16, 10 fps, 10 frames):
  - `libx264_packets_carry_an_avcc_and_a_codec_string`: the first answer with packets has `config.codec` starting `"avc1."`, `description[0] == 1`, `description[5] & 0x1F == 1`; ten packets in all after `finish_video`, the first `key`, and each packet's first four bytes read as a length no larger than the packet.
  - `ten_bit_h264_picks_yuv420p10_when_offered`: `bitDepth = "10"` with `[Yuv420p, Yuv420p10, Rgba16]` → `input == Yuv420p10` and `codec` starts `"avc1.6E"`.
  - `rgba_input_is_converted`: offered `[Rgba8]` → `input == Rgba8`; the packets fed to libavcodec's `h264` decoder (`avcodec_find_decoder(AV_CODEC_ID_H264)`, `extradata` = the avcC) decode to ten 32×16 frames (a muxed round trip is Task 13's).
  - `prores_4444_keeps_alpha_and_hq_does_not`: `profile` `4444` → `alpha, codec == "ap4h"`; `hq` → `!alpha, "apch"`.
  - `vp9_alpha_rides_as_block_additions`: `alpha = true` → every packet has `additional == [SideData { id: 1, .. }]`; `alpha = false` → none.
  - `describe_names_every_manifest_component`: `plugin.json`'s ids per kind equal `describe`'s, and `libx264` is available on this machine.
  - `a_missing_encoder_has_a_reason`: `components::availability("no_such_encoder") == Err("this FFmpeg build has no no_such_encoder")`.
- [ ] **Step 2:** `PKG_CONFIG_PATH=… cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --test encoders` fails to compile.
- [ ] **Step 3: Implement.** `tests/export.rs` and `tests/process.rs`'s export tests, and `examples/codec-bench.rs`, are updated in Task 13; mark them `#[ignore = "ported in Task 13"]` only where they cannot compile otherwise.
- [ ] **Step 4:** The `encoders` and `decode` tests pass; `cargo clippy --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --all-targets -- -D warnings` passes.
- [ ] **Step 5: Commit:** `feat(codec-ffmpeg): video encoders on libavcodec with describe`.

### Task 13: The FFmpeg plugin's audio encoders, muxers, and export tests (`plugins/ffmpeg`)

**Files:**
- Create: `plugins/ffmpeg/src/audio_encoder.rs`, `plugins/ffmpeg/src/muxer.rs`
- Modify: `plugins/ffmpeg/src/lib.rs` (`AudioEncoder`, `Muxer`), `plugins/ffmpeg/src/components.rs`, `plugins/ffmpeg/tests/export.rs`, `plugins/ffmpeg/tests/process.rs`, `plugins/ffmpeg/examples/codec-bench.rs`, `scripts/test-package.mjs:70-72` (also assert `videoEncoders`, `audioEncoders`, and `muxers` are non-empty)
- Test: `plugins/ffmpeg/tests/export.rs`, `plugins/ffmpeg/tests/process.rs`

**Interfaces:**
- Produces (ids `aac`, `opus`, `pcm`; `mp4`, `mov`, `webm`):
  - `aac`: FFmpeg's `aac` encoder, `bitrate` number integer 32000–512000 unit `bit/s` default 192000; `sample_rates: Only(the encoder's supported list)`, `channels: [1, 2]`; answers the request's rate when supported, else 48 000; `delay = initial_padding` (1024); description = the 2-byte AudioSpecificConfig extradata; codec `mp4a.40.2`. `opus`: `libopus`, `bitrate` default 192000; always 48 000; `delay = initial_padding`; description = the OpusHead extradata; codec `opus`. `pcm`: no libavcodec; `format` select `s16le` / `s24le` (default) / `f32le`; any rate; `delay 0`; codec `lpcm.<format>`; samples packed as libswresample and `pcm_s24le` do on `main`: `s24 = clip_i32(round(x × 2³¹)) >> 8` as three LE bytes, `s16 = clip_i16(round(x × 2¹⁵))`, `f32le` raw.
  - Buffering: an `AVAudioFifo` gathers `frame_size` frames per `AVFrame` (`fltp` for aac, `flt` for libopus); the remainder is flushed at `finish_audio`. Packet `pts`/`dts`/`duration` in `1/sample_rate`; the encoder's own `pts` already includes `-delay`.
  - Muxers: `avformat_alloc_output_context2` with `"mp4"`, `"mov"`, `"webm"`; `avio_open` the given `output`; one `AVStream` per `Track` with `codecpar` from the config: codec id by family (`avc1`→H264, `hvc1`→HEVC, `av01`→AV1, `vp09`→VP9, `ap4h`/`apch`→PRORES with `codec_tag` from the family, `mp4a`→AAC, `opus`→OPUS, `lpcm.s16le`/`s24le`/`f32le`→`PCM_S16LE`/`PCM_S24LE`/`PCM_F32LE` with `bits_per_coded_sample`), `extradata` = description, `width`/`height`, `time_base = 1/timescale`, `sample_rate`, `ch_layout`, `initial_padding = delay`, and `format = AV_PIX_FMT_YUVA420P` for a `vp09` track with `alpha`; an unknown family is `MediaError::Codec("{muxer} does not take {codec}")`. Settings: `mp4` and `mov` `faststart` boolean default `true` → `movflags=+faststart`; `webm` none. `write_packets`: an `AVPacket` per `TrackPacket` with `pts`, `dts`, `duration`, `AV_PKT_FLAG_KEY`, and `AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL` side data rebuilt (8-byte big-endian id, then bytes) → `av_interleaved_write_frame`. `finish_muxer` → `av_write_trailer`, close → `[output]`; `abort_muxer` → close without a trailer → `[output]`. Negative audio timestamps pass through: libavformat writes MP4/MOV edit lists from them, and WebM's `CodecDelay` from `initial_padding`.
  - `codec-bench`: `encode` opens `libx264` through `VideoEncoder`, sends rgba8 frames, discards packets.

- [ ] **Step 1: Write failing tests.**
  - `tests/export.rs`, through `celesta_codec_ffmpeg::install()` and the exporter (`OutputChoice::default()` unless said), keeping today's helpers: `exports_frame_exact_mp4_with_silent_audio` (decoded frames as today; the probe's audio codec `aac`), `exports_transparent_prores_4444` (`.mov`), `exports_transparent_vp9_webm`, `every_format_muxes_audio_as_long_as_the_video` (mp4, mov, webm), `a_looping_mp4_starts_its_sound_from_its_end` (unchanged assertions), `brings_the_mix_to_a_loudness_target`, `mov_and_webm_stay_tagged_bt709_and_mp4_untagged` (today's `video_stream` tags), `a_looping_webm_is_written_as_without_loop` (identical decoded samples with and without `looping`), `mp4_and_mov_start_with_an_edit_list_hiding_the_delay` (`start_time == 0` and the decoded sample count equals the mix's), `opus_webm_declares_its_codec_delay` (decoded length within one Opus frame of the mix), `pcm_mov_decodes_to_the_mix_exactly` (`.mov` with the default `pcm` `s24le`: samples within 2⁻²³ of the mix).
  - `tests/process.rs`: `writes_an_export_through_the_program` ported: describe (`libx264` available), open/encode/finish video, open/encode/finish audio (`aac`, delay 1024), open/write/finish `mp4`, then probe and decode as today.
  - `the_manifest_lists_every_kind` on `--manifest`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg` fails.
- [ ] **Step 3: Implement.** Remove every `ez_ffmpeg::VideoWriter`, `FfmpegContext` mux use, and `capabilities::is_encoder_available`; ez-ffmpeg stays for decoding and the tests' readback.
- [ ] **Step 4:** `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`, its clippy with `-D warnings`, `cargo fmt --check` in `plugins/`, and `node --check scripts/test-package.mjs` pass. `node scripts/plugins/build.mts plugins/ffmpeg` writes a `celesta-plugin.json` with the three new lists.
- [ ] **Step 5: Commit:** `feat(codec-ffmpeg): audio encoders and muxers on libavformat`.

### Task 14: `@celesta.studio/exporter` (`packages/exporter`)

**Files:**
- Modify: `packages/exporter/src/types.ts`, `packages/exporter/src/args.ts`, `packages/exporter/src/index.ts`, `packages/exporter/README.md`
- Test: `packages/exporter/test/args.test.mts`, `packages/exporter/test/run.test.mts` (a fixture report with the new `video` output shape)

**Interfaces:**
- Produces:
  ```ts
  export type SettingValue = string | number | boolean;
  export interface EncoderOptions { id?: string; options?: Record<string, SettingValue> }
  // VideoOptions: `format`, `preset` (libx264), `crf`, `bitDepth`, `colorConversion` removed; added:
  //   preset?: string; presetFile?: string; muxer?: string; videoEncoder?: EncoderOptions; audioEncoder?: EncoderOptions | 'none'
  // StillsOptions: `format` removed (the extension decides)
  // ExportOutput video: `format` removed; added muxer: string; video: { component: string; codec: string };
  //   audio: { component: string; codec: string } | null; files: string[]
  export type ProgressEvent = … | { event: 'output'; muxer: string; muxerName: string;
      video: { component: string; name: string; settings: string[] }; audio: { component: string; name: string; settings: string[] } | null };
  export interface ComponentInfo { id: string; kind: 'decoder' | 'videoEncoder' | 'audioEncoder' | 'muxer'; name: string; plugin: string;
      origin: 'user' | 'bundled' | 'project'; available?: boolean; reason?: string; [manifest: string]: unknown }
  export interface ComponentDescription extends ComponentInfo { available: boolean; settings: SettingField[]; inputFormats?: string[]; transports?: string[]; sampleRates?: 'any' | number[]; channels?: number[] }
  export interface SettingField { key: string; type: 'select' | 'number' | 'boolean' | 'string'; label: string; default: SettingValue; … }
  export async function listComponents(options?: CompositionOptions & { check?: boolean }): Promise<ComponentInfo[]>;   // --list-components [--check] --json
  export async function describeComponent(id: string, options?: Omit<RunOptions, 'onProgress'>): Promise<ComponentDescription>;   // --describe <id> --json
  ```
  Flags: `--muxer=`, `--video-encoder=`, one `--video-option=key=value` per
  entry (numbers as JS prints them, booleans `true`/`false`),
  `--audio-encoder=none` for `'none'`, `--audio-option=`, `--preset=`,
  `--preset-file=`.

- [ ] **Step 1: Write failing tests**:
  - `a video export passes its component flags`: the options above give exactly `['--muxer=ffmpeg/mp4', '--video-encoder=ffmpeg/libx264', '--video-option=crf=20', '--video-option=preset=slow', '--audio-encoder=none']` among the arguments, in that order.
  - `listComponents and describeComponent build their commands`: `['--json', '--list-components', '--check']` and `['--json', '--describe', 'ffmpeg/libx264']` (through the exported arg builders `componentsArgs`, `describeArgs`).
  - `a video output carries its tracks` in `run.test.mts`: a fixture report's video output has `muxer`, `video.codec`, `audio === null`, `files`.
- [ ] **Step 2:** `pnpm --dir packages/exporter test` fails.
- [ ] **Step 3: Implement.** Update the README's option list.
- [ ] **Step 4:** `pnpm --dir packages/exporter test` and `pnpm run test:runtime` pass.
- [ ] **Step 5: Commit:** `feat(exporter-js): component options and listings`.

### Task 15: Docs, the spec's deviations, the handoff note, the end-to-end check, and the performance measurement

**Files:**
- Modify: `packages/website/content/en/docs/02-(guides)/09-export.md` (sections "From the command line", "Encoding quality and speed" → "Choosing encoders and settings", "Videos that play on repeat", "Transparent video", the JSON paragraph) and `ja/docs/02-(guides)/09-export.md` (the same sections: コマンドラインから, 画質と書き出し速度, 繰り返し再生する動画, 透過動画); `05-(more)/05-plugins.md` (en, ja: media plugins now declare encoders and muxers; `--list-components`); `05-(more)/04-troubleshooting.md` (en, ja: `no_codec` → `unsupported_output` / `component_unavailable`)
- Modify: `skills/celesta/references/export.md` (Options table, Transparent video, Looping video, JSON results' codes, the preset advice at `:252-254`), `skills/celesta/references/errors.md` (`no_codec`, `encoder_unavailable`, `invalid_crf` rows), `skills/celesta/references/plugins.md:105-106`, `skills/celesta/references/verify.md` where it names `--preset`
- Modify: `HANDOFF.md` rows for `celesta-media`, `celesta-codec-ffmpeg`, `celesta-codec-test`, `celesta-exporter` ("Frame-exact export through media components (MP4, MOV, WebM, and whatever muxers plugins add)"), `celesta-plugin-host` ("their decoders" → "their components"), and `packages/exporter`; the stale-program paragraph (`no_codec` → `unsupported_output`)
- Modify: `docs/superpowers/specs/2026-11-08-media-components-design.md`: deviations 8–17 in Pixel formats and color, Audio, Capability discovery, Exporter API, Registry and routing, CLI, and Phases → P2a
- Create: `docs/handoff/2026-11-08-media-encoders.md`

- [ ] **Step 1: Write the docs.** The guide explains: the extension picks the container; `--list-components` and `--describe`; `--video-encoder` / `--video-option` with the bundled FFmpeg plugin's keys as of this release (libx264 `preset`, `rateControl`, `crf`, `bitrate`, `bitDepth`, `tune`; prores `profile`; vp9 `crf`, `alpha`; aac/opus `bitrate`; pcm `format`; mp4/mov `faststart`); `--audio-encoder none`; that `--preset` names an export preset (P3) and `--video-option preset=` the x264 one; `--loop` on any output, hidden by muxers that trim; the JSON output's `muxer`, `video`, `audio`, `files`; the error codes. Keep en and ja in step; the Agent Skill's `export.md` documents the same keys and points to `--describe`.
- [ ] **Step 2: Verify.** Each passes: `cargo test --workspace`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo fmt --all -- --check`; with `PKG_CONFIG_PATH`: `cargo test --manifest-path plugins/Cargo.toml --workspace --locked` and its clippy; `pnpm run test:runtime`; `pnpm --dir scripts/plugins test`; `pnpm --dir packages/website test`; `git diff --check`.
- [ ] **Step 3: The end-to-end check against `main`.** Build `main`'s exporter and plugin in a temporary worktree and this branch's, release, each staged alone as P1's note did (own `CELESTA_PLUGIN_PATH`, empty `HOME`). Export, with each: `examples/greenscreen/composite.tsx` as MP4; a `<Video>` (the green-screen footage) + `<Audio>` (`examples/assets/voices/001.wav`) entry as MP4, as MP4 with `--loop`, as MOV, and as WebM. Then decode each pair with `ffmpeg` (`-pix_fmt rgba64le -f rawvideo` for video, `-f f32le` for audio, the branch's plugin's FFmpeg or Homebrew's `ffmpeg@8`) and compare with `cmp`.
  - **Expected: identical** decoded frames and samples for every pair. This is realistic on this machine because the default path converts on the GPU (`yuv420p`, no libswscale), libx264 gets the same parameters (preset, CRF, pixel format, frame rate, `GLOBAL_HEADER`, auto threads) through the same library, the AAC/Opus/PCM encoders get the same `f32` samples, and libavformat writes the same edit lists from the same negative timestamps. If an MP4 differs, compare the x264 `options:` SEI strings of the two files (`ffmpeg -i … -c copy -bsf:v trace_headers -f null -` or `strings | grep options:`): a parameter that differs there is a bug to fix. The RGBA-input paths (MOV, WebM, and MP4 on a software renderer) go through `sws_scale` with `SWS_BICUBIC` where `main`'s filter graph used `scale` with the same flags; a difference there is a rounding difference of libswscale's two entry points: record the maximum per-channel difference and the PSNR, and accept only ≤ 1 code value (16-bit codes for MOV/WebM) with the cause named in the handoff note. Any audio difference is a bug.
  - Also: the branch's exporter with no plugin fails with `unsupported_output` and the install hint; with `--preset ultrafast`, `unknown_preset` and the hint; `--list-components --check` lists FFmpeg's ten components as available.
- [ ] **Step 4: The performance measurement** (release builds, Apple M4, medians of three interleaved runs each, against `main`):
  - `codec-bench` encode at 1080p and 4K (its decode half too, unchanged);
  - a React entry of 300 frames at 1920×1080 and the same entry at 3840×2160 as MP4, with `--video-option preset=ultrafast` (`main`: `--preset ultrafast`) and with the default `medium`;
  - the 4K entry as MOV (ProRes 4444), whose packets cross the pipe twice.
  P2a must not be slower: its median is at or below `main`'s within `main`'s own run spread. Record every number in the handoff note; an excess is a blocker to fix before the PR.
- [ ] **Step 5: Write `docs/handoff/2026-11-08-media-encoders.md`** in the P1 note's format: the specification as built (components, protocol, resolve, the session, the interleaver, the FFmpeg and test codec components, the CLI/MCP/TS surface), deviations 8–17, docs, checks (Steps 2–4's results, with the SHA-256 or `cmp` results and the timings), and what is not done (the ring and send window, BT.709, presets, stills through components).
- [ ] **Step 6: Commit:** `docs: media encoders and muxers (P2a)`.
