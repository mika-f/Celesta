# The AVFoundation media plugin, P5 (2026-11-09)

Phase P5 of the media components design
(`docs/superpowers/specs/2026-11-08-media-components-design.md`; plan
`docs/superpowers/plans/2026-11-08-media-components-p5.md`), cut from P2c
(`docs/handoff/2026-11-08-media-bt709.md`) and to be rebased onto P4. A
second first-party media plugin, `plugins/avfoundation`
(`celesta-codec-avfoundation`), encodes with VideoToolbox and AudioToolbox,
muxes with AVAssetWriter, and decodes with AVAssetReader, on macOS only,
through `objc2` bindings and Apple's frameworks alone (MIT OR Apache-2.0,
no FFmpeg). It proves the design's claim that components of independent
plugins combine: VideoToolbox's HEVC with FFmpeg's Opus in FFmpeg's MP4,
FFmpeg's x264 with AudioToolbox's AAC in AVFoundation's MP4.

## Specification

- **Plugin.** Id `avfoundation`, Cargo package and program
  `celesta-codec-avfoundation`, Celesta's version (`0.9.0`, bumped by
  `bump-version.mts` with the others: `FOLLOWS_CELESTA`), `roles:
  ["media"]`, protocol 2, `licenses` MIT and Apache, `"os": ["macos"]`. A
  member of the plugins workspace. `src/components.rs`, `color.rs`,
  `params.rs`, `esds.rs`, and `names.rs` hold everything that calls no
  framework and build and test on every system; `apple.rs`, `video.rs`,
  `audio.rs`, `formats.rs`, `muxer.rs`, `decoder.rs`, and `provider.rs`
  are `#[cfg(target_os = "macos")]`. Elsewhere the program prints
  `avfoundation: this media plugin runs on macOS only` and exits 1, so CI's
  Linux and Windows Plugins jobs still build the workspace; its Clippy
  step lints it on Linux and macOS (not Windows). Every `AvfCodec` method
  that reaches a framework runs in an autorelease pool of its own
  (`objc2::rc::autoreleasepool`), as does each buffer `decode_audio`
  reads: the program serves on a plain thread, which has none, and what
  the frameworks autorelease (convenience constructors,
  `localizedDescription`, `copyNextSampleBuffer`'s internals) would
  otherwise live as long as the process.
- **Components** (manifest and `describe`, in this order):
  - decoder `avfoundation` "AVFoundation", extensions `mp4`, `mov`, `m4a`;
  - video encoders `h264` "H.264 (VideoToolbox)" (`avc1`; inputs `nv12`,
    `rgba16`), `hevc` "HEVC (VideoToolbox)" (`hvc1`; `nv12`, `p010`,
    `rgba16`), and `prores` "ProRes (VideoToolbox)" (`ap4h`, `apch`;
    `rgba16`), each describing `transports: ["sharedMemory", "pipe"]`;
  - audio encoder `aac` "AAC (AudioToolbox)" (`mp4a`), describing the
    sample rates AudioToolbox lists
    (`kAudioFormatProperty_AvailableEncodeSampleRates`: 8000, 11025, 12000,
    16000, 22050, 24000, 32000, 44100, 48000 here) and channels 1 and 2;
  - muxers `mp4` "MP4 (AVFoundation)" (`mp4`, `m4v`; video `avc1`, `hvc1`;
    audio `mp4a`; defaults `avc1`, `mp4a`) and `mov` "QuickTime
    (AVFoundation)" (`mov`; video also `ap4h`, `apch`; audio `mp4a`, `lpcm`;
    defaults `ap4h`, `mp4a`), both `trimsLeadingAudio: true`.

  A video encoder is available when `VTCopyVideoEncoderList` lists its
  codec type (`prores` needs `ap4h`; `hq` without `apch` is
  `componentUnavailable` at `open`), otherwise `VideoToolbox has no
  {H.264|HEVC|ProRes 4444} encoder on this Mac`; `aac` when
  `kAudioFormatProperty_Encoders`, asked with `kAudioFormatMPEG4AAC` as its
  specifier, lists it, otherwise `AudioToolbox has no AAC encoder on this
  Mac`. The decoder and the muxers are always available.
