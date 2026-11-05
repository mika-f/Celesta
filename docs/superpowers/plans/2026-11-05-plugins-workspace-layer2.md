# Lossless test codec in the core (layer 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The core's tests read and write media through a lossless, pure-Rust test codec, so no core crate depends on `celesta-codec-ffmpeg` or `ez-ffmpeg`, and the tests of the contract with FFmpeg live in `celesta-codec-ffmpeg`'s own tests.

**Architecture:** A new root member, `crates/codec-test` (`celesta-codec-test`, `publish = false`), has a library with `TestCodec` (a `CodecProvider` that stores the exact bytes and samples it is given, LZ4-compressed, and reads them and PCM WAV back), `install()`, fixture writers, a `read()` that exposes what was stored, and `TestCodecPlugin`, which writes a temporary codec plugin folder for the crate's program, `celesta-codec-test`, that serves the codec protocol. In-process tests call `install()`; tests that start `celesta-exporter` or `celesta-mcp` hand the child a `TestCodecPlugin` folder. The FFmpeg-specific assertions move, unchanged, into `crates/codec-ffmpeg/tests/export.rs`, which drives the real exporter.

**Tech Stack:** Rust 1.99.1 (edition 2024), `lz4_flex` block API, `serde_json`, `tempfile`, Cargo dev-dependencies, `CodecProcess` and the codec protocol from `celesta-media`.

**Spec:** `docs/handoff/2026-11-05-plugins-workspace.md` (Goals, Anti-goals, Constraints, and "Layer 2: a lossless test codec in the core"). Layers 3–4 get their own plans.

## Global Constraints

- The stack is managed with `gh stack`; this layer is `claude/core-test-codec`, on layer 1's `claude/plugins-workspace`. Never amend a commit. Commit and test steps are separate calls.
- Edit files with the Edit tool. No sed, perl, or python rewrites.
- Every test step runs only the affected scope: the new or changed tests, as `cargo test -p <crate> <filter>` or one integration-test target. No task runs a crate-wide or workspace-wide suite; that is CI's job. Task 7's checks are a workspace build of all targets, clippy for the touched crates, and the focused runs.
- Rust tests stay Rust (`cargo test`); no JavaScript test changes in this layer.
- GPU tests: on Linux CI `CELESTA_REQUIRE_GPU=1` makes a missing adapter fail; locally on macOS they run on Metal. Moved tests keep their `skip_without_gpu` handling.
- `celesta-codec-ffmpeg` stays in `crates/codec-ffmpeg` and stays a root member; the root `Test` CI job keeps setting up FFmpeg in this layer (layer 3 removes it). `InstalledPlugins::codec()`, `CODEC_PROGRAM`, the `no_codec` hint, and packaging do not change.
- The test codec "honours the `CodecProvider` contract the exporter relies on: containers, `check_encoder`, the lead-in, and errors for what it does not support."
- "`CELESTA_PLUGIN_PATH` is set by hand and never from `mise.toml` or another file every shell picks up. Tests do not depend on that variable."
- `celesta-codec-test` is `publish = false`, and its `[workspace.dependencies]` entry is `path` only (no `version`), so `cargo publish` strips it from the publishable crates' dev-dependencies.
- After Task 7, `celesta-codec-ffmpeg` and `ez-ffmpeg` are depended on by no root crate (normal, build, or dev) other than `celesta-codec-ffmpeg` itself.

## Decisions

