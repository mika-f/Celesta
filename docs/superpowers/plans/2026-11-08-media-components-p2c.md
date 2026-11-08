# Media components P2c: BT.709 — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every video export is BT.709 with limited range and says so: the
GPU converts with BT.709 instead of BT.601, `nv12` and `p010` join the
formats the GPU reads back and Celesta offers, FFmpeg's encoders convert
RGB input with the `Color` they are given and tag their streams, and every
container (MP4, MOV, WebM) is tagged.

**Architecture:**
- **GPU.** `yuv420p.wgsl` changes its matrix. Two new chroma entry points
  write the 2x2 chroma into one two-channel target (`Rg8Unorm` for `nv12`,
  `Rg16Uint` for `p010`), and a `luma_p010` entry shifts 10-bit codes into
  the high bits. `YuvConverter` is built per `ReadbackFormat`, with one or
  two chroma planes.
- **Exporter.** The offered list grows to `[yuv420p, yuv420p10, nv12, p010,
  rgba16, rgba8]` on a hardware GPU, and `readback_of` maps the two new
  formats. The export order P2a built (renderer → capabilities → open →
  readback format and background) is kept.
- **FFmpeg plugin.** A `tags.rs` turns `celesta_media::Color` into
  libavutil's enums once and applies them to encoder contexts (libx264's
  VUI, ProRes's frame header, libvpx-vp9's color space), to frames, to
  libswscale's coefficients for RGB input, and to the muxers' `codecpar`,
  from which libavformat writes `colr` (MP4 `nclx`, MOV `nclc`) and WebM's
  `Colour`. libx264 takes `nv12` directly and `p010` through a libswscale
  repack.
- **Checking.** Output is no longer byte-identical to `main`: decoded code
  values of known colors are asserted in the GPU, FFmpeg, and exporter
  tests, tags are read back through libavformat and found in the container
  bytes, and the PR carries a reference render beside before/after frames
  decoded as a BT.709 player shows them.

**Tech stack:** Rust 2024 (MSRV 1.89), wgpu/WGSL in `celesta-gpu-renderer`,
`ffmpeg-sys-next` 8.1 (libavcodec, libavformat, libswscale, libavutil) in
`plugins/ffmpeg`, `celesta-codec-test` for the core's tests.

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`
(Pixel formats and color; Protocol 2 → Encoding's `color` and Muxing's
`Track.color`; Testing → GPU, `celesta-codec-ffmpeg`, Rendering changes;
Phases → P2c). Read it with this plan. P2a's plan
(`2026-11-08-media-components-p2a.md`: deviations 8 and 18, Task 9's
`ExportSession`, Tasks 12–13's FFmpeg encoders and muxers) and P2b's
(`2026-11-08-media-components-p2b.md`, the ring) describe what this builds
on; both are planned, so where this plan names their items it names them
as their plans do, and the implementer uses the names they shipped with.

## Global constraints

- No Celesta crate except `celesta-codec-ffmpeg` links FFmpeg. The core's
  tests use `celesta-codec-test` (HANDOFF.md).
- `Color` is `{"primaries":"bt709","transfer":"bt709","matrix":"bt709","range":"limited"}`
  in every `openVideoEncoder` and video `Track` (`Color::bt709_limited()`,
  P2a). It describes the conversion and the tags, not the input frames: RGB
  inputs stay full-range sRGB-encoded values, and YUV inputs are already in
  that matrix and range.
- The GPU conversion is BT.709 limited range for all four YUV formats:
  `Y' = 16 + 219·(0.2126 R + 0.7152 G + 0.0722 B)`,
  `Cb = 128 + 224·(B − Y)/1.8556`, `Cr = 128 + 224·(R − Y)/1.5748` in 8-bit
  codes, chroma the mean of each 2x2 block, 10-bit codes the 8-bit value
  times four, `p010` the 10-bit code shifted left by 6, each plane rounded
  with `dither.wgsl` as today.
- Offered formats, best first: `yuv420p`, `yuv420p10`, `nv12`, `p010` when
  the renderer `supports_yuv420p_readback()` and is not `is_software()`
  (deviation 18 kept), then `rgba16`, then `rgba8`. YUV formats need even
  dimensions (`GpuRenderError::OddYuv420pSize`, unchanged).
- Frame sizes: `nv12` is `width × height × 3 / 2` bytes, `p010`
  `width × height × 3`, the same as `yuv420p` and `yuv420p10`, so P2b's
  ring slots need no new size.
- Every FFmpeg video encoder applies the given `Color` and every FFmpeg
  muxer tags from `Track.color`; a color name libavutil does not know is
  `MediaError::Codec`, never a silent default.
- `celesta_media::Color` on the wire is unchanged (P2a); only its consumers
  change.
- Stills (`--frame`, contact sheets) are RGBA and unchanged.
- Rust tests stay in Rust; JS/TS tests use Vitest (AGENTS.md). Rendering
  changes carry before/after screenshots of the same scene in the PR
  (AGENTS.md).
