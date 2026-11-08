# Media encoders and muxers, P2a (2026-11-08)

Phase P2a of the media components design
(`docs/superpowers/specs/2026-11-08-media-components-design.md`, #782;
plan `docs/superpowers/plans/2026-11-08-media-components-p2a.md`), on top
of P1 (`docs/handoff/2026-11-08-media-decoders.md`). Exports stop going
through protocol 1's encoder messages and one media plugin: a media plugin
declares video encoders, audio encoders, and muxers beside its decoders,
`resolve_output` turns the export's choice into components, Celesta
interleaves the encoders' packets and hands them to the muxer, and the
CLI, MCP, and `@celesta.studio/exporter` choose components and their
settings. Everything goes through the plugin's pipe, one request at a
time; the shared-memory ring and the send window are P2b. Decoded output is
identical to `main`'s on every path checked (see Checks).

## Specification

- **Manifest.** `media` gains `videoEncoders` (`{ id, name, codecs,
  inputFormats? }`), `audioEncoders` (`{ id, name, codecs }`), and
  `muxers` (`{ id, name, extensions, video, audio?, defaultVideo?,
  defaultAudio?, trimsLeadingAudio, sequence }`), parsed by
  `MediaManifest::from_json` in `celesta-plugin-host`. Ids are unique per
  kind in a plugin; a malformed entry is skipped and reported as P1's
  decoders are. Codecs are families, the part of a codec string before its
  first `.`, with `avc3` read as `avc1` and `hev1` as `hvc1`
  (`codec_family`). `InstalledPlugins::media()` keeps one `MediaPlugin`
  per plugin, now with the whole `media` manifest (deviation 15).
- **Components and `describe`.** `MediaRegistry::components()` lists one
  `MediaComponentInfo { reference, kind, name, origin, entry }` per
  component, in search order and, within a plugin, decoders, video
  encoders, audio encoders, muxers. `MediaRegistry::describe(plugin, kill)`
  starts the plugin (`ProviderUse::Describe`, no shared memory), says
  `hello`, asks `describe`, and stops it; `describe_with` asks a given
  process (deviation 10). Answers are cached in the registry, keyed by the
  plugin's folder, its manifest `version`, and its executable's size and
  modification time; failures are not cached and make every component of
  the plugin unavailable with the failure as the reason. A described
  component the manifest lacks is dropped with one stderr line per plugin;
  a manifest component the answer leaves out is unavailable ("the plugin
  did not describe it"). An unavailable answer keeps only its reason.
- **Settings schema** (`celesta_media::settings`). Fields of kind
  `select`, `number` (`min`, `max`, `step`, `integer`, `unit`), `boolean`,
  and `string`, with `group`, `advanced`, `description`, and `visibleWhen`.
  `validate` fills missing keys with defaults, judges visibility on the
  effective values, rejects unknown keys, wrong types, out-of-range values,
  and values for hidden fields (`crf applies only when rateControl is
  crf`), and drops hidden fields. `parse_setting` reads the CLI's text by
  the field's kind; `differences` gives the visible settings that differ
  from their defaults, which the progress labels show.
- **Protocol 2.** `describe` / `described`; `openVideoEncoder`,
  `encodeVideoFrame`, `finishVideoEncoder`; `openAudioEncoder`,
  `encodeAudio`, `finishAudioEncoder`; `abortEncoder { session }`;
  `openMuxer`, `writePackets`, `finishMuxer`, `abortMuxer`, as the spec's
  tables give them, without `ring`. Packets travel as a JSON header per
  packet (`pts`, `dts`, `duration`, `key`, byte lengths, `additional`)
  with the bytes after the header, back to back; a `TrackConfig`'s
  `description` is base64. New error codes `invalidSettings` (`key`,
  `message`) and `componentUnavailable` (`component`, `reason`). The
  deadlines are the spec's: `describe` 30 s, the opens 60 s, each encode
  and `writePackets` 60 s, the encoder finishes 10 min, `finishMuxer` 1 h,
  the aborts 5 s, after which the process is killed. Protocol 1's
  `checkEncoder`, `openEncoder`, `encodeFrame`, `finishEncoder`, and
  `muxAudio`, `CodecProvider`'s encoder methods, `Container`, the old
  `PixelFormat`, `EncoderSettings`, `MuxRequest`, `MediaError::NoCodec`,
  and `EncoderUnavailable` are gone. `MediaProvider` advertises
  `decoder()`, `video_encoder()`, `audio_encoder()`, and `muxer()`, and
  `celesta_media::protocol::serve` dispatches to them.
- **`resolve_output`** (`celesta-exporter`, `resolve.rs`), the one place a
  choice becomes components:
  1. `preset` is `unknown_preset` (presets are P3), with the hint
     `use --video-option preset=<name>` (MCP: `set
     videoEncoder.options.preset to <name>`) for an x264 preset name.
  2. The muxer is the named one, which may be any installed muxer and is
     not checked against the output's extension (R7), or else the first
     muxer listing the extension, bundled plugins' first, then user
     plugins' in search order, never a project's. None is
     `unsupported_output` listing the extensions there are, or, with no
     muxer at all, "no media plugin offers a muxer" with the install hint.
  3. The video encoder is the named one, which must write a codec the
     muxer takes (`incompatible_components`), or else the first
     *available* encoder writing the muxer's `defaultVideo` (the first of
     `video` when it has none): the muxer's own plugin first, then bundled,
     then user plugins; a project's only when it is the named muxer's (R5).
     None available is the first candidate's `component_unavailable`; no
     candidate at all is the muxer's (R4, R6).
  4. The audio encoder likewise against `defaultAudio`; none for
     `AudioChoice::None` or a muxer without `audio`. A muxer taking audio
     with no audio encoder installed needs `--audio-encoder none` (R6).
  5. Each chosen component must be described as available
     (`component_unavailable` with the reason).
  6. Options are read by each field's type (`SettingInput::Text` from the
     CLI, `Value` from MCP and TypeScript; deviation 24) and validated
     (`invalid_option`). Muxers take their defaults.

  `describe` is asked at most once per plugin per resolve.