- **Crate:** `crates/codec-test`, package `celesta-codec-test`, library `celesta_codec_test`, program `celesta-codec-test`. A separate crate rather than a `celesta-media` feature, so the program never ships and no feature unification can pull it into a release build.
- **File format** (one file per output, whatever its extension; tests name fixtures `*.ctc`): 8-byte magic `b"CLSTTEST"`, `u32` LE format version `1`, `u32` LE header length, a JSON header, then chunks, each a tag byte, a `u32` LE payload length, and a payload compressed with `lz4_flex::block::compress_prepend_size`: `b'V'` one video frame (the exact bytes passed to `encode_frame`), `b'A'` the whole audio as `f32` LE interleaved samples (lead-in included), `b'E'` the end (empty payload). The header is `{"container": "mp4"|"mov"|"webm"|null, "video": {"width", "height", "frameRate": Rational, "format": PixelFormat} | null, "audio": {"sampleRate", "channels", "leadInFrames"} | null}` with the existing serde forms of `Container`, `PixelFormat`, and `Rational`. The frame count is the number of `V` chunks. A file without `E` is unfinished and is refused. `mux_audio` refuses a video that already has audio. Probe durations: the video's frames over its rate, the audio's played frames (after the lead-in) over its rate, and the file's the longer of the two.
- **Video decoding** returns RGBA8: `Rgba8` frames as stored, `Rgba16` rounded per channel `((v as u32 * 255 + 32767) / 65535) as u8`. YUV frames are refused (`MediaError::Codec`); tests compare them through `read()`, byte for byte. Frame at `time`: index `floor(time * fps + 1e-9)`, clamped to the last frame (an overrun holds the final frame); a negative or non-finite time is `MediaError::InvalidTimestamp`, as FFmpeg's decoder answers.
- **Audio decoding** returns the samples after the lead-in. It never resamples: a requested rate other than the file's is `MediaError::Codec("the test codec does not resample: {path} is {file} Hz, not {requested} Hz")`. Channels: equal as stored, mono to stereo duplicates each sample, stereo to mono averages; anything else is an error. A file with no audio decodes to no samples.
- **WAV:** RIFF/WAVE, unknown chunks skipped (`voice-1.wav` has an `FLLR` chunk), format tag 1 with 16 bits only, samples `i16 as f32 / 32768.0`; any other format is `MediaError::Codec` naming the tag and bit depth. Probe codec name `pcm_s16le`; test-codec files probe as `celesta-test` (video) and `pcm_f32le` (audio).
- **Helper sharing:** `TestCodecPlugin` lives in the `celesta-codec-test` library, which `celesta-exporter` and `celesta-mcp` take as a dev-dependency; it is written once.
- **Tolerances kept** where they measure the core's own DSP or the half-float canvas, not codec loss: the pitched clip's frequency (±3 %), the fades' and the ducked bed's RMS (±10 %), the loudness target (±0.1 LU of −14 LUFS), and the transparent quadrant's straight color and alpha (one 8-bit code, 257, for the `Rgba16Float` canvas). **Made exact:** frame contents and counts, the audio's length, the silent mix, the lead-in, reported versus written loudness, repeated exports, and the WAV samples.
- **FFmpeg-contract tests moved to `celesta-codec-ffmpeg`:** the spec's list (MP4 with silent audio, ProRes 4444 and VP9 WebM with alpha, every format muxing audio as long as the video, `converting_colors_on_the_gpu_matches_the_encoder_conversion`) plus the two tests whose assertions measure FFmpeg's encoders: `a_looping_mp4_starts_its_sound_from_its_end` (AAC edit list) and `brings_the_mix_to_a_loudness_target` (24-bit PCM and AAC), and the encoder check `the_ffmpeg_codec_has_every_format_encoder`. Each also gets a core counterpart on the test codec (Task 3).
- **No react-bridge test moves to `celesta-codec-ffmpeg`:** the spec's example, `with-video.tsx`'s `./clip.mp4`, is not checked in, and the bridge tests only read its timing; the only media they decode are the checked-in WAVs. So `celesta-codec-ffmpeg` gets no dev-dependency on `celesta-react-bridge`.
- **`audio-bench`** uses the installed codec plugin (`InstalledPlugins::find_reporting(&[]).codec()`, which still finds `celesta-codec-ffmpeg` beside it in `target/release`), as `preview-bench` does, so it keeps timing real decoding. `preview-bench` does not change (it links no codec crate).

## Inventory (file → task)

| File | Use today | Task |
| --- | --- | --- |
| `Cargo.toml` (root) | `celesta-codec-ffmpeg`, `ez-ffmpeg` workspace deps | stay (layer 3); 1 adds `celesta-codec-test` |
| `crates/exporter/Cargo.toml` | dev-deps `celesta-codec-ffmpeg`, `ez-ffmpeg` | 2 adds `celesta-codec-test`; 7 removes both |
| `crates/react-bridge/Cargo.toml` | dev-dep `celesta-codec-ffmpeg` | 2 adds `celesta-codec-test`; 7 removes it |
| `crates/mcp/Cargo.toml` | none (child finds the sibling codec) | 2 adds `celesta-codec-test` |
| `crates/codec-ffmpeg/Cargo.toml` | — | 2 adds dev-deps for the moved tests |
| `crates/exporter/src/tests.rs:559` | `celesta_codec_ffmpeg::install()` | 3 (core), 6 (FFmpeg) |
| `crates/exporter/tests/integration/main.rs:13` | `celesta_codec_ffmpeg::install()` | 3 |
| `crates/exporter/tests/integration/export.rs` | `FfmpegBackend`, `ez_ffmpeg` probing/decoding, `lavfi` fixtures | 3 (core), 6 (FFmpeg) |
| `crates/exporter/tests/integration/stills.rs` (`react_export`) | sibling codec in `target/<profile>` | 5 |
| `crates/mcp/tests/integration/export.rs`, `common/mod.rs` | sibling codec in `target/<profile>` | 5 |
| `crates/react-bridge/src/decode.rs:156,188` | `FfmpegCodec::factory()` | 4 |
| `crates/react-bridge/tests/node_integration.rs:25,35,46` | `celesta_codec_ffmpeg::install()` | 4 |
| `crates/react-bridge/examples/audio-bench.rs:12` | `celesta_codec_ffmpeg::install()` | 4 |
| `crates/exporter/examples/preview-bench.rs` | codec program beside it | unchanged (layer 3) |
| `crates/exporter/src/report.rs`, `crates/editor/src/plugins.rs`, `crates/plugin-host/src/manifest.rs` | the name `celesta-codec-ffmpeg` (hint, `CODEC_PROGRAM`) | unchanged (layer 3) |
| `crates/codec-ffmpeg/src/*`, `tests/*` | the codec itself | unchanged; 6 adds `tests/export.rs` |

