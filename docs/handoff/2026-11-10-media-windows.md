# The Media Foundation media plugin, P6 (2026-11-10)

Phase P6 of the media components design
(`docs/superpowers/specs/2026-11-08-media-components-design.md`; plan
`docs/superpowers/plans/2026-11-08-media-components-p6.md`), stacked on P5
(`docs/handoff/2026-11-09-media-avfoundation.md`). A third first-party media
plugin, `plugins/mediafoundation` (`celesta-codec-mediafoundation`), encodes
H.264 and HEVC with Media Foundation's encoder transforms (MFTs), hardware
first, and AAC with Microsoft's AAC encoder MFT, and decodes with
`IMFSourceReader`, on Windows only, through the `windows` crate and
Windows' own libraries (MIT OR Apache-2.0, no FFmpeg). It is P5's twin
less a muxer: Media Foundation's MPEG-4 sink cannot hide an encoder's delay
(deviation 71), so the plugin's packets go into another plugin's muxer,
`ffmpeg/mp4` by the bundled ranking. It shows again that components of
independent plugins combine: Media Foundation's HEVC with FFmpeg's Opus in
FFmpeg's MP4, FFmpeg's x264 with Media Foundation's AAC in FFmpeg's MP4.

## Specification

- **Plugin.** Id `mediafoundation`, Cargo package and program
  `celesta-codec-mediafoundation`, Celesta's version (`0.9.0`, bumped by
  `bump-version.mts` with the others: `FOLLOWS_CELESTA`), `roles:
  ["media"]`, protocol 2, `licenses` MIT and Apache, `"os": ["windows"]`. A
  member of the plugins workspace. `src/components.rs`, `aac_user_data.rs`,
  `annexb.rs`, `color.rs`, `convert.rs`, `edit_list.rs`, `hns.rs`,
  `names.rs`, `params.rs`, and `vui.rs` call no Media Foundation and build
  and test on every system; `src/mf/` (`startup`, `enumerate`, `mft`,
  `media_type`, `video`, `audio`, `decoder`, `provider`) is
  `#[cfg(windows)]`, and so is every test file that reaches Media
  Foundation. Elsewhere the program prints `mediafoundation: this media
  plugin runs on Windows only` and exits 1, so CI's Linux and macOS Plugins
  jobs still build the workspace. CI's Plugins Clippy step now runs on every
  system, so each OS-native plugin is linted on its own. Codec strings,
  reorder depths, the AAC codec string, colour names, and the settings
  helpers are copied from `plugins/avfoundation` (P5) and FFmpeg's plugin,
  each module saying where from; a shared crate is not done.
- **Start-up.** `MfCodec::new()` calls `CoInitializeEx(COINIT_MULTITHREADED)`
  and `MFStartup(MF_VERSION, MFSTARTUP_FULL)` once and keeps a failure for
  `describe` rather than failing. The program serves on its main thread;
  no COM object crosses threads. Sessions are released before `MFShutdown`
  (field order), and `CoUninitialize` runs only on the thread that
  initialised COM.
- **Components** (manifest and `describe`, in this order):
  - decoder `mediafoundation` "Media Foundation", extensions `mp4`, `mov`,
    `m4a`;
  - video encoders `h264` "H.264 (Media Foundation)" (`avc1`; inputs
    `nv12`, `rgba8`) and `hevc` "HEVC (Media Foundation)" (`hvc1`; `nv12`,
    `p010`, `rgba16`, `rgba8`), each describing `transports:
    ["sharedMemory", "pipe"]`;
  - audio encoder `aac` "AAC (Media Foundation)" (`mp4a`), describing
    sample rates 44100 and 48000 and channels 1 and 2;
  - no muxer: no `muxers` in `plugin.json`, none in `describe`, and
    `MediaProvider::muxer` is `None` (deviation 71).

  `h264` and `hevc` are available when `MFTEnumEx` with the hardware flags
  lists an NV12 → H.264 or NV12 → HEVC encoder, otherwise `Media Foundation
  has no H.264 encoder on this system` or `Media Foundation has no HEVC
  encoder on this system; install a GPU driver with one or the HEVC Video
  Extensions`; `aac` when it lists PCM → AAC. The decoder is available
  whenever Media Foundation started. When `MFStartup` fails, every
  component is unavailable with `Media Foundation is not available on this
  system (MFStartup failed: 0x…); on Windows Server add the Media
  Foundation feature, on an N edition install the Media Feature Pack`.