- **Settings** (keys follow FFmpeg's where they mean the same):
  - `h264`: `profile` `high` / `main` / `baseline` (`high`; the
    `kVTProfileLevel_H264_*_AutoLevel`s); `rateControl` `quality` /
    `bitrate` (`quality`, group "Rate control"); `quality` 0–100 (70;
    VideoToolbox's `Quality` is the value / 100); `bitrate` at least
    100000 bit/s (8000000; `AverageBitRate`), group "Rate control";
    `keyframeInterval` 1–1000 (250, advanced; `MaxKeyFrameInterval`);
    `bFrames` (`true`, advanced; `AllowFrameReordering`). Baseline sends
    `AllowFrameReordering` false whatever `bFrames` says (ruling R9).
  - `hevc`: the same without `profile`, plus `bitDepth` `8` (Main) / `10`
    (Main 10) (`8`).
  - `prores`: `profile` `4444` / `hq` (`4444`); the codec type is the
    profile.
  - `aac`: `bitrate` 32000–320000 bit/s (192000). Outside what
    `kAudioConverterApplicableEncodeBitRates` gives at the opened rate and
    channels it is `InvalidSettings { key: "bitrate" }`, such as `stereo
    AAC at 48000 Hz takes 64000 to 320000 bit/s, not 32000` (ruling R10:
    strict, no clamping). The ranges depend on the rate too: 32–256 kbit/s
    mono and 64–320 kbit/s stereo at 44.1 and 48 kHz, 8–24 and 16–48 at
    8 kHz, 32–128 stereo at 22.05 kHz, 48–192 stereo at 32 kHz.
  - `mp4`, `mov`: `faststart` (`true`;
    `AVAssetWriter.shouldOptimizeForNetworkUse`).
- **Input choice** (`choose_input`). `h264` takes `nv12` when offered,
  else `rgba16`; `hevc` 8-bit the same, 10-bit `p010`, else `rgba16`;
  `prores` always `rgba16`, and `alpha` is true for ProRes 4444 only. An
  offer without the fallback is `InvalidSettings { key: "input" }`
  (`needs nv12 or rgba16`). `h264` and `hevc` refuse an odd width or
  height whatever the input (`H.264 needs even dimensions, not 63x35`,
  ruling R8): VideoToolbox encodes 4:2:0 and crops such a frame silently.
  ProRes keeps odd sizes (63x35 encodes and decodes at 63x35). The pixel
  buffers come from the session's pool: `nv12` is `420v`, `p010` is
  `x420`, `rgba16` is `kCVPixelFormatType_64RGBALE` (`l64r`), copied row
  by row into the buffer's own stride.
- **Colour.** Only `bt709 / bt709 / bt709 / limited`, mapped to the
  `ITU_R_709_2` primaries, transfer function, and matrix, set on the
  session and attached to every pixel buffer; any other name is
  `` Codec("unknown color {field} `{name}`") ``, FFmpeg's wording. The
  muxers tag every video track from `Track.color` the same way, with
  `FullRangeVideo` false.
- **Encoded packets.** VideoToolbox's output handler queues each sample
  (pts, dts, duration, `NotSync`, the 4-byte length-prefixed bytes) and,
  from the first, the `avcC` or `hvcC` in
  `SampleDescriptionExtensionAtoms`. Codec strings: `avc1.PPCCLL` from the
  SPS's profile, constraint, and level bytes, upper-case hex; `hvc1.` per
  ISO/IEC 14496-15 Annex E from the `hvcC`; `ap4h` or `apch`;
  `mp4a.40.<object type>` from the AudioSpecificConfig in AudioToolbox's
  magic cookie (an `ES_Descriptor`).
- **DTS shift.** `dts = vt_dts − depth × frame duration`, where `depth` is
  H.264's `max_num_reorder_frames` (VUI `bitstream_restriction`) or HEVC's
  `sps_max_num_reorder_pics` of the highest sub-layer, read from the first
  format description's parameter sets; 16 without that field (an upper
  bound: too large a shift is valid, too small is not); 0 with `bFrames`
  false, for Baseline, and for ProRes. VideoToolbox's H.264 SPS has a VUI
  without `bitstream_restriction`, so **H.264 is shifted by 16 frames**
  (an MP4's video edit list then starts at 16 frames' media time, 16016
  at 30000/1001); HEVC's says 2. A packet that would still have `dts >
  pts` is `MediaError::Codec` naming the depth and is never written.
- **AAC.** AudioToolbox's AAC-LC encoder through `AudioConverter`, from
  interleaved `f32` at the mix's rate when listed, else 48000, and 1
  channel for a mono mix, else 2. **Constant bitrate**
  (`kAudioCodecBitRateControlMode_Constant`, ruling R12): with
  AudioToolbox's default, long-term-average rate control, the first second
  came out worse than the rest, which is where a looping export's opening
  is: in `aac_codes_its_first_second_as_well_as_the_rest` the first hit's
  error was 0.00584 against at most 0.00048 for the later ones. `Constant`
  is even from the first packet, as libavcodec's `aac_at` chooses when
  given a bitrate: in `a_looping_avfoundation_mp4_starts_its_sound_from_its_end`
  the one-pass export's first hit is off by 0.000066 and the looped one's
  by 0.000085, below the later hits. The default `bitrate`, 192000, is
  refused for stereo below 32 kHz and mono below 44.1 kHz (AudioToolbox's
  ranges there top out at 128000 and 96000). `delay` is
  `kAudioConverterPrimeInfo.leadingFrames`, 2112. The k-th packet has `pts
  = first + 1024k − 2112`, `dts = pts`, duration 1024, except the last,
  whose duration is the frames it covers, so muxers trim the end.
  AudioToolbox holds the last three packets until the end of the stream,
  so the last always comes from `finish`.
- **Muxers** (`AVAssetWriter`, passthrough: `outputSettings` nil, the
  format description built from the `TrackConfig` as the source format
  hint).
  - **Output path.** Celesta stages its output as an empty file, and
    `AVAssetWriter` refuses a URL that exists, so the muxer removes the
    file at `output` when it is an empty regular file and refuses anything
    else (`{output} already exists`), after every track has been checked.
  - **Tracks.** A codec the muxer does not take is `{muxer} does not take
    {codec}` (`mp4 does not take opus`). AAC is `mp4a.40.2` (AAC-LC) only,
    whose packets the format description gives as 1024 frames: any other
    `mp4a` (`mp4a.40.5`, HE-AAC's 2048) is refused the same way.
    `lpcm.s16le`, `lpcm.s24le`, and `lpcm.f32le` are `mov`'s PCM. Audio
    tracks must be timed in sample frames. The movie timescale is the
    video track's (at AVFoundation's default of 600 a 30000/1001 video's
    edit list ended at 1.000 s rather than 1.001 s), and `mediaTimeScale`
    is set on video inputs only (an audio input throws).
  - **Trims.** AAC buffers before 0 carry
    `kCMSampleBufferAttachmentKey_TrimDurationAtStart` (`min(duration, −pts,
    what is still hidden)` of `delay + leadingAudio`), a short last packet
    `TrimDurationAtEnd = 1024 − duration`, and the first buffer
    `GradualDecoderRefresh = 1` and `"IsGradualDecoderRefreshAuthoritative"
    = true`, every attachment `ShouldPropagate` (deviation 60). An AAC
    track whose `delay` is 0 needs no start trim; AVAssetWriter takes it
    without the exception ruling R11 feared. PCM before 0 is cut from the
    packets' bytes (deviation 61).
  - **Interleaving.** Each input has a queue of sample buffers. After each
    packet `writePackets` appends to every input while it
    `isReadyForMoreMediaData`, until none takes more. **Back-pressure:**
    then, while the queues hold more than `MAX_PENDING_BYTES` (64 MiB) of
    packet bytes and no unfinished input is ready, it polls every 1 ms as
    `finish` does: the writer is then bound by its output (a disk-bound
    ProRes 4444 4K file is about 150 MB/s), and without the wait the
    exporter's 32 MiB batches of a spilled backlog would be queued here
    whole. An input that is ready has an empty queue, so the writer is
    waiting for that track's packets, which come only once `writePackets`
    returns: it returns then, and a long run of one track (all the video,
    then the audio) is still queued whole rather than stalling. `finish`
    drains the queues, polling every 1 ms, marks each input finished once
    its queue is empty, then waits for
    `finishWritingWithCompletionHandler`. A writer whose status leaves
    `writing` fails the next `writePackets` with its error (`AVAssetWriter
    could not write the file: …`); `writePackets` and `finish` give up
    after 60 s without progress. An abort cancels the writer.
  - **Exceptions.** `appendSampleBuffer`, `markAsFinished`, and
    `finishWritingWithCompletionHandler` run inside
    `objc2::exception::catch` (objc2's `exception` feature, which adds
    `objc2-exception-helper` to the lockfile): an Objective-C exception,
    such as one from a writer that failed on its own thread between the
    status check and the append, is `Codec("AVAssetWriter raised an
    exception as it tried to {append a sample|finish a track|finish the
    file}: {reason}")` rather than an aborted process.
- **FFmpeg's sample entries.** FFmpeg's MP4 and MOV muxers tag H.264 and
  HEVC with the codec string's spelling of its family (`hvc1` or `hev1`,
  `avc1` or `avc3`). libavformat's default in MP4 is `hev1`, which
  QuickTime does not play, so VideoToolbox's HEVC in FFmpeg's MP4 now gets
  `hvc1`. The FFmpeg plugin's own encoders write `avc1.` strings, so its
  files are unchanged.
- **Decoder** (`AVURLAsset` with precise timing, keys loaded with a
  completion handler, no run loop). The probe gives the displayed size
  (quarter-turn `preferredTransform`s swap it), the frame rate from
  `minFrameDuration`, FFmpeg's codec names (`h264`, `hevc`, `prores`,
  `aac`, `pcm_s16le` …, from the ASBD for PCM, else the four-character
  code) and colour names (`bt709`, `bt2020`, `smpte2084`, `arib-std-b67`,
  `bt2020nc` …). A file AVFoundation cannot open, or one with no audio and
  no video track, is `Unsupported`; a missing one `NotFound`. Frames are
  the ones shown at the requested time (`timestamps_match`), the last past
  the end, as 8-bit BGRA with `AVVideoColorPropertiesKey` BT.709 turned
  into RGBA and rotated upright (deviation 64). A sequential session keeps
  its `AVAssetReader` while requests move forward by at most a second and
  reopens it otherwise; idle sessions close as FFmpeg's do. Audio is the
  first audio track through `AVAssetReaderAudioMixOutput` as interleaved
  `f32`.
