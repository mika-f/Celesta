# Media components P6: Windows native (Media Foundation) — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A third, independent media plugin, `plugins/mediafoundation`
(`celesta-codec-mediafoundation`), serving protocol 2 on Windows. It has:
- H.264 and HEVC encoder MFTs (`h264`, `hevc`), hardware first;
- Microsoft's AAC encoder MFT (`aac`);
- an `IMFSinkWriter` muxer (`mp4`) taking compressed input, when Task 1
  confirms that the MPEG-4 sink can hide the delay;
- an `IMFSourceReader` decoder (`mp4`, `mov`, `m4a`).

It is bundled in the Windows package beside FFmpeg and can be combined with
FFmpeg's components in one export.

**Architecture:**
- **The plugin is P5's twin.** P5 is `plugins/avfoundation`; read it with
  this plan. Its crate layout, provider, tests, and every core-side choice
  are the pattern here.
  - Every module that touches Media Foundation or COM is
    `#[cfg(windows)]`. Elsewhere the library is empty and the program exits
    with a message, so the plugins workspace still builds and tests on
    macOS and Linux.
  - The bitstream helpers and pure choices are platform-independent and
    tested everywhere:
    - Annex B and length-prefixed NAL units, avcC and hvcC;
    - codec strings and reorder depth;
    - the AAC user data;
    - the RGB-to-YUV conversion;
    - time units;
    - settings, input choice, and availability.
- **Components.** `serve` (P2b) hands each frame over as an owned
  `Vec<u8>`, already read out of its ring slot.
  - The video encoders copy it into an `IMFSample` and drive an encoder MFT
    in one of two ways: synchronously for Microsoft's software MFT, or
    through its event generator for a hardware (asynchronous) MFT. The MFT
    produces Annex B; the plugin converts it to the length-prefixed samples
    and the avcC or hvcC that every Celesta muxer takes.
  - The AAC encoder converts `f32` to `s16` and drives Microsoft's AAC MFT
    synchronously.
  - The muxer converts back to Annex B and appends samples to an
    `IMFSinkWriter` with the MPEG-4 container type, passing each stream
    through without re-encoding.
- **Selection.** Nothing changes in Celesta's core. P4's `default_rank`
  (deviation 58) keeps FFmpeg first among bundled plugins, so Media
  Foundation's components are used only when named or in a preset. The
  plugin does not touch FFmpeg's code.
- **Packaging.**
  - `plugin.json` has `"os": ["windows"]`, so `build.mts` (P5 Task 1)
    skips the plugin elsewhere.
  - `scripts/package-windows.ps1` builds it into `plugins/mediafoundation`
    beside `plugins/ffmpeg`.
  - The package test exports through it.