- **Settings** (keys follow P5's):
  - `h264`: `profile` `high` / `main` / `baseline` (`high`); `rateControl`
    `quality` / `bitrate` (`quality`, group "Rate control"); `quality`
    0–100 (70; `CODECAPI_AVEncCommonQuality`); `bitrate` at least 100000
    bit/s (8000000; `CODECAPI_AVEncCommonMeanBitRate` in
    `eAVEncCommonRateControlMode_UnconstrainedVBR`); `keyframeInterval`
    1–1000 (250, advanced; `CODECAPI_AVEncMPVGOPSize`); `bFrames` (`true`,
    advanced; `CODECAPI_AVEncMPVDefaultBPictureCount` 2, else 0; Baseline
    is always 0); `hardware` (`true`, advanced, "Hardware encoder").
    `CODECAPI_AVEncCommonLowLatency` is set false. The output type's
    `MF_MT_AVG_BITRATE` is `bitrate`, or 8000000 in quality mode, since the
    MFT requires one.
  - `hevc`: the same without `profile`, plus `bitDepth` `8` (Main) / `10`
    (Main 10) (`8`).
  - `aac`: `bitrate` a select of `96000`, `128000`, `160000`, `192000`
    (`192000`), the four bitrates Microsoft's encoder takes.
- **The MFT driver.** `MFTEnumEx` with `HARDWARE | SYNCMFT | ASYNCMFT |
  LOCALMFT | SORTANDFILTER` (`hardware: true`) or `SYNCMFT |
  SORTANDFILTER` (`false`; AAC always). Candidates are tried in order; one
  that fails to activate, or refuses the output type, the input type, or
  the rate control, is released and the next is tried. Refusals are led by
  the MFT's friendly name, and when every candidate refuses, a rate-control
  refusal outranks later generic ones (`InvalidSettings` on `rateControl`
  or `bitrate`: `{name} has no constant quality mode; choose Average
  bitrate`, with `, or turn Hardware off` when that MFT is a hardware one
  and `hardware` is on); without one, the component is
  `ComponentUnavailable`, every refusal in its reason (the DX12 encoders
  `MFTEnumEx` lists always refuse with `0x80041000`). The fallback covers
  `open` only. Any other `ICodecAPI` property whose
  `IsSupported` or `SetValue` fails is skipped. An MFT is asynchronous when
  `MF_TRANSFORM_ASYNC` says so once activated; it is unlocked
  (`MF_TRANSFORM_ASYNC_UNLOCK`), fed on `METransformNeedInput`, and read on
  `METransformHaveOutput`, with events polled every 1 ms on the serving
  thread (`GetEvent(MF_EVENT_FLAG_NO_WAIT)`); 60 s without one is
  `{name} stopped answering`. Draining sends `MFT_MESSAGE_COMMAND_DRAIN`
  and reads until `METransformDrainComplete`. `MFT_OUTPUT_STREAM_PROVIDES_SAMPLES`
  is honoured; `MF_E_TRANSFORM_STREAM_CHANGE` renegotiates from
  `GetOutputAvailableType(0)`, refusing another subtype, and more than 8 in
  a row are `{name} kept changing its output type`. Each try is a
  `Transform` that shuts the MFT down when dropped.
- **Video.** The output type (subtype, size, rate, square pixels,
  progressive, bitrate, `MF_MT_MPEG2_PROFILE`, colour) is set before the
  input type (`NV12` or `P010`, same size, rate, and colour). `open`
  refuses an odd width or height whatever the input (`H.264 needs even
  dimensions, not 63x35`, P5's rule), and a frame of the wrong length.
  - **Input choice** (`choose_input`): `h264` and 8-bit `hevc` take `nv12`
    when offered, else `rgba8`; 10-bit `hevc` `p010`, else `rgba16`; an
    offer without them is `InvalidSettings { key: "input" }`. `alpha` is
    always false.
  - **RGB** (deviation 72): `convert.rs` turns `rgba8` into `nv12` and
    `rgba16` into `p010` on the CPU with BT.709 limited coefficients and
    the 2×2 block's mean chroma; pure red is (63, 102, 240) in 8 bits and
    (250, 409, 960) in 10.
  - **Colour.** Only `bt709 / bt709 / bt709 / limited`, as
    `MF_MT_VIDEO_PRIMARIES`, `MF_MT_TRANSFER_FUNCTION`, `MF_MT_YUV_MATRIX`,
    and `MF_MT_VIDEO_NOMINAL_RANGE` on every type (and the `ICodecAPI`
    output-colour properties, for an MFT that honours them); any other name
    is `` Codec("unknown color {field} `{name}`") ``. The SPS that goes into
    the `avcC` or `hvcC` is rewritten to carry BT.709 limited when it lacks
    a colour description (`vui.rs`, deviation 79).
  - **Bitstream.** The MFTs give Annex B with in-band parameter sets. The
    plugin builds the `avcC` (SPS, PPS) or `hvcC` (VPS, SPS, PPS) from the
    first parameter sets it sees (else `MF_MT_MPEG_SEQUENCE_HEADER`), writes
    each sample as 4-byte length-prefixed NAL units without access unit
    delimiters or parameter sets, and refuses a later different set
    (`{name} changed its parameter sets mid-stream`). Codec strings come
    from the record: `avc1.64002A` at 1080p60 and `avc1.640034` at 4K
    whichever MFT encodes; NVIDIA's HEVC gives `hvc1.1.6.H123.90` (High
    tier), the HEVC Video Extensions' `hvc1.1.2.L123.B0`.
  - **Times.** `to_hns(t, timescale) = round(t × 10⁷ / timescale)` and back,
    in `i128` with round-half-away-from-zero: every frame time of a
    30000/1001 or 60000/1001 track round-trips over 10⁶ frames. `pts` is the
    sample time less the MFT's own delay (its first output's time less the
    first input's, subtracted in 100 ns units), so every `pts` is an
    input's exactly. `dts` is `MFSampleExtension_DecodeTimestamp` (else the
    `pts`) shifted back by `depth × frame duration` whenever the stream
    reorders, `depth` being the SPS's `max_num_reorder_frames` or
    `sps_max_num_reorder_pics` (16 without one; 0 with `bFrames` false and
    for Baseline). A packet that would have `dts > pts`, or a `dts` that
    does not increase, is `MediaError::Codec` naming the depth and is never
    written.