- Never amend commits; each task ends in a new commit. Commits are signed
  through 1Password; if signing fails with "failed to fill whole buffer",
  ask the user to unlock 1Password. Each commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Building or testing `plugins/` needs FFmpeg 8 and the shims on `PATH`:
  `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`,
  `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$PATH"`.
- **Builds between tasks.** Task 2 adds `ReadbackFormat::Nv12` and `P010`;
  the exporter's `readback_of` already names both (P2a, as an error), so
  every crate compiles after each task.

## Deviations from the spec (recorded in Task 7's spec edit)

Numbered after P2a's 8–18 and P2b's 19–29.

30. **The test codec stores no color.** The spec has every encoder and muxer
    tag its stream; `celesta-codec-test`'s file has no reader but Celesta's
    tests, so its components accept `Color` and `Track.color` and store
    nothing. The exporter's `encode.rs` tests pin the `Color` the muxer
    receives instead (Task 3).
31. **The GPU reference is Celesta's matrix, not libswscale's 10-bit
    output.** The spec checks that "`nv12` and `p010` readbacks match a CPU
    reference conversion" and that "BT.709 values hold for known colors".
    The reference is a CPU function of the formula above, within the
    dither's one code (exact on whole codes). Its 8-bit codes equal
    libswscale's for every color checked; at 10 bits libswscale's fixed
    point differs by up to 2 codes (red's Cb 410 against 409, green's Y 692
    against 691), which the GPU tests do not chase.

## Decisions the spec left to this plan

- The 8-bit coefficients (Task 1): `Y' = 16 + (46.559, 156.629, 15.812)·RGB`,
  `Cb = 128 + (−25.664, −86.336, 112.0)·RGB`, `Cr = 128 + (112.0, −101.730, −10.270)·RGB`,
  derived from the Global constraints' formula (`219 × 0.2126 = 46.559`,
  `224 / 1.8556 × 0.2126 = 25.664`, …).
- One capability gates every YUV format: `supports_yuv420p_readback()`
  (`max_color_attachments >= 2`) stays the gate for `nv12` and `p010` too,
  though they need one target; no GPU with one attachment is expected.
- The offered order puts the planar formats before the interleaved ones:
  libx264's native input is planar, and VideoToolbox (P5) picks `nv12` by
  itself whatever the order.
- libx264 takes `nv12` as libavcodec's `libx264` lists it; it lists no
  10-bit semi-planar 4:2:0 format (`nv20le` is 4:2:2), so the component
  repacks `p010` to `yuv420p10le` with libswscale (`SWS_POINT`, no color
  change). It prefers `yuv420p` / `yuv420p10` when offered, so from Celesta
  it never needs either.
- `colr` and `Colour` come from libavformat with no muxer flag: FFmpeg 8.1
  writes `colr` whenever primaries, transfer, and matrix are all set on
  `codecpar` (checked on this machine: MP4 `nclx`, MOV `nclc`), and
  matroskaenc writes `Colour` with MatrixCoefficients, Range,
  TransferCharacteristics, and Primaries.
- Color names are mapped with libavutil (`av_color_primaries_from_name`,
  `av_color_transfer_from_name`, `av_color_space_from_name`); `range` maps
  `limited` / `tv` / `mpeg` to `AVCOL_RANGE_MPEG` and `full` / `pc` / `jpeg`
  to `AVCOL_RANGE_JPEG`. libswscale's table is `SWS_CS_ITU709` for `bt709`,
  `SWS_CS_ITU601` for `bt470bg` / `smpte170m`, `SWS_CS_BT2020` for
  `bt2020nc`; another matrix is `MediaError::Codec`.