## Review Focus

- **A file the test codec cannot read** (a real MP4 or an unsupported WAV that some test or example happens to open). It fails with a `MediaError` that names the file; a missing file is `MediaError::NotFound(path)`, so the bridge still says "could not decode missing.wav: media file missing.wav was not found". Task 1 tests both.
- **A cancelled or aborted export** (the exporter's and MCP's cancel tests). `abort_encoder` drops the writer at once (Windows can then remove the file), and the unfinished file left behind is refused by `probe` and `read` with "unfinished". Task 1 tests it.
- **Audio asked at another rate or channel count.** The codec refuses to resample with a message naming both rates, rather than returning audio that `MediaBackend` would reject without saying why; mono and stereo convert exactly. Task 1 tests it.
- **The test codec program missing, or the developer's `CELESTA_PLUGIN_PATH` naming the real FFmpeg codec.** `cargo test -p celesta-mcp` alone fails with "build it first"; with the program built, the child uses the test codec whatever the developer set. Task 2 tests the message; Task 5 asserts each child-written video reads back with `celesta_codec_test::read`, which only a test-codec file does.
- **A temporary folder on another volume than `target/`, and Windows.** The program is hard-linked into the plugin folder, falling back to a copy (the host refuses a symlink that leads outside the folder), and the manifest names it with `EXE_SUFFIX`. Task 2 tests that `InstalledPlugins::find(&[folder]).codec()` returns that executable inside the folder.

## Parallel groups

1. Task 1, then Task 2 (Task 2 makes every Cargo manifest and `Cargo.lock` edit the parallel tasks need).
2. Tasks 3, 4, 5, and 6 in parallel, each in its own worktree: no shared files (Task 3 owns `exporter/src/tests.rs` and `tests/integration/{main,export}.rs`; Task 5 owns `tests/integration/stills.rs` and `crates/mcp/tests/`; Task 4 owns `crates/react-bridge/`; Task 6 owns `crates/codec-ffmpeg/tests/export.rs`), and none needs another's interface. Merge them in task order.
3. Task 7 last.

---

### Task 1: The test codec, in-process

**Files:**
- Create: `crates/codec-test/Cargo.toml` (`publish = false`, workspace `version`/`edition`/`license`/`repository`/`rust-version`, a `description`; deps `celesta-composition`, `celesta-media`, `serde`, `serde_json` from the workspace, and `lz4_flex` added with `cargo add -p celesta-codec-test lz4_flex`)
- Create: `crates/codec-test/src/lib.rs` (`TestCodec`, `install`, the constants, the re-exports), `src/format.rs` (the file layout, `read`, the writers), `src/wav.rs` (PCM WAV probe and decode)
- Modify: `Cargo.toml` (root: member `"crates/codec-test"`; `celesta-codec-test = { path = "crates/codec-test" }` with a comment that it is path-only because it is never published), `Cargo.lock`
- Test: `crates/codec-test/tests/codec.rs`