- **The export session** (`encode.rs`, `ExportSession`). One process per
  plugin, started for `ProviderUse::Export` (no shared memory) on first
  need and kept for the export; plugins the output does not use are
  stopped after resolving. Order at start: construct the renderer, offer
  formats best first (`yuv420p`, then `yuv420p10` when the renderer
  supports YUV readback and is not software (deviation 18), then `rgba16`,
  then `rgba8`), open the video encoder with timescale = the frame rate's
  numerator and BT.709 limited `Color`, check the answer (`input` must be
  offered, `alpha` only with `rgba16`), and set the renderer's readback
  format and background from it. An `ExportProgress::Output` event names
  the components before the first frame (deviation 12).
- **Audio.** Without an audio encoder no mix is made, and the report has
  no loudness; a loudness target then warns (R10). Otherwise the finished
  mix opens the audio encoder at its rate and channel count; another
  channel count answered is converted (mono to stereo by copying, stereo
  to mono by averaging), another rate resampled with `rubato` (deviation
  9), and the mix is sent in chunks of `1024 × 16` sample frames in one
  pass (deviation 11). With `--loop` and a muxer declaring
  `trimsLeadingAudio`, `LOOP_LEAD_IN_FRAMES` (16 × 1024) of the mix's end
  go first at negative `pts`, and `openMuxer` gets them as `leadingAudio`;
  `--loop` is accepted for every video output (deviation 17). A project's
  mix runs beside the render and is polled after each frame (the mix
  thread finishes loudness normalization, R9); a React entry's after the
  render.
- **The interleaver** (`interleave.rs`). Merges video and audio packets in
  decode order (`dts` compared across the two timescales): a video packet
  waits until the audio up to its time is known, or the audio is finished
  or absent. Up to 256 MB of video packets stay in memory; the rest go to a
  temporary file beside the output (raw bytes, index in memory), removed
  afterwards. The muxer opens once both tracks' configs are known, and
  ready packets go to it in `writePackets` batches of at most 32 MiB, so a
  staged backlog never crosses the pipe at once (R13). Finish, cancel, and
  failure abort or finish the muxer and remove what it wrote; the
  published files are reported in `files`.