- The screenshot scene is `examples/spectra/film.tsx` at film frame 90
  (IGNITION's gradient lights and title), exported as `--from 1.5 --to 2.0`
  and decoded as a BT.709 player decodes it (Task 6). Three images: the
  RGBA reference (`--frame 90`), before (P2b's MP4), after (P2c's MP4).
- Input decoding is out of scope: `plugins/ffmpeg/src/color.rs` and
  `celesta_media::VideoColor` tag and tone-map decoded video and do not
  change. The macOS native preview's NV12 (`native_preview.wgsl`) tells
  CoreVideo its own matrix and does not change either.
- No example scene demonstrates the color conversion, so none is updated.

## Review focus

The tests that pin each of these are in the task named.

1. **An odd-sized composition with an encoder that takes `nv12` or `p010`**
   fails with `OddYuv420pSize`, as `yuv420p` does, rather than reading back
   a torn plane (Task 2, `nv12_and_p010_need_even_dimensions`).
2. **A `Color` with a name FFmpeg does not know** (a later Celesta sending
   `bt2020`, or a typo in a test) is refused at `open` naming the field,
   not encoded with an unspecified tag (Task 4,
   `an_unknown_color_name_is_refused`).
3. **An encoder that takes RGB** (ProRes, VP9, or libx264 on a software
   renderer) converts with BT.709 limited range, so its colors equal the
   GPU path's (Task 4, `rgba_input_is_converted_with_bt709`).
4. **10-bit H.264 from the GPU** (`bitDepth=10`, `yuv420p10`) holds the
   BT.709 codes times four and is tagged like 8-bit (Task 5,
   `mp4_colors_hold_bt709_code_values`).
5. **A player that ignores WebM's `Colour` element** still decodes BT.709:
   the VP9 frame header carries `color_space = 2` and limited range
   (Task 5, `vp9_frames_carry_bt709_in_their_header`).

---

### Task 1: BT.709 in `yuv420p.wgsl` (`celesta-gpu-renderer`)

**Files:**
- Modify: `crates/gpu-renderer/src/yuv420p.wgsl` (header comment, `luma_value`, `chroma_values`)
- Modify: `crates/gpu-renderer/src/types.rs:137-147` (`Yuv420p` and `Yuv420p10` doc comments: BT.709)
- Test: `crates/gpu-renderer/src/tests.rs` (`dithers_gentle_gradients_converted_to_yuv420p`, `converts_frames_to_within_a_dithered_code_of_libswscale`, `pipelines_yuv420p_frames_with_padded_plane_rows`, `converts_frames_to_ten_bit_yuv420p`)

**Interfaces:**
- Produces: the same entry points (`luma`, `chroma`, `luma10`, `chroma10`)
  with BT.709 limited-range values. The conversion math, the only code this
  plan writes out:

  ```wgsl
  // BT.709 with limited ("TV") range, in 8-bit code values.
  fn luma_value(pixel: vec2<u32>) -> f32 {
      return 16.0 + dot(rgb(pixel), vec3<f32>(46.559, 156.629, 15.812));
  }
  // … `color` is the 2x2 block's mean, as today:
      return vec2<f32>(
          128.0 + dot(color, vec3<f32>(-25.664, -86.336, 112.0)),
          128.0 + dot(color, vec3<f32>(112.0, -101.730, -10.270)),
      );
  ```

- The known colors every test in this plan uses, as 8-bit codes with the
  unrounded value (10-bit codes are the unrounded value times four, rounded;
  "exact" marks a whole code the dither leaves alone):

  | Color | Y | Cb | Cr | Unrounded | 10-bit Y, Cb, Cr |
  | --- | --- | --- | --- | --- | --- |
  | `#ff0000` | 63 | 102 | 240 (exact) | 62.56, 102.34, 240.00 | 250, 409, 960 (exact) |
  | `#00ff00` | 173 | 42 | 26 | 172.63, 41.66, 26.27 | 691, 167, 105 |
  | `#0000ff` | 32 | 240 (exact) | 118 | 31.81, 240.00, 117.73 | 127, 960 (exact), 471 |
  | `#ffffff` | 235 | 128 | 128 | all exact | 940, 512, 512 |
  | `#000000` | 16 | 128 | 128 | all exact | 64, 512, 512 |
  | `#808080` | 126 | 128 (exact) | 128 (exact) | 125.93 | 504, 512, 512 |
  | `#102030` | 42 | 137 | 120 | 41.55, 136.64, 120.33 | 166, 547, 481 |

  libswscale (`scale=out_color_matrix=bt709:out_range=tv` from RGBA) gives
  the same 8-bit codes for all seven.

- [ ] **Step 1: Update the tests** to the table (they fail against the BT.601 shader):
  - `converts_frames_to_within_a_dithered_code_of_libswscale`: luma `[63, 63, 32, 32, 235, 235, 16, 16]` with exact `[false, false, false, false, true, true, true, true]`; U `[102, 240, 128, 128]` exact `[false, true, true, true]`; V `[240, 118, 128, 128]` exact `[true, false, true, true]`.
  - `pipelines_yuv420p_frames_with_padded_plane_rows`: red `[63, 102, 240]`, blue `[32, 240, 118]`, white and black unchanged, gray `[126, 128, 128]`.
  - `converts_frames_to_ten_bit_yuv420p`: red `[250, 409, 960]` with exact `[false, false, true]`; white and black unchanged.
  - `dithers_gentle_gradients_converted_to_yuv420p`: `#102030`'s luma is within one code of `42` (41.55 before rounding); the `longest_run(row) <= 28` bound stays (luma still rises 219/255 of a code per code of gray).
- [ ] **Step 2:** `cargo test -p celesta-gpu-renderer yuv420p` fails on the four tests (skipped, not failed, on a machine without a GPU: then run the rest of this task on the user's Mac).
- [ ] **Step 3: Implement** the shader change and rewrite the shader's header comment (BT.709 limited range, "the matrix players assume for HD video and the one every Celesta export is tagged with") and the two doc comments in `types.rs`.
- [ ] **Step 4:** `cargo test -p celesta-gpu-renderer` passes.
- [ ] **Step 5: Commit:** `feat(gpu-renderer)!: convert yuv420p readbacks with BT.709`.

### Task 2: `nv12` and `p010` readbacks (`celesta-gpu-renderer`)

**Files:**
- Modify: `crates/gpu-renderer/src/yuv420p.wgsl` (entry points `chroma_nv12`, `luma_p010`, `chroma_p010`)
- Modify: `crates/gpu-renderer/src/yuv.rs` (`YuvConverter::new(device, format)`, one or two chroma targets), `crates/gpu-renderer/src/readback.rs` (`YuvReadback.planes: Vec<YuvPlane>`), `crates/gpu-renderer/src/renderer.rs:533-552` (`set_readback_format` builds the converter for the format), `crates/gpu-renderer/src/types.rs` (`ReadbackFormat::Nv12`, `P010`, `is_yuv`, docs)
- Test: `crates/gpu-renderer/src/tests.rs`, `crates/gpu-renderer/src/tests/readback.rs`

**Interfaces:**
- Produces, in `celesta_gpu_renderer`:
  ```rust
  pub enum ReadbackFormat { Rgba8, Rgba16, Rgba16Straight, Yuv420p, Yuv420p10, Nv12, P010 }
  impl ReadbackFormat { pub const fn is_yuv(self) -> bool }   // the four YUV formats
  ```
  `Nv12`: the Y plane (`width × height` bytes), then one interleaved UV
  plane (`width / 2 × height / 2` pairs), 1.5 bytes a pixel, FFmpeg's `nv12`.
  `P010`: the same planes in 16-bit LE samples with the 10-bit code in the
  high bits (`code << 6`), 3 bytes a pixel, FFmpeg's `p010le`.
- `yuv.rs`: `pub(crate) fn new(device: &wgpu::Device, format: ReadbackFormat) -> Self`
  picks `(luma format, luma entry, chroma format, chroma entry, chroma targets)`:
  `Yuv420p` → `(R8Unorm, "luma", R8Unorm, "chroma", 2)`;
  `Yuv420p10` → `(R16Uint, "luma10", R16Uint, "chroma10", 2)`;
  `Nv12` → `(R8Unorm, "luma", Rg8Unorm, "chroma_nv12", 1)`;
  `P010` → `(R16Uint, "luma_p010", Rg16Uint, "chroma_p010", 1)`.
  `readback` makes the Y plane and then one or two chroma planes
  (`ReadbackLayout::with_bytes_per_pixel(width / 2, height / 2, chroma format's block_copy_size)`),
  `encode` attaches them. `YuvReadback::buffer_size` uses the last plane.
- Shader: `chroma_nv12` returns `@location(0) vec2<f32>` of `code(values.x, offset), code(values.y, offset)`;
  `luma_p010` returns `@location(0) u32` of `code10(luma_value(pixel), dither(pixel)) << 6u`;
  `chroma_p010` returns `@location(0) vec2<u32>` of the two `code10` values `<< 6u`.
- `submit`'s even-size check and `set_readback_format`'s capability check
  cover the new formats through `is_yuv`.

- [ ] **Step 1: Write failing tests** in `tests/readback.rs` (where `columns` lives; `samples16` comes through `super::*`), each skipping as the yuv420p tests do when the GPU cannot convert:
  - `nv12_interleaves_the_yuv420p_planes`: the 16x4 columns scene (`columns(&["#ff0000", "#00ff00", "#0000ff", "#ffffff", "#000000", "#808080", "#102030", "#c07090"], false)`) read back as `Yuv420p` and as `Nv12`: `nv12.pixels().len() == 16 * 4 * 3 / 2`; `nv12[..64] == yuv420p[..64]`; `nv12[64..].chunks(2)` yields `(u, v)` pairs equal to `yuv420p[64..96].iter().zip(&yuv420p[96..])`.
  - `p010_is_yuv420p10_in_the_high_bits`: the same scene as `Yuv420p10` and `P010`: `p010.pixels().len() == 16 * 4 * 3`; every `samples16(p010)[i] >> 6 == samples16(yuv420p10)[i]` after the UV plane is de-interleaved as above, and every `samples16(p010)[i] & 0x3F == 0`.
  - `nv12_and_p010_match_a_cpu_reference`: a test helper `fn reference_yuv(rgba: &[u8], width: usize, height: usize, bits: u32) -> (Vec<f64>, Vec<f64>, Vec<f64>)` computes the Global constraints' formula (unrounded; chroma from the mean of each 2x2 block's RGB in 0–1; `bits == 10` scales by four) from the scene's `Rgba8` render; for `Nv12` every Y, U, and V sample is within 1 of the rounded reference and equal where the reference is within 0.125 of a whole code; for `P010` the same after `>> 6`. The column colors above exercise every row of the table (red's Cr 240 and blue's Cb 240 exact; `#102030` unrounded on all three).
  - `nv12_and_p010_need_even_dimensions`: `submit(&empty_scene(6, 3))` with each format is `Err(OddYuv420pSize { width: 6, height: 3 })`.
  - `tests/readback.rs`: add `Nv12` and `P010` to the format list of `reads_composited_frames_back_like_directly_drawn_ones`.
- [ ] **Step 2:** `cargo test -p celesta-gpu-renderer nv12 p010` fails to compile (`ReadbackFormat::Nv12` undefined).
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-gpu-renderer` and `cargo clippy -p celesta-gpu-renderer --all-targets -- -D warnings` pass.
- [ ] **Step 5: Commit:** `feat(gpu-renderer): nv12 and p010 readbacks`.

### Task 3: Offering `nv12` and `p010`; the `Color` the muxer gets (`celesta-exporter`, `celesta-media`)

**Files:**
- Modify: `crates/exporter/src/encode.rs` (`offered_formats`, `readback_of`)
- Modify: `crates/media/src/types.rs` (or wherever P2b put the per-format byte count; see Interfaces)
- Test: `crates/exporter/src/encode.rs` tests, `crates/exporter/src/tests.rs`, `crates/exporter/tests/integration/export.rs`, `crates/media/src` tests

**Interfaces:**
- Consumes: P2a's `ExportSession::offered_formats(renderer: &GpuRenderer) -> Vec<PixelFormat>` and `readback_of(format: PixelFormat) -> Result<ReadbackFormat, ExportError>`; P2b's ring sizing, which needs a byte count per `PixelFormat` at a frame size. P2b's Task 3 adds it as `PixelFormat::frame_bytes(self, width: u32, height: u32) -> usize`, already sizing `Nv12` and `P010` like their planar forms; this task only relies on it (and adds a test only if P2b's `frame_bytes_follow_the_format` does not already pin those sizes).
- Produces:
  - `offered_formats`: `[Yuv420p, Yuv420p10, Nv12, P010, Rgba16, Rgba8]` when `supports_yuv420p_readback() && !is_software()`, else `[Rgba16, Rgba8]`.
  - `readback_of`: `Nv12 → ReadbackFormat::Nv12`, `P010 → ReadbackFormat::P010` (the "not produced yet" error goes).
  - `frame_bytes`: `Rgba8` 4/px, `Rgba16` 8/px, `Yuv420p` and `Nv12` `w × h × 3 / 2`, `Yuv420p10` and `P010` `w × h × 3`.

- [ ] **Step 1: Write failing tests:**
  - `crates/media`: `nv12_and_p010_are_as_large_as_their_planar_forms`: `frame_bytes(64, 64)` is `6_144` for `Yuv420p` and `Nv12`, `12_288` for `Yuv420p10` and `P010`, `16_384` for `Rgba8`, `32_768` for `Rgba16`.
  - `encode.rs` tests (P2a's fake registry whose muxer records every request): `the_video_track_carries_bt709_color`: after `open_video` and the muxer opening, the recorded `VideoEncoderRequest.color` and the `Track::Video { color, .. }` both equal `Color::bt709_limited()`.
  - `crates/exporter/src/tests.rs`: `nv12_and_p010_are_offered_after_the_planar_formats`: a `GpuRenderer::new(GpuRenderOptions::default())` (skip without a GPU, as the crate's GPU tests do) with `supports_yuv420p_readback() && !is_software()` gives exactly `[Yuv420p, Yuv420p10, Nv12, P010, Rgba16, Rgba8]`; otherwise `[Rgba16, Rgba8]`.
  - `tests/integration/export.rs`, through the test codec (`solid_video_project(dir, &[[200, 40, 90, 255], [30, 160, 220, 255]])`, 64x64), skipping without a GPU or when `UnsupportedReadbackFormat`: `nv12_and_p010_are_read_back_interleaved`: four exports with `--video-option input=` `yuv420p`, `nv12`, `yuv420p10`, `p010`; stored `video.format` is each; frame lengths `6_144, 6_144, 12_288, 12_288`; `nv12[..4096] == yuv420p[..4096]` and its UV pairs equal yuv420p's U and V planes; every `p010` sample `>> 6` equals the `yuv420p10` sample.
- [ ] **Step 2:** `cargo test -p celesta-media frame_bytes` and `cargo test -p celesta-exporter nv12` fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-media -p celesta-exporter` passes (`celesta-codec-test` built).
- [ ] **Step 5: Commit:** `feat(exporter): offer nv12 and p010 to encoders`.

### Task 4: FFmpeg's video encoders apply `Color` (`plugins/ffmpeg`)

**Files:**
- Create: `plugins/ffmpeg/src/tags.rs`
- Modify: `plugins/ffmpeg/src/video_encoder.rs` (every encoder: context and frame tags, swscale coefficients from `Color`, libx264's `nv12` / `p010` input), `plugins/ffmpeg/src/ffi.rs` (`Scaler` takes its coefficients), `plugins/ffmpeg/src/components.rs` (libx264's `input_formats`), `plugins/ffmpeg/src/lib.rs` (`mod tags;`), `plugins/ffmpeg/plugin.json` (libx264 `inputFormats`)
- Test: `plugins/ffmpeg/tests/encoders.rs`

**Interfaces:**
- Consumes: P2a's `VideoEncoderRequest.color: &Color`, `ffi::Scaler`, `components::describe`.
- Produces, in `tags.rs`:
  ```rust
  pub(crate) struct AvColor { pub primaries: AVColorPrimaries, pub transfer: AVColorTransferCharacteristic,
                              pub matrix: AVColorSpace, pub range: AVColorRange }
  impl AvColor {
      pub(crate) fn from_color(color: &Color) -> Result<Self, MediaError>;   // Codec("unknown color {primaries|transfer|matrix|range} `{name}`")
      pub(crate) unsafe fn apply_to_codec(&self, context: *mut AVCodecContext);          // colorspace, color_primaries, color_trc, color_range
      pub(crate) unsafe fn apply_to_frame(&self, frame: *mut AVFrame);
      pub(crate) unsafe fn apply_to_codecpar(&self, parameters: *mut AVCodecParameters);
      pub(crate) fn sws_coefficients(&self) -> Result<(*const c_int, c_int), MediaError>;   // (sws_getCoefficients(table), destination range: 1 for JPEG, else 0)
  }
  ```
  - `ffi::Scaler::rgb_to_yuv(width, height, source: AVPixelFormat, destination: AVPixelFormat, color: &AvColor)`:
    `SWS_BICUBIC`, then `sws_setColorspaceDetails(ctx, sws_getCoefficients(SWS_CS_DEFAULT), 1, coefficients, range, 0, 1 << 16, 1 << 16)`;
    `ffi::Scaler::repack(width, height, source, destination)`: `SWS_POINT`, no colorspace details (YUV to YUV of the same matrix and range).
  - `open_video`, every encoder: `AvColor::from_color(request.color)?`, `apply_to_codec` before `avcodec_open2`, `apply_to_frame` on every frame sent. libx264's RGB conversion moves from default coefficients to `rgb_to_yuv(.., &color)` (P2a's deviation 8 ends); ProRes's and VP9's `sws_getCoefficients(SWS_CS_ITU709)` becomes the color's.
  - libx264's input choice: `bitDepth` 8: `Yuv420p`, else `Nv12` (context `pix_fmt = AV_PIX_FMT_NV12`, no conversion), else `Rgba8`, else `Rgba16`; 10: `Yuv420p10`, else `P010` (`repack(P010LE → YUV420P10LE)`), else `Rgba16`, else `Rgba8`.
  - `components.rs`: libx264 `input_formats == [Yuv420p, Yuv420p10, Nv12, P010, Rgba8, Rgba16]`; `plugin.json` the same six, as the spec's Manifest example. ProRes and VP9 keep `["rgba16"]`.