**Interfaces:**
- Produces, in `celesta_codec_test`:
  - `pub const PLUGIN_ID: &str = "celesta-test-codec";` and `pub const PROGRAM: &str = "celesta-codec-test";`
  - `pub struct TestCodec` with `new() -> Self`, `factory() -> CodecFactory`, and `impl CodecProvider`; `pub fn install()` (`set_codec(Some(TestCodec::factory()))`).
  - `pub fn read(path: &Path) -> Result<TestMedia, MediaError>`; `pub struct TestMedia { pub container: Option<Container>, pub video: Option<StoredVideo>, pub audio: Option<StoredAudio> }`; `pub struct StoredVideo { pub width: u32, pub height: u32, pub frame_rate: Rational, pub format: PixelFormat, pub frames: Vec<Vec<u8>> }`; `pub struct StoredAudio { pub sample_rate: u32, pub channels: u16, pub lead_in_frames: usize, pub samples: Vec<f32> }` with `lead_in(&self) -> &[f32]` and `played(&self) -> &[f32]`. All derive `Clone, Debug, PartialEq`.
  - `pub fn write_rgba_video(path: &Path, width: u32, height: u32, frame_rate: Rational, frames: &[Vec<u8>]) -> Result<(), MediaError>` (through the encoder path, `container: None`) and `pub fn write_audio(path: &Path, sample_rate: u32, channels: u16, samples: &[f32]) -> Result<(), MediaError>` (an audio-only file).

- [ ] **Step 1: Write the failing tests** in `tests/codec.rs`, each in a `tempfile::tempdir()`:
  - `stores_every_pixel_format_byte_for_byte`: for each `PixelFormat` at 6x4 (`Rgba8` 96 bytes, `Rgba16` 192, `Yuv420p` 36, `Yuv420p10` 72) open an encoder (`Container::Mov`, `Rational::new(25, 1)`), push three distinct frames, finish; `read` gives the container, format, size, rate, and the three frames equal; `probe` gives 6x4, `Some(Rational::new(25, 1))`, codec `"celesta-test"`, duration `Time::new(3, 25)`, no audio.
  - `yuv_frame_sizes_round_chroma_up`: a 5x3 `Yuv420p` frame is 15 + 2 × 6 = 27 bytes; 26 bytes fails with a `MediaError::Codec` that contains `27`.
  - `refuses_what_it_cannot_write`: an unknown `EncoderId` ("no open encoder"), width 0, a frame rate of 0/1, and an MP4 with `bit_depth: 12` or `crf: 52` each fail; `check_encoder` is `Ok` for `Mp4`, `Mov`, and `Webm`.
  - `an_aborted_encode_is_unfinished`: after `abort_encoder`, the file exists, and `probe` and `read` fail with a message containing `unfinished`.
  - `decodes_rgba_frames_at_their_times`: an `Rgba8` video at 2 fps with frames A, B: times 0.0 and 0.49 give A, 0.5 gives B, 60.0 gives B; −0.1 and `f64::NAN` give `InvalidTimestamp`; an `Rgba16` frame of `[65535, 32896, 0, 257]` decodes to `[255, 128, 0, 1]`; a `Yuv420p` file fails; an audio-only file gives `NoVideoStream`.
  - `muxes_audio_with_its_lead_in`: mux a finished `Container::Mp4` video with PCM of 10 stereo frames whose first 4 are the lead-in; `read` keeps the video's frames, `container == Some(Mp4)`, `lead_in_frames == 4`, all 20 samples; `decode_audio(path, rate, 2)` returns the last 12 samples exactly; `probe`'s audio stream has the rate, 2 channels, codec `"pcm_f32le"`, duration of 6 frames. Muxing a `Mov` video into `Webm` fails.
  - `converts_channels_but_never_resamples`: `write_audio` mono `[0.5, -0.25]` decodes as stereo `[0.5, 0.5, -0.25, -0.25]`; stereo `[1.0, 0.0]` as mono `[0.5]`; asking 44100 of a 48000 file fails with a message containing `48000` and `44100`; a video without audio decodes to no samples.
  - `reads_the_checked_in_voices`: `examples/assets/voices/001.wav` probes as one `pcm_s16le` stream, 44 100 Hz, mono, duration `Time::new(84_022, 44_100)`, and decodes to 84 022 samples starting `-11.0 / 32768.0, -15.0 / 32768.0`; `examples/assets/voice-folder/shizuku/voice-1.wav` (with an `FLLR` chunk) probes at 16 000 Hz, mono.
  - `names_files_it_cannot_read`: a missing path gives `MediaError::NotFound(path)` from `probe`, `decode_video`, and `decode_audio`; a file of random bytes and a WAV with format tag 3 each fail with a message that contains the path.