- **AAC** (deviations 73, 77). Microsoft's AAC encoder MFT, synchronous,
  AAC-LC, raw payload (`MF_MT_AAC_PAYLOAD_TYPE` 0), at the mix's rate when
  44100 or 48000, else 48000, and 1 channel for a mono mix, else 2. `f32`
  becomes `s16` as `round(clamp(x, −1, 1) × 32767)`, fed in chunks of at
  most 16384 frames. `open` feeds 1024 frames of silence first
  (`AAC_LEAD_IN`), and `delay` is that lead-in (`AAC_DELAY = 1024`). The
  k-th packet has `pts = first + 1024k − 1024`, `dts = pts`, duration 1024,
  except the last, whose duration is the frames it covers (48000 frames
  give 48 packets, the last of 896). The config is `mp4a.40.2` with the
  AudioSpecificConfig, the tail of the output type's `MF_MT_USER_DATA`
  after `HEAACWAVEINFO`'s 12 bytes (`11 90` at 48 kHz stereo).
- **Decoder** (deviations 75, 76). `MFCreateSourceReaderFromURL` with
  advanced video processing; video as `RGB32` made opaque RGBA (the
  stride from `MF_MT_DEFAULT_STRIDE`, bottom-up when negative), audio as
  `f32` at the file's rate and channels, then mixed to the requested
  channels and resampled with `celesta_media::resample_audio`. The reader
  rotates by `MF_MT_VIDEO_ROTATION` itself, so the plugin uses the native
  type's rotation only for the probe's displayed size. Probe names are
  FFmpeg's (`h264`, `hevc`, `aac`, `pcm_s16le` …, colour names `bt709`,
  `bt2020`, `smpte2084`, `arib-std-b67`, `bt2020nc` …), else the subtype's
  FourCC, the stream walk stopping at 64 indexes.
  - **Edit lists.** The reader ignores them, so `edit_list.rs`
    (platform-independent) reads `moov/mvhd` and, for each `trak`, `hdlr`,
    `mdhd`, and `edts/elst` (v0 and v1, MP4 and MOV), seeking past `mdat`
    and closing the file before the reader opens it. Video maps a requested
    time `t` to media time `t − start_delay + media_time / timescale`;
    audio drops the leading `media_time` frames at the file's rate, keeps
    at most the edit's duration and nothing past the `mdhd` end, and
    prepends the empty edits' delay as silence (ignored when longer than
    the presentation, so a hostile `elst` allocates nothing).
  - **Frames** are the ones shown at the requested time, the last past the
    end, seeking with `SetCurrentPosition` (kept inside `MF_PD_DURATION`,
    since the reader refuses positions past it with `0xC00D36E5`) and
    reading forward. A sequential session keeps its reader while requests
    move forward by at most a second, and closes after 30 frames unasked or
    past 16 sessions (P5's constants); nothing else keeps a reader, and
    every other request releases it before answering.
  - A file the reader cannot open, or one with no audio and no video, is
    `Unsupported`; a missing one `NotFound`. A WebM this Windows can read
    (it has the Web Media Extensions) is decoded rather than refused:
    decoders are chosen by extension, and the manifest lists only `mp4`,
    `mov`, and `m4a`. Media Foundation applies neither Opus's pre-skip nor
    its end trim there (48648 frames where FFmpeg gives 48000).
- **Packaging** (Task 9). `scripts/package-windows.ps1` builds the plugin
  with `build.mts` into the package's `plugins\mediafoundation` after
  FFmpeg, copies the MSVC runtime DLLs beside its program, and walks both
  bundled roots (`celesta-codec-ffmpeg`, `celesta-codec-mediafoundation`)
  for the licence inventory, each required to be exactly one workspace
  member. `scripts/test-package.mjs` checks the installed manifest (roles,
  ids, no muxers, one `windows-*` executable, MIT and Apache licences, its
  crates in the inventory) and exports one MP4 through `--video-encoder
  mediafoundation/h264` when `--list-components --check --json` says it is
  available, else prints `skipping the Media Foundation export: {reason}`.
  It also gained the MCP tools `describe_component` and `list_components`,
  which an earlier phase added without updating it.

## Deviations from the spec

Numbered after P5's 60–64; recorded in the spec's OS-native plugins section.

70. **No ProRes and no QuickTime muxer.** Media Foundation has neither a
    ProRes encoder nor a QuickTime sink; ProRes and MOV come from FFmpeg.
71. **No muxer.** The MPEG-4 sink cannot hide the delay (Task 1, below), so
    the plugin declares none, and its encoders go into another plugin's.
72. **RGB is converted in the plugin**, on the CPU with BT.709 limited
    coefficients (`h264` takes `rgba8`, `hevc` `rgba8` or, at 10 bits,
    `rgba16`), not by the Video Processor MFT, whose matrix the
    documentation does not pin.
73. **`aac` takes 16-bit input at 44.1 or 48 kHz, at four bitrates**, so
    `bitrate` is a select of 96000, 128000, 160000, and 192000.
74. **Hardware encoders first, with a `hardware` setting.** `false`
    enumerates software MFTs only, which the value-comparing tests pin.
    Here the software HEVC encoder is the Store's HEVC Video Extensions'
    (8-bit, no B-frames), so 10-bit `hevc` with `hardware: false` is
    `componentUnavailable` (`no software HEVC encoder; turn Hardware on`).
75. **The decoder converts as Media Foundation does and drops alpha**; the
    reader rotates, and the plugin applies edit lists itself.
76. **No file is held open between requests**, except by a sequential
    session, with P5's idle limits. The reader shares delete access, so a
    held file can be deleted but not opened for writing; the tests check
    write access.
77. **`aac` feeds a 1024-frame lead-in** and answers it as its delay: the
    MFT primes nothing but loses up to the first 700 frames it is given.
78. **A GPU MFT that lacks the chosen rate control falls back to the next
    encoder**, so the default export (`hardware: true`, `rateControl:
    quality`) works on a GPU without a quality mode. Only when every
    candidate refuses is the refusal the error.
79. **The plugin writes BT.709 into an SPS without a colour description.**
    Microsoft's software H.264 and HEVC MFTs write no `video_signal_type`,
    whatever the types and `ICodecAPI` say, and libavcodec's HEVC decoder
    then converts as BT.601 despite the MP4's `colr` box (full red decoded
    to 231). `vui.rs` parses the SPS to its VUI (HEVC's scaling lists, PCM,
    short- and long-term reference sets included), splices in a
    `video_signal_type` of 1/1/1 limited (or a minimal VUI), keeps every
    other bit, and re-adds emulation prevention; an SPS that already has a
    description is returned unchanged.