- **FFmpeg's components** (`plugins/ffmpeg`, on libavcodec and libavformat
  through `ffmpeg-sys-next`; ez-ffmpeg stays for decoding):
  - Video encoders `libx264` (`avc1`; takes `yuv420p`, `yuv420p10`,
    `rgba16`, `rgba8`; settings `preset` (ultrafast … veryslow, `medium`),
    `rateControl` (`crf` / `vbr`), `crf` (0–51, 18), `bitrate` (8000000,
    with `vbr`), `bitDepth` (`8` / `10`), `tune`), `prores` (`prores_ks`,
    `ap4h` / `apch`; `rgba16`; `profile` `4444` / `hq`), and `vp9`
    (`libvpx-vp9`, `vp09.00.10.08`; `rgba16`; `crf` 0–63, 20; `alpha`,
    true, as BlockAdditions with id 1). The encoder's time base is one
    frame (`den/num`) as `main`'s was, with packets rescaled to the track's
    timescale (R19); a zero packet duration becomes one frame (R17). avcC
    is built from libx264's Annex B extradata with the High-profile fields
    libavformat writes (R16), and packets become length-prefixed. RGBA
    input is converted with `sws_scale` (`SWS_BICUBIC`; BT.601 for
    libx264, BT.709 for ProRes and VP9, as `main`, deviation 8). An
    encoder missing from the linked FFmpeg is described unavailable ("this
    FFmpeg build has no libx264"). FFmpeg's log level is `AV_LOG_ERROR`
    (R18).
  - Audio encoders `aac` (`mp4a.40.2`; the full extradata as description,
    R22; delay 1024), `opus` (`libopus`, 48 kHz only, delay 312), each with
    `bitrate` (32000–512000, 192000), and `pcm` (`lpcm.s16le` /
    `lpcm.s24le` / `lpcm.f32le` by `format`, default `s24le`, packed by the
    plugin; deviation 13).
  - Muxers `mp4`, `mov` (both `trimsLeadingAudio`, `faststart` default
    on) and `webm`. Negative audio times become MP4/MOV edit lists and
    WebM's `CodecDelay` (`initial_padding`). MOV and WebM streams of ProRes
    and VP9 are tagged BT.709 from `Track.color`; H.264 stays untagged
    (deviation 8). Video streams are marked progressive and ProRes with
    alpha gets depth 32 (R21). Opus's trailing discard padding comes from
    the last packet's shortened duration (deviation 26).
- **The test codec** (`celesta-codec-test`): a lossless video encoder
  (`ctst`, every offered format), a PCM encoder (`lpcm.f32le`), and its own
  muxer (file format version 2, any extension), beside P1's decoder, in
  process and as the program, with settings and `describe`.
- **CLI.** `--muxer`, `--video-encoder`, `--video-option key=value`,
  `--audio-encoder <plugin/id|none>`, `--audio-option key=value`,
  `--list-components [--check]` and `--describe <plugin/id>` (plain or
  `--json`; no source or output), `--preset` (always `unknown_preset`
  until P3), and `--preset-file` (read and required to be a JSON object,
  otherwise unused, R15). Removed: `--output-format`, `--crf`,
  `--bit-depth`, `--color-conversion`, and `--preset`'s libx264 meaning.
  Stills keep `--frame` and their other flags and pick their format by
  extension (P4 moves them to components). Component flags with stills or
  without an output are usage errors.
- **JSON report.** A video output has `muxer`, `video` and `audio` (`{
  component, codec }`, `audio: null` without sound), `alpha` (from the
  encoder), and `files`, instead of `format` and the `audio` boolean.
  `--json-progress` gains `output` (`muxer`, `muxerName`, `video` and
  `audio` as `{ component, name, settings }`). New codes
  `component_unavailable`, `incompatible_components`, `invalid_option`,
  and `unknown_preset`; `unsupported_output` now means no muxer writes the
  extension; `no_codec`, `encoder_unavailable`, and `invalid_crf` are
  gone. The progress dashboard labels the encoders with their names and
  changed settings ("H.264 (x264) · Slow · CRF 20").
- **MCP and TypeScript.** `export_video` takes `preset`, `presetFile`,
  `muxer`, `videoEncoder` and `audioEncoder` (`{ id?, options? }`, or
  `"none"` for audio) instead of `preset`/`crf`/`bitDepth`; `loop` for
  every video. New tools `list_components` (`check`) and
  `describe_component` (`component`). `@celesta.studio/exporter`'s
  `exportVideo` takes the same options (each option becomes
  `--video-option=key=value`), and `listComponents({ check })` and
  `describeComponent(id)` run `--list-components` and `--describe`,
  killing the exporter at once on abort.

## Deviations from the spec

Numbered after P1's seven; recorded in the spec's Capability discovery,
Exporter API, Registry and routing, CLI, Pixel formats and color, Audio,
and Phases sections:

8. P2a's encoders do not apply `Color`: libx264 converts RGBA with
   libswscale's default (BT.601, untagged), ProRes and VP9 keep `main`'s
   BT.709 conversion and tags, the GPU's `yuv420p` stays BT.601, and only
   ProRes and VP9 streams are tagged (from `Track.color`). P2c switches
   everything at once.
9. Audio sample-rate conversion uses `rubato` (`resample_audio`); the
   shared resampler resamples images.
10. A `describe` from the export reuses the export's process
    (`describe_with`); `MediaRegistry::describe` serves the CLI and MCP.
11. The mix is encoded in one pass once it is done, not interleaved with
    later frames.
12. Progress labels come from an `ExportProgress::Output` event
    (`--json-progress`'s `output`).
13. `lpcm` codec strings name the sample format (`lpcm.s24le`).
14. `MediaError`'s variant names stay (`Codec`, `CodecUnavailable`,
    `CodecStopped`); only their messages say "media plugin".
15. `InstalledPlugins::media()` stays one `MediaPlugin` per plugin;
    `MediaRegistry::components()` gives one entry per component.
16. Export processes come from the same factory as decoding ones, told
    their purpose by `ProviderUse`; `Describe` and `Export` get no shared
    memory in P2a.
17. `--loop` is accepted for every video output; WebM ignores it.
18. A software renderer is offered no YUV format (today's
    `ColorConversion::Auto` rule).
24. `EncoderChoice.options` holds `SettingInput { Text, Value }`.
25. Decoded frames on the RGBA-input paths (MOV, WebM, MP4 on a software
    renderer) may differ from `main`'s by at most 1 code value, since
    `sws_scale` and the `scale` filter can round differently. None did in
    the checks below.
26. The WebM muxer derives Opus's trailing discard padding from the last
    packet's shortened `duration` against the samples its TOC gives, since
    `Packet` has no skip-samples field (R20).

Differences from the spec's phase plan that need no deviation:

- The deadlines and the manifest's `inputFormats` land in P2a, not P2b.
- `plugin.json`'s libx264 `inputFormats` leaves out the spec example's
  `nv12` and `p010`, which P2a never offers.
- `MediaRegistry::describe` takes a `kill` argument (the kill switch the
  process is started with), so Cancel export and the editor's kill reach a
  describing process too.

The spec's Decisions table said the `describe` cache is keyed by "plugin
folder and approval fingerprint"; it now says what its Capability
discovery section and the code use (folder, manifest version, executable
size and modification time).

## Docs

The user guide's export page (en and ja) explains that the extension picks
the container, replaces "Encoding quality and speed" with "Choosing
encoders and settings" (`--list-components`, `--describe`,
`--video-encoder`/`--video-option`, `--audio-encoder none`, the bundled
FFmpeg plugin's keys, `--preset` as an export preset, the GPU or encoder
conversion), says `--loop` works on any video and WebM ignores it, and
documents the JSON output's `muxer`, `video`, `audio`, and `files` and the
new error codes, with MCP's tools and `@celesta.studio/exporter`. The
plugins page describes the encoder and muxer keys and how components are
chosen; the troubleshooting page says `unsupported_output` and
`component_unavailable`. The Agent Skill's `export.md` gains "Encoders and
settings" (the same keys, `--describe` first, MCP and TypeScript), and
`errors.md`, `plugins.md`, `setup.md`, `verify.md`, `performance.md`,
`subtitles.md`, and `SKILL.md` follow. HANDOFF.md's crate rows and export
notes, and the READMEs and scripts of `examples/greenscreen`,
`examples/afterimage`, `examples/prism`, and `examples/perfect-loop`, use
`--video-option`. Left as they are: `examples/delta/v0.6.0` (a film of
v0.6.0's features, which shows `--bit-depth 10` as that release had it)
and the code highlighter's token example (`--bit-depth` as one
`parameter`, which its tests check).

