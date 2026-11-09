# Media components P5: macOS native (AVFoundation) — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A second, independent media plugin, `plugins/avfoundation`
(`celesta-codec-avfoundation`), serving protocol 2 on macOS with
VideoToolbox video encoders (`h264`, `hevc`, `prores`), an AudioToolbox
`aac` encoder, AVAssetWriter muxers (`mp4`, `mov`) in passthrough, and an
AVAssetReader decoder (`mp4`, `mov`, `m4a`), bundled in the macOS app beside
FFmpeg and combinable with FFmpeg's components in one export.

**Architecture:**
- **Crate.** A member of the plugins workspace, written in Rust on `objc2`
  bindings. Every module that touches an Apple framework is
  `#[cfg(target_os = "macos")]`; elsewhere the library is empty and the
  program exits with a message, so the workspace (and CI's Linux and
  Windows jobs) still build and test. Bitstream helpers (codec strings,
  reorder depth, the AAC magic cookie) and the pure choices (input format,
  settings to VideoToolbox properties, availability from an encoder list)
  are platform-independent and tested everywhere.
- **Components.** `serve` (P2b) hands every frame to the plugin as an owned
  `Vec<u8>`, read out of its ring slot; the plugin copies it into an
  IOSurface-backed `CVPixelBuffer` from the session's pool and encodes
  asynchronously, collecting packets from VideoToolbox's output handler.
  The AAC encoder is a synchronous `AudioConverter`. The muxers build
  `CMFormatDescription`s from each `Track` (`TrackConfig`'s avcC / hvcC /
  AudioSpecificConfig, `Track.color`) and append `CMSampleBuffer`s to
  passthrough `AVAssetWriterInput`s.
- **Selection.** Nothing changes in Celesta's core: P4's Task 1
  (`default_rank`, deviation 58) already keeps FFmpeg first among bundled
  plugins, so AVFoundation's components are used only when named or in a
  preset. The one core-side change is in the FFmpeg plugin: its MP4 and MOV
  muxers keep the sample-entry spelling the codec string gives
  (`hvc1` vs `hev1`), so VideoToolbox's HEVC in FFmpeg's MP4 plays in
  QuickTime.
- **Packaging.** `build.mts` learns a plugin's operating systems
  (`"os": ["macos"]`) and skips it elsewhere; `package-macos.py` builds it
  into `Contents/PlugIns/avfoundation`; the package test exports through it.