No deviation beyond 79 was needed. The rulings taken while building, in
short: `hardware_encoders_keep_the_invariants` and
`hevc_10_bit_from_p010_is_main10` run at 160x90, since NVIDIA's MFTs refuse
64x36; the reordering checks print `skipping … the encoder did not reorder`
for encoders that do not reorder, except that Microsoft's software H.264
MFT with B-frames must reorder (the final review); without a
`DecodeTimestamp` (no MFT here lacks one), `dts` is the earliest input time
not yet answered, less the reorder depth, so it never goes back; and a WebM
Windows can read is accepted, as above.

## Decisions

Verified on this machine with throwaway Rust probes before the encoders
were written (`windows` 0.62.2, kept outside the repository), on Windows 11
Home 10.0.26300.9818 (26H2), an Intel Core i7-13700F, and an NVIDIA GeForce
RTX 4070 with driver 610.88 (`32.0.16.1088`):

- **Start-up.** `MFStartup(0x00020070, MFSTARTUP_FULL)` answers `S_OK`.
- **Encoders listed**, with the hardware flags (`0x57`), in order: NV12 →
  H.264 "NVIDIA H.264 Encoder MFT", "Microsoft AVC DX12 Encoder", "H264
  Encoder MFT" (software); NV12 → HEVC "NVIDIA HEVC Encoder MFT",
  "HEVCVideoExtensionEncoder" (software), "Microsoft HEVC DX12 Encoder";
  P010 → HEVC NVIDIA's and DX12's only; PCM → AAC "Microsoft AAC Audio
  Encoder MFT". With the software flags (`0x41`): "H264 Encoder MFT",
  "HEVCVideoExtensionEncoder", none for P010, and the AAC MFT.
  `MF_TRANSFORM_FLAGS_Attribute` is `0x4` for NVIDIA's (no `ASYNCMFT` bit)
  and `0x6` for DX12's, but all four answer `MF_TRANSFORM_ASYNC = 1` once
  activated.