## Checks

On macOS 26.6.2, Apple M4 (10 cores, 32 GB), Metal, with this branch's
FFmpeg plugin staged by `build.mts` on `CELESTA_PLUGIN_PATH` and
`PKG_CONFIG_PATH` at Homebrew's `ffmpeg@8`:

- `cargo build -p celesta-codec-test`, then `cargo test --workspace`
  (2,573 passed, 1 ignored as before),
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, and
  `cargo fmt --all -- --check` (clean once Task 11's `celesta-mcp` files
  were formatted, in their own commit).
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (458 passed), its clippy with `-D warnings`, and `cargo fmt
  --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg -- --check`.
- `pnpm run test:runtime` (every package passed; `@celesta.studio/cli`
  118), `pnpm --dir scripts/plugins test` (217 passed),
  `pnpm --dir packages/website test` (52 passed), and `git diff --check`.

Unit and integration tests cover
the rest (Tasks 1–14): manifests with every component kind and bad
entries; the settings rules; every protocol 2 message through a fake
provider; the `describe` cache, its key, and its failures; every
`resolve_output` step, ranking, and error code; the interleaver's order,
spill, and batches; the session's negotiation, channel conversion,
resampling, and pre-roll; the CLI's flags and listings; MCP's arguments and
tools; the TypeScript options; and each FFmpeg component through the
program (MP4, MOV, and WebM export and read back, the looping MP4's first
hit, the GPU color path against the encoder's).

### End to end, against `main`

`main` at `14d0c5ab` (origin's, which includes #761 after this branch's
base `01b85f23`) was built in a temporary `git worktree` (release
`celesta-exporter`, its FFmpeg plugin staged with its own `build.mts`),
and so was this branch. Each exporter was copied alone into a scratch
folder with the same React runtime, given only its own plugin folder on
`CELESTA_PLUGIN_PATH`, and run with an empty `HOME`. Each exported:
`examples/greenscreen/composite.tsx` as MP4, and an entry with a `<Video>`
(the green-screen footage) and an `<Audio>` (`examples/assets/voices/001.wav`)
as MP4, as MP4 with `--loop`, as MOV, and as WebM. Every pair was decoded
with Homebrew's `ffmpeg@8` (`-pix_fmt rgba64le -f rawvideo` for the video,
`-f f32le` for the audio) and compared with `cmp`: **every decoded video
and every decoded audio stream is identical**, the MOV and WebM paths
included (RGBA input through `sws_scale`), so their maximum per-channel
difference is 0 and their PSNR infinite; deviation 25's tolerance was not
needed. The full SHA-256 of each decoded stream (the same for both
exporters):

