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
     CLI and from TypeScript, which passes them to the CLI as
     `--video-option`/`--audio-option` text; `Value` from MCP; deviation
     24) and validated
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
    input is converted by libswscale on its own threads (`SWS_BICUBIC`;
    BT.601 for libx264, BT.709 for ProRes and VP9, as `main`, deviation
    8), into frames the session reuses once the encoder lets go of them. An
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

Release builds of `main` (`14d0c5ab`, built in a temporary `git worktree`)
and of this branch before and after the fix below, the same machine,
medians of six interleaved runs (two rounds of three, the order rotating
between runs), with each one's range beside it. The machine had an IDE,
browsers, and another project's `esbuild` watcher open; the 1-minute load
average read 17 at the first round's start, 102 at the second's, and 37
at the end, so every figure is higher and wider than on an idle machine,
and differences under about 10% are within its noise (the decode rows,
which run no changed code, differ from `main`'s by up to 8%).

`codec-bench` (`plugins/ffmpeg/examples/codec-bench.rs`, 120 frames of a
`testsrc2` H.264 file; ms per frame):

| | `main` | P2a before | P2a |
| --- | --- | --- | --- |
| decode in process, 1080p | 0.72 (0.61–0.80) | 0.77 (0.59–0.90) | 0.78 (0.60–1.34) |
| decode through the plugin, 1080p | 1.25 (0.87–1.58) | 1.24 (1.00–1.65) | 1.22 (0.95–1.89) |
| decode in process, 2160p | 3.31 (2.74–3.86) | 3.74 (3.27–3.84) | 3.49 (2.86–3.70) |
| decode through the plugin, 2160p | 5.05 (4.79–6.18) | 5.06 (4.88–5.65) | 5.22 (4.63–5.32) |
| encode in process, 1080p | 2.52 (2.10–3.80) | 3.91 (2.62–4.67) | 2.53 (1.71–4.08) |
| encode through the plugin, 1080p | 4.46 (2.66–6.34) | 4.93 (3.94–5.87) | 4.03 (2.72–4.37) |
| encode in process, 2160p | 11.00 (9.86–14.51) | 15.26 (13.37–18.76) | 11.30 (9.03–12.64) |
| encode through the plugin, 2160p | 19.77 (15.16–23.83) | 22.17 (19.16–31.69) | 16.87 (14.67–17.34) |

The bench's encode differs in one parameter: `main`'s used CRF 23 and
P2a's leaves the default 18; a build of P2a's bench at CRF 23 measured the
same as at 18 before the fix.

Exports of a React entry of 300 frames (24 moving rectangles and a text
over a full-frame background; seconds of wall time, including start-up):

| | `main` | P2a before | P2a |
| --- | --- | --- | --- |
| 1920×1080 MP4, x264 `ultrafast` | 0.996 (0.922–1.098) | 0.948 (0.852–1.732) | 0.995 (0.854–1.785) |
| 1920×1080 MP4, x264 `medium` | 3.397 (2.901–4.080) | 3.354 (2.680–3.970) | 3.382 (2.790–3.668) |
| 3840×2160 MP4, x264 `ultrafast` | 3.778 (2.694–5.481) | 3.571 (3.024–5.120) | 3.399 (2.986–4.078) |
| 3840×2160 MP4, x264 `medium` | 11.215 (9.135–13.615) | 10.184 (9.139–15.790) | 11.241 (9.399–12.168) |
| 3840×2160 MOV, ProRes 4444 | 26.62 (24.13–29.18) | 32.52 (30.42–35.62) | 26.76 (25.07–29.01) |

(`main`: `--preset ultrafast`; P2a: `--video-option preset=ultrafast`.)
The MOV export's user CPU time is 199–217 s for `main`, 151–170 s before
the fix, and 192–219 s after it. A separate run of the MOV export alone,
six interleaved pairs, gave `main` 26.59 (21.74–29.96) against 25.26
(21.58–31.22).

**After the fix, every row's median is within `main`'s measured range.**
Before it, wherever the encoder converted RGBA (the bench's encode, which
offers `rgba8` only, and the ProRes 4444 export, `rgba16`), P2a was slower:
beyond `main`'s range in process (55% at 1080p, 39% at 2160p) and for the
ProRes export (22%); through the plugin its medians (10% and 12% slower)
fell inside `main`'s wide range here. An earlier, quieter measurement had
the bench's encode 38–64% slower and the ProRes export 25%. The MP4
exports, which the GPU converts to `yuv420p`, and decoding do not go
through the changed code.

The cause, profiled in the bench: P2a converted with a single-threaded
`sws_scale` on the request's thread (2.1 of 2.7 ms per frame at 1080p, 9.3
of 11.6 ms at 2160p, against 0.3 and 1.1 ms for the encode), where
`main`'s `scale` filter ran on libswscale's threads (libavfilter gives a
graph's filters its automatic thread count, which ez-ffmpeg leaves at 0).
And for a frame-threaded encoder (`prores_ks`), the encoder still held the
session's one frame at the next request, so `av_frame_make_writable` copied
the whole 4K frame first (7.6 ms per frame, 39 of 40 frames), where
`main`'s frames came from libavfilter's pool. The fix
(`plugins/ffmpeg/src/ffi.rs`, `video_encoder.rs`): the `Scaler` is set up
with `sws_alloc_context`, an automatic thread count, and `sws_init_context`,
and converts with `sws_scale_frame` from a frame borrowing the request's
pixels; the session keeps a list of frames and takes one the encoder has
let go of, adding one only when it holds them all (libx264 copies each
frame in and keeps one; `prores_ks` settles at about eleven at 4K, as many
as `av_frame_make_writable` had kept alive, which stay allocated until the
session ends; see Not done). The conversion still runs on
the request's thread, before the encode; a converting thread ahead of the
encoder was not needed to reach `main`'s speed.

The output is unchanged. A unit test
(`threaded_conversions_equal_single_threaded_ones`) checks that every
conversion the encoders make gives the same bytes on threads as
`sws_getContext` and `sws_scale` give at 642×362. Against `main`'s
ez-ffmpeg `VideoWriter` in the same process (a scratch test, 40 frames
of noisy content, at 320×240, 642×362, and 1920×1080): RGBA8 to libx264
8-bit (`medium`, CRF 18) and RGBA16 to ProRes 4444 and VP9 with alpha give
identical packets at every size, as YUV input to libx264 does. RGBA16 and
`yuv420p10` to 10-bit libx264 are identical at 320×240 only: above it,
10-bit libx264 differs from run to run in `main`'s pipeline too (0 of 40
packets equal between two runs of the same input), so those packets cannot
be compared. End to end, this branch's `<Video>` + `<Audio>` entry as MOV
and as WebM and the 4K ProRes export above have the same encoded video
packets as `main`'s (`-c copy -f hash`) and decode to the SHA-256 values
in the table under Checks.