- **Microsoft's software H.264 MFT** takes quality mode, quality 70, GOP
  250, and `BPictureCount` 2; `LowLatency` answers `E_NOTIMPL` to
  `SetValue`. With B-frames it reorders (decode order 0, 3, 1, 2, 6, 4, 5,
  …), its SPS says `max_num_reorder_frames` 1, `DecodeTimestamp` is the
  input times in order, and **every sample time is the input's plus one
  frame** (333667 hns at 30000/1001). Without B-frames it does not reorder
  and keeps the input times. Its SPS has no `video_signal_type`.
- **NVIDIA's MFTs** refuse `BPictureCount` and `LowLatency`
  (`IsSupported` `0x80004001`, `SetValue` `0x80070057`) and never reorder;
  they refuse 64x36, 128x72, 144x80, 144x90, 146x48, and 160x36 at
  `SetOutputType` (`0xC00D6D76`, `MF_E_UNSUPPORTED_D3D_TYPE`) and accept
  146x50, 160x90, and larger; their SPS carries 1/1/1 limited.
  **Microsoft's DX12 encoders** refuse every output type (`0x80041000`),
  presumably wanting a D3D manager, and are passed over. **The HEVC Video
  Extensions' encoder** refuses `BPictureCount` 2 (`0x80070057`) and so
  never reorders (Task 4).
- **AAC.** At 48 kHz stereo and 192 kbit/s the output type's
  `MF_MT_USER_DATA` is `00 00 29 00 00 00 00 00 00 00 00 00 11 90` and
  there is no `MF_SD_AUDIO_ENCODER_DELAY`. One second of silence with a
  click at 4800 gives 47 packets of 512 bytes, and through FFmpeg's MP4
  with `delay: 0` the click decodes at 4800: **no priming**. But a click at
  frame 0, 100, or 300 decodes to silence, one at 500 to −0.28 at 523, and
  only from 700 on is it whole: **the MFT loses the start of its input**.
  After 1024 frames of silence first, a click at frame 0 decodes at full
  strength (0.85), and with `delay: 1024` lands at frame 0 (deviation 77).
- **The MPEG-4 sink** (passthrough, compressed input): negative sample
  times are dropped without an error (the audio loses its 17 pre-roll
  packets; a reordered H.264 loses its IDR, whose decode time is −1 frame,
  and then decodes no frame); no `edts`/`elst` is written from negative
  times, `MFSampleExtension_Discontinuity`, or `MF_SD_AUDIO_ENCODER_DELAY =
  17408` (the click lands at 22208); with non-negative times `ctts` is
  written but the track starts 0.033367 s late; `colr nclx` 1/1/1 limited
  is written; `MF_MPEG4SINK_MOOV_BEFORE_MDAT` gives `E_ACCESSDENIED` and
  no `moov` from a URL, a corrupt file from a byte stream. Hence ruling B
  (deviation 71).
- **The source reader** gives `RGB32` at the native size; an FFmpeg MP4
  with `-display_rotation 90` (native `MF_MT_VIDEO_ROTATION` 270) comes out
  36x64 and upright, agreeing with FFmpeg's autorotated frame; `f32` at
  48000 × 2 (MP4, MOV) and 44100 × 1 (M4A). It ignores edit lists: FFmpeg's
  own AAC M4A (`elst` 1024) puts the click at 5824 where FFmpeg finds
  4800, and a MOV's video (`elst` 2002) starts at 667333 hns; the delay
  and padding attributes are absent (`0xC00D36E6`). While a reader holds a
  file, opening it for writing fails (os error 32) but `remove_file`
  succeeds.