- **Building elsewhere** (Task 1). `plugin.json`'s optional `os` is a list
  of `std::env::consts::OS` names; `checkPluginSource` rejects anything
  else (`os must be a list of operating systems, such as ["macos"]`), it
  is not copied into `celesta-plugin.json`, and `build.mts` skips a plugin
  whose `os` leaves out the host (or `--target`'s) system: `avfoundation:
  skipped, it is built for macos only`, writing nothing for it. CI's
  Plugins Clippy step now runs on macOS as well as Linux.
- **Packaging** (Task 9). `scripts/package-macos.py` builds
  `plugins/avfoundation` with `build.mts` into `Contents/PlugIns` after
  FFmpeg (no `dylibbundler`: it links system frameworks only), checks its
  program as it checks the others' Mach-O files (architectures, only
  system dependencies, signing), and adds its crates to the licence
  inventory (`bundled_packages`). `scripts/test-package.mjs` checks the
  installed manifest, its licences, and its crates, and exports one MP4
  through `--muxer avfoundation/mp4`. `.github/workflows/package.yml`
  checks both plugins' versions.

## Deviations from the spec

Numbered after P4's 50–59; recorded in the spec's OS-native plugins and
Audio sections.

60. **The trim needs two more attachments.** With only
    `TrimDurationAtStart`, a passthrough AAC track gets an edit list of
    `trim − 2112` and no `roll` group: AVFoundation, which assumes 2112
    frames of implicit priming, plays it correctly, and FFmpeg plays it
    2112 frames late. The first audio buffer also carries
    `GradualDecoderRefresh = 1` and `"IsGradualDecoderRefreshAuthoritative"
    = true` (a key AVAssetReader's own passthrough buffers carry; no public
    constant), every attachment `ShouldPropagate`; AVAssetWriter then
    writes an edit list hiding exactly `delay + leadingAudio` and a `roll`
    group of −1, as its own AAC encoding does.
61. **Leading LPCM is cut, not trimmed**: the `mov` muxer drops the sample
    frames before 0 from the packets' bytes.
62. **`h264` and `hevc` also take `rgba16`**, for a software renderer's
    offer (no YUV, deviation 18), which VideoToolbox converts with the
    session's BT.709 limited matrix. They refuse odd dimensions (ruling R8,
    above); ProRes keeps them. `h264` takes no `p010`: VideoToolbox's H.264
    is 8-bit only.
63. **`avfoundation/mov`'s default audio is `mp4a`**, not FFmpeg's `lpcm`:
    the plugin has no PCM encoder and works alone.
64. **The decoder converts colour as AVFoundation does**, not with the
    FFmpeg plugin's HDR tone curve. FFmpeg decodes first among bundled
    plugins (P4's deviation 58), so this applies to files FFmpeg cannot
    read or when FFmpeg is turned off.

Rulings that changed the plan: R8 (odd sizes refused, above), R9
(Baseline never reorders), R10 (strict AAC bitrate), R11 (a delay-free AAC
track needs no start trim, checked), R12 (constant-bitrate AAC). The plan's
review focus 5 now reads: a software renderer's offer exports with `h264`
and `hevc` through `rgba16`, and an odd size is refused at `open` rather
than cropped.

## Decisions

Verified on this Mac (Apple M4, macOS 26.6.2) with Swift and Python probes
before planning (kept outside the repository):

- **ProRes 4444 takes `l64r` directly, no swizzle.** A frame of
  straight-alpha red and half-transparent green, as
  `kCVPixelFormatType_64RGBALE`, encodes byte-identical to the same frame
  as the documented big-endian `kCVPixelFormatType_64ARGB`, and decodes
  (FFmpeg, `yuva444p12le`) to red Y 1001, Cb 1637, Cr 3840, A 4095 and
  green at half alpha Y 2762, Cb 666, Cr 420, A 2048: BT.709 limited, the
  alpha kept.
- **Every VideoToolbox encoder** (`avc1`, `hvc1`, `ap4h`, `apch`) takes
  `420v`, `x420`, `l64r`, `b64a`, `y416`, and `BGRA`; a
  `kCVPixelFormatType_32RGBA` pixel buffer cannot be created (−6680), which
  is why `rgba8` is no input. VideoToolbox has software encoders beside
  the hardware ones for H.264, HEVC, and every ProRes profile.
- **B-frames.** VideoToolbox's H.264 and HEVC reorder by default, with
  decode timestamps after the presentation ones (pts 1001 at dts 3003);
  the `avcC` / `hvcC` come in the first sample buffer's format
  description, and samples are 4-byte length-prefixed.
- **AAC.** AudioToolbox's AAC-LC encoder primes 2112 frames, takes 8–48
  kHz, and its applicable bitrates are 32–256 kbit/s mono and 64–320
  kbit/s stereo (at 44.1 and 48 kHz; Task 5 found the lower rates'
  ranges); its magic cookie is an MPEG-4 `ES_Descriptor` whose
  DecoderSpecificInfo is the AudioSpecificConfig.
- **The trim recipe** (deviation 60), against a click at frame 4800 after
  2112 frames of priming and 16384 of pre-roll: with
  `TrimDurationAtStart` alone, the edit list is `trim − 2112` with no
  `roll` group and FFmpeg finds the click 2112 frames late (6912); with
  the two attachments and negative `pts`, both FFmpeg and AVFoundation
  find it at 4800, the edit list's media time is 18496 (2112 + 16384),
  and the `sgpd` `roll` is −1.

## Docs

The export guide (en and ja) gains "AVFoundation components on macOS" (en)
/ "macOS の AVFoundation コンポーネント" (ja): the components, their settings
and AAC's narrower bitrate ranges, that FFmpeg stays the default and how to
name them (`--muxer`, `--video-encoder`, `--audio-encoder`, a preset, MCP
and the package), mixing with FFmpeg's components, `avfoundation/mov`'s AAC
default, `--loop`, colour, availability, the decoder, and one combined
example. The plugins page (en and ja) lists `avfoundation` among the
bundled plugins and says `ffmpeg` goes first among them for decoding and
default components. The Agent Skill's `export.md` has the AVFoundation keys
beside FFmpeg's, and `plugins.md` the bundled plugin, its build, and
`plugin.json`'s `os`. HANDOFF.md has the crate's map row, the plugin's
macOS-only build and packaging, and the architecture boundary. The spec
records "As built (P5)", deviations 60–64, and the P5 phase row. No example
scene covers export codecs, so none is updated.

## Checks

On macOS 26.6.2, Apple M4, Metal, at `a5ceeb5a` plus this note's commit,
with `PKG_CONFIG_PATH` at Homebrew's `ffmpeg@8` and the `mbx` cargo shim on
`PATH`:

- `cargo build -p celesta-codec-test`, then `cargo test --workspace`
  (2,644 passed, 1 ignored as before), `cargo clippy --workspace
  --all-targets --locked -- -D warnings`, and `cargo fmt --all -- --check`.
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (552 passed; 1 ignored, `ffmpeg_stays_the_default_beside_avfoundation`,
  see Not done), `cargo clippy --manifest-path plugins/Cargo.toml
  --workspace --all-targets --locked -- -D warnings`, and `cargo fmt
  --check` in `plugins/`. With `--nocapture`, no test printed `skipping`
  in either workspace: every AVFoundation component is available on this
  Mac and the GPU tests ran on Metal.
- `pnpm run test:runtime` (17 packages, 1,046 tests),
  `pnpm --dir scripts/plugins test` (222), `pnpm --dir packages/website
  test` (52), `python3 -m unittest scripts/test_package_macos.py` (8), and
  `git diff --check`, all passed.

The tests that pin the plan's review focus (Tasks 4–8):
`h264_and_hevc_decode_times_precede_presentation`,
`odd_sizes_and_rgb_offers_take_rgba16`, `odd_sizes_are_refused`,
`aac_delay_and_preroll_are_hidden_for_every_reader`,
`a_looping_avfoundation_mp4_starts_its_sound_from_its_end`,
`avfoundation_hevc_with_ffmpeg_opus_in_ffmpeg_mp4`,
`ffmpeg_x264_with_avfoundation_aac_in_avfoundation_mp4`,
`an_empty_staged_output_is_replaced`, `a_file_with_content_is_not_replaced`,
`long_runs_of_one_track_do_not_stall`, `a_failed_writer_takes_no_more_packets`,
`an_aac_track_without_delay_is_written`,
`aac_codes_its_first_second_as_well_as_the_rest`, and the decoder's
`decodes_the_frame_shown_at_each_time` and
`rotated_video_is_displayed_upright`.

### The final review's fixes

After the whole-branch review, on the same Mac:

- **Back-pressure.** `muxer::tests::a_busy_writer_holds_writes_back`
  writes 16 batches of 4 MiB of one PCM track with the cap lowered to
  1 MiB and asserts that no `write` returns with more than the cap and a
  batch queued; at most 1 MiB was. With the wait taken out it fails at
  batch 4 with 6 MiB queued. `long_runs_of_one_track_do_not_stall` still
  passes, video first and audio first.
- **Autorelease pools.** A scratch test (not committed) ran 3,000
  `decode_video` calls without a sequential session (a reader each), then
  3,000 one-packet `write_packets`, in one process (`ps -o rss=`).
  Without the pools RSS grew from 31.8 MiB to 77.2 MiB over the decodes
  (about 15 KiB a call) and stayed flat over the writes; with them it
  stayed between 31.5 and 34.3 MiB throughout.
- **CI's encoders.** `h264_and_hevc_decode_times_precede_presentation`
  checks the 30 packets, the keyframe, `dts ≤ pts`, and increasing `dts`
  always, and a first `dts` before 0 only when some packet's `dts` differs
  from its `pts`; otherwise it prints `skipping {component}'s reordering
  checks: the encoder did not reorder` (a software encoder may make no
  B-frames). The looping export allows the first hit up to three times
  the later ones' error; the encoder's own test pins constant bitrate.
- **AAC-LC only.** `muxer::tests::only_aac_lc_is_taken`, the formats test,
  and `an_unknown_codec_is_refused` (`mp4 does not take mp4a.40.5`, and
  `mov`'s).
- **Exceptions.** `muxer::tests::an_exception_is_an_error` appends to an
  input whose writer never started, which raises: the error is
  `AVAssetWriter raised an exception as it tried to append a sample: ***
  -[AVAssetWriterInput appendSampleBuffer:] Cannot append sample buffer:
  Cannot call method when status is 0`.
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (555 passed, 1 ignored as before; no AVFoundation test printed
  `skipping`), Clippy and `cargo fmt --check` in `plugins/`, and `pnpm
  --dir packages/website test` (52) passed. The root workspace was not
  rebuilt: these fixes change no root crate.

### Measurements

Release builds of `celesta-exporter` and of both plugins (`node
scripts/plugins/build.mts plugins/ffmpeg plugins/avfoundation --out
<scratch>/dist`), `CELESTA_PLUGIN_PATH` holding only that folder, an empty
`HOME`, and every component named (`--muxer`, `--video-encoder`,
`--audio-encoder`) with default settings. 300 frames (`--from 0 --to 5`) of
`examples/spectra/film.tsx` (1920x1080, 60 fps; it has no sound, so no
audio track is written), and for 4K a scratch copy of it whose
`<Composition>` is 3840x2160 with everything after `<Assets>` in a `<Group
scale={2}>` (not committed). Wall time including start-up, three rounds
interleaved, median (each run); size of the file; PSNR (average of R, G,
B) of film frame 90 against the exporter's RGBA `--frame 90` PNG, decoded
as a BT.709 player does (Homebrew `ffmpeg` 8:
`select='eq(n\,90)',scale=in_color_matrix=bt709:in_range=tv,format=rgb24`
and `psnr`):

| Export | Wall time (s) | Size (bytes) | PSNR (dB) |
| --- | --- | --- | --- |
| 1080p `ffmpeg/mp4` + `ffmpeg/libx264` (`medium`, CRF 18) | 6.104 (4.680, 6.104, 6.364) | 5,201,777 | 39.36 |
| 1080p `avfoundation/mp4` + `avfoundation/h264` (quality 70) | 2.970 (3.936, 2.970, 2.632) | 6,126,571 | 38.88 |
| 1080p `avfoundation/mp4` + `avfoundation/hevc` (quality 70) | 3.130 (1.796, 3.367, 3.130) | 4,475,895 | 39.05 |
| 4K `ffmpeg/mp4` + `ffmpeg/libx264` | 18.302 (18.424, 17.473, 18.302) | 13,851,241 | 41.41 |
| 4K `avfoundation/mp4` + `avfoundation/h264` | 5.398 (6.014, 5.184, 5.398) | 15,095,214 | 41.06 |
| 4K `avfoundation/mp4` + `avfoundation/hevc` | 5.541 (5.402, 5.541, 5.681) | 9,529,649 | 41.31 |
| 4K `ffmpeg/mov` + `ffmpeg/prores` (4444) | 41.067 (42.870, 36.973, 41.067) | 929,562,480 | 53.22 |
| 4K `avfoundation/mov` + `avfoundation/prores` (4444) | 10.907 (10.250, 14.123, 10.907) | 945,663,921 | 54.49 |

Every file has 300 frames, BT.709 tags (`bt709`, `bt709`, `bt709`, `tv`),
and the sample entry its muxer was given (`avc1`, `hvc1`, `ap4h`); the
codec strings are `avc1.64002A` (1080p, both H.264 encoders),
`avc1.640034` (4K), `hvc1.1.6.L123.B0` (1080p HEVC, Main tier) and
`hvc1.1.6.H153.B0` (4K HEVC, High tier); ProRes decodes as
`yuva444p12le`. x264's defaults took 3.3–3.4 times VideoToolbox's wall time
at 4K and about twice it at 1080p; its H.264 at the default
quality is 9–18% larger than x264's CRF 18 at 0.35–0.48 dB lower PSNR,
and its HEVC 14–31% smaller at 0.10–0.31 dB lower. VideoToolbox's ProRes
4444 took about a quarter of `prores_ks`'s time, 1.7% larger, 1.27 dB higher
(it converts the 16-bit RGB to 12-bit codes, where FFmpeg's converts to
`yuva444p10le` first). These are recorded only: the spec's performance
requirement is the FFmpeg path's, which this branch does not change.

The machine was not quiet: load averages were 33–74 during the runs
(other work besides these exports), so the 1080p runs spread widely
(x264 4.7–6.4 s, HEVC 1.8–3.4 s) and the medians are rough. The 4K rows
spread less. The release build of the plugins printed a warning that
`rust-objcopy` could not strip debug info (this toolchain's `libLLVM.dylib`
is missing), so the measured plugin programs were not stripped.

Also seen while measuring: Turborepo's cache in this worktree reported
every `build:runtime` task as a hit without restoring `packages/*/dist`,
so `celesta-exporter --react` failed to find `@celesta.studio/react`
until `TURBO_FORCE=1 pnpm run build:runtime` rebuilt them.

## Not done

- **FFmpeg-first ranking.** This branch is cut from P2c, where bundled
  plugins keep search order and `avfoundation`'s folder sorts before
  `ffmpeg`'s. Until it is rebased onto P4, whose Task 1 ranks FFmpeg first
  among bundled plugins by id (deviation 58), a choice-free `.mp4` export
  with both plugins installed takes `avfoundation/mp4`,
  `avfoundation/h264`, and `avfoundation/aac`, and AVFoundation's decoder
  reads `mp4`, `mov`, and `m4a` before FFmpeg's. The user guide and the
  skill describe the behaviour after the rebase (FFmpeg the default,
  AVFoundation by name or preset). `ffmpeg_stays_the_default_beside_avfoundation`
  (`plugins/avfoundation/tests/export.rs`) pins it and is
  `#[ignore = "needs P4's FFmpeg-first ranking (deviation 58)"]`; run once
  with `--ignored` it fails today, having picked
  `["avfoundation/mp4", "avfoundation/h264", "avfoundation/aac"]`. Remove
  the `#[ignore]` after the rebase. Presets are P3's as well: this
  branch's guide still says `--preset` fails with `unknown_preset`, while
  the new section names presets as one way to choose the components.
- **The macOS package build was not run end to end** (Task 9): `mbx`'s
  disk garbage collector deleted the root `target` mid-build on this Mac
  (under its 45 GiB free-space minimum). `test_package_macos.py` passes,
  and `test-package.mjs`'s new block was only syntax-checked; CI's
  Package job, or a run with more free disk, must check it.
- **P6** (`plugins/mediafoundation`, Windows), in its own session.
- **GPU texture sharing** (`transports: ioSurface`): frames still cross
  shared memory or the pipe and are copied into a CoreVideo pool buffer.
- **HEVC with alpha** (VideoToolbox's `hvc1` with an alpha layer):
  neither a component nor a setting yet.
- **The decoder's HDR tone curve**: deviation 64 takes AVFoundation's
  conversion rather than the FFmpeg plugin's curve.
- **Deferred minors from review**: an AAC rate query that fails leaves
  `aac` described available with no sample rates; doc comments in
  `apple.rs` still say `kAudioFormatProperty_Encoders` lists format ids;
  `params.rs` says VideoToolbox's SPS "has no VUI" (it has one without
  `bitstream_restriction`); an encode error mid-drain drops the packets
  already drained in that answer; padded strides and mid-frame decoder
  starts are untested; the decoder's past-the-end request without a frame
  rate answers `NoVideoStream`; its asset cache per path is unbounded and
  can go stale after a file is replaced; a permission error reads as
  `Unsupported`; `test-package.mjs` relies on the muxer's own plugin
  ranking first for the encoder.