## Not done

- **A performance re-run on a quiet machine** (ruling R25): the
  measurement above ran at load averages of 17–102, so it shows parity
  with `main` only within about 10%, while the spec asks for "not slower".
  R25 made it owed before #786 left draft. #786 left draft without it:
  since then the sessions for P2b through P7 have been building on this
  machine in parallel, and no quiet window has come (P2b's note on #793
  measured at load 9–78, and the load read 37 when this was written). The
  condition is moved, not dropped: the run is owed before #786 merges.
  It covers the five export rows and the two encode bench rows, release,
  six interleaved runs, replacing or added beside the table above. Until
  then parity stands as "within `main`'s range under load", for P2a and
  for P2b alike.
- **The frame pool is not capped**: a frame-threaded encoder keeps as
  many frames as it has in flight, and the session keeps them until it
  ends. For ProRes 4444 at 4K that is about eleven `yuva444p10le` frames
  of 66 MB, about 0.7 GB resident for the export: the peak the old
  `av_frame_make_writable` path reached too, but no longer freed between
  frames. The encoder's thread count bounds it (`thread_count` 0
  is the CPU count).
- **10-bit H.264 is not comparable packet for packet**: libx264 at 10
  bits differs between runs of the same input above 320×240 on `main` too
  (see Performance), so only 8-bit H.264, ProRes, and VP9 packets were
  checked identical at larger sizes.
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
- Runtime type names still say "codec" where deviation 14 keeps them.
- **The audio of the whole mix is held in memory, unbounded.**
  `encode_audio` (`crates/exporter/src/encode.rs`) collects every chunk's
  packets before any reaches the interleaver. The spec accepted this for
  AAC (about 14 MB per 10 minutes), but `ffmpeg/pcm` at s24, 48 kHz
  stereo is about 1 MB/s, some 170 MB for a 10-minute MOV, and
  `convert_channels` and `resample_audio` copy the mix on top, while the
  video side is capped at 256 MB.