| Output | Decoded video (`rgba64le`) | Decoded audio (`f32le`) |
| --- | --- | --- |
| green screen, MP4 | `2eb3d95fdbb1061d1bbef50a297ea0b10591f452331e283be9e1058240045856` | `b1407a10b6eb268a871cd104a767f27d3895e5f06208b6c2a8aadef58932c8ec` |
| `<Video>` + `<Audio>`, MP4 | `3c163399ef8570b8b8dfc42248d088955c6b09f642dd3f0e09ce15c0ef07886a` | `7afaceff4d0bb275b6a2b4335d542283eb51026034e9e966982ce541012faf43` |
| the same with `--loop` | `3c163399ef8570b8b8dfc42248d088955c6b09f642dd3f0e09ce15c0ef07886a` | `5c298749f9a4354b9832789e69041550e09205c432b38286548170fd2711dee9` |
| the same as MOV | `258b4b36ee9af20c1643b5326b49038d2c3f929f00b12697b2f9026a1d9b474d` | `c1e4318c94a33b51554bc7b4e8bd71fd50fa7c0ba15dd66e3731343a31efffde` |
| the same as WebM | `9923908c28ad600a8332a6a07fee08242274ac4d4b5e809dcb4541348b5c1bd7` | `6cd2ad43b8e55e365b082e9d5199732f5492c2e1b0d58ec48ac46938d82bf17d` |