- **Colour in the SPS** (Task 7's review): setting
  `CODECAPI_AVEncVideoOutputColorPrimaries`, `…TransferFunction`,
  `…TransferMatrix`, and `…ColorNominalRange` does not make the software
  MFTs write a colour description, hence deviation 79.
- **CI** was not probed: the PR's first CI run shows whether
  `windows-latest` starts Media Foundation (see Not done).

## Docs

The export guide (en and ja) gains "Media Foundation components on
Windows" (en) / "Windows の Media Foundation コンポーネント" (ja) after the
AVFoundation section, in its shape: the components, that they have no muxer
and go into FFmpeg's, that FFmpeg stays the default and how to name them
(`--video-encoder`, `--audio-encoder`, `--muxer`, a preset, the dialog, MCP
and the package, where AAC's `bitrate` is a string), the measured speed and
size, the settings with `hardware`, B-frames as a request, the rate-control
fallback, 8-bit-only software HEVC, AAC's rates, `--loop`, colour,
availability and `MFStartup`, the decoder, and one combined example (run on
this machine through the release exporter: `ffmpeg/mp4` with
`mediafoundation/hevc` and `mediafoundation/aac` at 160 kbit/s). The plugins
page (en and ja) lists `mediafoundation` among the bundled plugins. The
Agent Skill's `export.md` has the Media Foundation keys beside
AVFoundation's, and `plugins.md` the bundled plugin, its build, and
`["windows"]` for `os`. HANDOFF.md has the crate's map row, the plugin's
Windows-only build and packaging, the Windows prerequisites, CI's Clippy on
every system, and the architecture boundary. The spec records "As built
(P6)", deviations 70–79 with Task 1's answers, and the P6 phase row. No
example scene covers export codecs, so none is updated.

## Checks

On Windows 11 Home 10.0.26300 (26H2), an Intel Core i7-13700F, and an
NVIDIA GeForce RTX 4070 with driver 610.88, Rust 1.99.1, `VCPKG_ROOT` at
vcpkg's FFmpeg 8.1.2 (x264, vpx, opus), at `9d1fc5c0` plus this note's
edits (the branch changes no root workspace code: `git diff 88ba9809 HEAD
-- crates Cargo.toml Cargo.lock packages/cli packages/react` is empty):

- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (611 passed, none failed or ignored), `cargo clippy --manifest-path
  plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`,
  and `cargo fmt --check` in `plugins/`: all passed. With `--show-output`,
  three lines printed `skipping`, all allowed (an encoder that does not
  reorder):
  - `h264_and_hevc_decode_times_precede_presentation (hevc's reordering
    checks)`: the HEVC Video Extensions' software encoder makes no
    B-frames;
  - `hardware_encoders_keep_the_invariants (NVIDIA H.264 Encoder MFT's
    reordering checks)` and `(NVIDIA HEVC Encoder MFT's …)`: NVIDIA's MFTs
    never reorder.

  No whole test skipped: every Media Foundation component is available
  here and the export tests ran on the GPU. Some tests also print what
  they found (the probe of `probes_what_it_wrote`, `this Windows reads
  WebM: …` from `refuses_what_it_cannot_read`, and `hevc: available =
  true` from `this_machine_lists_the_basic_encoders`).
- `pnpm install --frozen-lockfile`, `pnpm --dir scripts/plugins test`
  (219 passed, 6 skipped as before), `pnpm run test:runtime` (17 packages,
  1,044 tests), `pnpm --dir packages/website test` (52), and `git diff
  --check`: all passed.
- `cargo build -p celesta-codec-test` and `cargo fmt --all -- --check`
  passed. `cargo test --workspace` and the root Clippy failed on this
  machine for reasons outside this branch, whose root crates are those of
  P5's head:
  - `celesta-gpu-renderer`'s
    `tests::transform_3d::blurs_inside_a_plane_in_output_pixels` reads
    `[254, 0, 0, 254]` where it expects `[255, 0, 0, 255]` at the flat
    plane's centre, a rounding of this GPU (431 passed, 1 failed, 1
    ignored in that crate; twice). Run with `--no-fail-fast`, 2,112 root
    tests passed, 1 failed, 1 ignored before the hangs below.
  - `cargo clippy --workspace --all-targets --locked -- -D warnings` stops
    at `celesta-editor`: `parse_login_shell_output`
    (`crates/editor/src/editor_launch.rs:59`) is never used on Windows
    (`-D dead-code`). CI runs the root Clippy on Linux only.
  - Three tests that end a Node runtime whose `prepare()` never resolves
    hung and were stopped: `celesta-mcp`'s
    `inspect::inspect_cancel_ends_a_hanging_prepare` and
    `inspect::inspect_times_out_a_hanging_prepare_and_ends_its_runtime`
    (about 52 minutes before the test binary was killed), and
    `celesta-react-bridge`'s
    `a_runtime_stop_ends_a_prepare_that_never_resolves_when_node_is_available`
    (about 13 minutes). The root crates after them were not run.

After the final review's fixes (`5bae5023`, `625aa621`, `3aeb5639`), on
the same machine: `cargo test --manifest-path plugins/Cargo.toml
--workspace --locked` passed 617 tests (none failed or ignored), once as it
is and once with `CELESTA_REQUIRE_MEDIA_FOUNDATION=1`; both printed only the
three allowed `skipping` lines above. The workspace Clippy, `cargo fmt
--check`, and `git diff --check` passed. The Linux cross-check still stops
at `ring` and `signalsmith-stretch`; in its place, a throwaway build turned
every `cfg(windows)` outside `src/mf` off (as on Linux and macOS) and ran
the crate's Clippy with `--all-targets -D warnings`. Before the fixes it
found `VideoCodec::display_name` and `params::record_sps` dead in the test
build; after them it is clean. The edit was reverted.

The tests that pin the plan's review focus: `probing_does_not_lock_the_file`;
`h264_and_hevc_decode_times_precede_presentation` and
`mediafoundation_h264_with_ffmpeg_aac_in_ffmpeg_mp4`;
`frame_times_round_trip_through_hns` and `every_input_pts_comes_back`;
`a_silent_transform_stalls_out`, `a_refusing_candidate_falls_back`, and
`a_rate_control_refusal_falls_back`; `aac_through_ffmpegs_mp4_is_aligned`
and `aac_keeps_its_first_frames`. Also `video_streams_carry_bt709_in_the_sps`,
`edit_lists_are_honoured`, `rotated_video_is_displayed_upright`,
`hostile_start_delays_allocate_nothing`,
`a_looping_export_with_mediafoundation_aac_starts_from_its_end`, and
`ffmpeg_stays_the_default_beside_mediafoundation`. Task 9 ran
`scripts/package-windows.ps1 -ZipOnly`; its package test exported through
`mediafoundation/h264`.

### Measurements

Release builds of `celesta-exporter` (`cargo build --release -p
celesta-exporter --locked`) and of both plugins (`node
scripts/plugins/build.mts plugins/ffmpeg plugins/mediafoundation --out
<scratch>/dist`), `CELESTA_PLUGIN_PATH` holding only that folder, and every
video component named (`--muxer ffmpeg/mp4 --video-encoder …`) with default
settings except `hardware`. 300 frames (`--from 0 --to 5`) of
`examples/spectra/film.tsx` (1920x1080, 60 fps; no sound, so no audio
track), and for 4K P5's scratch copy of it (its `<Composition>` 3840x2160,
everything after `<Assets>` in a `<Group scale={2}>`; not committed). Wall
time including start-up, three rounds interleaved (each round every 1080p
export, then every 4K one), median (each run); size of the file; PSNR of
film frame 90 against the exporter's RGBA `--frame 90` PNG, decoded as a
BT.709 player does (`ffmpeg` 8:
`select='eq(n\,90)',scale=in_color_matrix=bt709:in_range=tv,format=rgb24`
and `psnr`, whose `average` is the PSNR of the mean squared error over R,
G, and B):