- [ ] **Step 1: Write failing tests** in `tests/encoders.rs` (32x16, 10 fps, through `FfmpegCodec` in process; decode packets with libavcodec's `h264`, `prores`, and `libvpx-vp9` decoders as P2a's `rgba_input_is_converted` does, reading the decoded `AVFrame`'s color fields and planes):
  - `libx264_writes_bt709_vui`: offered `[Yuv420p]`; every decoded frame has `colorspace == AVCOL_SPC_BT709`, `color_primaries == AVCOL_PRI_BT709`, `color_trc == AVCOL_TRC_BT709`, `color_range == AVCOL_RANGE_MPEG`.
  - `rgba_input_is_converted_with_bt709`: offered `[Rgba8]`, `crf = 0` (lossless), three solid frames `#ff0000`, `#00ff00`, `#0000ff`; the decoded planes are exactly `(63, 102, 240)`, `(173, 42, 26)`, `(32, 240, 118)`; the same with `[Rgba16]` and `bitDepth = "10"`: `(250, 410, 959)`, `(692, 167, 104)`, `(128, 960, 472)` (libswscale's 10-bit values, deviation 31).
  - `libx264_takes_nv12_and_p010`: offered `[Nv12]` → `input == Nv12`, and the decoded frames equal those of a `[Yuv420p]` run fed the same planes (the chroma interleaved); `bitDepth = "10"` with `[P010]` → `input == P010`, `codec` starts `"avc1.6E"`, decoded frames equal the `[Yuv420p10]` run's.
  - `prores_and_vp9_are_tagged_bt709`: ProRes 4444 packets decode to frames with the three BT.709 fields; VP9 packets decode to `colorspace == BT709` and `color_range == MPEG`, and the first packet's `bytes[4] >> 5 == 2` (the frame header's `color_space` after the sync code).
  - `an_unknown_color_name_is_refused`: `Color { matrix: "nonsense".into(), ..Color::bt709_limited() }` → `open_video` is `Err(MediaError::Codec(m))` with `m` containing `"unknown color matrix `nonsense`"`; the same for `range: "wide"`.
  - `describe_lists_six_inputs_for_libx264`: `input_formats == [Yuv420p, Yuv420p10, Nv12, P010, Rgba8, Rgba16]`, and `plugin.json`'s `inputFormats` equal them.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --test encoders` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** The `encoders` tests and `cargo clippy --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --all-targets -- -D warnings` pass.
- [ ] **Step 5: Commit:** `feat(codec-ffmpeg): encode with the given color and take nv12 and p010`.

### Task 5: FFmpeg's muxers tag their streams; read-back tests (`plugins/ffmpeg`)

**Files:**
- Modify: `plugins/ffmpeg/src/muxer.rs` (video `Track`'s `color` onto `codecpar`)
- Test: `plugins/ffmpeg/tests/export.rs`, `plugins/ffmpeg/tests/process.rs`

**Interfaces:**
- Consumes: Task 4's `AvColor`; P2a's `Track::Video { color, .. }`.
- Produces: `open_muxer` applies `AvColor::from_color(&color)?.apply_to_codecpar(stream.codecpar)` for every video track, before `avformat_write_header`. libavformat then writes MP4's `colr` `nclx` (with the range flag), MOV's `colr` `nclc`, and WebM's `Colour`. No `movflags` change.
- Test helpers in `export.rs`:
  `fn stream_color(path: &Path) -> (i32, i32, i32, i32)` (`avformat_open_input` + `avformat_find_stream_info` through `ffmpeg_sys_next`; the video `codecpar`'s `color_primaries`, `color_trc`, `color_space`, `color_range`);
  `fn colr_atom(path: &Path) -> Option<[u8; 4]>` (the four bytes after the first `colr` in the file);
  `fn webm_has_colour(path: &Path) -> bool` (`55 B0` occurs before the first Cluster id `1F 43 B6 75`);
  `fn decoded_planes(path: &Path, pix_fmt: &str) -> Vec<Vec<u16>>` (each frame's Y, U, V samples through ez-ffmpeg's rawvideo output in `yuv420p` or `yuv420p10le`).

- [ ] **Step 1: Write failing tests** in `tests/export.rs` (through the exporter and `FfmpegCodec`, `gpu()` held, skipping without a GPU):
  - `mp4_mov_and_webm_are_tagged_bt709` (replaces P2a's `mov_and_webm_stay_tagged_bt709_and_mp4_untagged`): the transparent project exported as `.mp4`, `.mov`, and `.webm`; each `stream_color == (1, 1, 1, 1)`; `colr_atom` is `Some(*b"nclx")` for the MP4 and `Some(*b"nclc")` for the MOV; `webm_has_colour` for the WebM.
  - `mp4_colors_hold_bt709_code_values`: for each of `#ff0000`, `#00ff00`, `#0000ff`, a one-clip 64x64, 2 fps project as `converting_colors_on_the_gpu_matches_the_encoder_conversion` builds, whose source is `generate_source(.., "color=c=<hex>:size=64x64:rate=2:duration=0.5,format=rgba", (2, 1))` (ffv1 keeps the RGBA exact), exported with `--video-option crf=0`: the decoded `yuv420p` planes of each frame are within 1 of `(63, 102, 240)`, `(173, 42, 26)`, `(32, 240, 118)`; with `bitDepth=10`, the decoded `yuv420p10le` planes are within 1 of `(250, 409, 960)`, `(691, 167, 105)`, `(127, 960, 471)` (the GPU converted; `stream_color` is `(1, 1, 1, 1)` for both).
  - `vp9_frames_carry_bt709_in_their_header`: the WebM's first video packet (through `av_read_frame`) has `data[4] >> 5 == 2`.
  - `exports_transparent_prores_4444` and `exports_transparent_vp9_webm` keep their `(1, 1, 1)` assertions; `converting_colors_on_the_gpu_matches_the_encoder_conversion` (P2a's port, if kept) stays within its mean difference of 2.
  - `tests/process.rs`: `writes_an_export_through_the_program`'s MP4 has `stream_color == (1, 1, 1, 1)`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --test export tagged` fails (`stream_color` gives `(2, 2, 2, 0)`, every field unspecified, for the MP4).
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`, its clippy with `-D warnings`, and `cargo fmt --check` in `plugins/` pass.
- [ ] **Step 5: Commit:** `feat(codec-ffmpeg): tag every container BT.709`.

### Task 6: The check against P2b and the before/after screenshots

**Files:**
- Create (scratch, not committed): `before.mp4`, `after.mp4`, `reference.png`, `before.png`, `after.png`, `body.md` in the scratchpad directory
- Modify: nothing in the repository; the numbers go into Task 7's handoff note and the images into the PR body

**Interfaces:**
- Consumes: a release build of P2b's branch (the PR's base) and of this branch, each `celesta-exporter` with its `celesta-codec-ffmpeg` staged alone as P2a's Task 15 did (own `CELESTA_PLUGIN_PATH`, empty `HOME`), and Homebrew's `ffmpeg@8` (`"$(brew --prefix ffmpeg@8)/bin/ffmpeg"`, `ffprobe` beside it).

- [ ] **Step 1: Export the scene with both builds.** `celesta-exporter --react examples/spectra/film.tsx --from 1.5 --to 2.0 <before|after>.mp4`, and the same as `.mov` and `.webm`; `celesta-exporter --react examples/spectra/film.tsx --frame 90 reference.png` with this branch.
- [ ] **Step 2: Tags.** `ffprobe -v error -select_streams v -show_entries stream=color_space,color_primaries,color_transfer,color_range -of default=nw=1 <file>`: the three P2c files report `bt709`, `bt709`, `bt709`, `tv`; P2b's MP4 reports `unknown` for all four and its MOV and WebM `bt709`.
- [ ] **Step 3: Decoded colors.** For each pair, decode frame 0 as a BT.709 player does: `ffmpeg -i <file> -vf "scale=in_color_matrix=bt709:in_range=tv,format=rgb24" -frames:v 1 -update 1 <before|after>.png`. Compare each PNG with `reference.png` (`ffmpeg -i x.png -i reference.png -lavfi psnr -f null -`): the MP4's PSNR after is several dB above before (BT.601 codes read as BT.709 shift reds and greens); MOV's and WebM's are equal before and after. Also decode P2b's MP4 with `in_color_matrix=bt601`: its PSNR equals P2c's `bt709` one within 0.5 dB, which shows the pixels changed only by the matrix. Record every PSNR.
- [ ] **Step 4: What must not change.** `cmp` the MOV's and WebM's decoded video (`-pix_fmt rgba64le -f rawvideo`) between before and after: identical (the same libswscale coefficients). `cmp` each pair's decoded audio (`-f f32le`): identical. `celesta-exporter --react examples/spectra/film.tsx --frame 90` with both builds: identical PNGs. Time the 1080p `--from 1.5 --to 2.0` MP4 export three times with each build, interleaved; P2c's median is within P2b's spread (the matrix costs nothing; `nv12` is not on this path).
- [ ] **Step 5: The PR body.** Write `body.md` with a "Before/after" section: `![Reference render](./reference.png)`, `![Before: P2b's MP4 decoded as BT.709](./before.png)`, `![After: P2c's MP4 decoded as BT.709](./after.png)`, one sentence on why the before frame is decoded with the BT.709 matrix (that is how QuickTime, Chrome, and VLC treat untagged HD), and the PSNRs. From the images' directory: `gh pr edit <number> --body-file body.md --attach ./reference.png --attach ./before.png --attach ./after.png` (gh rewrites each reference to an uploaded URL). If the PR does not exist yet, keep `body.md` for the PR's creation and run the `gh pr edit` right after.
- [ ] **Step 6:** No commit; Task 7 records the numbers.

### Task 7: Docs, the spec's deviations, HANDOFF.md, and the handoff note

**Files:**
- Modify: `packages/website/content/en/docs/02-(guides)/09-export.md` (the encoder section P2a renamed, "Choosing encoders and settings": a paragraph "Color"; the "Transparent video" sentence already says BT.709) and `ja/docs/02-(guides)/09-export.md` (the same section; 透過動画 unchanged)
- Modify: `skills/celesta/references/export.md` ("Quality notes": the same two sentences; "Transparent video" unchanged)
- Modify: `HANDOFF.md` (the export bullets around `:288-298`: the GPU reads back I420 / NV12 at 1.5 bytes a pixel or their 10-bit forms at 3, BT.709 limited range; every encoder is told BT.709 and every container is tagged; `celesta-gpu-renderer`'s row if it names the readbacks)
- Modify: `docs/superpowers/specs/2026-11-08-media-components-design.md` (Pixel formats and color: "As built (P2c)" with deviations 30–31 and the end of deviation 8's temporary state; Phases → P2c: as built)
- Create: `docs/handoff/2026-11-08-media-bt709.md`

- [ ] **Step 1: Write the docs.** The guide's paragraph (en): every video export is BT.709 with limited range, converted on the GPU or by the encoder when it takes RGB, and tagged in the file (H.264's VUI, MP4 and MOV `colr`, WebM `Colour`), so players show the colors as rendered; earlier releases wrote untagged MP4s with BT.601 coefficients, which players that assume BT.709 for HD showed slightly off. The same in Japanese (BT.709、リミテッドレンジ、タグ). The Agent Skill's "Quality notes" gets the two sentences and that `--describe ffmpeg/libx264` shows no color option: there is nothing to choose.
- [ ] **Step 2: Verify.** Each passes: `cargo test --workspace`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo fmt --all -- --check`; with `PKG_CONFIG_PATH`: `cargo test --manifest-path plugins/Cargo.toml --workspace --locked` and its clippy; `pnpm --dir packages/website test`; `git diff --check`.
- [ ] **Step 3: Write `docs/handoff/2026-11-08-media-bt709.md`** in the P1 note's format: the specification as built (the matrix and coefficients, the four YUV readbacks and their layouts, the offered list, how each FFmpeg encoder and muxer applies `Color`, libx264's `nv12` / `p010`), deviations 30–31 and that 18 is kept, docs, checks (Step 2's commands; Task 6's ffprobe tags, PSNRs, `cmp` results, and timings; the known-color table), and what is not done (presets and the export dialog, stills through components, OS-native plugins).
- [ ] **Step 4: Commit:** `docs: BT.709 export color and nv12/p010 readbacks (P2c)`.