- **The spill file survives a hard kill.** The interleaver's spill is a
  `NamedTempFile` beside the output (`crates/exporter/src/interleave.rs`),
  so a SIGKILL or crash leaves a `.tmpXXXX` file there, as it leaves the
  staged `.celesta-*` output. Cancel and error paths remove both
  (`cancel_during_encoding_leaves_no_files`).
- **WebM's timestamps are shifted by the Opus delay.** `ff_webm_muxer`
  has no `AVFMT_TS_NEGATIVE` and the muxer leaves `avoid_negative_ts` at
  its default, so libavformat shifts both streams by the audio's first
  dts (−312 samples): video starts at +6.5 ms. `main` did the same
  through ez-ffmpeg, so parity holds;
  `opus_webm_declares_its_codec_delay` checks only the decoded length,
  within 960 frames, not the first video packet's time.
- **The `describe` cache key can miss a rebuild.** It is the plugin's
  folder, manifest `version`, and executable size and modification time
  (`MediaRegistry`), so a plugin rebuilt within the same second at the
  same size, on a filesystem with 1 s modification times, keeps its old
  answer until Celesta restarts. Recorded as a deviation; low impact.

## Review follow-ups

Each task's review, and the whole-branch review after Task 16, changed
the following. The rulings are the executor's, numbered in the session's
ledger; only those with an effect on the code are listed.

Rulings:

- R2: `validate` and `differences` judge `visibleWhen` on the given
  values over the schema's defaults, so a missing key's default decides
  visibility.
- R4, R5, R6: a default encoder is the first *available* candidate in
  rank order; a named project muxer's own plugin ranks first for its
  encoders (tested); a muxer without `defaultVideo`/`defaultAudio` takes
  its first codec, and a muxer taking audio with no audio encoder
  installed needs `--audio-encoder none` rather than dropping the audio.
- R7: a named muxer is not checked against the output's extension.
- R8: `resample_audio` trims no output delay: rubato's sinc output is
  already lined up with its input (now pinned by a test, below).
- R9, R10: `finish_mix` stays on the mix thread; with no audio encoder no
  mix is made and the report has no loudness (warned when a loudness
  target was given).
- R12: `--loop` is accepted for every output from Task 9 (deviation 17).
- R13: `flush` writes the staged backlog in batches of at most 32 MiB,
  so the 256 MB memory limit holds; the final review added a packet cap
  (below).
- R15: `--preset-file` must be a readable JSON object; nothing reads it
  until P3.
- R16, R17, R19: the avcC carries the High-profile fields beyond
  Baseline/Main/Extended; a zero packet duration becomes the frame
  duration; the encoder's time base is one frame, as `main`'s, with
  packets rescaled to the track timescale, so NTSC rates stay identical.
- R18: FFmpeg's log level is `AV_LOG_ERROR`, so libx264's statistics do
  not flood the plugin's stderr.
- R20: WebM's Opus trailing padding comes from the last packet's
  shortened duration (deviation 26).
- R21, R22: video streams are marked progressive and ProRes with alpha has
  depth 32, and AAC's description is FFmpeg's full extradata, as `main`'s
  files have them.