| Export | Wall time (s) | Size (bytes) | PSNR (dB) |
| --- | --- | --- | --- |
| 1080p `ffmpeg/libx264` (`medium`, CRF 18) | 4.786 (4.389, 4.848, 4.786) | 5,166,044 | 38.83 |
| 1080p `mediafoundation/h264`, NVIDIA (quality 70) | 3.724 (3.656, 3.724, 3.841) | 5,178,615 | 37.65 |
| 1080p `mediafoundation/h264` `hardware=false` (quality 70) | 3.943 (3.926, 3.943, 3.987) | 7,956,470 | 37.06 |
| 1080p `mediafoundation/hevc`, NVIDIA (quality 70) | 3.830 (3.895, 3.830, 3.803) | 5,199,240 | 38.04 |
| 1080p `mediafoundation/hevc` `hardware=false` (quality 70) | 3.905 (3.919, 3.851, 3.905) | 6,814,691 | 37.45 |
| 4K `ffmpeg/libx264` | 12.293 (12.293, 12.339, 12.112) | 13,817,247 | 40.45 |
| 4K `mediafoundation/h264`, NVIDIA | 6.387 (6.489, 6.387, 6.286) | 11,934,666 | 39.34 |
| 4K `mediafoundation/h264` `hardware=false` | 7.450 (7.412, 7.450, 7.508) | 20,702,089 | 38.88 |
| 4K `mediafoundation/hevc`, NVIDIA | 6.103 (6.087, 6.153, 6.103) | 11,409,023 | 39.95 |
| 4K `mediafoundation/hevc` `hardware=false` | 7.399 (7.280, 7.462, 7.399) | 15,184,906 | 38.15 |