**Tech stack:**
- Rust 2024 (MSRV 1.89; CI's toolchain is 1.99.1).
- `windows` 0.62.2 and `windows-core` 0.62.2, the versions `plugins/Cargo.lock`
  already locks.
- Media Foundation (`mfplat`, `mfreadwrite`, `mfuuid`) and COM, on Windows
  10 1809 or later and Windows 11. The installer's `MinVersion` is 10.0.
- `celesta-codec-ffmpeg`, in process and a Windows-only **dev**-dependency,
  for the tests' decode-and-compare and the cross-plugin exports.

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`.
Read these sections with this plan:
- OS-native plugins (P5, P6), including the P6 paragraph and deviations
  70–76;
- Phases (P5, P6);
- Manifest; Protocol 2; Capability discovery; Settings schema;
- Transport; Pixel formats and color;
- Audio (delay, `trimsLeadingAudio`);
- Selection (Ranking); Testing.

The rest of this plan is self-contained: whoever executes it has not seen
the conversation that wrote it.

## How to start (on the Windows machine)

1. **Prerequisites.** Each must be in place before Task 1:
   - Git for Windows. Set `git config --global core.autocrlf false` before
     cloning: the repository has no `.gitattributes`, and CRLF files fail
     CI's `cargo fmt --check` on Linux.
   - Visual Studio 2022 Build Tools, "Desktop development with C++",
     including the Windows 11 SDK (10.0.22621 or later), for MSVC and
     `mfplat.lib`, `mfreadwrite.lib`, and `mfuuid.lib`.
   - `rustup` with `rustup toolchain install 1.99.1 --component clippy,rustfmt`.
     The target is `x86_64-pc-windows-msvc`, rustup's default on Windows.
     Run `rustup override set 1.99.1` in the checkout: `mise.toml` pins
     1.99.1 through `mr-boxington`, which is macOS-only.
   - Node.js 26.13.0 and pnpm 12.17.0, the versions `mise.toml` pins:
     either `mise` with `MISE_DISABLE_TOOLS=rust,mr-boxington`, as CI sets
     it, or the official installers.
   - LLVM (`winget install LLVM.LLVM`). `ffmpeg-sys-next`'s bindgen needs
     `libclang`; set `LIBCLANG_PATH` to `C:\Program Files\LLVM\bin` if it
     is not found.
   - FFmpeg 8.1.x development libraries, for the FFmpeg plugin, which this
     plugin's tests use. Use either:
     - vcpkg: `vcpkg install ffmpeg[x264,vpx,opus]:x64-windows-static-md`,
       then `$env:VCPKG_ROOT = '<vcpkg folder>'` (HANDOFF.md); or
     - CI's pre-built package. `.github/actions/setup-ffmpeg/action.yml`
       downloads `celesta.ffmpeg.windows` 8.1.2-1 from GitHub Packages
       (this needs a token with `read:packages`) and sets `VCPKG_ROOT` and
       `VCPKGRS_TRIPLET=x64-windows-static-md-release`.
   - A GPU with a current driver. Export tests skip without one, and on
     this machine none may skip.
   - Commit signing configured. Every commit in this repository is signed,
     through the user's 1Password SSH agent. If a commit fails to sign
     (with 1Password, "failed to fill whole buffer" means it is locked),
     stop and ask the user to unlock it or fix signing. Never pass
     `--no-gpg-sign` or change `commit.gpgsign`.
   - `gh` (GitHub CLI), signed in, and the `gh stack` extension, which the
     stack is managed with.
2. **Branch.** Work on `claude/media-components-p6`, the planning branch,
   which carries this plan on top of P5's head (`d15155fa` when it was
   written). Run `git fetch origin`, then
   `git switch -c claude/media-components-p6 origin/claude/media-components-p6`.
   If `origin` has no such branch, the plan was not pushed; ask the user.
   The stack runs P1 → P2a → P2b → P2c → P3 → P4 → P5 → P6, one PR per
   phase, each on the last:
   - mika-f/Celesta#785 → #786 → #793 → #797 → #794 → #795 → #799 → this one.
   When P5's branch moves, this branch is **rebased** onto it (`gh stack
   rebase` or `git rebase`), never merged.
3. **Read** `AGENTS.md`, `HANDOFF.md`, the spec sections above, P5's plan
   (`docs/superpowers/plans/2026-11-08-media-components-p5.md`), P5's
   handoff note (`docs/handoff/2026-11-09-media-avfoundation.md`), and
   `plugins/avfoundation` (`src/provider.rs`, `src/components.rs`,
   `tests/process.rs`, `tests/export.rs`).
4. **Execute** with superpowers:subagent-driven-development. Its ledger
   goes in `.superpowers/sdd/` (git-ignored). Task 1 comes first and
   records its results in this plan: they decide Task 6, and they settle
   the constants other tasks name.

## Global constraints

- **Branch.** Work on `claude/media-components-p6`, on P5
  (`claude/media-components-p5`, mika-f/Celesta#799). Rebase, never merge.
  Use every name P2a–P5 define, spelled as at the base.
- **The plugin's identity.**
  - id `mediafoundation`; Cargo package and program
    `celesta-codec-mediafoundation`;
  - version Celesta's: `0.9.0` today, bumped with the others by
    `bump-version.mts` through `FOLLOWS_CELESTA`;
  - `roles: ["media"]`, `protocol` 2;
  - `licenses: ["../../LICENSE-MIT", "../../LICENSE-APACHE"]`. Nothing in
    it is GPL; it links Windows system libraries only.
  - `"os": ["windows"]`.
- **Component ids and names**, exactly:
  - decoder `mediafoundation` "Media Foundation", extensions
    `["mp4", "mov", "m4a"]`;
  - video encoders `h264` "H.264 (Media Foundation)" `["avc1"]` and `hevc`
    "HEVC (Media Foundation)" `["hvc1"]`;
  - audio encoder `aac` "AAC (Media Foundation)" `["mp4a"]`;
  - muxer `mp4` "MP4 (Media Foundation)": `extensions ["mp4", "m4v"]`,
    `video ["avc1", "hvc1"]`, `audio ["mp4a"]`, `defaultVideo "avc1"`,
    `defaultAudio "mp4a"`, `trimsLeadingAudio: true`. This only if Task 1
    rules A; under ruling B the manifest has no `muxers` (deviation 71).
- **`inputFormats`**, in the manifest and `describe`, best first:
  `h264` `["nv12", "rgba8"]`; `hevc` `["nv12", "p010", "rgba16", "rgba8"]`.
  - Both video encoders describe `transports: ["sharedMemory", "pipe"]`.
  - `aac` describes `sampleRates: [44100, 48000]` and `channels: [1, 2]`.
- **Colour.** `Color` is only `bt709 / bt709 / bt709 / limited`. The plugin
  maps exactly those names, and refuses any other with
  `MediaError::Codec("unknown color {field} `{name}`")`, P5's and FFmpeg's
  wording. They map to:
  - `MF_MT_VIDEO_PRIMARIES = MFVideoPrimaries_BT709`;
  - `MF_MT_TRANSFER_FUNCTION = MFVideoTransFunc_709`;
  - `MF_MT_YUV_MATRIX = MFVideoTransferMatrix_BT709`;
  - `MF_MT_VIDEO_NOMINAL_RANGE = MFNominalRange_16_235`.
  Every encoder input and output type, and every muxer video type, carries
  them.
- **Linking.** No Celesta crate links FFmpeg (HANDOFF.md), and this plugin
  links none. `celesta-codec-ffmpeg` is a `cfg(windows)` **dev**-dependency.
- **Tests.** Rust tests stay in Rust. Script tests use Vitest, written in
  TypeScript. Packaging logic is PowerShell, with no unit tests today;
  check it through `scripts/test-package.mjs` (AGENTS.md, existing files).
  - Every test that needs Media Foundation is in a `#[cfg(windows)]` module,
    or in a `tests/*.rs` file starting with `#![cfg(windows)]`.
  - A test whose component describes unavailable prints
    `skipping <test>: <reason>` and passes; CI's "Report skipped tests"
    lists it. Export tests also skip without a GPU, as the FFmpeg plugin's
    do.
  - On the user's Windows machine, only hardware-encoder tests may skip,
    and only where no hardware MFT is listed.
- **Hardware-dependent tests** (P5's lesson, `4c4875ad`, `37ce1a2b`,
  `d2ebdd1c`). A test that compares decoded values or bitstream structure
  sets `hardware: false`, pinning Microsoft's software MFT. It still allows
  a few code values of tolerance: 2 for 8-bit codes, 4 for 10-bit codes,
  as P5 started. Widen a tolerance only with the measured values in the
  commit message. Hard invariants are never loosened:
  - the packet count, a first keyframe;
  - `dts ≤ pts`, strictly increasing `dts`;
  - every input `pts` answered.
  A check that holds only when the encoder reorders runs only when some
  packet has `dts ≠ pts`, and otherwise prints `skipping …: the encoder did
  not reorder`. Tests of `hardware: true` check only the hard invariants,
  and colour within 16 codes.
- **Windows file semantics** (P2a's Windows CI failure, test
  `a_muxer_whose_finish_failed_can_still_be_aborted`, now `#[cfg(unix)]`).
  A file still open cannot be deleted or replaced.
  - Every session releases its COM objects (`IMFSinkWriter`,
    `IMFSourceReader`, `IMFByteStream`) before answering the paths of a
    finish or an abort.
  - Tests that delete or overwrite files do so only after the plugin has
    answered.
- **Commits.** Never amend; each task ends in a new commit. Stage files
  explicitly; never commit `.superpowers/`, `plugins/dist`, or scratch
  files. Commits are signed (see How to start); never bypass signing.
  **Every commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`**, whatever
  model wrote it. Never push or open a PR without asking the user first.
- **Commands** (PowerShell, in the checkout, `VCPKG_ROOT` set):
  - root workspace: `cargo build -p celesta-codec-test`, then
    `cargo test --workspace`;
    `cargo clippy --workspace --all-targets --locked -- -D warnings`;
    `cargo fmt --all -- --check`;
  - plugins workspace:
    `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`;
    `cargo clippy --manifest-path plugins/Cargo.toml --workspace --all-targets --locked -- -D warnings`;
    `cargo fmt --check`, run inside `plugins/`;
  - this plugin alone:
    `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-mediafoundation --locked`;
  - scripts: `pnpm install --frozen-lockfile` once, then
    `pnpm --dir scripts/plugins test`, `pnpm run test:runtime`, and
    `pnpm --dir packages/website test`;
  - `git diff --check`.
- **Builds between tasks.** Task 2 adds the crate. Until its own task lands,
  every component answers `componentUnavailable` at `open`. The workspace
  builds, and its tests pass, after every task, on Windows and on macOS
  and Linux. Nothing Windows-only may sit outside a `#[cfg(windows)]`
  module. Reviewers check the gating. To check that the crate still builds
  for the other systems, run `cargo check --manifest-path plugins/Cargo.toml -p celesta-codec-mediafoundation --target x86_64-unknown-linux-gnu`
  after `rustup target add x86_64-unknown-linux-gnu` (`check` needs no
  linker), or rely on CI.

## Deviations from the spec

Deviations 70–76 are in the spec's P6 paragraph (OS-native plugins), which
Task 10 rewrites as built. In short:
- 70: no ProRes and no `mov`.
- 71: the `mp4` muxer only under Task 1's ruling A.
- 72: RGB converted in the plugin.
- 73: AAC is 16-bit, at 44.1 or 48 kHz, at four bitrates, with a measured
  delay.
- 74: hardware first, with the `hardware` setting.
- 75: the decoder converts as Media Foundation does and drops alpha.
- 76: no file is held open between requests.

A new deviation gets the next number (77 on) and goes in Task 10's spec
edit and the handoff note.

## Decisions the spec left to this plan

- **Time.** Media Foundation's times are 100 ns units (`hns`). Track times
  are converted with `to_hns(t, timescale) = round(t × 10⁷ / timescale)`
  and back with `from_hns(h, timescale) = round(h × timescale / 10⁷)`, in
  `i128` with round-half-away-from-zero. Every frame time of a 30000/1001
  or 60000/1001 track round-trips exactly over 10⁶ frames (Task 3 test).
- **The MFT driver.**
  - Enumeration is `MFTEnumEx(category, flags, input, output)`, with
    `MFT_CATEGORY_VIDEO_ENCODER` for video and `MFT_CATEGORY_AUDIO_ENCODER`
    for audio. With `hardware: true` the flags are
    `MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_LOCALMFT | MFT_ENUM_FLAG_SORTANDFILTER`;
    with `false`, only `MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER`.
  - Candidates are tried in order. One that fails to activate, or refuses
    the output or input type, is released, and the next is tried. The last
    refusal is the error.
  - An asynchronous MFT is unlocked (`MF_TRANSFORM_ASYNC_UNLOCK`) and fed
    on `METransformNeedInput`, its output taken on `METransformHaveOutput`.
    Draining sends `MFT_MESSAGE_COMMAND_DRAIN` and reads until
    `METransformDrainComplete`.
  - Events are read on the serving thread with
    `GetEvent(MF_EVENT_FLAG_NO_WAIT)`, polling every 1 ms
    (`MF_E_NO_EVENTS_AVAILABLE` means "not yet"), through a generic
    `wait_for(poll, stall)` helper. After 60 s with no event (P5's stall
    rule) it is `MediaError::Codec("{name} stopped answering")`.
  - `MFT_OUTPUT_STREAM_PROVIDES_SAMPLES` is honoured; otherwise output
    samples are allocated at `GetOutputStreamInfo().cbSize`.
  - `MF_E_TRANSFORM_STREAM_CHANGE` renegotiates the output type, from
    `GetOutputAvailableType(0)` with the same subtype.
  - The MFT's friendly name (`MFT_FRIENDLY_NAME_Attribute`) is kept for
    errors and for tests to print.
- **H.264 and HEVC.** The output type is set before the input type, with:
  - `MF_MT_SUBTYPE` `MFVideoFormat_H264` or `MFVideoFormat_HEVC`;
  - `MF_MT_FRAME_SIZE`, `MF_MT_FRAME_RATE` (the frame rate),
    `MF_MT_PIXEL_ASPECT_RATIO` 1:1;
  - `MF_MT_INTERLACE_MODE` progressive;
  - `MF_MT_AVG_BITRATE`: `bitrate`, or 8 000 000 in quality mode, since the
    MFT requires one;
  - `MF_MT_MPEG2_PROFILE` (below);
  - the colour attributes.

  The input type is `MFVideoFormat_NV12` or `MFVideoFormat_P010` with the
  same size, rate, and colour. Encoder properties go through `ICodecAPI`:
  - `CODECAPI_AVEncCommonRateControlMode`: `eAVEncCommonRateControlMode_Quality`
    with `CODECAPI_AVEncCommonQuality` = `quality`, or
    `eAVEncCommonRateControlMode_UnconstrainedVBR` with
    `CODECAPI_AVEncCommonMeanBitRate` = `bitrate`;
  - `CODECAPI_AVEncMPVGOPSize` = `keyframeInterval`;
  - `CODECAPI_AVEncMPVDefaultBPictureCount` = 2 with `bFrames`, else 0;
  - `CODECAPI_AVEncCommonLowLatency` = false.

  A property the MFT reports unsupported (`IsSupported` false) is skipped,
  except the rate-control mode. An unsupported mode is
  `MediaError::InvalidSettings { key: "rateControl", message: "{name} has no constant quality mode; choose Average bitrate, or turn Hardware off" }`
  (and likewise for bitrate).
  - Profiles: `h264` `high` → `eAVEncH264VProfile_High`, `main` →
    `eAVEncH264VProfile_Main`, `baseline` → `eAVEncH264VProfile_Base`;
    `hevc` 8 → `eAVEncH265VProfile_Main_420_8`, 10 →
    `eAVEncH265VProfile_Main_420_10`.
  - Baseline takes `bFrames` as false.
  - Odd width or height is refused at `open`, whatever the input:
    `MediaError::Codec("H.264 needs even dimensions, not WxH")`, likewise
    `HEVC` (P5's rule, deviation 62's).
- **Bitstream.**
  - The MFT gives Annex B with in-band parameter sets, and the output type's
    `MF_MT_MPEG_SEQUENCE_HEADER` gives them too.
  - The plugin builds `TrackConfig.description` from the first parameter
    sets it sees: avcC from SPS and PPS, hvcC from VPS, SPS, and PPS.
  - It writes each sample as 4-byte length-prefixed NAL units, dropping
    access unit delimiters and parameter sets (H.264 types 9, 7, 8; HEVC
    types 35, 32, 33, 34). Every other NAL unit, SEI included, is kept.
  - A later parameter set that differs from the first is
    `MediaError::Codec("{name} changed its parameter sets mid-stream")`.
  - The codec string comes from the record (P5's `avc_codec_string`,
    `hevc_codec_string`).
- **DTS.**
  - `dts` comes from `MFSampleExtension_DecodeTimestamp` when the sample
    has it, else from the sample time.
  - Then, as P5 does, it is shifted back by the reorder depth whenever the
    stream reorders: `dts = dts − depth × frame duration`. `depth` comes
    from the SPS (P5's `avc_reorder_depth`, `hevc_reorder_depth`), else 16;
    it is 0 with `bFrames: false` and for Baseline.
  - A packet that would still have `dts > pts`, or a `dts` not strictly
    increasing, is `MediaError::Codec` naming the depth, and is never
    written.
- **RGB input** (deviation 72): `convert.rs`.
  - `rgba8_to_nv12` and `rgba16_to_p010`, with straight alpha ignored.
  - Per pixel, with R, G, B in [0, 1]:
    `Y' = 0.2126 R + 0.7152 G + 0.0722 B`;
    `Y = 16 + 219 Y'`, `Cb = 128 + 224 (B − Y') / 1.8556`,
    `Cr = 128 + 224 (R − Y') / 1.5748`, scaled by 4 for 10 bits. Chroma is
    the mean of the 2×2 block's Cb and Cr before rounding.
  - Round to nearest; 10-bit samples are `code << 6`, little-endian.
  - Pure red is (63, 102, 240) in 8 bits and (250, 409, 960) in 10 bits.
- **Input choice** (`choose_input`):
  - `h264` → `nv12` when offered, else `rgba8`.
  - `hevc` 8-bit → `nv12`, else `rgba8`; 10-bit → `p010`, else `rgba16`.
  - An offer without the needed format is
    `MediaError::InvalidSettings { key: "input", .. }`, the wording P5 and
    FFmpeg use.
  - `alpha` is always false.
- **AAC** (deviation 73).
  - Output `MFAudioFormat_AAC` with `MF_MT_AAC_PAYLOAD_TYPE` 0 (raw),
    `MF_MT_AUDIO_SAMPLES_PER_SECOND`, `MF_MT_AUDIO_NUM_CHANNELS`,
    `MF_MT_AUDIO_BITS_PER_SAMPLE` 16, and `MF_MT_AUDIO_AVG_BYTES_PER_SECOND`
    = `bitrate / 8`. Input `MFAudioFormat_PCM` 16-bit interleaved.
  - `f32 → i16` is `round(clamp(x, −1, 1) × 32767)`.
  - The rate answered: the mix's when 44100 or 48000, else 48000.
  - `delay` is the constant `AAC_DELAY` that Task 1 measures. The `k`-th
    packet has `pts = 1024·k − AAC_DELAY`, `duration = 1024`. The last packet
    has the duration it covers, `frames + delay − 1024 × (packets − 1)`, as
    in P5.
  - `config` is `mp4a.40.2` with the AudioSpecificConfig. The ASC is the
    tail of the output type's `MF_MT_USER_DATA`, after the 12-byte
    `HEAACWAVEINFO` fields (`wPayloadType`, `wAudioProfileLevelIndication`,
    `wStructType`, two reserved).
- **Settings.** Keys follow P5's wherever they mean the same.
  - `h264`:
    - `profile` select `high` "High" / `main` "Main" / `baseline`
      "Baseline", default `high`.
    - `rateControl` select `quality` "Constant quality" / `bitrate`
      "Average bitrate", default `quality`, group "Rate control".
    - `quality` integer 0–100, step 1, default 70, group "Rate control",
      `visibleWhen {rateControl: quality}`, description "Higher is better
      quality and larger."
    - `bitrate` integer, minimum 100000, unit `bit/s`, default 8000000,
      `visibleWhen {rateControl: bitrate}`.
    - `keyframeInterval` integer 1–1000, default 250, advanced, description
      "The most frames between keyframes."
    - `bFrames` boolean, default `true`, advanced, label "B-frames",
      description "Reordered frames compress better; turn off for players
      that cannot decode them."
    - `hardware` boolean, default `true`, advanced, label "Hardware
      encoder", description "Use the GPU's encoder when Windows has one;
      off uses Microsoft's software encoder."
  - `hevc`: `rateControl`, `quality`, `bitrate`, `keyframeInterval`,
    `bFrames`, `bitDepth` (select `8` "8-bit (Main)" / `10` "10-bit
    (Main 10)", default `8`), and `hardware`.
  - `aac`: `bitrate` select `96000` "96 kbit/s" / `128000` / `160000` /
    `192000`, default `192000`.
  - `mp4` (ruling A only): `faststart` boolean, default `true`, label "Fast
    start", which sets `MF_MPEG4SINK_MOOV_BEFORE_MDAT`.
- **Availability** (`describe`).
  - When `MFStartup` failed, every component is unavailable with "Media
    Foundation is not available on this system (MFStartup failed:
    0x{hresult:08X}); on Windows Server add the Media Foundation feature,
    on an N edition install the Media Feature Pack".
  - Otherwise:
    - `h264` is available when `MFTEnumEx` (hardware flags) lists an
      `MFVideoFormat_NV12` → `MFVideoFormat_H264` encoder, else "Media
      Foundation has no H.264 encoder on this system".
    - `hevc` likewise for `MFVideoFormat_HEVC`, else "Media Foundation has
      no HEVC encoder on this system; install a GPU driver with one or the
      HEVC Video Extensions".
    - `aac` likewise for `MFAudioFormat_PCM` → `MFAudioFormat_AAC`, else
      "Media Foundation has no AAC encoder on this system".
    - The decoder and the muxer are always available.
  - `hardware: false` with no software encoder listed (HEVC usually) is
    `componentUnavailable` at `open`, with "no software HEVC encoder;
    turn Hardware on".
- **COM and start-up.**
  - `MfCodec::new()` calls `CoInitializeEx(COINIT_MULTITHREADED)` and
    `MFStartup(MF_VERSION, MFSTARTUP_FULL)` once, and keeps the failure
    (for `describe`) rather than failing.
  - `Drop` calls `MFShutdown` and `CoUninitialize` for what succeeded.
  - The program serves on its main thread; no COM object crosses threads.
- **Output path.** Celesta stages its output as an empty file beside the
  final path (`stage_beside`) before `openMuxer`. The muxer accepts an
  empty regular file at `output`, which the sink writer overwrites. It
  refuses anything else with `MediaError::Codec("{output} already exists")`,
  as P5 does.
- **Muxer** (ruling A).
  - `MFCreateSinkWriterFromURL(output, None, attributes)` with
    `MF_TRANSCODE_CONTAINERTYPE = MFTranscodeContainerType_MPEG4`,
    `MF_SINK_WRITER_DISABLE_THROTTLING = TRUE` (the host paces), and
    `MF_READWRITE_DISABLE_CONVERTERS = TRUE`.
  - One stream per track, whose input type equals its output type
    (passthrough), built from `Track`:
    - video: subtype from the family (`avc1` and `avc3` → H.264, `hvc1`
      and `hev1` → HEVC), size, rate, colour, and
      `MF_MT_MPEG_SEQUENCE_HEADER` as Annex B from the avcC or hvcC;
    - audio: `mp4a.40.2` only, with the `HEAACWAVEINFO` user data built
      from the ASC.
  - Anything else is `MediaError::Codec("{muxer} does not take {codec}")`.
  - Samples are Annex B, with `MFSampleExtension_CleanPoint` for keys and
    `MFSampleExtension_DecodeTimestamp`. How the delay and pre-roll are
    hidden is Task 1's finding, written into Task 6 before it starts.
- **Decoder** (deviations 75, 76).
  - `MFCreateSourceReaderFromURL` with
    `MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING = TRUE`; video
    output `MFVideoFormat_RGB32`, made opaque RGBA in place; audio output
    `MFAudioFormat_Float` at the file's rate and channels.
  - Frames are the ones shown at the requested time (`timestamps_match`),
    the last frame past the end, at the displayed size. Rotation by
    `MF_MT_VIDEO_ROTATION` (0, 90, 180, 270) is applied on the CPU.
  - `SetCurrentPosition` seeks; the reader then reads forward to the frame.
  - A sequential session keeps its reader while requests move forward. It
    reopens on a backward jump or a forward jump of more than
    `MAX_FORWARD_WALK_SECONDS = 1.0`, and closes after
    `IDLE_SESSION_FRAMES = 30` frames unasked or past
    `MAX_VIDEO_SESSIONS = 16`: P5's constants. Nothing else keeps a reader.
  - Probe codec names are FFmpeg's (`h264`, `hevc`, `aac`, `pcm_s16le`,
    …), else the subtype's FourCC. Colour names are FFmpeg's (`bt709`,
    `bt2020`, `smpte2084`, `arib-std-b67`, `bt2020nc`, …).
  - A file the reader cannot open, or one without audio and video, is
    `MediaError::Unsupported`.
- **Shared code with P5.** `plugins/avfoundation`'s `params.rs` (codec
  strings, reorder depth), `esds.rs`'s `aac_codec_string`, `color.rs`, and
  the settings helpers in `components.rs` are private to that crate. P5
  copied FFmpeg's helpers the same way. Copy what this plugin needs into
  its own modules, and say where each came from in its module comment.
  Moving them into a shared crate is a follow-up (Task 10's Not done), not
  this phase.
- **Packaging.**
  - `package-windows.ps1` builds `plugins/mediafoundation` with
    `build.mts` into `$package/plugins`, after FFmpeg. It copies the VC
    runtime DLLs beside its program, as for FFmpeg.
  - Its license closure walks both bundled roots,
    `celesta-codec-ffmpeg` and `celesta-codec-mediafoundation`, as
    `package-macos.py`'s `bundled_packages` does.
  - `test-package.mjs` checks the plugin's manifest on Windows. It exports
    one MP4 through `--video-encoder mediafoundation/h264` when
    `--list-components --check --json` says that encoder is available, and
    otherwise prints `skipping the Media Foundation export: {reason}`.
- No example scene covers export codecs, so none is updated.

## Review focus

The tests that pin each of these are in the task named.

1. **A file is not left locked.** After an `abort` or a failed `finish`,
   and after a probe or a single-frame decode, `std::fs::remove_file`
   succeeds at once (Task 6, `abort_releases_the_output`; Task 8,
   `probing_does_not_lock_the_file`).
2. **Media Foundation's B-frames go into another plugin's muxer.** Every
   H.264 and HEVC packet has `dts ≤ pts` and strictly increasing `dts`, so
   FFmpeg's MP4 takes them (Task 4,
   `h264_and_hevc_decode_times_precede_presentation`; Task 7,
   `mediafoundation_h264_with_ffmpeg_aac_in_ffmpeg_mp4`).
3. **Times survive 100 ns units.** A 30000/1001 export's packets have
   exactly the input `pts`, with no frame doubled or dropped by rounding
   (Task 3, `frame_times_round_trip_through_hns`; Task 4,
   `every_input_pts_comes_back`).
4. **An asynchronous hardware MFT that stops answering**, or a hardware MFT
   that refuses system-memory input, does not hang the export. The first
   gives an error after the stall limit; the second falls back to the next
   candidate (Task 4's unit tests in `src/mf/mft.rs`:
   `a_silent_transform_stalls_out` and `a_refusing_candidate_falls_back`).
5. **AAC delay and pre-roll are hidden, whoever reads the file**: through
   FFmpeg's MP4 always, and through this plugin's `mp4` under ruling A,
   the click lands at the same sample (Task 5,
   `aac_through_ffmpegs_mp4_is_aligned`; Task 6,
   `aac_delay_and_preroll_are_hidden_for_every_reader`).

---

### Task 1: Verify Media Foundation on Windows, and rule (no product code)

The spec's deviations 71 and 73 depend on what Media Foundation does where
its documentation is silent. This task finds out with throwaway probes,
outside the repository, in a scratch Cargo project depending on `windows`
0.62.2. It records the results by editing this plan.

- [ ] **Step 1: Probe this machine.** Record each answer (numbers, HRESULTs,
  MFT friendly names), and run each probe once with hardware MFTs and once
  with `hardware: false`:
  1. `MFStartup` succeeds. Note the Windows version (`winver`) and the GPU.
  2. `MFTEnumEx` lists, for NV12 → H.264, NV12 → HEVC, P010 → HEVC, and
     PCM → AAC: hardware first, then software, each with its friendly name,
     and whether it is asynchronous.
  3. Microsoft's software H.264 MFT encodes 30 frames of 64x36 NV12 at
     30000/1001 with `CODECAPI_AVEncMPVDefaultBPictureCount = 2`:
     - whether output samples carry `MFSampleExtension_DecodeTimestamp`;
     - whether any `dts ≠ pts`;
     - whether the SPS has `bitstream_restriction` (`max_num_reorder_frames`);
     - whether `MF_MT_MPEG_SEQUENCE_HEADER` is set on the output type after
       the first output;
     - whether samples are Annex B with in-band SPS/PPS.
     Repeat with the first hardware H.264 MFT, if any.
  4. The AAC MFT at 48 kHz stereo, 192 kbit/s, 1 s of silence with a click
     (1.0) at frame 4800. Mux its packets with `FfmpegCodec`'s `mp4`,
     with `delay: 0` in a scratch test, and decode with FFmpeg. The click's
     offset from 4800 is the priming, **`AAC_DELAY`** (expected near 1024
     or 2112: whichever it is, record it). Confirm at 44.1 kHz mono.
  5. The SinkWriter, MPEG-4 container, passthrough H.264 (B-frames) + AAC,
     each stream's input type equal to its output type:
     - (a) Does it accept negative sample times? If not, what error?
     - (b) Write the AAC track starting at `−(AAC_DELAY + 16384)` frames.
       If (a) is refused, start it at 0 with the leading samples marked
       `MFSampleExtension_Discontinuity`. Does either file have an `elst`
       whose media time hides exactly `AAC_DELAY + 16384`? Also check that
       the sink accepts the `HEAACWAVEINFO` user data with
       `wAudioProfileLevelIndication` 0xFE; if not, record the value it
       takes (0x29 is AAC Profile L2).
     - (c) Does the video track get a `ctts` box with B-frames, and is
       every frame decoded by FFmpeg at its pts?
     - (d) Is the file tagged `colr nclx` BT.709 limited?
     - (e) Does `MF_MPEG4SINK_MOOV_BEFORE_MDAT` put `moov` first?
     - (f) Inspect each file with `ffprobe -show_packets -show_entries stream_side_data`
       and a box dump (`mp4dump` or `ffprobe -v trace`).
  6. `MFCreateSourceReaderFromURL` on that MP4, on a MOV (FFmpeg's `mov`),
     and on an M4A, with advanced video processing:
     - RGB32 output at the native size;
     - `MF_MT_VIDEO_ROTATION` on an FFmpeg MP4 written with a 90° display
       matrix (`-display_rotation 90`);
     - `MFAudioFormat_Float` output at the native rate;
     - and whether `std::fs::remove_file` succeeds while the reader is
       alive (expected: no) and after it is released (expected: yes).
- [ ] **Step 2: Probe CI.** CI's Windows runner is `windows-latest`, a
  Windows Server image.
  - Ask the user before pushing anything. With their yes, push a throwaway
    branch whose workflow runs probe 1 (`MFStartup`) and probe 2
    (`MFTEnumEx`) on `windows-latest`.
  - If `MFStartup` fails, add a step `Install-WindowsFeature Server-Media-Foundation`
    and run again.
  - Record whether it works without a reboot, and which encoders the
    runner lists (expected: software H.264 and AAC, no HEVC).
  - Delete the throwaway branch afterwards (ask first).
- [ ] **Step 3: Rule, and edit this plan.** Write a "Verified on Windows"
  subsection under Decisions with every answer, then rule:
  - **`AAC_DELAY`**: the measured value goes into Task 5's interface and
    tests.
  - **DTS**: keep the Decisions' rule. If the software MFT never reorders
    even with `BPictureCount = 2`, say so: Task 4's reordering checks then
    run only on hardware.
  - **Ruling A**, if 5(b), 5(c), and 5(d) all pass: write the exact recipe
    for hiding the delay and pre-roll into Task 6's Interfaces before it
    starts.
  - **Ruling B** otherwise:
    - Task 6 becomes "no muxer": `plugin.json`'s `muxers` is removed;
      `describe` has none; the export tests' combinations change to
      `ffmpeg/mp4` with this plugin's encoders.
    - Write deviation 71's outcome ("the MPEG-4 sink cannot …: {finding}")
      into the spec's P6 paragraph.
  - **CI**: if the runner needs `Server-Media-Foundation`, the CI step goes
    into Task 2.
    - If it needs a reboot, or does not work, every test there skips with
      the `describe` reason. The handoff note says so, and the user's
      machine is the only full run.
    - Either way, CI must show the skips, through the existing "Report
      skipped tests" step.
  - Commit the plan edit: `docs: P6's Media Foundation findings and rulings`.

### Task 2: The `mediafoundation` plugin's crate, manifest, and `describe` (`plugins/mediafoundation`, CI)

**Files:**
- Create in `plugins/mediafoundation/`:
  - `Cargo.toml`, `plugin.json`;
  - `src/main.rs`, `src/lib.rs`, `src/components.rs` (every platform);
  - `src/mf/mod.rs`, `src/mf/startup.rs` (`MfCodec::new`, COM
    and `MFStartup`), `src/mf/enumerate.rs` (`MFTEnumEx` lists), and
    `src/mf/provider.rs` (`MfCodec`), all Windows-only.
- Modify:
  - `plugins/Cargo.toml`: `members = ["avfoundation", "ffmpeg", "mediafoundation", "vrm"]`;
  - `plugins/Cargo.lock`;
  - `scripts/plugins/version.mts`: `FOLLOWS_CELESTA` gains
    `'plugins/mediafoundation'`;
  - `.github/workflows/ci.yml`: the Plugins job's Clippy step runs on every
    system. Its `if` becomes `steps.affected.outputs.cargo != ''`, and its
    comment says each OS-native plugin is linted only on its own system.
    Add Task 1's `Server-Media-Foundation` step if ruled, as
    `if: steps.affected.outputs.any == 'true' && runner.os == 'Windows'`.
  - `.github/workflows/package.yml`: the plugin-version loop becomes
    `for plugin in ffmpeg avfoundation mediafoundation`.
- Test:
  - `plugins/mediafoundation/src/components.rs` tests, on every platform;
  - `plugins/mediafoundation/tests/describe.rs`, on Windows;
  - `scripts/plugins/test/bump-version.test.mts`: its fixture gains
    `plugins/mediafoundation/Cargo.toml` and `plugin.json`.

**Interfaces:**
- `Cargo.toml`:
  - `[package]`: `name = "celesta-codec-mediafoundation"`,
    `version = "0.9.0"`, edition 2024, `license = "MIT OR Apache-2.0"`,
    `rust-version = "1.89"`, description "Celesta's Windows media plugin:
    Media Foundation encoders, muxer, and decoder, in a process of its
    own.", and the comments P5's `Cargo.toml` has (it builds everywhere;
    `os` skips it elsewhere).
  - `[dependencies]`: `celesta-composition`, `celesta-media`, and
    `celesta-plugin-host` (path, `=0.9.0`), and `serde_json`.
  - `[target.'cfg(windows)'.dependencies]`: `windows = { version = "0.62.2", features = [...] }`,
    starting from `Win32_Foundation`, `Win32_System_Com`,
    `Win32_System_Com_StructuredStorage`, `Win32_System_Variant`,
    `Win32_Media_MediaFoundation`, and `Win32_Media_Audio` (for
    `WAVEFORMATEX`); add only what `cargo check` asks for. Also
    `windows-core = "0.62.2"`.
  - `[target.'cfg(windows)'.dev-dependencies]`: `celesta-codec-ffmpeg`
    (path), `celesta-exporter`, `celesta-gpu-renderer`, and
    `celesta-project` (path, `=0.9.0`), `ffmpeg-sys-next = "8.1.0"`,
    `png = "0.18.1"`, and `tempfile = "3.23.0"`, as P5's.
- `src/lib.rs`, on every platform:
  ```rust
  pub const PLUGIN_ID: &str = "mediafoundation";
  pub const DECODER_ID: &str = "mediafoundation";
  mod components;   // + annexb, convert, hns, params, aac_user_data, color, names (Tasks 3, 8), cfg_attr(expect(dead_code)) off Windows as P5's lib.rs
  #[cfg(windows)] mod mf;           // not `windows`, which would shadow the `windows` crate
  #[cfg(windows)] pub use mf::provider::{MfCodec, install, registered};
  ```
- `components.rs`:
  ```rust
  pub(crate) enum VideoCodec { H264, Hevc }      // ids "h264", "hevc"; ALL; from_id; input_formats(); settings()
  pub(crate) enum AudioCodec { Aac }             // id "aac"; settings()
  pub(crate) enum MuxerFormat { Mp4 }            // id "mp4"; settings()   (ruling B: removed in Task 6)
  pub(crate) struct Encoders { pub h264: bool, pub hevc: bool, pub aac: bool }   // what MFTEnumEx listed
  pub(crate) fn describe_from(startup: Result<(), String>, encoders: &Encoders) -> Vec<ComponentDescription>
  ```
  The settings schemas and the availability reasons are the Decisions'.
  Copy P5's private `field`, `select`, `integer`, `when`, and `in_group`
  helpers.
- `mf/provider.rs`:
  ```rust
  pub struct MfCodec { /* startup: Result<Started, String>, decoder, sessions by SessionId, one counter */ }
  impl MfCodec { pub fn new() -> Self; pub fn factory() -> ProviderFactory }
  impl MediaProvider for MfCodec   // describe = describe_from(startup, enumerate::encoders()); every kind Some(self)
  pub fn registered() -> RegisteredPlugin   // Bundled, from plugin.json's media, factory making MfCodec in process
  pub fn install()
  ```
  - Until Tasks 4–8, each `open*`, `probe`, and decode returns
    `MediaError::ComponentUnavailable { component, reason: "not implemented yet" }`.
  - Sessions of every kind share one counter, and `abort_video` and
    `abort_audio` ignore a session of the other kind (P2a's rule).
- `main.rs`:
  - On Windows: `celesta_media::protocol::serve(&mut MfCodec::new(), PLUGIN_ID, env!("CARGO_PKG_VERSION"))`.
  - Elsewhere: `eprintln!("{PLUGIN_ID}: this media plugin runs on Windows only")`
    and exit 1.

- [ ] **Step 1: Write failing tests.**
  - `components.rs`, on every platform:
    - `plugin_json_declares_every_component`: `MediaManifest::from_json`
      of `plugin.json`'s `media` has no problems, and equals the Global
      constraints'.
    - `describe_matches_the_manifest`: with every encoder listed, one
      available description per manifest component, in manifest order.
      Video encoders have both transports and the manifest's
      `input_formats`; `aac` has `[44100, 48000]` and `[1, 2]`.
    - `a_missing_encoder_has_a_reason`: `hevc: false` gives the HEVC
      reason.
    - `a_failed_startup_makes_everything_unavailable`:
      `describe_from(Err("MFStartup failed: 0xC00D36E3".into()), ..)` makes
      every component unavailable, with a reason containing `MFStartup
      failed` and `Media Feature Pack`.
    - `settings_follow_the_decisions`: the keys, in order: `h264`'s
      `profile, rateControl, quality, bitrate, keyframeInterval, bFrames, hardware`;
      `hevc`'s `rateControl, quality, bitrate, keyframeInterval, bFrames, bitDepth, hardware`;
      `aac`'s `bitrate`; `mp4`'s `faststart`. And
      `celesta_media::settings::validate` of `{}` fills the defaults.
  - `tests/describe.rs`, on Windows:
    - `this_machine_lists_the_basic_encoders`: `h264` and `aac` are
      available. Print `hevc`'s state.
    - `the_program_describes_what_the_library_does`: through
      `MediaProcess::factory` with `ProviderUse::Describe` and
      `env!("CARGO_BIN_EXE_celesta-codec-mediafoundation")`.
  - `bump-version.test.mts`: the bump test's expectations include
    `plugins/mediafoundation`.
- [ ] **Step 2:** The crate's tests fail to compile, and
  `pnpm --dir scripts/plugins test bump-version` fails.
- [ ] **Step 3: Implement.**
  - Run `cargo update -p celesta-codec-mediafoundation` in `plugins/`.
    `git diff plugins/Cargo.lock` must only add this package and its
    dependency entries.
  - Check the cross-OS `cargo check` (Global constraints).
- [ ] **Step 4:** The tests, Clippy, `cargo fmt --check` in `plugins/`, and
  `node scripts/plugins/build.mts plugins/mediafoundation` all pass. The
  last writes `plugins/dist/mediafoundation/celesta-plugin.json` with
  `"protocol": 2` and `executable["windows-x86_64"]`.
- [ ] **Step 5: Commit:** `feat(codec-mediafoundation): the Media Foundation plugin's components`.

### Task 3: Bitstream, time, AAC user data, and RGB conversion (every platform)

**Files:**
- Create in `plugins/mediafoundation/src/`:
  - `annexb.rs`;
  - `params.rs` and `color.rs`, copied from P5 (the parts used);
  - `hns.rs`;
  - `aac_user_data.rs`;
  - `convert.rs`.
- Test: in-module tests, which run on Linux and macOS CI too.

**Interfaces:**
```rust
// annexb.rs
pub(crate) enum Codec { H264, Hevc }
pub(crate) fn nal_units(annexb: &[u8]) -> Vec<&[u8]>;                     // split on 00 00 01 / 00 00 00 01
pub(crate) struct ParameterSets { pub vps: Option<Vec<u8>>, pub sps: Vec<u8>, pub pps: Vec<u8> }
pub(crate) fn split_sample(codec: Codec, annexb: &[u8]) -> (Option<ParameterSets>, Vec<u8>);  // parameter sets seen; length-prefixed sample without AUD/VPS/SPS/PPS
pub(crate) fn avcc_from(sets: &ParameterSets) -> Result<Vec<u8>, MediaError>;   // version 1, profile/compat/level from SPS bytes 1–3, lengthSizeMinusOne 3
pub(crate) fn hvcc_from(sets: &ParameterSets) -> Result<Vec<u8>, MediaError>;   // ISO/IEC 14496-15 §8.3.3: PTL, chroma, bit depths, temporal layers from the SPS; arrays VPS, SPS, PPS
pub(crate) fn sequence_header(codec: Codec, record: &[u8]) -> Result<Vec<u8>, MediaError>;   // avcC/hvcC → Annex B parameter sets
pub(crate) fn to_annexb(sample: &[u8]) -> Result<Vec<u8>, MediaError>;             // 4-byte length-prefixed → 00 00 00 01
// params.rs (from avfoundation): avc_codec_string, hevc_codec_string, avc_reorder_depth, hevc_reorder_depth, UNKNOWN_REORDER_DEPTH
// color.rs (from avfoundation): check_color
// hns.rs
pub(crate) fn to_hns(t: i64, timescale: u32) -> i64;
pub(crate) fn from_hns(h: i64, timescale: u32) -> i64;
// aac_user_data.rs
pub(crate) fn asc_from_user_data(user_data: &[u8]) -> Result<Vec<u8>, MediaError>;   // skip the 12 HEAACWAVEINFO bytes
pub(crate) fn user_data_from_asc(asc: &[u8]) -> Vec<u8>;                             // payload type 0, profile-level 0xFE, struct type 0, zeros, then asc
pub(crate) fn aac_codec_string(asc: &[u8]) -> Result<String, MediaError>;           // from avfoundation's esds.rs
// convert.rs
pub(crate) fn rgba8_to_nv12(frame: &[u8], width: u32, height: u32) -> Vec<u8>;
pub(crate) fn rgba16_to_p010(frame: &[u8], width: u32, height: u32) -> Vec<u8>;
```
A truncated or malformed input is `MediaError::Codec("Media Foundation gave
a malformed {what}")`.

- [ ] **Step 1: Write failing tests.**
  - `annexb`:
    - `splits_and_strips`: an Annex B access unit of AUD + SPS + PPS + IDR
      (3- and 4-byte start codes mixed) → parameter sets found, and a
      sample of exactly `[len(IDR) as u32 BE, IDR…]`.
    - `avcc_round_trips`: `avcc_from` of the SPS and PPS inside P5's avcC
      test vector (`0164000dffe1000f2764000dac56505067e79a8080808101000428ee3cb0fdf8f800`)
      gives that avcC byte for byte, and `sequence_header` of it gives
      `00 00 00 01 SPS 00 00 00 01 PPS`.
    - `hvcc_from_parameter_sets`: `hvcc_from` of the VPS, SPS, and PPS
      inside P5's hvcC vector has the same codec string,
      `hevc_codec_string → "hvc1.2.4.L60.B0"`, and the same reorder
      depth, `Some(2)`, as that vector.
    - `to_annexb_inverts_split`.
    - `malformed_is_refused`: a cut SPS, and a length prefix past the end.
  - `hns`:
    - `frame_times_round_trip_through_hns`: for each (timescale, step) of
      (30000, 1001), (60000, 1001), (24000, 1001), (25, 1), (48000, 1024),
      and (44100, 1024), `from_hns(to_hns(k × step)) == k × step` for every
      k ≤ 10⁶.
    - Negative times round-trip too.
  - `aac_user_data`: `user_data_from_asc(&[0x11, 0x90])` is 14 bytes, and
    `asc_from_user_data` of it is `[0x11, 0x90]`; 11 bytes is an error.
    `aac_codec_string(&[0x11, 0x90]) == "mp4a.40.2"`.
  - `convert`:
    - `red_is_bt709_limited`: 2x2 opaque red gives NV12 `[63; 4]` +
      `[102, 240]`; `rgba16` red (`0xFFFF`) gives P010 Y `250 << 6` and
      Cb/Cr `409 << 6`, `960 << 6`.
    - `chroma_averages_the_block`: a 2x2 of two red and two blue gives the
      mean chroma.
    - `alpha_is_ignored`.
    - `sizes`: output length equals
      `PixelFormat::Nv12.frame_bytes(w, h)` / `P010`'s for 64x36.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-mediafoundation annexb hns aac_user_data convert`
  fails.
- [ ] **Step 3: Implement.** The copied files keep P5's tests that apply.
- [ ] **Step 4:** The tests and Clippy pass, on Windows and through CI's
  Linux and macOS jobs.
- [ ] **Step 5: Commit:** `feat(codec-mediafoundation): Annex B, avcC and hvcC, time units, and RGB conversion`.

### Task 4: H.264 and HEVC encoder MFTs (`plugins/mediafoundation`)

**Files:**
- Create:
  - `src/mf/mft.rs` (the driver);
  - `src/mf/video.rs`;
  - `src/mf/media_type.rs` (`IMFMediaType` builders: video, audio,
    and colour).
- Modify:
  - `src/components.rs`: `choose_input`, `codec_settings`;
  - `src/mf/provider.rs`: `VideoEncoder` for `MfCodec`.
- Test: `components.rs` tests (every platform), and
  `plugins/mediafoundation/tests/encoders.rs` (Windows).

**Interfaces:**
```rust
// components.rs, every platform
pub(crate) fn choose_input(codec: VideoCodec, settings: &Settings, offered: &[PixelFormat]) -> Result<PixelFormat, MediaError>;
pub(crate) enum CodecSetting { RateControlQuality(u32), RateControlMeanBitRate(u32), GopSize(u32), BPictureCount(u32), LowLatency(bool) }
pub(crate) fn codec_settings(codec: VideoCodec, settings: &Settings) -> Result<(Profile, Vec<CodecSetting>), MediaError>;
pub(crate) enum Profile { H264High, H264Main, H264Baseline, HevcMain, HevcMain10 }
pub(crate) fn hardware(settings: &Settings) -> bool;
// mf/mft.rs
pub(crate) struct Transform;   // one activated MFT, sync or async
pub(crate) fn candidates(category: GUID, input: GUID, output: GUID, hardware: bool) -> Result<Vec<IMFActivate>, MediaError>;
impl Transform {
    pub(crate) fn activate(activate: &IMFActivate) -> Result<Self, MediaError>;
    pub(crate) fn name(&self) -> &str;
    pub(crate) fn set_types(&mut self, output: &IMFMediaType, input: &IMFMediaType) -> Result<(), MediaError>;
    pub(crate) fn codec_api(&self) -> Option<ICodecAPI>;
    pub(crate) fn push(&mut self, sample: &IMFSample) -> Result<Vec<IMFSample>, MediaError>;   // outputs ready after it
    pub(crate) fn drain(&mut self) -> Result<Vec<IMFSample>, MediaError>;
    pub(crate) fn output_type(&self) -> Result<IMFMediaType, MediaError>;
}
pub(crate) fn wait_for<T>(poll: impl FnMut() -> Result<Option<T>, MediaError>, stall: Duration, name: &str) -> Result<T, MediaError>;
pub(crate) fn first_accepting<C, S>(candidates: &[C], mut try_one: impl FnMut(&C) -> Result<S, MediaError>) -> Result<S, MediaError>;   // the last refusal when none accepts
// Both are generic, so mft.rs's unit tests drive them with fakes: integration
// tests cannot reach a #[cfg(test)] hook in the library.
// mf/video.rs
pub(crate) struct VideoSession;
pub(crate) fn open(codec: VideoCodec, request: &VideoEncoderRequest<'_>) -> Result<(VideoSession, PixelFormat), MediaError>;
impl VideoSession {
    pub(crate) fn encode(&mut self, pts: i64, frame: Vec<u8>) -> Result<Encoded, MediaError>;
    pub(crate) fn finish(self) -> Result<Encoded, MediaError>;
    pub(crate) fn encoder_name(&self) -> &str;
}
```
- Samples are `MFCreateMemoryBuffer` of the input format's
  `frame_bytes`, with `SetSampleTime(to_hns(pts))` and
  `SetSampleDuration(to_hns(frame_rate.den))`. An `rgba8` or `rgba16`
  frame is converted first (Task 3).
- Output samples go through Task 3's `split_sample`. The first parameter
  sets give `Encoded.config`; `key` is `MFSampleExtension_CleanPoint`;
  `pts = from_hns(sample time)`; `dts` follows the Decisions.
- Packets are answered as soon as the MFT gives them.
- A frame whose length is not `input.frame_bytes(width, height)` is
  `MediaError::Codec`.
- A failing call is `MediaError::Codec("{call} failed: 0x{hresult:08X}")`.

- [ ] **Step 1: Write failing tests.**
  - `components.rs`, on every platform:
    - `inputs_follow_the_offer`: `h264` with `OFFERABLE` gives `Nv12`;
      offered `[Rgba16, Rgba8]` gives `Rgba8`. `hevc` 10-bit gives `P010`,
      and offered `[Rgba16, Rgba8]` gives `Rgba16`. Offered `[Yuv420p]`
      gives `Err(InvalidSettings { key: "input", .. })`.
    - `settings_become_codec_api_values`: `h264` defaults give
      `(H264High, [RateControlQuality(70), GopSize(250), BPictureCount(2), LowLatency(false)])`.
      `rateControl bitrate, bitrate 5000000, bFrames false` gives
      `RateControlMeanBitRate(5_000_000)` and `BPictureCount(0)`.
      `baseline` gives `BPictureCount(0)`. `hevc` `bitDepth 10` gives
      `HevcMain10`.
  - `src/mf/mft.rs` unit tests, on Windows:
    - `a_silent_transform_stalls_out`: `wait_for` over a poll that always
      answers `Ok(None)`, with a 20 ms stall, fails with
      `stopped answering`, naming the transform.
    - `a_refusing_candidate_falls_back`: `first_accepting` over three fakes,
      the first two refusing, gives the third's session. When all three
      refuse, the error is the third's.
  - `tests/encoders.rs`, on Windows: 64x36 at 30000/1001, timescale 30000,
    `MfCodec` in process, `hardware: false` unless named. Decoding goes
    through `FfmpegCodec`: mux with its `mp4` into a temporary file and
    decode that, as P5's tests do.
    - `h264_from_nv12_keeps_bt709_codes`: 10 frames of NV12 red, input
      `Nv12`, `config.codec` starting `"avc1.64"`. The decoded planes are
      within 2 of (63, 102, 240), and FFmpeg's MP4 is tagged BT.709
      limited.
    - `rgba8_input_is_converted_with_bt709`: offered `[Rgba16, Rgba8]`,
      solid red gives the same codes within 2.
    - `h264_and_hevc_decode_times_precede_presentation`: 30 frames of a
      moving pattern. The Global constraints' hard invariants hold, and the
      reordering checks run only when the stream reorders. With
      `bFrames: false`, every `dts == pts`. HEVC runs when it is available,
      otherwise it prints `skipping`.
    - `every_input_pts_comes_back`: 300 frames at 30000/1001 give 300
      packets whose `pts` are exactly `k × 1001`.
    - `hevc_10_bit_from_p010_is_main10`, when HEVC is available:
      `config.codec` starts `"hvc1.2."`, and the decoded codes are within 4
      of (250, 409, 960).
    - `packets_come_with_the_config`: the first answer with packets carries
      `config`, and no later one does.
    - `odd_sizes_are_refused`: 63x35 gives `Err(Codec(m))` with `m`
      containing `even dimensions`.
    - `a_frame_of_the_wrong_size_is_refused`: the session still finishes.
    - `hardware_encoders_keep_the_invariants`: `hardware: true`. It skips
      when the first candidate is Microsoft's software MFT, printing its
      name. It checks only the hard invariants and colour within 16.
- [ ] **Step 2:** The new tests fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and Clippy pass. On this machine, only HEVC and
  hardware tests may skip, and only where Task 1 found no such encoder.
- [ ] **Step 5: Commit:** `feat(codec-mediafoundation): H.264 and HEVC encoder MFTs`.

### Task 5: The AAC encoder MFT (`plugins/mediafoundation`)

**Files:**
- Create: `src/mf/audio.rs`.
- Modify: `src/components.rs` (`aac_bitrate`), `src/mf/provider.rs`
  (`AudioEncoder` for `MfCodec`).
- Test: `plugins/mediafoundation/tests/encoders.rs`, on Windows.

**Interfaces:**
```rust
pub(crate) const AAC_DELAY: u64 = /* Task 1's measurement */;
pub(crate) fn aac_bitrate(settings: &Settings) -> u32;   // components.rs: 96000 | 128000 | 160000 | 192000
pub(crate) struct AudioSession;
pub(crate) fn open(request: &AudioEncoderRequest<'_>) -> Result<(AudioSession, u32, u16, u64), MediaError>;   // (session, rate, channels, AAC_DELAY)
impl AudioSession {
    pub(crate) fn encode(&mut self, pts: i64, samples: &[f32]) -> Result<Encoded, MediaError>;
    pub(crate) fn finish(self) -> Result<Encoded, MediaError>;
}
```
- Input is `s16`, in samples of at most 1024 × 16 frames.
- Packet times follow the Decisions, counted by the plugin, not taken from
  the MFT's sample times.
- A `pts` that does not follow the samples already given is
  `MediaError::Codec`.

- [ ] **Step 1: Write failing tests.** At 48 kHz stereo: 1 s of silence
  with a click at frame 4800, in chunks of `1024 × 16` frames.
  - `aac_opens_at_a_rate_it_takes`: 48000/2 gives `(48000, 2, AAC_DELAY)`;
    44100/1 gives `(44100, 1, _)`; 96000/2 and 32000/2 give `(48000, 2, _)`.
  - `aac_packets_start_before_zero`: `pts` runs `−AAC_DELAY`,
    `1024 − AAC_DELAY`, …. Every `duration` is 1024 but the last, and
    `last.pts + last.duration == 48_000`. `config.codec == "mp4a.40.2"`,
    and its ASC starts `0x11 0x90`.
  - `aac_through_ffmpegs_mp4_is_aligned`: muxed by `FfmpegCodec`'s `mp4`
    with `delay: AAC_DELAY` and `leading_audio: 0`, then decoded by FFmpeg
    at 48 kHz: 48000 ± 1024 frames, and the click's peak at frame 4800
    exactly.
  - `aac_preroll_is_hidden_in_ffmpegs_mp4`: the same after 16384 frames of
    pre-roll (constant 0.25), with `leading_audio: 16384`. The click is at
    4800, and there is no 0.25 in the first 4000 frames.
  - `clipping_saturates`: samples of ±2.0 encode without error, and decode
    to within 1% of full scale.
- [ ] **Step 2:** `--test encoders aac` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and Clippy pass.
- [ ] **Step 5: Commit:** `feat(codec-mediafoundation): AAC encoder MFT`.

### Task 6: The SinkWriter `mp4` muxer (ruling A) — or no muxer (ruling B)

**Under ruling B**, this task is:
- remove `muxers` from `plugin.json`, and `MuxerFormat` with its tests;
- answer `None` from `MediaProvider::muxer`;
- add a test, `the_plugin_declares_no_muxer`, that the manifest and
  `describe` have none;
- commit: `feat(codec-mediafoundation): no muxer of its own (deviation 71)`.

Then go to Task 7. **Under ruling A:**

**Files:**
- Create: `src/mf/muxer.rs`.
- Modify: `src/mf/media_type.rs` (types from `Track`),
  `src/mf/provider.rs` (`Muxer` for `MfCodec`).
- Test: `plugins/mediafoundation/tests/muxers.rs`, on Windows.

**Interfaces:**
```rust
pub(crate) fn video_type(track: &Track) -> Result<IMFMediaType, MediaError>;   // media_type.rs
pub(crate) fn audio_type(track: &Track) -> Result<IMFMediaType, MediaError>;
pub(crate) struct MuxerSession;
pub(crate) fn open(request: &MuxerRequest<'_>) -> Result<MuxerSession, MediaError>;
impl MuxerSession {
    pub(crate) fn write(&mut self, packets: &[TrackPacket]) -> Result<(), MediaError>;
    pub(crate) fn finish(self) -> Result<Vec<PathBuf>, MediaError>;   // Finalize, release, then [output]
    pub(crate) fn abort(self) -> Vec<PathBuf>;                         // release without Finalize, then [output]
}
```
- The delay-and-pre-roll recipe is Task 1's, written here before this task
  starts.
- Times go through `to_hns`. Video samples are `to_annexb` of the packet,
  with `CleanPoint` for keys and `DecodeTimestamp`.
- After a failed `write` or `finish`, the session is released at once; a
  later `abort` answers `[output]` without a COM call.

- [ ] **Step 1: Write failing tests.** 64x36 at 30000/1001; packets from
  Tasks 4–5 and from `FfmpegCodec`'s `libx264` and `aac`. Read back with
  `FfmpegCodec`'s decoder, and with this plugin's decoder once Task 8
  lands; until then, with a test-file helper over `IMFSourceReader`.
  - `h264_and_aac_in_mp4_read_back`: `finish` answers `[output]`.
    FFmpeg probes 30 frames at 30000/1001, with RGBA within 3 of
    `(255, 0, 0)`. The file has `colr nclx` and an `avc1` sample entry.
  - `ffmpeg_x264_b_frames_keep_their_order`: libx264's B-frame packets
    come back in order: every frame at its pts, with a `ctts` box.
  - `aac_delay_and_preroll_are_hidden_for_every_reader`: the click track
    after 16384 frames of pre-roll, with `leading_audio: 16384`. FFmpeg's
    decoder and the Media Foundation reader both find the click at 4800,
    and no 0.25 in the first 4000 frames. Run it for this plugin's AAC
    and for FFmpeg's (its `delay`).
  - `an_empty_staged_output_is_replaced` / `a_file_with_content_is_not_replaced`:
    a 1-byte file gives `"already exists"`, and the byte is still there.
  - `long_runs_of_one_track_do_not_stall`: 300 video packets in one
    `write`, then all the audio; and the reverse. Both finish within 10 s,
    with every packet in the file (FFmpeg's demuxer counts them).
  - `an_unknown_codec_is_refused`: `opus` and `mp4a.40.5` give
    `Err(Codec("mp4 does not take …"))`.
  - `abort_releases_the_output`: after one `write`, `abort` answers
    `[output]`, and `std::fs::remove_file(output)` then succeeds. The same
    holds after a failed `write` (a packet on a track index out of range),
    and after a failed `finish` (video packets whose `dts` goes backwards,
    which the sink refuses at `Finalize`; if Task 1 found the sink takes
    them, any input Task 1 saw `Finalize` refuse).
- [ ] **Step 2:** `--test muxers` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and Clippy pass.
- [ ] **Step 5: Commit:** `feat(codec-mediafoundation): SinkWriter MP4 muxer`.

### Task 7: The program, the ring, and exports across plugins

**Files:**
- Test: `plugins/mediafoundation/tests/process.rs` and
  `plugins/mediafoundation/tests/export.rs`, both Windows-only.

**Interfaces:**
- Consumes:
  - everything above, and `celesta_codec_ffmpeg::registered()`;
  - `celesta_media::{MediaProcess, ProviderUse, ring_layout}`;
  - the exporter's options as P5's `tests/export.rs` builds them: P3's
    names, `component("…")`, `options(&[…])`.
- A test-only registry `[mediafoundation::registered(), celesta_codec_ffmpeg::registered()]`,
  both `Bundled`, installed once per test binary, as P5's.

- [ ] **Step 1: Write failing tests.**
  - `tests/process.rs`:
    - `encodes_through_the_ring`: the program, started with
      `ProviderUse::Export { ring: ring_layout(..) }` for 64x36 `nv12`,
      opens `h264` (`hardware: false`) with the ring declared
      (`frames_in_ring()`). It sends 10 frames three deep through
      `Pipeline::send_video_frame`, receives them in order, and finishes
      with 10 packets that decode as Task 4's do.
    - `errors_keep_their_kind`: `bitrate` as text gives
      `InvalidSettings { key: "bitrate" }`, and probing a WebM gives
      `Unsupported` (after Task 8: until then, `ComponentUnavailable`).
  - `tests/export.rs`: it holds the GPU and skips without one. It uses
    the 64x36 two-colour project and the click audio of P5's
    `tests/export.rs`.
    - `mediafoundation_h264_with_ffmpeg_aac_in_ffmpeg_mp4`:
      `--muxer ffmpeg/mp4 --video-encoder mediafoundation/h264 --audio-encoder ffmpeg/aac`.
      The JSON report names the three. FFmpeg decodes every frame within 3
      of the rendered colours, and the audio is as long as the video.
    - `ffmpeg_x264_with_mediafoundation_aac_in_ffmpeg_mp4`.
    - `mediafoundation_hevc_with_ffmpeg_opus_in_ffmpeg_mp4`: the spec's
      combination, when HEVC is available; it gives an `hvc1` sample
      entry.
    - Ruling A only: `mediafoundation_alone_writes_mp4` (`--muxer
      mediafoundation/mp4`, its default `h264` and `aac`) and
      `a_looping_mediafoundation_mp4_starts_its_sound_from_its_end` (P5's
      looping test, with `--muxer mediafoundation/mp4`).
    - Ruling B: `a_looping_export_with_mediafoundation_aac_starts_from_its_end`
      (`--muxer ffmpeg/mp4 --audio-encoder mediafoundation/aac`).
    - `ffmpeg_stays_the_default_beside_mediafoundation`: an `.mp4` export
      with no choices reports `ffmpeg/mp4`, `ffmpeg/libx264`, and
      `ffmpeg/aac`.
    - `a_software_renderer_offer_takes_rgba8`: the export with the CPU
      renderer, if the test helpers can pick it (P5's export tests show
      how), through `mediafoundation/h264` in `ffmpeg/mp4`. The colours
      are within 3. Without such a helper, Task 4's
      `rgba8_input_is_converted_with_bt709` covers it; say so in the
      report.
- [ ] **Step 2:** The new tests fail.
- [ ] **Step 3: Implement** what the tests reveal: none of this task's code
  is new except tests, and fixes go in the module they belong to.
- [ ] **Step 4:** `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`,
  its Clippy, and `cargo fmt --check` pass.
- [ ] **Step 5: Commit:** `test(codec-mediafoundation): exports through the program and across plugins`.

### Task 8: The SourceReader decoder (`plugins/mediafoundation`)

**Files:**
- Create: `src/names.rs` (every platform: subtype and colour enums to
  FFmpeg's names), `src/mf/decoder.rs`.
- Modify: `src/mf/provider.rs` (`Decoder` for `MfCodec`). Task 6's
  test helper reader is replaced by the decoder.
- Test: `names.rs` tests; `plugins/mediafoundation/tests/decode.rs`
  (Windows).

**Interfaces:**
```rust
// names.rs: takes plain values so it is tested everywhere
pub(crate) fn codec_name(fourcc: u32) -> String;              // H264/avc1 → "h264", HEVC/hvc1/hev1 → "hevc", "mp4a"/AAC subtype → "aac", PCM → "pcm_s16le"…; else the FourCC
pub(crate) fn primaries_name(value: u32) -> Option<&'static str>;   // MFVideoPrimaries_*: BT709 → "bt709", BT2020 → "bt2020", SMPTE170M → "smpte170m", EBU3213 → "bt470bg", DCI_P3 → "smpte431"
pub(crate) fn transfer_name(value: u32) -> Option<&'static str>;    // MFVideoTransFunc_*: 709 → "bt709", 2084 → "smpte2084", HLG → "arib-std-b67", sRGB → "iec61966-2-1", 10 → "linear"
pub(crate) fn matrix_name(value: u32) -> Option<&'static str>;      // MFVideoTransferMatrix_*: BT709 → "bt709", BT601 → "smpte170m", BT2020_10/12 → "bt2020nc"
// mf/decoder.rs
pub(crate) struct MfDecoder;
impl MfDecoder {
    pub(crate) fn probe(&mut self, path: &Path) -> Result<MediaProbe, MediaError>;
    pub(crate) fn decode(&mut self, request: &VideoRequest<'_>) -> Result<VideoFrame, MediaError>;
    pub(crate) fn decode_audio(&mut self, path: &Path, sample_rate: u32, channels: u16) -> Result<AudioBuffer, MediaError>;
}
```
- The enum values in `names.rs` are the numeric values of the
  `windows` crate's constants, written as literals with the constant's name
  in a comment, so the module builds without `windows`. A Windows-only
  test asserts that each literal equals the crate's constant.
- The probe's frame rate is `MF_MT_FRAME_RATE` as a `Rational`, reduced.
  Its size is the displayed one (90 and 270 swap), its duration
  `MF_PD_DURATION`, and its colour comes from the native type's
  attributes. There is one `AudioStream` per audio stream.
- Audio: mono is the channels' mean, and stereo from mono duplicates.
  Then `celesta_media::resample_audio` when the rates differ.
- A frame request on a file without video is `NoVideoStream`; a negative
  or non-finite time is `InvalidTimestamp`; a missing path is `NotFound`.

- [ ] **Step 1: Write failing tests.**
  - `names.rs`: one assertion per mapping, and an unknown value gives
    `None`.
  - `tests/decode.rs`. The files are written in the test by Tasks 4–6's
    components or by `FfmpegCodec`; a WebM by `FfmpegCodec`.
    - `probes_what_it_wrote`: a 10-frame 64x36 `h264` + `aac` MP4 at
      30000/1001 gives codec `h264`, 64x36, `Rational::new(30000, 1001)`,
      colour `bt709` ×3, and one audio stream at 48000 Hz, 2 channels,
      `aac`.
    - `decodes_the_frame_shown_at_each_time`: four colours at 10 fps.
      Times `0.0`, `0.15`, `0.299`, `0.3`, and `5.0` give frames 0, 1, 2, 3,
      and 3, within 3 per channel. The same through a `Sequential` session
      forward, then backward to `0.0`.
    - `decodes_audio_as_ffmpeg_does`: the click file. At `48000, 2` the
      click is at 4800; at `44100, 1` it is at `round(4800 × 44100 / 48000)` ± 1.
    - `reads_mov_and_m4a`: an FFmpeg MOV (H.264 + AAC) decodes. An
      audio-only `.m4a` probes with no video, and `decode_video` gives
      `NoVideoStream`.
    - `refuses_what_it_cannot_read`: the WebM gives `Unsupported` (unless
      this Windows has a WebM source: then the test asserts it decodes,
      and says so); a text file named `x.mp4` gives `Unsupported`; a
      missing path gives `NotFound`.
    - `rotated_video_is_displayed_upright`: FFmpeg's MP4 with a 90° display
      matrix probes as 36x64. Each corner is judged by its nearest quadrant
      colour (P5's `d2ebdd1c` rule).
    - `probing_does_not_lock_the_file`: after `probe`, and after a
      non-sequential `decode`, `std::fs::remove_file` succeeds. After a
      sequential decode, it succeeds once the session goes idle
      (`IDLE_SESSION_FRAMES` requests to another path).
- [ ] **Step 2:** `--test decode` and `names` fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The tests and Clippy pass.
- [ ] **Step 5: Commit:** `feat(codec-mediafoundation): SourceReader decoder`.

### Task 9: The Windows package

**Files:**
- Modify:
  - `scripts/package-windows.ps1`: build `plugins/mediafoundation` into
    `$package/plugins` after FFmpeg; copy the VC runtime DLLs beside its
    program; the license closure starts from both bundled roots, and fails
    when either is missing.
  - `scripts/test-package.mjs`: a `if (!mac) { … }` block after the FFmpeg
    checks.
  - `packaging/windows/README.md`: Redistribution. `plugins\mediafoundation`
    holds the Media Foundation plugin, MIT/Apache-2.0, linking only
    Windows system libraries.
- Test: `node scripts/test-package.mjs <package>` on this machine.

**Interfaces:**
- `package-windows.ps1`: `$bundled = @('celesta-codec-ffmpeg', 'celesta-codec-mediafoundation')`.
  Each must be exactly one workspace member, or it throws "The plugins
  workspace must have one {name} member, not {n}". The closure walk pushes
  every root.
- `test-package.mjs`, on Windows, checks:
  - `plugins/mediafoundation/celesta-plugin.json` has `roles` `["media"]`,
    the component lists of the Global constraints (ids only), and one
    `windows-*` executable that exists;
  - its `licenses/` equals `LICENSE-MIT` and `LICENSE-APACHE`;
  - `rust-packages.json` names `celesta-codec-mediafoundation` and
    `windows`;
  - then the conditional export of the Decisions, which writes an `ftyp`
    file.

- [ ] **Step 1:** Change the script and the test.
- [ ] **Step 2:** Run `pwsh scripts/package-windows.ps1` (see
  `packaging/windows/README.md` for its parameters), then
  `node scripts/test-package.mjs <package folder>`. If the build needs what
  this machine lacks, say so in the report and leave it to CI's Package
  job, which runs on `push`.
- [ ] **Step 3: Commit:** `feat(packaging): bundle the Media Foundation plugin in the Windows package`.

### Task 10: Measurements, docs, the spec, HANDOFF.md, and the handoff note

**Files:**
- Modify the user guide, English and Japanese:
  - `packages/website/content/en/docs/02-(guides)/09-export.md` and its
    `ja/…/09-export.md`: a section "Media Foundation components on
    Windows" after "AVFoundation components on macOS", with the same
    shape. It covers the components, their settings (`hardware` too),
    that FFmpeg stays the default, how to choose them with `--muxer`,
    `--video-encoder`, or a preset, and one combined example.
  - `en/ja/docs/05-(more)/05-plugins.md`: the bundled plugins list.
- Modify the Agent Skill: `skills/celesta/references/export.md` (the Media
  Foundation keys beside AVFoundation's) and
  `skills/celesta/references/plugins.md` (bundled plugins; `build.mts`
  skips it off Windows).
- Modify `HANDOFF.md`:
  - a workspace map row for `celesta-codec-mediafoundation`;
  - development setup: the plugin builds on Windows only, it needs
    `plugin.json`'s `os`, and the Windows prerequisites of How to start;
  - the Windows package's `plugins\mediafoundation`.
- Modify `docs/superpowers/specs/2026-11-08-media-components-design.md`:
  rewrite the P6 paragraph as built, with deviations 70–76 (and any
  later ones) and Task 1's verified answers; Phases → P6 as built.
- Create: `docs/handoff/<date>-media-windows.md`.

- [ ] **Step 1: Measure.** Release builds on this machine, medians of three
  interleaved runs, with `CELESTA_PLUGIN_PATH` holding both plugins built
  by `build.mts --out <scratch>`. Export 300 frames of
  `examples/spectra/film.tsx` at 1080p, and at 4K through P5's scratch 4K
  copy (described in P5's handoff note, Measurements). Compare
  `ffmpeg/libx264` (defaults) against `mediafoundation/h264` with
  `hardware` true and false, and `mediafoundation/hevc` where available.
  Record wall time, size, and PSNR against the RGBA render of frame 90, as
  P5's table. These are recorded only, with no threshold: the spec's hard
  requirement is the FFmpeg path, unchanged here.
- [ ] **Step 2: Write the docs.** English first, then Japanese with the same
  structure. Component names and keys are not translated.
- [ ] **Step 3: Verify.** Run every command of the Global constraints. List
  any test that printed `skipping` and why.
- [ ] **Step 4: Write `docs/handoff/<date>-media-windows.md`** in P5's
  note's format:
  - the specification as built;
  - the deviations;
  - Task 1's findings and their numbers;
  - Step 1's table;
  - the checks, with the machine (Windows version, CPU, GPU, driver);
  - what is not done:
    - GPU texture sharing (`dxgiShared`);
    - a muxer of its own, under ruling B;
    - HDR;
    - a shared bitstream crate for P5 and P6;
    - anything Task 1 or the reviews deferred.
- [ ] **Step 5: Commit:** `docs: the Media Foundation media plugin (P6)`.

### After Task 10

- Run the final whole-branch review (subagent-driven-development's last
  step), then the finishing skill.
- **Ask the user before pushing.** With their yes:
  - push `claude/media-components-p6`;
  - add it to the stack on P5 as a **draft** PR with `gh stack`, its base
    `claude/media-components-p5`;
  - put rendering screenshots in the PR only if a change renders
    differently. This phase changes no rendering, so say so in the body.
- The PR body ends with
  `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