- [ ] **Step 2:** Run `cargo test -p celesta-codec-test --test codec`. It fails to compile: the crate has no code.
- [ ] **Step 3:** Implement `format.rs`, `wav.rs`, and `lib.rs` per the Decisions. `TestCodec` keeps `HashMap<EncoderId, OpenEncoder>` (a `BufWriter<File>`, the settings, the next id counter); `encode_frame` checks the length for the format, compresses, and writes a `V` chunk; `finish_encoder` writes `E` and syncs; `mux_audio` writes a new file copying the video's `V` payloads as they are. Sniff the magic first, then `RIFF`/`WAVE`.
- [ ] **Step 4:** Run `cargo test -p celesta-codec-test --test codec`. It passes.
- [ ] **Step 5:** Commit: `test(codec-test): a lossless codec that stores exactly what the exporter writes`.

### Task 2: The codec program, the plugin folder helper, and the dev-dependencies

**Files:**
- Create: `crates/codec-test/src/main.rs` (as `crates/codec-ffmpeg/src/main.rs`, serving `TestCodec::new()` with `PLUGIN_ID` and `CARGO_PKG_VERSION`), `crates/codec-test/src/plugin.rs`
- Modify: `crates/codec-test/Cargo.toml` (deps `celesta-plugin-host` and `tempfile = "3.23.0"`), `crates/codec-test/src/lib.rs`
- Modify (dev-dependencies for Tasks 3–6, in this task so the parallel tasks touch no manifest): `crates/exporter/Cargo.toml`, `crates/react-bridge/Cargo.toml`, `crates/mcp/Cargo.toml` (each `celesta-codec-test.workspace = true`, beside the existing FFmpeg ones in the first two), `crates/codec-ffmpeg/Cargo.toml` (`celesta-exporter`, `celesta-gpu-renderer`, `celesta-project`, `strum` from the workspace, `png = "0.18.1"`), `Cargo.lock`
- Test: `crates/codec-test/tests/process.rs`, `crates/codec-test/tests/plugin_folder.rs`

**Interfaces:**
- Consumes: Task 1's `TestCodec`, `PLUGIN_ID`, `PROGRAM`, `read`, `write_rgba_video`.
- Produces, in `celesta_codec_test`:
  - `pub use celesta_plugin_host::PLUGIN_PATH_ENV;`
  - `pub fn profile_dir(test_executable: &Path) -> PathBuf`: the executable's folder, or its parent when that folder is `deps` (where Cargo puts test binaries).
  - `pub struct TestCodecPlugin` (owns a `tempfile::TempDir`) with `new() -> Self`, which panics with `in_profile`'s error for `profile_dir(&current_exe())`; `in_profile(profile: &Path) -> Result<Self, String>`; and `folder(&self) -> &Path`. The folder holds the program, hard-linked or else copied, and `celesta-plugin.json`: `{"id": PLUGIN_ID, "version": CARGO_PKG_VERSION, "protocol": 1, "roles": ["codec"], "executable": {OS: PROGRAM + EXE_SUFFIX}}`. The error when the program is missing: `"{PROGRAM} is not built in {profile}: build it first with cargo build -p celesta-codec-test (with --release for release tests)"`.

- [ ] **Step 1: Write the failing tests.**
  - `tests/process.rs` `the_program_answers_as_the_codec_in_process_does`: start `CodecProcess::factory(env!("CARGO_BIN_EXE_celesta-codec-test"), its folder)`; `probe` and `decode_frame` of a `write_rgba_video` file and `decode_audio` of `001.wav` equal `TestCodec`'s in-process answers; an encode of two `Rgba8` frames plus `mux_audio` through the program reads back with `read` byte for byte; `probe` of a missing file gives `MediaError::NotFound` through the process.
  - `tests/plugin_folder.rs` `the_folder_is_a_codec_plugin_for_this_platform`: `TestCodecPlugin::new()`, then `InstalledPlugins::find(&[folder.to_path_buf()])` gives no errors and `.codec()` gives `id == PLUGIN_ID` and an `executable` that starts with the folder and ends with `PROGRAM` + `EXE_SUFFIX`.
  - `tests/plugin_folder.rs` `a_missing_program_says_to_build_it`: `TestCodecPlugin::in_profile(empty tempdir)` is an `Err` containing `cargo build -p celesta-codec-test`; `profile_dir(Path::new("/t/debug/deps/x"))` is `/t/debug`, and `profile_dir(Path::new("/t/debug/x"))` is `/t/debug`.