**Tech stack:** Rust 2024 (MSRV 1.89); `objc2` 0.6.5, `block2` 0.6.2,
`objc2-foundation`, `objc2-core-foundation`, `objc2-av-foundation`,
`objc2-core-media`, `objc2-core-video`, `objc2-video-toolbox`,
`objc2-audio-toolbox`, `objc2-core-audio-types`, all 0.3.2 (the versions the
root workspace's `celesta-gpu-renderer` already locks: `objc2` 0.6,
`objc2-core-video` 0.3.2); VideoToolbox, AudioToolbox, CoreMedia,
CoreVideo, AVFoundation on macOS 11+; `celesta-codec-ffmpeg` (in process)
for the tests' decode-and-compare and the cross-plugin exports.

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`
(OS-native plugins (P5, P6); Phases → P5; Manifest; Protocol 2; Capability
discovery; Settings schema; Transport; Pixel formats and color; Audio —
delay, `trimsLeadingAudio`, `kCMSampleBufferAttachmentKey_TrimDurationAtStart`;
Selection → Ranking; Testing). Read it with this plan. The FFmpeg plugin
(`plugins/ffmpeg`: `lib.rs`, `components.rs`, `video_encoder.rs`,
`audio_encoder.rs`, `muxer.rs`, `h264.rs`, `tests/process.rs`,
`tests/export.rs`) is the reference for every component kind; P4's plan
(`claude/media-components-p4`, `docs/superpowers/plans/2026-11-08-media-components-p4.md`,
Task 1) defines the ranking this plan relies on.

## Global constraints

- Branch `claude/media-components-p5`, cut from `claude/media-components-p2c`
  (`c172d5d6`). The stack is P2c → P3 (#794) → P4 → P5; when P4 has a PR,
  this branch is **rebased** onto P4's (never merged). Use every name
  P2a–P2c define as spelled at the base; P3 renamed some exporter entry
  points (`resolve_preset_output`), so after the rebase use them as P3/P4
  spell them.
- The plugin: id `avfoundation`, Cargo package and program
  `celesta-codec-avfoundation`, version Celesta's (`0.9.0` today, bumped
  with the others by `bump-version.mts`), `roles: ["media"]`,
  `protocol` 2, `licenses: ["../../LICENSE-MIT", "../../LICENSE-APACHE"]`
  (no GPL: nothing in it is GPL), `"os": ["macos"]`.
- Component ids and names, exactly:
  decoder `avfoundation` "AVFoundation", extensions `["mp4", "mov", "m4a"]`;
  video encoders `h264` "H.264 (VideoToolbox)" `["avc1"]`,
  `hevc` "HEVC (VideoToolbox)" `["hvc1"]`,
  `prores` "ProRes (VideoToolbox)" `["ap4h", "apch"]`;
  audio encoder `aac` "AAC (AudioToolbox)" `["mp4a"]`;
  muxers `mp4` "MP4 (AVFoundation)" (`extensions ["mp4", "m4v"]`,
  `video ["avc1", "hvc1"]`, `audio ["mp4a"]`, `defaultVideo "avc1"`,
  `defaultAudio "mp4a"`, `trimsLeadingAudio: true`) and
  `mov` "QuickTime (AVFoundation)" (`extensions ["mov"]`,
  `video ["avc1", "hvc1", "ap4h", "apch"]`, `audio ["mp4a", "lpcm"]`,
  `defaultVideo "ap4h"`, `defaultAudio "mp4a"`, `trimsLeadingAudio: true`).
- `inputFormats` (manifest and `describe`, best first): `h264`
  `["nv12", "rgba16"]`; `hevc` `["nv12", "p010", "rgba16"]`; `prores`
  `["rgba16"]`. Every video encoder describes
  `transports: ["sharedMemory", "pipe"]`. `aac` describes
  `sampleRates: [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000]`
  (AudioToolbox's `kAudioFormatProperty_AvailableEncodeSampleRates`, read at
  `describe`) and `channels: [1, 2]`.
- Pixel buffers: `nv12` → `kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`
  (`420v`), `p010` → `kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange`
  (`x420`, 10 bits in the high bits of 16-bit LE samples, P2c's `p010`
  layout), `rgba16` → `kCVPixelFormatType_64RGBALE` (`l64r`, straight
  alpha, no swizzle; verified, see Decisions).
- `Color` is only `bt709 / bt709 / bt709 / limited` (P2a); the plugin maps
  exactly those names to `kCVImageBufferColorPrimaries_ITU_R_709_2`,
  `kCVImageBufferTransferFunction_ITU_R_709_2`,
  `kCVImageBufferYCbCrMatrix_ITU_R_709_2`, and limited range; any other
  name is `MediaError::Codec("unknown color {field} `{name}`")`, the FFmpeg
  plugin's wording.
- No Celesta crate links FFmpeg (HANDOFF.md); this plugin links none
  either. `celesta-codec-ffmpeg` is a macOS-only **dev**-dependency, for
  decoding in tests and for the cross-plugin exports.
- Rust tests stay in Rust; script tests use Vitest in TypeScript; Python
  packaging tests stay `unittest` (AGENTS.md, existing files).
- Every test that needs an Apple framework is in a `#[cfg(target_os = "macos")]`
  module or file (`#![cfg(target_os = "macos")]` at the top of a
  `tests/*.rs`). A test whose component `describe` reports unavailable
  prints `skipping <test>: <reason>` and passes (CI's report lists it); on
  the user's Mac (Apple M4, macOS 26) none may skip. Export tests also
  skip without a GPU, as the FFmpeg plugin's do.
- Never amend commits; each task ends in a new commit. Stage files
  explicitly; never commit `.superpowers/`, `plugins/dist`, or scratch
  files. Commits are signed through 1Password; if signing fails with
  "failed to fill whole buffer", stop and ask the user to unlock 1Password,
  never bypass signing. **Every commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`**, whatever
  model wrote it.
- Builds: `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$HOME/.local/share/mise/installs/node/26/bin:$HOME/.local/share/mise/installs/pnpm/12/bin:$PATH"`
  and `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`
  (Homebrew's default `ffmpeg` is 9 and breaks the build). Root workspace
  tests need `cargo build -p celesta-codec-test` first.
- **Builds between tasks.** Task 2 adds the crate with every component
  answering `componentUnavailable` at `open` until its task lands; the
  workspace builds and its tests pass after every task. Nothing
  macOS-only may sit outside a `#[cfg(target_os = "macos")]` module, so
  CI's Linux and Windows Plugins jobs keep building; reviewers check the
  gating, since this Mac cannot build for Linux.

## Deviations from the spec (recorded in Task 10's spec edit)

Numbered from 60 (P2a 8–18 and 24–26, P2b 19–23, 27, 28, P2c 30–31, P3
40–44, P4 50–59).

60. **The trim needs two more attachments.** The spec has the AVAssetWriter
    muxers "set `kCMSampleBufferAttachmentKey_TrimDurationAtStart` on the
    leading sample buffers". Verified on this Mac (macOS 26.6): with only
    that, a passthrough AAC track gets an edit list of `trim − 2112` and no
    `roll` sample group; AVFoundation, which assumes 2112 frames of
    implicit priming for such a file, plays it correctly, and FFmpeg (and
    any reader honouring the edit list alone) plays it 2112 frames late.
    The muxer therefore also sets `kCMSampleBufferAttachmentKey_GradualDecoderRefresh = 1`
    and `"IsGradualDecoderRefreshAuthoritative" = kCFBooleanTrue` (a key
    AVAssetReader's own passthrough buffers carry; no public constant) on
    the first audio sample buffer, and sets every attachment with
    `kCMAttachmentMode_ShouldPropagate`. AVAssetWriter then writes an edit
    list hiding exactly `delay + leadingAudio` and a `roll` group of −1,
    as its own AAC encoding does, and both readers agree to the sample.
61. **Leading LPCM is cut, not trimmed.** PCM has no priming and every
    sample stands alone, so the `mov` muxer drops the sample frames before
    0 (`leadingAudio`; `delay` is 0 for PCM) from the packets' bytes
    instead of trimming them: the file plays the same everywhere and no
    edit-list behaviour is relied on.
62. **`h264` and `hevc` also take `rgba16`.** The spec has the VideoToolbox
    encoders take "`nv12`, `p010`, or `rgba16` for ProRes 4444". A software
    renderer offers no YUV (deviation 18), and an odd-sized frame cannot be
    4:2:0, so `h264` and `hevc` fall back to `rgba16`, which VideoToolbox
    converts with the session's BT.709 limited matrix (verified for ProRes:
    the decoded 12-bit codes are BT.709's to the code). `h264` takes no
    `p010`: VideoToolbox's H.264 is 8-bit only.
63. **`avfoundation/mov`'s default audio is `mp4a`.** FFmpeg's `mov`
    defaults to `lpcm`; this plugin has no PCM encoder, so its `mov` takes
    `lpcm` from another plugin when named and defaults to its own AAC, and
    the plugin works alone.
64. **The decoder converts colour as AVFoundation does.** FFmpeg's decoder
    tone-maps HDR to sRGB with Celesta's curve (`plugins/ffmpeg/src/color.rs`);
    this one asks AVAssetReader for 8-bit BGRA with
    `AVVideoColorPropertiesKey` BT.709 and takes AVFoundation's conversion.
    FFmpeg decodes first (bundled order), so this applies only to files
    FFmpeg cannot read or when FFmpeg is disabled.

## Decisions the spec left to this plan

- **Verified on this Mac before planning** (Swift probes, results kept in
  Task 10's handoff note):
  - VideoToolbox's ProRes 4444 takes `kCVPixelFormatType_64RGBALE`
    directly: a frame of straight-alpha red and half-transparent green
    encodes byte-identical to the documented big-endian
    `kCVPixelFormatType_64ARGB`, and decodes (FFmpeg, `yuva444p12le`) to
    Y 1001, Cb 1637, Cr 3840, A 4095 and Y 2762, Cb 666, Cr 420, A 2048:
    BT.709 limited range, alpha kept. **No swizzle.**
  - Every VideoToolbox encoder accepted every pixel format tried
    (`420v`, `x420`, `l64r`, `b64a`, `y416`, `BGRA`); `kCVPixelFormatType_32RGBA`
    pixel buffers cannot be created (−6680), which is why `rgba8` is not
    an input.
  - VideoToolbox has software encoders beside the hardware ones for H.264,
    HEVC, and every ProRes profile (`com.apple.videotoolbox.videoencoder.h264`,
    `…hevc.vcp`, `…prores-4444`), so a Mac without media engines (a CI
    VM) still encodes.
  - H.264 and HEVC from VideoToolbox carry B-frames by default, with
    decode timestamps that exceed presentation timestamps (pts 1001 at
    dts 3003); the avcC and hvcC come in the first sample buffer's format
    description, `SampleDescriptionExtensionAtoms`; samples are
    length-prefixed (4 bytes). The streams are tagged BT.709 limited when
    the session's properties are set.
  - AudioToolbox's AAC-LC encoder primes 2112 frames
    (`kAudioConverterPrimeInfo.leadingFrames`), takes 8–48 kHz, and its
    applicable bitrates are 32–256 kbit/s mono and 64–320 kbit/s stereo.
    Its magic cookie is an MPEG-4 `ES_Descriptor` (tag 3) whose
    DecoderSpecificInfo (tag 5) is the AudioSpecificConfig.
  - Deviation 60's recipe, against a click at frame 4800 after 2112
    frames of priming and 16384 of pre-roll: FFmpeg and AVFoundation both
    decode the click at 4800, the edit list's media time is 18496.
- **DTS.** The plugin shifts VideoToolbox's decode timestamps back by the
  stream's reorder depth: `dts = vt_dts − depth × frame duration`, so
  `dts ≤ pts` and the first `dts` is negative, as libx264's are. `depth`
  is H.264's `max_num_reorder_frames` (VUI `bitstream_restriction`) or
  HEVC's `sps_max_num_reorder_pics[highest]`, read from the parameter sets
  in the first format description; without that field it is 16 (an upper
  bound: too large a shift is valid, too small is not); with
  `bFrames: false`, 0; ProRes is intra-only, 0. A packet that would still
  have `dts > pts` is `MediaError::Codec` naming the depth, never written.
- **Codec strings.** `avc1.PPCCLL` from avcC bytes 1–3, upper-case hex (as
  `plugins/ffmpeg/src/h264.rs` writes them). `hvc1.` per ISO/IEC 14496-15
  Annex E from hvcC: profile space letter (none, `A`, `B`, `C`) and
  profile idc, the 32 compatibility flags bit-reversed in hex without
  leading zeros, `L`/`H` for the tier and the level idc, then the six
  constraint bytes in hex, trailing zero bytes dropped. ProRes: `ap4h` or
  `apch`. AAC: `mp4a.40.<audio object type>` from the AudioSpecificConfig.
- **Settings** (keys follow FFmpeg's where they mean the same):
  - `h264`: `profile` select `high` "High" / `main` "Main" / `baseline`
    "Baseline", default `high`; `rateControl` select `quality`
    "Constant quality" / `bitrate` "Average bitrate", default `quality`,
    group "Rate control"; `quality` integer 0–100, step 1, default 70,
    group "Rate control", `visibleWhen {rateControl: quality}`,
    description "Higher is better quality and larger." (VideoToolbox's
    `Quality` is the value / 100); `bitrate` integer min 100000, unit
    `bit/s`, default 8000000, `visibleWhen {rateControl: bitrate}`
    (`AverageBitRate`); `keyframeInterval` integer 1–1000, default 250,
    advanced, description "The most frames between keyframes."
    (`MaxKeyFrameInterval`); `bFrames` boolean default `true`, advanced,
    label "B-frames", description "Reordered frames compress better; turn
    off for players that cannot decode them." (`AllowFrameReordering`).
  - `hevc`: the same without `profile`, plus `bitDepth` select `8` "8-bit
    (Main)" / `10` "10-bit (Main 10)", default `8`
    (`kVTProfileLevel_HEVC_Main_AutoLevel` / `Main10_AutoLevel`).
  - `prores`: `profile` select `4444` "4444" / `hq` "422 HQ", default
    `4444` (FFmpeg's keys and values).
  - `aac`: `bitrate` integer 32000–320000, unit `bit/s`, default 192000;
    outside the encoder's applicable range at the opened channel count
    (mono above 256000, stereo below 64000) it is
    `MediaError::InvalidSettings { key: "bitrate", message }`.
  - `mp4`, `mov`: `faststart` boolean default `true`, label "Fast start"
    (`AVAssetWriter.shouldOptimizeForNetworkUse`).
- **Input choice** (`choose_input`): `h264` → `nv12` when offered and both
  dimensions are even, else `rgba16`; `hevc` 8-bit the same, 10-bit →
  `p010` (even) else `rgba16`; `prores` → `rgba16`. `alpha` is true for
  ProRes 4444 only. An offer without the needed fallback is
  `MediaError::InvalidSettings { key: "input", .. }` as FFmpeg's encoders
  say it.
- **Sample rate.** `aac` answers the mix's rate when AudioToolbox lists it,
  else 48000; the mix's channel count (1 or 2).
- **The last AAC packet** has the duration it actually covers
  (`frames + delay − 1024 × (packets − 1)`), as libavcodec's encoders give
  it, so muxers trim the end; AVAssetWriter gets it as
  `kCMSampleBufferAttachmentKey_TrimDurationAtEnd = 1024 − duration`.
- **Output path.** Celesta stages its output as an empty file beside the
  final path (`stage_beside`) before `openMuxer`; AVAssetWriter refuses a
  URL that exists. The muxer removes the file at `output` when it is an
  empty regular file, and refuses anything else with
  `MediaError::Codec("{output} already exists")`.
- **Interleaving into AVAssetWriter.** Each input has a pending queue;
  `writePackets` appends while the input `isReadyForMoreMediaData` and
  keeps the rest queued (never blocking on one input while the other's
  packets wait); `finishMuxer` drains every queue, polling every 1 ms and
  failing with the writer's error when its status is `failed`, then marks
  the inputs finished and waits for `finishWritingWithCompletionHandler`.
- **Availability** (`describe`): a video encoder is available when
  `VTCopyVideoEncoderList` lists its codec type (`prores` needs `ap4h`;
  `hq` without `apch` is `componentUnavailable` at `open`), reason
  "VideoToolbox has no {H.264|HEVC|ProRes 4444} encoder on this Mac"; `aac`
  when `kAudioFormatProperty_Encoders` lists `kAudioFormatMPEG4AAC`,
  reason "AudioToolbox has no AAC encoder on this Mac"; the muxers and the
  decoder always.
- **Building elsewhere.** `plugin.json` gains `"os": ["macos"]`;
  `build.mts` checks it is a list of `std::env::consts::OS` names and, on
  another OS (rustc's `target_os`), prints `avfoundation: skipped, it is
  built for macos only` and writes nothing for that plugin. CI's Plugins
  Clippy step runs on macOS too (it ran on Linux only, "little code is
  behind a cfg for one OS", which this plugin ends).
- **FFmpeg's sample entries.** FFmpeg's MP4 and MOV muxers tag HEVC
  `hvc1` when the codec string's family was spelled `hvc1` and `hev1` when
  `hev1` (libavformat's default in MP4 is `hev1`, which QuickTime does not
  play), and H.264 `avc1` / `avc3` likewise: the spec's "which spelling a
  file gets is the muxer's choice, from the `TrackConfig` it receives".
- **Decoder.** Frames are the ones shown at the requested time (as
  `timestamps_match`), the last frame past the end (FFmpeg's
  `clamp_to_source_end`), at the displayed size (quarter-turn
  `preferredTransform`s rotate, as FFmpeg's autorotation does). A
  sequential session keeps its `AVAssetReader` open while requests move
  forward and reopens it on a backward or far (more than one second)
  forward jump. Audio is the first audio track through
  `AVAssetReaderAudioMixOutput` as interleaved `f32` at the requested rate
  and channels. Codec names in the probe are FFmpeg's (`h264`, `hevc`,
  `prores`, `aac`, `pcm_s16le`, …), else the four-character code; colour
  tags are FFmpeg's names (`bt709`, `bt2020`, `smpte2084`,
  `arib-std-b67`, `bt2020nc`, …). A file AVFoundation cannot open, or one
  with no audio and no video track, is `MediaError::Unsupported`.
- **Packaging.** `package-macos.py` builds `plugins/avfoundation` into
  `Contents/PlugIns` after FFmpeg (no `dylibbundler`: it links system
  frameworks only), its crates join the licence inventory, and the package
  test checks its manifest and exports one MP4 through
  `--muxer avfoundation/mp4`.
- No example scene covers export codecs, so none is updated.

## Review focus

The tests that pin each of these are in the task named.

1. **A looping export into AVFoundation's `mp4` or `mov`** hides exactly
   the encoder's delay plus the pre-roll for every reader: FFmpeg's and
   AVFoundation's decoders find a click at the same sample of the mix
   (Task 6, `aac_delay_and_preroll_are_hidden_for_every_reader`; Task 7,
   `a_looping_avfoundation_mp4_starts_its_sound_from_its_end`).
2. **VideoToolbox's B-frames in another plugin's muxer:** every H.264 and
   HEVC packet has `dts ≤ pts`, strictly increasing `dts`, so FFmpeg's
   MP4 takes them (Task 4, `h264_and_hevc_decode_times_precede_presentation`;
   Task 7, `avfoundation_hevc_with_ffmpeg_opus_in_ffmpeg_mp4`).
3. **Celesta's staged output already exists** when `openMuxer` comes:
   AVAssetWriter's refusal is not surfaced, and a non-empty file is never
   deleted (Task 6, `an_empty_staged_output_is_replaced`,
   `a_file_with_content_is_not_replaced`).
4. **One input of AVAssetWriter is not ready while the other's packets keep
   coming** (a long audio stretch after the mix arrives): no deadlock, no
   lost or reordered packet (Task 6, `long_runs_of_one_track_do_not_stall`).
5. **An odd-sized composition, or a software renderer's offer,** exports
   with `h264` and `hevc` through `rgba16` instead of failing on 4:2:0
   (Task 4, `odd_sizes_and_rgb_offers_take_rgba16`).

---

### Task 1: Plugins that build for some systems only (`scripts/plugins`, CI)

**Files:**
- Modify: `scripts/plugins/manifest.mts` (`PluginSource.os`, `PLUGIN_SOURCE_KEYS`, `checkPluginSource`), `scripts/plugins/build.mts` (skip), `.github/workflows/ci.yml` (Plugins' Clippy step: Linux and macOS, its comment)
- Test: `scripts/plugins/test/manifest.test.mts`, `scripts/plugins/test/build.test.mts`

**Interfaces:**
- Produces, in `manifest.mts`:
  ```ts
  export interface PluginSource { /* … */ os?: string[] }   // std::env::consts::OS names
  export function buildsOn(source: PluginSource, os: string): boolean   // true without `os`
  ```
  `checkPluginSource` refuses `os` that is not a non-empty array of
  non-empty strings: `${sourcePath}: os must be a list of operating systems, such as ["macos"]`.
  `os` is not copied into `celesta-plugin.json`.
- `build.mts`: after reading every `plugin.json` and before building, a
  plugin whose `buildsOn(plugin, rustPlatform(...).os)` is false is logged
  `${id}: skipped, it is built for ${os.join(', ')} only` and dropped from
  the run (its `out` directory is left as it is).

- [ ] **Step 1: Write failing tests.**
  - `manifest.test.mts`: `os is a list of systems` — `checkPluginSource({ id: 'a', version: '1.0.0', cargoPackage: 'a', os: ['macos'] }, 'p')` passes; `os: 'macos'`, `os: []`, and `os: ['']` throw the message above; `buildsOn({ os: ['macos'] }, 'linux') === false`, `buildsOn({}, 'linux') === true`; `pluginManifest` output has no `os` key.
  - `build.test.mts`: `skips a plugin built for another system` — a temporary plugin folder with `"os": ["no-such-os"]` and a `cargoPackage` that does not exist: the script exits 0, prints `skipped, it is built for no-such-os only`, runs no Cargo (the missing package would fail), and creates no output directory.
- [ ] **Step 2:** `pnpm --dir scripts/plugins test` fails on both.
- [ ] **Step 3: Implement.** In `ci.yml`, the Plugins job's Clippy step's `if` becomes `steps.affected.outputs.cargo != '' && runner.os != 'Windows'`, and its comment says why ("the AVFoundation plugin's code builds on macOS only"); Format stays Linux only (rustfmt formats `cfg`-gated modules too).
- [ ] **Step 4:** `pnpm --dir scripts/plugins test` passes.
- [ ] **Step 5: Commit:** `feat(plugins): plugins that build for some systems only`.

### Task 2: The `avfoundation` plugin's crate, manifest, and `describe` (`plugins/avfoundation`)

**Files:**
- Create: `plugins/avfoundation/Cargo.toml`, `plugin.json`, `src/main.rs`, `src/lib.rs`, `src/components.rs`, `src/apple.rs` (macOS: encoder lists, `describe`'s availability), `src/provider.rs` (macOS: `AvfCodec`)
- Modify: `plugins/Cargo.toml` (`members = ["avfoundation", "ffmpeg", "vrm"]`), `plugins/Cargo.lock`, `scripts/plugins/version.mts` (`FOLLOWS_CELESTA = ['plugins/avfoundation', 'plugins/ffmpeg']`, so `bump-version.mts` bumps its Cargo package, pins, and `plugin.json` with Celesta's)
- Test: `plugins/avfoundation/src/components.rs` tests (all platforms), `plugins/avfoundation/tests/describe.rs` (macOS), `scripts/plugins/test/bump-version.test.mts` (its fixture gains `plugins/avfoundation/Cargo.toml` and `plugin.json`, bumped like FFmpeg's)

**Interfaces:**
- `Cargo.toml`: `[package] name = "celesta-codec-avfoundation"`, `version = "0.9.0"`, edition 2024, `license = "MIT OR Apache-2.0"`, `rust-version = "1.89"`, description "Celesta's macOS media plugin: VideoToolbox and AudioToolbox encoders, AVFoundation muxers and decoder, in a process of its own."; `[dependencies]` `celesta-composition`, `celesta-media`, `celesta-plugin-host` (path, `=0.9.0`, as FFmpeg's), `serde_json`, `base64` only if a helper needs it; `[target.'cfg(target_os = "macos")'.dependencies]` the `objc2` crates of the Tech stack with `default-features = false` and the features each module uses (as `crates/gpu-renderer/Cargo.toml` does); `[target.'cfg(target_os = "macos")'.dev-dependencies]` `celesta-codec-ffmpeg` (path), `celesta-exporter`, `celesta-gpu-renderer`, `celesta-project` (path, `=0.9.0`), `tempfile`.
- `src/lib.rs` (every platform):
  ```rust
  pub const PLUGIN_ID: &str = "avfoundation";
  pub const DECODER_ID: &str = "avfoundation";
  mod components;   // ids, settings, input choice, availability rules: platform-independent
  mod params;       // Task 3
  mod esds;         // Task 3
  #[cfg(target_os = "macos")] mod apple;
  #[cfg(target_os = "macos")] mod provider;
  #[cfg(target_os = "macos")] pub use provider::{AvfCodec, registered, install};
  ```
- `components.rs`:
  ```rust
  pub(crate) enum VideoCodec { H264, Hevc, ProRes }       // ids "h264", "hevc", "prores"; ALL; from_id; input_formats(); settings(); codec_type() -> u32 (fourcc)
  pub(crate) enum AudioCodec { Aac }                      // id "aac"; settings()
  pub(crate) enum MuxerFormat { Mp4, Mov }                // ids "mp4", "mov"; settings()
  pub(crate) fn video_availability(codec: VideoCodec, encoders: &[u32]) -> Result<(), String>   // encoders: VTCopyVideoEncoderList's codec types
  pub(crate) fn aac_availability(encoders: &[u32]) -> Result<(), String>                         // kAudioFormatProperty_Encoders' format ids
  pub(crate) fn describe_from(video_encoders: &[u32], audio_encoders: &[u32], sample_rates: &[u32]) -> Vec<ComponentDescription>
  ```
  The settings schemas and reasons are the Decisions' (use FFmpeg's
  `field` / `select` / `integer` / `when` / `in_group` helpers' shape;
  copy them, they are private to that crate).
- `apple.rs` (macOS): `pub(crate) fn video_encoder_types() -> Vec<u32>`
  (`VTCopyVideoEncoderList`), `pub(crate) fn audio_encoder_formats() -> Vec<u32>`
  (`AudioFormatGetProperty(kAudioFormatProperty_Encoders)`),
  `pub(crate) fn aac_sample_rates() -> Vec<u32>`
  (`kAudioFormatProperty_AvailableEncodeSampleRates` for
  `kAudioFormatMPEG4AAC`, each range's minimum).
- `provider.rs` (macOS):
  ```rust
  #[derive(Default)] pub struct AvfCodec { /* sessions by SessionId, one counter for every kind */ }
  impl MediaProvider for AvfCodec   // describe = describe_from(apple lists); decoder/video_encoder/audio_encoder/muxer = Some(self)
  pub fn registered() -> RegisteredPlugin   // Bundled, from plugin.json's media (MediaManifest::from_json), factory making AvfCodec in process
  pub fn install()                          // set_registry with registered() alone
  ```
  Until Tasks 4–8, each `open*`, `probe`, and decode returns
  `MediaError::ComponentUnavailable { component, reason: "not implemented yet" }`.
  Sessions of every kind share one counter, and `abort_video` /
  `abort_audio` ignore a session of the other kind (`abortEncoder` reaches
  both, P2a's rule); an aborted video session invalidates its
  `VTCompressionSession` without completing frames.
- `main.rs`: on macOS, `celesta_media::protocol::serve(&mut AvfCodec::default(), PLUGIN_ID, env!("CARGO_PKG_VERSION"))` as FFmpeg's `main.rs` (no `--manifest`: only FFmpeg is a `cargo install` sibling); elsewhere `eprintln!("{PLUGIN_ID}: this media plugin runs on macOS only"); std::process::exit(1)`.

- [ ] **Step 1: Write failing tests.**
  - `components.rs` (every platform): `plugin_json_declares_every_component` — `MediaManifest::from_json(plugin.json's media)` has no problems, and its decoders, video and audio encoders, and muxers equal the Global constraints' ids, names, codecs, `inputFormats`, extensions, and defaults; `describe_matches_the_manifest` — `describe_from` with every encoder present gives one available description per manifest component, kinds and ids in manifest order, video encoders with `transports == ["sharedMemory", "pipe"]` and `input_formats` as the manifest, `aac` with the given rates and `channels == [1, 2]`; `a_missing_encoder_has_a_reason` — `describe_from(&[avc1 only], &[], rates)` leaves `hevc` and `prores` unavailable with "VideoToolbox has no HEVC encoder on this Mac" / "… ProRes 4444 …" and `aac` with "AudioToolbox has no AAC encoder on this Mac"; `settings_follow_the_decisions` — `h264`'s keys in order `profile, rateControl, quality, bitrate, keyframeInterval, bFrames` with the defaults and `visibleWhen` above, `hevc`'s `rateControl, quality, bitrate, keyframeInterval, bFrames, bitDepth`, `prores`'s `profile`, `aac`'s `bitrate`, both muxers' `faststart`; and `celesta_media::settings::validate` of `{}` against each fills the defaults.
  - `tests/describe.rs` (macOS): `this_mac_has_every_component` — `AvfCodec::default().describe()` has every component available (on the user's Mac; elsewhere it prints the unavailable ones and passes); `the_program_describes_what_the_library_does` — the built program (`env!("CARGO_BIN_EXE_celesta-codec-avfoundation")`, started through `MediaProcess::factory` with `ProviderUse::Describe`) answers the same descriptions.
  - `bump-version.test.mts`: the existing bump test's expectations include `plugins/avfoundation/Cargo.toml` and `plugin.json` moved to the new version.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-avfoundation` fails to compile; `pnpm --dir scripts/plugins test bump-version` fails.
- [ ] **Step 3: Implement**, and `cargo update -p celesta-codec-avfoundation` in `plugins/` to add the crate and the `objc2` crates to `plugins/Cargo.lock` (no other version changes: `git diff plugins/Cargo.lock` adds packages only).
- [ ] **Step 4:** The tests, `cargo clippy --manifest-path plugins/Cargo.toml -p celesta-codec-avfoundation --all-targets --locked -- -D warnings`, `cargo fmt --check` in `plugins/`, and `node scripts/plugins/build.mts plugins/avfoundation` (writes `plugins/dist/avfoundation/celesta-plugin.json` with `"protocol": 2` and `executable["macos-aarch64"]`) pass.
- [ ] **Step 5: Commit:** `feat(codec-avfoundation): the AVFoundation media plugin's components`.

### Task 3: Codec strings, reorder depth, and the AAC magic cookie (`plugins/avfoundation`, every platform)

**Files:**
- Create: `plugins/avfoundation/src/params.rs`, `plugins/avfoundation/src/esds.rs`
- Test: in-module tests (run on Linux CI too)

**Interfaces:**
- `params.rs`:
  ```rust
  pub(crate) fn avc_codec_string(avcc: &[u8]) -> Result<String, MediaError>;     // "avc1.64000D"
  pub(crate) fn hevc_codec_string(hvcc: &[u8]) -> Result<String, MediaError>;    // "hvc1.2.4.L60.B0"
  pub(crate) fn avc_reorder_depth(avcc: &[u8]) -> Result<Option<u32>, MediaError>;  // None: no bitstream_restriction
  pub(crate) fn hevc_reorder_depth(hvcc: &[u8]) -> Result<Option<u32>, MediaError>;
  pub(crate) const UNKNOWN_REORDER_DEPTH: u32 = 16;
  ```
  The SPS is the first SPS in the record (avcC after its 8-byte header;
  hvcC's array of NAL type 33), emulation-prevention bytes removed before
  reading (as `plugins/ffmpeg/src/h264.rs`'s `unescaped`). A truncated
  record is `MediaError::Codec("VideoToolbox gave a malformed {avcC|hvcC}")`.
- `esds.rs`:
  ```rust
  pub(crate) fn asc_from_cookie(cookie: &[u8]) -> Result<Vec<u8>, MediaError>;  // the DecoderSpecificInfo (tag 5) inside the ES_Descriptor (tag 3) → DecoderConfigDescriptor (tag 4)
  pub(crate) fn cookie_from_asc(asc: &[u8], bitrate: u32) -> Vec<u8>;           // ES_Descriptor: ES_ID 0, DecoderConfigDescriptor (objectType 0x40, streamType 5, buffer 0, max/avg bitrate), DecoderSpecificInfo, SLConfig (2); 4-byte 0x80-extended lengths as AudioToolbox writes them
  pub(crate) fn aac_codec_string(asc: &[u8]) -> Result<String, MediaError>;   // "mp4a.40.2"
  ```
  Descriptor lengths are read in both the 1-byte and the 0x80-extended
  forms.

- [ ] **Step 1: Write failing tests** with the bytes VideoToolbox and AudioToolbox gave on this Mac (Decisions):
  - `avc_strings_and_depth`: avcC `0164000dffe1000f2764000dac56505067e79a8080808101000428ee3cb0fdf8f800` → `"avc1.64000D"`, `avc_reorder_depth == Some(2)` (if the SPS carries no `bitstream_restriction`, the test asserts `None` instead and the implementer says so in the report: the fallback of 16 then serves VideoToolbox's H.264).
  - `hevc_strings_and_depth`: hvcC `010220000000b000000000003cf000fcfdfafa00000b03a00001001840010c01ffff022000000300b0000003000003003c15c090a100010023420101022000000300b0000003000003003ca00a080c1f3b62057b9165535010101008a2000100084401c02835182992` (Main 10) → `"hvc1.2.4.L60.B0"`, `hevc_reorder_depth == Some(2)`.
  - `malformed_records_are_refused`: the avcC cut to 5 bytes, the hvcC cut to 30 → `Err(MediaError::Codec(..))`.
  - `the_cookie_holds_the_asc`: cookie `03808080220000000480808014401400180000000b300002ee0005808080021190068080800102` → ASC `[0x11, 0x90]`; `aac_codec_string(&[0x11, 0x90]) == "mp4a.40.2"`; `asc_from_cookie(&cookie_from_asc(&[0x11, 0x90], 192_000)) == [0x11, 0x90]`; a cookie with 1-byte lengths (`03 19 00 00 00 04 11 40 15 …`) reads the same ASC; a cookie without tag 5 is an error.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-avfoundation params esds` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and clippy pass.
- [ ] **Step 5: Commit:** `feat(codec-avfoundation): codec strings, reorder depth, and AAC cookies`.

### Task 4: VideoToolbox encoders `h264`, `hevc`, `prores` (`plugins/avfoundation`)

**Files:**
- Create: `plugins/avfoundation/src/video.rs` (macOS), `plugins/avfoundation/src/color.rs` (every platform: the name check)
- Modify: `plugins/avfoundation/src/components.rs` (`choose_input`, `vt_settings`), `src/provider.rs` (`VideoEncoder` for `AvfCodec`)
- Test: `components.rs` tests (every platform), `plugins/avfoundation/tests/encoders.rs` (macOS)

**Interfaces:**
- Consumes: Task 2's `VideoCodec`, Task 3's `params`, P2a's `VideoEncoderRequest`, `VideoEncoderOpened`, `Encoded`, `Packet`, `TrackConfig`.
- Produces:
  ```rust
  // components.rs, every platform
  pub(crate) fn choose_input(codec: VideoCodec, settings: &Settings, width: u32, height: u32,
                             offered: &[PixelFormat]) -> Result<(PixelFormat, bool), MediaError>;  // (input, alpha)
  pub(crate) enum VtSetting { ProfileLevel(&'static str), Quality(f32), AverageBitRate(i64),
                              MaxKeyFrameInterval(i32), AllowFrameReordering(bool) }
  pub(crate) fn vt_settings(codec: VideoCodec, settings: &Settings) -> Result<Vec<VtSetting>, MediaError>;
  // ProfileLevel names: "H264_High_AutoLevel", "H264_Main_AutoLevel", "H264_Baseline_AutoLevel",
  //                     "HEVC_Main_AutoLevel", "HEVC_Main10_AutoLevel"; ProRes gets none (the codec type picks the profile)
  // color.rs, every platform
  pub(crate) fn check_color(color: &Color) -> Result<(), MediaError>;   // only bt709/bt709/bt709/limited
  // video.rs, macOS
  pub(crate) struct VideoSession;
  pub(crate) fn open(codec: VideoCodec, request: &VideoEncoderRequest<'_>) -> Result<(VideoSession, PixelFormat, bool), MediaError>;
  impl VideoSession {
      pub(crate) fn encode(&mut self, pts: i64, frame: &[u8]) -> Result<Encoded, MediaError>;
      pub(crate) fn finish(self) -> Result<Encoded, MediaError>;   // VTCompressionSessionCompleteFrames, then invalidate
  }
  ```
- The session: `VTCompressionSessionCreate` with `imageBufferAttributes`
  `{PixelFormatType, Width, Height, IOSurfaceProperties: {}}`, codec type
  `avc1`, `hvc1`, `ap4h` or `apch` (`profile`); properties from
  `vt_settings`, `kVTCompressionPropertyKey_ColorPrimaries`,
  `TransferFunction`, `YCbCrMatrix` (BT.709), `RealTime = false`,
  `ExpectedFrameRate` from `frame_rate`. Frames are copied row by row into
  a buffer from `VTCompressionSessionGetPixelBufferPool` (its
  `bytesPerRow` may be padded), with the three colour attachments, and
  encoded with `VTCompressionSessionEncodeFrameWithOutputHandler` at
  `CMTime(pts, timescale)`, duration `frame_rate.den`. The handler pushes
  `Result<Packet, OSStatus>` and the format description into a shared
  queue that `encode` and `finish` drain; the first drained sample gives
  `Encoded.config` (`TrackConfig { codec, description }`: avcC or hvcC
  from `SampleDescriptionExtensionAtoms`, none for ProRes). `key` is the
  absence of `kCMSampleAttachmentKey_NotSync`; `dts` per the Decisions'
  shift; bytes are the sample's block buffer, length-prefixed as given.
- A frame whose length is not `input.frame_bytes(width, height)` is
  `MediaError::Codec`. A failing `OSStatus` names the call and the status
  (`"VTCompressionSessionEncodeFrame failed: -12902"`).

- [ ] **Step 1: Write failing tests.**
  - `components.rs` (every platform): `odd_sizes_and_rgb_offers_take_rgba16` — `choose_input(H264, {}, 64, 36, OFFERABLE) == (Nv12, false)`; `(63, 35)` → `(Rgba16, false)`; offered `[Rgba16, Rgba8]` → `Rgba16`; `Hevc` with `bitDepth 10` → `P010`, odd → `Rgba16`; `ProRes` 4444 → `(Rgba16, true)`, `hq` → `(Rgba16, false)`; offered `[Rgba8]` → `Err(InvalidSettings { key: "input", .. })`. `settings_become_videotoolbox_properties` — `h264` defaults → `[ProfileLevel("H264_High_AutoLevel"), Quality(0.7), MaxKeyFrameInterval(250), AllowFrameReordering(true)]`; `rateControl bitrate, bitrate 5000000, bFrames false` → `AverageBitRate(5_000_000)` and `AllowFrameReordering(false)` with no `Quality`; `hevc` `bitDepth 10` → `ProfileLevel("HEVC_Main10_AutoLevel")`. `only_bt709_limited_is_known` — `check_color(&Color::bt709_limited())` is `Ok`; `matrix: "nonsense"` → `Err(Codec(m))`, `m` contains ``unknown color matrix `nonsense` ``; `range: "full"` the same for `range`.
  - `tests/encoders.rs` (macOS, 64x36 at 30000/1001, timescale 30000, `AvfCodec` in process; decoding through `celesta_codec_ffmpeg`'s libavcodec as its `tests/encoders.rs` does, or by muxing with `FfmpegCodec`'s `mp4`/`mov` into a temporary file and decoding that with `FfmpegCodec`'s decoder):
    - `h264_from_nv12_keeps_bt709_codes`: 10 frames of `nv12` solid BT.709 red (Y 63, Cb 102, Cr 240): `input == Nv12`, `config.codec` starts `"avc1.64"`, the decoded `yuv420p` planes are within 2 of the codes, and the stream is tagged BT.709 limited (`colr` `nclx` in an FFmpeg-muxed MP4, as P2c's `colr_atom`).
    - `hevc_10_bit_from_p010_is_main10`: `bitDepth 10`, `p010` red (250, 409, 960 `<< 6`): `config.codec` starts `"hvc1.2."`, decoded `yuv420p10le` within 4 of the codes.
    - `prores_takes_rgba16_little_endian`: `profile 4444`, `rgba16` frames left half `#ff0000` opaque, right half `#00ff00` at alpha 32768: `alpha == true`, `config == TrackConfig { codec: "ap4h", description: None }`, decoded RGBA8 within 2 of `(255, 0, 0, 255)` and `(0, 255, 0, 128)`; `hq`: `alpha == false`, codec `"apch"`.
    - `h264_and_hevc_decode_times_precede_presentation`: 30 frames of a moving pattern, default settings: across `encode` and `finish` answers, exactly 30 packets, the first `key`, every `dts <= pts`, `dts` strictly increasing, the first `dts` negative, `pts` the 30 inputs; with `bFrames false` every `dts == pts`.
    - `packets_come_with_the_config`: the first answer with packets carries `config`; no later answer does.
    - `rgba16_input_is_converted_with_bt709`: `h264` offered `[Rgba16, Rgba8]`, solid red: decoded Y, Cb, Cr within 2 of (63, 102, 240).
    - `a_frame_of_the_wrong_size_is_refused`: `encode` with `nv12` bytes of 64x34 → `Err(Codec(..))`, and the session still finishes.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-avfoundation choose_input vt_settings check_color` and `--test encoders` fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and clippy pass; no test skipped on this Mac.
- [ ] **Step 5: Commit:** `feat(codec-avfoundation): VideoToolbox H.264, HEVC, and ProRes encoders`.

### Task 5: AudioToolbox `aac` (`plugins/avfoundation`)

**Files:**
- Create: `plugins/avfoundation/src/audio.rs` (macOS)
- Modify: `src/provider.rs` (`AudioEncoder` for `AvfCodec`)
- Test: `plugins/avfoundation/tests/encoders.rs` (macOS)

**Interfaces:**
- Consumes: Task 3's `esds`, P2a's `AudioEncoderRequest`, `AudioEncoderOpened`.
- Produces:
  ```rust
  pub(crate) struct AudioSession;
  pub(crate) fn open(request: &AudioEncoderRequest<'_>, rates: &[u32]) -> Result<(AudioSession, u32, u16, u64), MediaError>;  // (session, sample_rate, channels, delay)
  impl AudioSession {
      pub(crate) fn encode(&mut self, pts: i64, samples: &[f32]) -> Result<Encoded, MediaError>;
      pub(crate) fn finish(self) -> Result<Encoded, MediaError>;
  }
  ```
- `AudioConverterNew` from interleaved `f32` LPCM to `kAudioFormatMPEG4AAC`
  (AAC-LC, 1024 frames a packet), `kAudioConverterEncodeBitRate` from
  `bitrate`, `delay = kAudioConverterPrimeInfo.leadingFrames`.
  `AudioConverterFillComplexBuffer` with an input callback over the
  session's pending samples; it returns 0 frames when they run out before
  `finish` (no end of stream) and signals end of stream in `finish`. The
  `k`-th packet has `pts = 1024·k − delay`, `duration = 1024` (the last as
  the Decisions say), `key = true`, `dts = pts`. `config`: `mp4a.40.2` and
  the ASC from `kAudioConverterCompressionMagicCookie`, with the first
  packets. A `pts` that does not follow the samples already given is
  `MediaError::Codec`.

- [ ] **Step 1: Write failing tests** (48 kHz stereo, 1 s of silence with a click (1.0) at frame 4800, in chunks of `1024 × 16` frames as the exporter sends them):
  - `aac_opens_with_its_priming`: `open` answers `(48000, 2, 2112)`; at 44100 Hz mono `(44100, 1, 2112)`; at 96000 Hz `(48000, 2, _)`.
  - `aac_packets_start_before_zero`: across answers, packets have `pts` `−2112, −1088, −64, 960, …`, all `duration` 1024 but the last, and `last.pts + last.duration == 48_000`; `config == TrackConfig { codec: "mp4a.40.2", description: Some(asc) }` with `asc == [0x11, 0x90]`.
  - `aac_through_ffmpegs_mp4_is_aligned`: the packets muxed by `FfmpegCodec`'s `mp4` (audio track `delay: 2112`, `leading_audio: 0`) and decoded by `FfmpegCodec`'s decoder at 48 kHz: 48000 ± 1024 frames, the click's peak at frame 4800 exactly.
  - `aac_bitrate_follows_the_channel_count`: mono with `bitrate 320000` → `Err(InvalidSettings { key: "bitrate", .. })`; stereo `32000` the same; stereo `128000` opens.
- [ ] **Step 2:** `--test encoders aac` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and clippy pass.
- [ ] **Step 5: Commit:** `feat(codec-avfoundation): AudioToolbox AAC encoder`.

### Task 6: AVAssetWriter muxers `mp4` and `mov` (`plugins/avfoundation`)

**Files:**
- Create: `plugins/avfoundation/src/muxer.rs` (macOS), `plugins/avfoundation/src/formats.rs` (macOS: `CMFormatDescription`s from `Track`s)
- Modify: `src/provider.rs` (`Muxer` for `AvfCodec`)
- Test: `plugins/avfoundation/tests/muxers.rs` (macOS)

**Interfaces:**
- Consumes: Tasks 3–5; P2a's `MuxerRequest`, `Track`, `TrackPacket`.
- Produces:
  ```rust
  // formats.rs
  pub(crate) fn video_format(track: &Track) -> Result<CFRetained<CMVideoFormatDescription>, MediaError>;
  pub(crate) fn audio_format(track: &Track) -> Result<CFRetained<CMAudioFormatDescription>, MediaError>;
  // muxer.rs
  pub(crate) struct MuxerSession;
  pub(crate) fn open(format: MuxerFormat, request: &MuxerRequest<'_>) -> Result<MuxerSession, MediaError>;
  impl MuxerSession {
      pub(crate) fn write(&mut self, packets: &[TrackPacket]) -> Result<(), MediaError>;
      pub(crate) fn finish(self) -> Result<Vec<PathBuf>, MediaError>;   // [output]
      pub(crate) fn abort(self) -> Vec<PathBuf>;                         // cancelWriting; [output]
  }
  ```
- Video formats: codec type from the family (`avc1`, `hvc1` (also for
  `hev1`), `ap4h`, `apch`), `width × height`, extensions
  `SampleDescriptionExtensionAtoms {avcC|hvcC: description}` (none for
  ProRes), the three BT.709 colour extensions from `Track.color` (through
  Task 4's `check_color`), `FullRangeVideo = false`, and for ProRes
  `Depth` 32 when `alpha` else 24. Audio: `mp4a` → ASBD
  `{rate, 'aac ', framesPerPacket 1024, channels}` with
  `cookie_from_asc(description, 0)`; `lpcm.s16le` / `lpcm.s24le` /
  `lpcm.f32le` → packed little-endian LPCM ASBDs (signed integer or
  float); anything else → `MediaError::Codec("{muxer} does not take {codec}")`.
- Writer: `AVFileTypeMPEG4` / `AVFileTypeQuickTimeMovie`,
  `shouldOptimizeForNetworkUse = faststart`; one passthrough input per
  track (`outputSettings` nil, `sourceFormatHint`, `mediaTimeScale =
  timescale`, `expectsMediaDataInRealTime = false`); `startWriting`,
  `startSessionAtSourceTime(zero)`; the Decisions' output-path rule and
  pending queues.
- Samples: video `CMSampleBufferCreateReady` with one timing entry
  (`duration`, `pts`, `dts`) and `NotSync` for non-key packets. AAC
  `CMAudioSampleBufferCreateReadyWithPacketDescriptions` (one packet, its
  `pts`), `TrimDurationAtStart = min(duration, −pts)` while
  `pts < 0` and frames remain of `delay + leading_audio`,
  `TrimDurationAtEnd` for a short last packet, deviation 60's two
  attachments on the first audio buffer, every attachment
  `ShouldPropagate`. LPCM: frames before 0 cut (deviation 61), the rest
  as one buffer per packet (`sampleCount` = frames, no packet
  descriptions).

- [ ] **Step 1: Write failing tests** (64x36, 30000/1001; packets from Tasks 4–5 and from `FfmpegCodec`; read back with `FfmpegCodec`'s decoder and with this plugin's own reading through a small AVAssetReader helper in the test file, until Task 8's decoder replaces it):
  - `h264_and_aac_in_mp4_read_back`: 30 `h264` frames of BT.709 red and 1 s of the click audio into `mp4`: `finish` answers `[output]`; FFmpeg's decoder probes 30 frames at 30000/1001 and RGBA within 3 of `(255, 0, 0)`; the file has `colr` `nclx` and an `avc1` sample entry.
  - `aac_delay_and_preroll_are_hidden_for_every_reader`: the click track encoded after 16384 frames of pre-roll (constant 0.25), muxed with `leading_audio: 16384`, once into `mp4` and once into `mov`: FFmpeg's decoder and AVFoundation's (the helper) both find the click at frame 4800, and neither sees the 0.25 pre-roll in the first 4000 frames.
  - `prores_and_lpcm_in_mov`: FFmpeg's `prores` 4444 with alpha and FFmpeg's `pcm` (`format s24le`) with `leading_audio: 1024` into `mov`: FFmpeg decodes alpha ≈ 128 in the transparent half and the PCM's first frame is the mix's frame 1024 exactly (cut, deviation 61).
  - `an_empty_staged_output_is_replaced` / `a_file_with_content_is_not_replaced`: an empty file at `output` → the export succeeds; a 1-byte file → `open` fails with `"already exists"` and the byte is still there.
  - `long_runs_of_one_track_do_not_stall`: 300 video packets in one `write`, then all audio packets in another (and the reverse order in a second file): both finish within 10 s with every packet in the file (FFmpeg's demuxer counts 300 video and all audio packets).
  - `an_unknown_codec_is_refused`: a `Track::Audio` with `opus` into `mp4` → `Err(Codec("mp4 does not take opus"))`.
  - `abort_answers_the_output`: after one `write`, `abort` answers `[output]`.
- [ ] **Step 2:** `--test muxers` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and clippy pass.
- [ ] **Step 5: Commit:** `feat(codec-avfoundation): AVAssetWriter MP4 and MOV muxers`.

### Task 7: The program, the ring, and exports across plugins (`plugins/avfoundation`, `plugins/ffmpeg`)

**Files:**
- Modify: `plugins/ffmpeg/src/muxer.rs` (`codec_id`'s tag for `hvc1`/`hev1`/`avc1`/`avc3`)
- Test: `plugins/ffmpeg/src/muxer.rs` tests, `plugins/avfoundation/tests/process.rs` (macOS), `plugins/avfoundation/tests/export.rs` (macOS)

**Interfaces:**
- Consumes: everything above; `celesta_codec_ffmpeg::registered()`;
  `celesta_media::{MediaProcess, ProviderUse, ring_layout}`; the
  exporter's options as the FFmpeg plugin's `tests/export.rs` builds them
  (`component("…")`, `options(&[…])`; P3's names after the rebase).
- Produces: FFmpeg's `codec_id` returns tag `hvc1` for a codec string
  whose family is spelled `hvc1`, `hev1` for `hev1`, `avc1` / `avc3`
  likewise (`MKTAG`, as ProRes's tags); `codec_family` still treats them
  as one family. A test-only registry `[avfoundation::registered(),
  celesta_codec_ffmpeg::registered()]`, both `Bundled`, installed once per
  test binary.

- [ ] **Step 1: Write failing tests.**
  - `plugins/ffmpeg` (`muxer.rs` tests): `hevc_keeps_its_sample_entry_spelling` — `codec_id("mp4", hvc1.1.6.L93.B0)` gives tag `hvc1`, `hev1.…` gives `hev1`, `avc1.64001F` gives `avc1`.
  - `tests/process.rs`: `encodes_through_the_ring` — the program started with `ProviderUse::Export { ring: ring_layout(..) }` for 64x36 `nv12`: `h264` opens with the ring declared (`frames_in_ring()`), 10 frames sent through `Pipeline::send_video_frame` three deep and received in order, then `finish`: 10 packets that decode as Task 4's in-process ones do (codes within 2); `errors_keep_their_kind` — `open_video` with `bitrate` as text → `InvalidSettings { key: "bitrate" }`; probing a WebM through the program → `Unsupported`.
  - `tests/export.rs` (GPU held, skip without one; the 64x36 two-color project and the click audio of the FFmpeg plugin's `with_audio`):
    - `avfoundation_hevc_with_ffmpeg_opus_in_ffmpeg_mp4` (the spec's example): `--muxer ffmpeg/mp4 --video-encoder avfoundation/hevc --audio-encoder ffmpeg/opus`: the JSON report names the three, the file's sample entry is `hvc1`, FFmpeg decodes every frame within 3 of the rendered colors and the audio as long as the video.
    - `ffmpeg_x264_with_avfoundation_aac_in_avfoundation_mp4`: `--muxer avfoundation/mp4 --video-encoder ffmpeg/libx264 --audio-encoder avfoundation/aac`.
    - `avfoundation_alone_writes_mp4_and_mov`: `--muxer avfoundation/mp4` (defaults `avfoundation/h264` + `avfoundation/aac`) and `--muxer avfoundation/mov` (`avfoundation/prores` + `avfoundation/aac`), transparent project for the MOV: alpha kept.
    - `a_looping_avfoundation_mp4_starts_its_sound_from_its_end`: the FFmpeg plugin's looping test (`a_looping_mp4_starts_its_sound_from_its_end`) with `--muxer avfoundation/mp4`: the first decoded audio frames equal the mix's end, through FFmpeg's decoder.
    - `ffmpeg_stays_the_default_beside_avfoundation`: the registry lists `avfoundation` before `ffmpeg` (folder order), an `.mp4` export with no choices reports `ffmpeg/mp4`, `ffmpeg/libx264`, `ffmpeg/aac`. On a base without P4's ranking it fails: mark it `#[ignore = "needs P4's FFmpeg-first ranking (deviation 58)"]` until the branch is rebased onto P4, then remove the `ignore` in that rebase.
- [ ] **Step 2:** The new tests fail (`codec_id` gives tag 0; the exports' sample entry is `hev1`).
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`, its clippy, and `cargo fmt --check` in `plugins/` pass.
- [ ] **Step 5: Commit:** `feat(codec-avfoundation): exports through the program and across plugins` (and, if kept separate, `fix(codec-ffmpeg): keep hvc1 and avc1 sample entries` first).

### Task 8: AVAssetReader decoder (`plugins/avfoundation`)

**Files:**
- Create: `plugins/avfoundation/src/decoder.rs` (macOS), `plugins/avfoundation/src/names.rs` (every platform: four-character codes and CoreVideo colour names to FFmpeg's)
- Modify: `src/provider.rs` (`Decoder` for `AvfCodec`)
- Test: `names.rs` tests, `plugins/avfoundation/tests/decode.rs` (macOS); Task 6's test helper reader is replaced by the decoder

**Interfaces:**
- Produces:
  ```rust
  // names.rs
  pub(crate) fn codec_name(fourcc: u32) -> String;                      // avc1/avc3 → "h264", hvc1/hev1 → "hevc", ap4h/apch/apcn/apcs/apco/ap4x → "prores", "aac " → "aac", lpcm → "pcm_s16le"/…; else the code
  pub(crate) fn primaries_name(cv: &str) -> Option<&'static str>;     // "ITU_R_709_2" → "bt709", "ITU_R_2020" → "bt2020", "P3_D65" → "smpte432", "SMPTE_C" → "smpte170m", "EBU_3213" → "bt470bg"
  pub(crate) fn transfer_name(cv: &str) -> Option<&'static str>;      // "ITU_R_709_2" → "bt709", "SMPTE_ST_2084_PQ" → "smpte2084", "ITU_R_2100_HLG" → "arib-std-b67", "sRGB" → "iec61966-2-1", "Linear" → "linear"
  pub(crate) fn matrix_name(cv: &str) -> Option<&'static str>;        // "ITU_R_709_2" → "bt709", "ITU_R_601_4" → "smpte170m", "ITU_R_2020" → "bt2020nc"
  // decoder.rs
  pub(crate) struct AvfDecoder;   // probes by path; sequential sessions by name
  impl AvfDecoder {
      pub(crate) fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError>;
      pub(crate) fn decode(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError>;
      pub(crate) fn decode_audio(&mut self, path: &Path, sample_rate: u32, channels: u16) -> Result<AudioBuffer, MediaError>;
  }
  ```
- Probe: `AVURLAsset` with `AVURLAssetPreferPreciseDurationAndTimingKey`
  true, loaded through the `load…completionHandler:` APIs and a channel
  (the synchronous accessors are deprecated). The video stream's frame
  rate is the track's `minFrameDuration` inverted exactly
  (`Rational::new(timescale, value)`, reduced), its size the displayed
  one, its colour from the format description's extensions; one
  `AudioStream` per audio track with its rate and channels; durations as
  `Time` from the `CMTime`s. Decoding: the Decisions' rules; BGRA
  converted to RGBA in place.
- A frame request on a file without video is `NoVideoStream`; a negative
  or non-finite time is `InvalidTimestamp`, as FFmpeg's.

- [ ] **Step 1: Write failing tests.**
  - `names.rs`: one assertion per mapping above, and `primaries_name("unknown") == None`.
  - `tests/decode.rs` (files written in the test by Tasks 4–6's components; a WebM written by `FfmpegCodec`):
    - `probes_what_it_wrote`: a 10-frame 64x36 `h264`+`aac` MP4 at 30000/1001 → `video.codec == Some("h264")`, `width 64`, `height 36`, `frame_rate == Rational::new(30000, 1001)`, colour `bt709` ×3, one audio stream at 48000 Hz, 2 channels, `codec Some("aac")`.
    - `decodes_the_frame_shown_at_each_time`: four frames of distinct colors at 10 fps; times `0.0`, `0.15`, `0.299`, `0.3`, `5.0` give frames 0, 1, 2, 3, 3 (within 3 per channel); the same through a `Sequential` session moving forward, then backward to `0.0`.
    - `decodes_audio_as_ffmpeg_does`: the click file from Task 6 (pre-roll and delay hidden) → `decode_audio(.., 48000, 2)` has the click at frame 4800, as `FfmpegCodec` decodes it; at `44100, 1` the click is at `round(4800 × 44100 / 48000)` ± 1.
    - `reads_m4a`: an audio-only file the `mp4` muxer wrote, named `.m4a` → probe has no video and one audio stream; `decode_video` → `NoVideoStream`.
    - `refuses_what_it_cannot_read`: the WebM → `Unsupported`; a text file named `x.mp4` → `Unsupported`; a missing path → `NotFound`.
    - `rotated_video_is_displayed_upright`: a 64x36 MP4 written with a 90° `preferredTransform` (the muxer test helper sets `AVAssetWriterInput.transform`) probes as 36x64 and decodes a frame of 36x64 whose top-left is the source's bottom-left color.
- [ ] **Step 2:** `--test decode` and `names` fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and clippy pass.
- [ ] **Step 5: Commit:** `feat(codec-avfoundation): AVAssetReader decoder`.

### Task 9: The macOS package (`scripts/package-macos.py`, `scripts/test-package.mjs`)

**Files:**
- Modify: `scripts/package-macos.py` (build `plugins/avfoundation` into `Contents/PlugIns`, `codec_packages` → the bundled plugins' packages: `celesta-codec-ffmpeg` and `celesta-codec-avfoundation` and what they reach, the Mach-O check's `required` set), `scripts/test_package_macos.py`, `scripts/test-package.mjs` (macOS only: the AVFoundation plugin's manifest, program, `licenses/` holding exactly its `plugin.json`'s files, an MP4 through `--muxer avfoundation/mp4`), `packaging/macos/README.md` (Redistribution), `.github/workflows/package.yml` (its plugin-version check covers `plugins/avfoundation/plugin.json` too)
- Test: `python3 -m unittest scripts/test_package_macos.py`

**Interfaces:**
- `package-macos.py`: `BUNDLED_PLUGINS = ("ffmpeg", "avfoundation")` with their Cargo packages; `codec_packages(metadata)` becomes `bundled_packages(metadata)`: the union of both roots' normal and build dependency closures (dev-dependencies excluded, as today); `RuntimeError` when either root is missing.
- `test-package.mjs`: `if (mac) { … }` block after the FFmpeg checks: `Contents/PlugIns/avfoundation/celesta-plugin.json` has `roles` `["media"]`, the five component lists of the Global constraints (ids only), one `macos-*` executable that exists; `licenses/` equals `LICENSE-MIT`, `LICENSE-APACHE`; `rust-packages.json` names `celesta-codec-avfoundation` and `objc2-video-toolbox`; `run(exporter, ['--react', entry, '--muxer', 'avfoundation/mp4', output2])` writes an `ftyp` file.

- [ ] **Step 1: Write failing tests** in `test_package_macos.py`: `test_the_plugins_workspace_contributes_what_the_bundled_plugins_ship` (the existing test extended: a fake metadata with `celesta-codec-avfoundation` → `objc2-video-toolbox` and VRM; the result holds both codecs' closures and not VRM's), `test_a_plugins_workspace_without_a_bundled_plugin_fails` (either root missing).
- [ ] **Step 2:** `python3 -m unittest scripts/test_package_macos.py` fails.
- [ ] **Step 3: Implement.** The README's Redistribution section gains a paragraph: `Contents/PlugIns/avfoundation` holds the AVFoundation plugin, MIT/Apache-2.0, linking only system frameworks.
- [ ] **Step 4:** The unit tests pass. Run `python3 scripts/package-macos.py` (unsigned) once if it finishes in under 30 minutes on this Mac, then `node scripts/test-package.mjs target/packages/<app>`; if it is too long or needs what this Mac lacks, say so in the report and leave it to CI's Package job.
- [ ] **Step 5: Commit:** `feat(packaging): bundle the AVFoundation plugin in the macOS app`.

### Task 10: Measurements, docs, the spec, HANDOFF.md, and the handoff note

**Files:**
- Modify: `packages/website/content/en/docs/02-(guides)/09-export.md` and `ja/…/09-export.md` (a section "macOS: AVFoundation components": the components, their settings, that FFmpeg stays the default, how to choose them with `--muxer` / `--video-encoder` / a preset, and one combined example), `en/ja/docs/05-(more)/05-plugins.md` (the bundled plugins list), `skills/celesta/references/export.md` (the AVFoundation keys as of this release, beside FFmpeg's) and `skills/celesta/references/plugins.md` (bundled plugins)
- Modify: `HANDOFF.md` (workspace map row for `celesta-codec-avfoundation`; development setup: the plugin builds on macOS only, `plugin.json`'s `os`; the macOS package's `PlugIns`)
- Modify: `docs/superpowers/specs/2026-11-08-media-components-design.md` (OS-native plugins → "As built (P5)" with deviations 60–64 and the verified answers; Audio's P5 bullet points to deviation 60; Phases → P5 as built)
- Create: `docs/handoff/2026-11-09-media-avfoundation.md`

- [ ] **Step 1: Measure** (release builds, this Mac, medians of three interleaved runs, `CELESTA_PLUGIN_PATH` holding both plugins built by `build.mts`): 300 frames of `examples/spectra/film.tsx` at 1080p and 4K with `ffmpeg/libx264` (defaults) against `avfoundation/h264` and `avfoundation/hevc` (defaults), and 4K `ffmpeg/prores` against `avfoundation/prores` (4444): wall time, file size, and PSNR against the RGBA render of frame 90 (`ffmpeg -lavfi psnr`, decoded as BT.709). Recorded only; no threshold (the spec's hard requirement is the FFmpeg path, unchanged here).
- [ ] **Step 2: Write the docs.** English first, then Japanese with the same structure; component names and keys are not translated.
- [ ] **Step 3: Verify.** Each passes: `cargo build -p celesta-codec-test` then `cargo test --workspace`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo fmt --all -- --check`; with `PKG_CONFIG_PATH`: `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`, `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`, `cargo fmt --check` in `plugins/`; `pnpm run test:runtime`; `pnpm --dir scripts/plugins test`; `pnpm --dir packages/website test`; `python3 -m unittest scripts/test_package_macos.py`; `git diff --check`. List any test that printed `skipping` (none expected on this Mac).
- [ ] **Step 4: Write `docs/handoff/2026-11-09-media-avfoundation.md`** in the P1 note's format: the specification as built (components, settings, input choice, DTS shift, trims, output path rule, interleaving, decoder rules, the `os` key, packaging), deviations 60–64, the verification probes and their numbers (Decisions), Step 1's table, the checks, and what is not done (P6, GPU texture sharing through `ioSurface`, HEVC with alpha, the decoder's HDR tone curve).
- [ ] **Step 5: Commit:** `docs: the AVFoundation media plugin (P5)`.