The files themselves differ, as the spec expects (packet-level muxing).
Their full SHA-256 values:

| Output | `main` | This branch |
| --- | --- | --- |
| green screen, MP4 | `34f34172ca6c0a3e02f9d0e7bb625668697ea8f4012fd0cef6ab388a807baff6` (322,920 bytes) | `bfc7be676037b6fd0e0d8a2f3bee11fe2f10e7511e93a3c449743ee1db3a547e` (322,920 bytes) |
| `<Video>` + `<Audio>`, MP4 | `52e07cbb91f4a8f4127e86b8743a448d82ab15ba00ee515c89008ad8910a3558` (210,981) | `51103796c26302e5d0d676b5b26a65a1c2c13c83c980e30c71fa0c4f111fff2e` (210,981) |
| the same with `--loop` | `dda59d644ef0211729c59754cfb9aff4b57625b5f5ee6d6b8551f096420cc077` (212,184) | `83febef6423621d303e6959f0125d1c9e2120b4188335ba02d9a1368614b46bf` (212,184) |
| the same as MOV | `ad2407817f0e6a144f2d2b192a33c7277072a0705b8feea099fbac7b9fd381d8` (13,009,555) | `35bd75332e27d244969bf5d6f69cd97a83ceea529f9207e351ac105f6a0a4e4b` (13,009,211) |
| the same as WebM | `b2327c3d70c6858d42794356262373e81a2af0fb78db32db38269a36ccd1610c` (184,446) | `7c8caba991176f68dd4f43ecf6312c342cbe380c7fb3d8576c412af319b30aec` (184,422) |

`main`'s four MP4 and MOV files match the shortened hashes P1's note
recorded, so `main`'s output has not changed since P1. The branch's
reports name `ffmpeg/mp4` with `ffmpeg/libx264` (`avc1.64001F` for the
green screen, `avc1.64001E` for the 640×360 entry) and `ffmpeg/aac`
(`mp4a.40.2`), `ffmpeg/mov` with `ffmpeg/prores` (`ap4h`, `alpha: true`)
and `ffmpeg/pcm` (`lpcm.s24le`), and `ffmpeg/webm` with `ffmpeg/vp9`
(`vp09.00.10.08`, `alpha: true`) and `ffmpeg/opus`; the loudness figures
equal `main`'s.

Also checked with the branch's exporter:

- With an empty plugin folder, a video export's JSON report is
  `unsupported_output`, "no media plugin offers a muxer", with the install
  hint.
- `--preset ultrafast` is `unknown_preset`, "unknown preset `ultrafast`",
  hint `use --video-option preset=ultrafast`.
- `--list-components --check` lists FFmpeg's ten components (decoder
  `ffmpeg`; `libx264`, `prores`, `vp9`; `aac`, `opus`, `pcm`; `mp4`,
  `mov`, `webm`), each available; `--describe ffmpeg/libx264` lists its six
  settings.

Not run: the MP4 path on a software renderer (RGBA input to libx264),
which needs lavapipe or WARP (CI); Linux and Windows; the editor by hand.

### Performance

Release builds of both, the same machine, medians of three interleaved
runs (the order alternating between runs), with `main`'s range beside
each. The machine had an IDE, a browser, and another project's `esbuild`
watcher open (about 10% of a core each); the 1-minute load average read
about 7 before the first round (falling after the builds) and up to 40 at
its end, mostly the encoders' own threads.

`codec-bench` (`plugins/ffmpeg/examples/codec-bench.rs`, 120 frames of a
`testsrc2` H.264 file; ms per frame):

| | `main` | P2a |
| --- | --- | --- |
| decode in process, 1080p | 0.57 (0.55–0.58) | 0.60 (0.54–0.62) |
| decode through the plugin, 1080p | 0.88 (0.87–0.88) | 0.89 (0.85–0.89) |
| decode in process, 2160p | 2.43 (2.41–2.67) | 2.44 (2.18–2.64) |
| decode through the plugin, 2160p | 3.73 (3.63–4.10) | 3.67 (3.65–4.05) |
| encode in process, 1080p | 1.66 (1.54–1.74) | **2.72** (2.46–2.86) |
| encode through the plugin, 1080p | 2.69 (2.67–2.76) | **3.83** (3.69–3.97) |
| encode in process, 2160p | 7.09 (6.39–7.09) | **11.65** (9.95–11.89) |
| encode through the plugin, 2160p | 12.24 (11.76–14.94) | **16.87** (16.28–18.19) |