- [ ] **Step 2:** Run `cargo test -p celesta-codec-test --test process --test plugin_folder`. It fails to compile.
- [ ] **Step 3:** Implement `main.rs` and `plugin.rs`, and make the five manifest edits above.
- [ ] **Step 4:** Run `cargo test -p celesta-codec-test --test process --test plugin_folder`. It passes. Run `cargo metadata --format-version 1 --locked --no-deps > /dev/null`. It exits 0.
- [ ] **Step 5:** Commit: `test(codec-test): serve the test codec from a program and a temporary plugin folder`.

### Task 3: Exporter tests on the test codec

**Files:**
- Modify: `crates/exporter/src/tests.rs` (`the_ffmpeg_codec_has_every_format_encoder` → `the_test_codec_has_every_format_encoder`, on `celesta_codec_test::install()`)
- Modify: `crates/exporter/tests/integration/main.rs` (`exporter()` calls `celesta_codec_test::install()`; doc comment: the lossless test codec)
- Modify: `crates/exporter/tests/integration/export.rs` (drop every `ez_ffmpeg` and `FfmpegBackend` use)

**Interfaces:**
- Consumes: Task 1's `install`, `read`, `TestMedia`, `write_rgba_video`, `write_audio`, `TestCodec` (for `decode_audio`).
- Produces: nothing for later tasks.

- [ ] **Step 1: Rewrite the tests.** Replace `generate_source` with solid-color `Rgba8` fixtures written by `write_rgba_video` (`source.ctc`), and the `lavfi` audio with Rust-generated samples written by `write_audio` (`sine(frequency, rate, seconds)` at amplitude 0.125, as FFmpeg's `sine` source; the `aevalsrc` expressions as closures). Changed assertions:
  - `exports_frame_exact_mp4_with_silent_audio`: two frames of distinct solid colors; export with `ColorConversion::Encoder`. `read(output)`: container `Mp4`, 64x64, `Rational::new(2, 1)`, format `Rgba8`, two frames, frame k every pixel equal to source color k with alpha 255; audio 8 000 Hz, 2 channels, 8 000 frames, every sample `0.0`. The cancellation half stays.
  - `exports_only_the_selected_range_shifted_to_zero`: four solid frames; the windowed file has 2 frames, equal byte for byte to the full file's frames 2 and 3.
  - Replace `converting_colors_on_the_gpu_matches_the_encoder_conversion` with `color_conversion_decides_the_frames_the_encoder_gets`: for each `BitDepth`, `ColorConversion::Gpu` stores `Yuv420p` (6 144 bytes a frame) or `Yuv420p10` (12 288), and `Encoder` stores `Rgba8` or `Rgba16`; `UnsupportedReadbackFormat` skips as today.
  - Replace the ProRes and VP9 tests with `alpha_formats_store_straight_16_bit_frames`: for `Mov` and `Webm`, `read` has that container and `Rgba16`; at (8, 8) red is within 257 of 65 535, green and blue within 257 of 0, alpha within 257 of 32 896; at (48, 48) all four are 0.
  - Replace `every_format_muxes_audio_as_long_as_the_video` with `every_format_stores_the_mix_as_long_as_the_video`: each format's audio has exactly 8 000 frames and `lead_in_frames == 0`.
  - `exports_a_pitched_audio_clip`, `exports_an_audio_clip_with_fades`, `exports_a_bed_ducked_under_a_voice`: decode with `TestCodec::new().decode_audio(output, rate, 1)`; their tolerances stay.
  - `a_looping_mp4_starts_its_sound_from_its_end`: the looped file's `lead_in_frames == 16 * 1024`, its `lead_in()` equals the once file's last 16 384 stereo frames, and its `played()` equals the once file's samples exactly.
  - `brings_the_mix_to_a_loudness_target` (MOV only): integrated within 0.1 of −14; `measure_loudness` of the decoded file equals the reported `integrated_lufs` exactly; true peak ≤ −1.0 + 1e-3; the gain check on the source stays; the second export's samples equal the first's. The MP4/AAC part goes to Task 6.
  - `exports_a_silent_react_entry_to_webm`: `read` has container `Webm` and no audio.
  - The other tests keep their assertions and only change the codec.
- [ ] **Step 2:** Run `cargo test -p celesta-exporter --lib the_test_codec_has_every_format_encoder` and `cargo test -p celesta-exporter --test integration export::`. They pass (GPU tests on Metal locally). If an exact assertion fails, report the difference rather than adding a tolerance.
- [ ] **Step 3:** Run `git grep -n -E "ez_ffmpeg|FfmpegBackend|celesta_codec_ffmpeg|lavfi" -- crates/exporter`. Nothing matches.
- [ ] **Step 4:** Commit: `test(exporter): check exports on the lossless test codec`.

### Task 4: React bridge tests and the audio bench

**Files:**
- Modify: `crates/react-bridge/src/decode.rs` (tests use `TestCodec::factory()`)
- Modify: `crates/react-bridge/tests/node_integration.rs` (the `Bridge` helpers call `celesta_codec_test::install()`; doc comment and the "FFmpeg's samples" comment name the test codec)
- Modify: `crates/react-bridge/examples/audio-bench.rs`

**Interfaces:**
- Consumes: Task 1's `TestCodec`, `install`.

- [ ] **Step 1:** In `writes_mono_samples_and_removes_them_once_read`, assert `audio.frames == 84_022` and that the file's first two `f32` values are `-11.0 / 32768.0` and `-15.0 / 32768.0`, in place of `frames > 44_100`. `reports_a_file_it_cannot_decode` is unchanged.
- [ ] **Step 2:** `audio-bench` sets the codec from `InstalledPlugins::find_reporting(&[]).codec()` through `CodecProcess::factory`, and without one exits with "no codec plugin found: build one first (cargo build --release -p celesta-codec-ffmpeg) or set CELESTA_PLUGIN_PATH"; its header comment says so.
- [ ] **Step 3:** Run `cargo test -p celesta-react-bridge --lib decode::tests` and, after `pnpm install && pnpm run build:runtime`, `cargo test -p celesta-react-bridge --test node_integration`. Both pass, with no `skipping` lines for the runtime. Run `cargo build -p celesta-react-bridge --example audio-bench`. It builds.
- [ ] **Step 4:** Commit: `test(react-bridge): read media with the lossless test codec`.

### Task 5: Child-process exports on the test codec program

**Files:**
- Modify: `crates/exporter/tests/integration/stills.rs` (`react_export`)
- Modify: `crates/mcp/tests/integration/common/mod.rs` (add `client_with_test_codec`), `crates/mcp/tests/integration/export.rs`

**Interfaces:**
- Consumes: Task 2's `TestCodecPlugin`, `PLUGIN_PATH_ENV`; Task 1's `read`.
- Produces, in `crates/mcp/tests/integration/common/mod.rs`: `pub async fn client_with_test_codec() -> (Client, mpsc::UnboundedReceiver<ProgressNotificationParam>, TestCodecPlugin)`, which starts `celesta-mcp` with `PLUGIN_PATH_ENV` set to the plugin's folder (replacing the developer's value).