- R23: the GPU colour test skips, with a `skipping` line, where the
  renderer offers no YUV even under `CELESTA_REQUIRE_GPU=1` (lavapipe).
- R24: the RGBA-path regression became Task 16 (swscale on threads, frame
  reuse; see Performance).

Fixed in the task reviews: the R5 test and fixture order (Task 5); the
batched flush, a warning for a loudness target with no audio, and the
cancel test (Task 9); the MCP tool listing and behavioural tests (Task
11); `test-package.mjs`'s loop, a GPU colour test that fails when forced
through RGBA, and a partial file left when a packet allocation failed
(Task 13); `listComponents` and `describeComponent` aborts that waited
ten seconds (Task 14); the spec's Phases rows (Task 15); and a test that a frame is
reused only once the encoder lets go of it (Task 16).

Fixed after the whole-branch review:

- A `writePackets` batch stops at 16 × 1024 packets as well as 32 MiB:
  each packet costs about 75 bytes of JSON header, and the header is
  capped at 16 MiB, so a staged backlog of tiny packets could fail the
  export at its end.
- Errors without `--json` print their hint on a `hint:` line, so
  `--preset fast` shows `use --video-option preset=fast` and an export
  with no media plugin shows how to install one. An unknown `--describe`
  under `--json`, and MCP's `describe_component` errors, go through the
  shared error report.
- `validate` checks every given value before visibility, so
  `{"rateControl":"abr","crf":18}` reports `rateControl`.
- `resample_audio` of no frames returns no frames; a test pins R8. It
  found that upsampling by a large ratio trails by just under one input
  frame (5 output frames from 8 to 48 kHz, 0.1 ms); 44.1 ↔ 48 kHz and
  downsampling are within one output frame.
- The test codec keeps a muxer's session until finishing succeeds, so a
  failed finish can still be aborted and its file removed; its
  garbage-packet test no longer makes lz4 allocate about 4 GiB.
- MCP holds one `MediaRegistry` (the one its exports use), so `describe`
  answers are cached across tool calls; a listing that discovers other
  plugins replaces it, for exports too.
- The FFmpeg plugin checks `av_dict_set`, refuses a `tune` x264 does not
  know as an invalid `tune` setting, and range-checks the SPS fields the
  avcC takes; tests read libx264's own High 10 and High 4:4:4 SPSs and one
  with an emulation prevention byte inside the fields read.
- An audio encoder answering other than 1 or 2 channels is refused as an
  invalid `channels` setting, and a muxer answering `files: []` reports
  the output path.

Left open from the whole-branch review: `describe`'s whole answer is
read strictly as one list of components, so anything in it Celesta does
not know, an `inputFormats` format or a setting's `type` alike, fails the
whole answer and makes every component of the plugin unavailable (as
spec:327 asks); a muxer may refuse a codec string the resolve matched
by family (`lpcm` against `lpcm.s24le`); one load-dependent run of
`a_plugins_refusal_is_an_invalid_option` saw the test codec's process
exit; resolving may start candidate processes it then stops; and
`--list-components --check` and MCP pass a kill switch nothing kills.

Fixed after the PR review (#786):

- The FFmpeg plugin keeps a muxer's session until finishing succeeds, as
  the test codec does, so `abortMuxer` after a failed `av_write_trailer`
  still names the file to remove (tested by failing a `+faststart`
  trailer).
- `validate` refuses an `integer` value outside i64's range (below -2^63,
  or from 2^63 on), naming the key (`invalid_option`), so a whole `1e300`
  for `bitrate`, which has no `max`, no longer saturates through `as i64`.
  `step` stays a hint for the editor and is not checked.
- `--loop` on MOV has a decode-and-compare test: the looping export's
  sound starts at 0 and lasts the video's second, and it decodes to the
  same samples as the export without `--loop`.
- `docs/performance/path-rasterization.md` uses `--video-option
  preset=medium --video-option crf=18`, and the media crate's module docs
  point at the spec and the P1 and P2a handoffs.