A second round gave the same picture (encode in process 1.63 against 2.82
at 1080p and 8.64 against 11.79 at 2160p; through the plugin 2.84 against
3.95 and 14.88 against 17.21). The bench's encode differs in one
parameter: `main`'s used CRF 23 and P2a's leaves the default 18. A scratch
build of P2a's bench at CRF 23 measured the same (3.00, 3.89, 11.74, and
17.47), so the CRF does not explain the gap.

Exports of a React entry of 300 frames (24 moving rectangles and a text
over a full-frame background; seconds of wall time, including start-up):

| | `main` | P2a |
| --- | --- | --- |
| 1920×1080 MP4, x264 `ultrafast` | 0.753 (0.748–0.809) | 0.722 (0.712–0.778) |
| 1920×1080 MP4, x264 `medium` | 2.244 (1.967–2.330) | 2.293 (1.970–2.310) |
| 3840×2160 MP4, x264 `ultrafast` | 2.266 (2.140–2.300) | 2.258 (2.155–2.461) |
| 3840×2160 MP4, x264 `medium` | 6.746 (6.395–7.220) | 6.919 (6.443–8.098) |
| 3840×2160 MOV, ProRes 4444 | 19.30 (18.04–19.73) | **24.14** (23.66–25.23) |

(`main`: `--preset ultrafast`; P2a: `--video-option preset=ultrafast`.)
A second round of the MOV export gave 20.16 (17.83–20.19) against 24.93
(24.31–26.53), with less CPU time on P2a's side (user 102–125 s against
`main`'s 145–169 s).

**P2a is slower than `main` beyond `main`'s spread wherever the encoder
converts RGBA**: the bench's encode (it offers `rgba8` only), 36–64%
slower, and the ProRes 4444 export at 4K (`rgba16`), about 25% slower.
The MP4 exports, which the GPU converts to `yuv420p`, and decoding are at
`main`'s speed. This breaks the spec's "the new path must not be slower"
on those paths. The likely cause, not investigated further in this task:
`main`'s ez-ffmpeg pipeline ran the `scale` filter and the encoder on
their own threads, so converting one frame overlapped encoding the last,
and the plugin answered once ez-ffmpeg had queued the frame; P2a converts with
`sws_scale` on the request's thread, then encodes, and answers with the
packets, one request at a time, so the conversion, the encoder, and the
renderer take turns (the lower CPU time fits). P2b's send window lets the
renderer overlap the plugin; the conversion itself would need threads in
the plugin (sliced `sws_scale`, or a converting thread ahead of the
encoder).

## Not done

- **Slower RGBA-input encoding** (see Performance), against the spec's
  requirement; to fix before the PR or in P2b.
- **The shared-memory ring and the send window** (P2b): every frame
  crosses the pipe, one request at a time, and `Export` processes get no
  shared memory.
- **BT.709** (P2c): the GPU still converts with BT.601, H.264 stays
  untagged, and `nv12` and `p010` are never offered.
- **Presets** (P3): `--preset` always fails with `unknown_preset`, and
  `--preset-file` is checked but unused. The editor has no export dialog
  and still picks the format by extension.
- **Stills through components** (P4): `--frame`, `--every`, and contact
  sheets keep their own encoders.
- **"FFmpeg first among bundled" is not enforced.** The spec's ranking
  puts FFmpeg first among bundled plugins; bundled plugins are ranked by
  their path, which happens to put `ffmpeg` first today but would put a
  P5 `avfoundation` plugin before it. P5 has to order them.
- Plain-text errors (without `--json`) do not print the error's hint, so
  `--preset ultrafast` and the no-plugin error lose theirs there.
- MCP's `list_components` and `describe_component` make a registry per
  call, so their `describe` is never cached.
- Runtime type names still say "codec" where deviation 14 keeps them.