- [ ] **Step 1:** `react_export` makes a `TestCodecPlugin`, runs the exporter with `.env_remove(PLUGIN_PATH_ENV)` and `--plugin-dir <folder>`, and keeps the plugin alive until the child exits. `separate_stills_and_videos_name_the_frame_that_failed` keeps its assertions.
- [ ] **Step 2:** `export_video_writes_an_mp4_with_progress` and `export_video_cancel_stops_and_frees_the_gpu` use `client_with_test_codec()`; the first also asserts `celesta_codec_test::read(&output)` is `Ok` with 60 frames and container `Mp4`. The other MCP tests keep `client()`.
- [ ] **Step 3:** Run `cargo build -p celesta-codec-test`, then `cargo test -p celesta-exporter --test integration stills::separate_stills_and_videos_name_the_frame_that_failed` and `cargo test -p celesta-mcp --test integration export::`. They pass. Then run the MCP target once with `CELESTA_PLUGIN_PATH` set to a folder holding a dummy codec manifest whose program does not exist; it still passes.
- [ ] **Step 4:** Commit: `test(exporter, mcp): give exporting children the test codec's plugin folder`.

### Task 6: The FFmpeg contract tests in `celesta-codec-ffmpeg`

**Files:**
- Create: `crates/codec-ffmpeg/tests/export.rs`

**Interfaces:**
- Consumes: `celesta_exporter`'s public API, `celesta_codec_ffmpeg::{install, FfmpegBackend, FfmpegCodec}`, and the dev-dependencies Task 2 added.