Every file has 300 frames, BT.709 tags (`bt709`, `bt709`, `bt709`, `tv`),
and the sample entry `avc1` or `hvc1`. Codec strings: `avc1.64002A`
(1080p) and `avc1.640034` (4K) from all three H.264 encoders;
`hvc1.1.6.H123.90` and `hvc1.1.6.H153.90` from NVIDIA's HEVC (High tier),
`hvc1.1.2.L123.B0` and `hvc1.1.2.L153.B0` from the HEVC Video Extensions'
(Main tier). `hardware=true` opened NVIDIA's MFTs, not a fallback: their
H.264 has no reordering (`has_b_frames` 0), the software MFT's has
(`has_b_frames` 1, x264's 2), and the HEVC encoders write different
profile and tier fields. `hardware=false` HEVC is the HEVC Video
Extensions' encoder.

At 4K, x264's defaults took 1.9–2.0 times the GPU encoders' wall time and
1.65 times the software MFTs'. NVIDIA's H.264 was 13.6% smaller than
x264's CRF 18 at 1.11 dB lower PSNR, and its HEVC 17.4% smaller at 0.50 dB
lower; at 1080p both were within 1% of x264's size, at 1.18 and 0.79 dB
lower. Microsoft's software H.264 was 50–54% larger than x264 at 1.57–1.76
dB lower, and the HEVC Video Extensions' 10–32% larger at 1.38–2.30 dB
lower. At 1080p the wall times are close because start-up (bundling the
React entry, starting the plugins) is about half of them: in the third
round the exporter's own render rate was 97 frames/s with x264 against 134–170 with Media
Foundation, and at 4K 32 against 53–73. These are recorded only: the
spec's performance requirement is the FFmpeg path's, which this branch
does not change. No build or test of mine ran during the exports.

## Not done

- **CI on `windows-latest` is unproven.** Task 1's CI probe was not run
  (it needs a push); the PR's first CI run will show whether Media
  Foundation starts there. If `MFStartup` fails, a follow-up adds
  `Install-WindowsFeature Server-Media-Foundation` to `ci.yml`; if that
  needs a reboot or does not help, every Media Foundation test there skips
  with the `describe` reason, which "Report skipped tests" lists, and this
  machine stays the only full run. The skips go through one gate
  (`src/mf/gate.rs`): with `CELESTA_REQUIRE_MEDIA_FOUNDATION=1` a missing
  Media Foundation or component fails the test instead, as
  `CELESTA_REQUIRE_GPU=1` does a missing adapter. This machine runs with it
  set; CI does not set it until Media Foundation on `windows-latest` is
  proven. The cross-OS `cargo check --target x86_64-unknown-linux-gnu`
  could not run here (`ring` and `signalsmith-stretch` need a Linux C
  compiler), so the crate's gating off Windows was checked by reading and
  by a throwaway build with every `cfg(windows)` outside `src/mf` off (see
  Checks); CI's Linux and macOS jobs check it.
- **GPU texture sharing** (`transports: dxgiShared`): frames still cross
  shared memory or the pipe and are copied into an `IMFSample`; the GPU
  MFTs get system memory, and the DX12 encoders, which want a D3D manager,
  are passed over.
- **A hardware MFT that fails after `open`**: the fallback to the next
  candidate covers `open` only, so a hardware MFT that accepts the types
  but fails its first `ProcessInput` fails the export.
- **A muxer of its own**, under ruling B: the MPEG-4 sink would need an
  edit list it cannot write. `ffmpeg/mp4` muxes the plugin's packets.
- **HDR**: no PQ or HLG encoding; the decoder takes Media Foundation's
  conversion (no FFmpeg tone curve).
- **A shared bitstream crate for P5 and P6**: `params.rs`, the codec
  strings, the AAC codec string, `color.rs`, and the settings helpers are
  copies of P5's and FFmpeg's.
- **HEVC reordering is never exercised on this machine**: neither NVIDIA's
  MFT nor the HEVC Video Extensions' encoder makes B-frames, so the HEVC
  reorder depth (`sps_max_num_reorder_pics`) is covered only by unit tests
  on recorded parameter sets, and the stream-change renegotiation is
  untested (its cap and the `dts` fallback without `DecodeTimestamp` only by
  unit tests).
- **Not checked by a CPU renderer export**: no test can pick the software
  renderer, so the `rgba8` path is covered by the encoder test that feeds
  RGBA (`rgba8_input_is_converted_with_bt709`), not by an export.
- **The package build ran with `-ZipOnly`** (Task 9): the Inno Setup
  installer was not built here.
- **Deferred minors from review**: a failed `MFTEnumEx` reads as "no …
  encoder" with its HRESULT lost; the AAC `finish` does not check that the
  MFT gave every packet (`last.pts + duration == end`), a zero-frame finish
  leaves a lone lead-in packet, and a failed feed mid-encode leaves the
  session unusable; a refused `MeanBitRate` says "has no average bitrate
  mode" even when only that property was refused; the asynchronous
  `ProcessOutput` right after a stream change does not wait for a new
  `HaveOutput`; the reorder checks assert the first `dts` < 0 and `pts` 0
  rather than `−depth × frame`; `reorders` stays true when `BPictureCount`
  was refused; the `avcC` High-profile tail is written for profiles 100,
  110, 122, and 144 but not 244; `vui.rs` counts `dPoc`-0 entries in
  `inter_set_size` and has no re-parse check after a rewrite; the
  decoder's stream duration is `None` for files without `edts`, any
  reader-open failure is `Unsupported` (an I/O cause is hidden), and an
  unknown `MF_PD_DURATION` drops any start delay; the decoder reads audio
  back to back, ignoring the reader's timestamps (gaps), and stereo from
  more than two channels keeps the first two; about 500 lines of
  export-test helpers are copied from P5's tests; `test-package.mjs` has
  two overlapping licence assertions.