- [ ] **Step 1:** Write `tests/export.rs` with a module comment saying these check the FFmpeg codec through the real exporter. Copy from the Task 2 commit's `crates/exporter/tests/integration/export.rs` (`git show <task-2 commit>:crates/exporter/tests/integration/export.rs`), unchanged in their assertions: `exports_frame_exact_mp4_with_silent_audio`, `converting_colors_on_the_gpu_matches_the_encoder_conversion`, `exports_transparent_prores_4444`, `exports_transparent_vp9_webm`, `every_format_muxes_audio_as_long_as_the_video`, `a_looping_mp4_starts_its_sound_from_its_end`, `brings_the_mix_to_a_loudness_target`, with the helpers they use (`skip_without_gpu`, `generate_source`, `workspace_root`, `transparent_project`, `export_as`, `with_audio`, `export_reporting`, the stream and decode helpers) and a local `exporter(options)` that calls `celesta_codec_ffmpeg::install()`. Add `the_ffmpeg_codec_has_every_format_encoder`: `FfmpegCodec::new().check_encoder(c)` is `Ok` for `Mp4`, `Mov`, and `Webm`.
- [ ] **Step 2:** Run `cargo test -p celesta-codec-ffmpeg --test export`. All eight pass on Metal.
- [ ] **Step 3:** Commit: `test(codec-ffmpeg): check the exporter's FFmpeg contract in the codec's own tests`.

### Task 7: Drop the FFmpeg dev-dependencies, docs, and the checks

**Files:**
- Modify: `crates/exporter/Cargo.toml` (remove `celesta-codec-ffmpeg` and `ez-ffmpeg` and their comment; comment the new dev-dependency: tests read and write media with the lossless test codec), `crates/react-bridge/Cargo.toml` (likewise), `Cargo.lock`
- Modify: `HANDOFF.md`: development setup (FFmpeg is linked by the codec plugin and its own tests only), the workspace map (a `celesta-codec-test` row: "The lossless test codec the core's tests read and write media with, in-process (`install`) or as the program `celesta-codec-test` in a temporary plugin folder (`TestCodecPlugin`); not published."; the `celesta-codec-ffmpeg` row loses "for tests to use in-process" and says its tests drive the exporter), the architecture boundary's last sentence (core tests use `celesta-codec-test`; only `celesta-codec-ffmpeg`'s own tests link FFmpeg), and the examples-and-validation paragraph (tests that spawn `celesta-exporter` or `celesta-mcp` need `celesta-codec-test` built, which `cargo test --workspace` does; otherwise `cargo build -p celesta-codec-test`)
- Modify: `README.md` lines 95–96 (FFmpeg: "the codec plugin (`crates/codec-ffmpeg`) and its tests link them; the core's tests use a lossless test codec instead")
- Modify: `docs/handoff/2026-11-05-plugins-workspace.md` (add "Layer 2: what was done" after the Layer 2 section)

The user guide and `skills/celesta/` do not change: no user-facing behavior or command changes in this layer.

- [ ] **Step 1:** Edit the two manifests and run `cargo metadata --format-version 1 --no-deps > /dev/null` to update `Cargo.lock`. Then `cargo tree --workspace -e normal,build,dev -i celesta-codec-ffmpeg` and `cargo tree --workspace -e normal,build,dev -i ez-ffmpeg` show no dependent other than `celesta-codec-ffmpeg` itself, and `git grep -n -E "celesta_codec_ffmpeg|ez_ffmpeg|FfmpegBackend|FfmpegCodec|lavfi" -- crates ':!crates/codec-ffmpeg'` matches nothing.
- [ ] **Step 2:** Edit the docs above.
- [ ] **Step 3:** Commit: `build: the core's tests no longer link FFmpeg`.
- [ ] **Step 4:** Run the checks, each as its own call:
  - `cargo build --workspace --all-targets --locked`
  - `cargo clippy -p celesta-codec-test -p celesta-exporter -p celesta-react-bridge -p celesta-mcp -p celesta-codec-ffmpeg --all-targets --locked -- -D warnings`
  - `rustfmt --edition 2024 --check` on the `.rs` files `git diff --name-only claude/plugins-workspace -- '*.rs'` lists (not `cargo fmt --all`, which fails on existing formatting in `crates/mcp` and `crates/react-bridge`)
  - the focused runs of Tasks 1–6: `cargo test -p celesta-codec-test`, `cargo test -p celesta-exporter --lib the_test_codec_has_every_format_encoder`, `cargo test -p celesta-exporter --test integration export::`, `cargo test -p celesta-exporter --test integration stills::separate_stills_and_videos_name_the_frame_that_failed`, `cargo test -p celesta-react-bridge --lib decode::tests`, `cargo test -p celesta-react-bridge --test node_integration`, `cargo test -p celesta-mcp --test integration export::`, `cargo test -p celesta-codec-ffmpeg --test export`
  - `git diff --check claude/plugins-workspace`
- [ ] **Step 5:** Record what was done and each check's result (pass counts, skips, the machine) in "Layer 2: what was done", in the style of layer 1's section, then commit: `docs: record the plugins workspace's second layer`.
