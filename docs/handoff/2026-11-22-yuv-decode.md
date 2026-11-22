# Negotiated SDR YUV decode — handoff A

## Specification

Branch `devin/1795326315-yuv-decode`, based on main
`67450afd166bc153d03e900574d8d7eb85b44f6f`. This completes the bounded
implementation approved after `2026-11-22-yuv-decode-gate.md`; that note
describes the earlier preflight, not the current implementation.

Add per-request RGBA8, NV12 and I420 decode negotiation. Only FFmpeg produces
YUV, for supported even-sized 8-bit SDR sources. BT.601/709 matrix and
limited/full range are supported. Odd width or height, alpha, display
transforms, HDR, BT.2020, unsupported formats and unsupported color/chroma
metadata retain plugin-side RGBA. No renderer opts in yet. GPU conversion,
performance measurements and screenshots belong to handoff B. Commit locally;
do not push or amend.

## Contract and implementation

- `crates/media/src/types.rs`: `DecodedPixelFormat`, `DecodedColor`, validated
  `VideoFrame::new`/`rgba`, `frame_bytes`, `validate` and `to_rgba`. Bytes remain
  owned through `Arc<Vec<u8>>`. RGBA materialization shares the original Arc.
  YUV materialization allocates RGBA and produces opaque alpha.
- Wire strings: `rgba8`, `nv12`, `yuv420p`; color contains `matrix` (`bt601` or
  `bt709`), `range` (`limited` or `full`), and `chromaLocation` (`left`). RGBA
  has no decoded YUV color metadata. Other chroma locations are not supported.
- Plane rows are tight and top-down. RGBA is W×H×4. I420 is Y, U, V; NV12 is
  Y then interleaved UV. Both have Y size W×H and chroma dimensions ceil(W/2)
  by ceil(H/2). Frame/layout arithmetic is checked, including zero dimensions.
  Layout constructors support odd dimensions; FFmpeg deliberately never
  negotiates YUV for them because the legacy reconstruction differs there.
- `provider.rs`, `backend.rs`, `protocol.rs`: optional request
  `acceptedPixelFormats`; missing means RGBA only. `MediaBackend` defaults
  to RGBA and has `with_accepted_pixel_formats`. Effective host wire offers
  always include RGBA. Missing response `pixelFormat` means RGBA. Unoffered
  non-RGBA answers, unknown formats/colors, missing YUV color and bad lengths
  are rejected. The same validation covers payloads and shared-memory slots.
  The single 40 MiB slot, copy-out, generation check and pipe fallback remain.
- `process.rs`: the accepted list is part of the complete prefetch session
  identity, so changing it invalidates prediction/stash reuse. Speculative
  requests retain it, and speculative responses validate against that offer.
- `plugins/ffmpeg/src/native.rs`: bounded raw-frame sink packs native I420
  (including full-range YUVJ420P) or NV12. No I420/NV12 conversion, RGB roundtrip,
  borrowed mapping or native padded stride crosses the wire. Left-tagged and
  unspecified chroma resolve to effective left siting; other sitings fall back.
  Untagged matrix/range resolve to BT.601 limited, as in the legacy path.
  Display matrices and alpha-mode tags are checked before opening a native run.
- `decoder.rs`, `sequential.rs`: accepted lists re-key retained sessions.
  Unsupported native frames restart the legacy run at the same seek point,
  replaying its already-delivered count before resuming. Timestamp selection,
  seeks, repeated frames, sequential reuse and freeze-at-end remain in the
  existing selection code. Unsupported streams may incur this extra initial
  decode; the default RGBA path never opens a native run.
- CPU renderer materializes YUV. GPU preparation also materializes injected
  YUV before its existing RGBA resize/upload path; its RGBA path and malformed
  RGBA error classification remain unchanged. AVFoundation, MediaFoundation
  and test-codec edits only label their existing RGBA constructors/offers.

### Reconstruction requirement for handoff B

Do not assume I420 and NV12 use the same reconstruction in the legacy RGBA
reference. I420 uses 2×2 chroma replication. NV12 uses vertical bicubic
reconstruction (B=0, C=0.6), horizontal replication, centered vertical position
`y/2 - 0.25`, normalized coefficients and clamped edge samples. The finite
filter window, including truncation toward zero at the top edge, matches the
legacy swscale filter setup. Matrix/range conversion floors and clips RGB.
See `to_rgba()` for the exact CPU oracle to reproduce in the GPU shader.
No transfer-function decoding, gamut transform or sRGB texture interpretation
is added. The output remains nonlinear RGB code values like current RGBA.

The first NV12 test exposed max RGB error 75 with I420 replication, then 4
with symmetric bicubic borders. Matching the normalized finite window fixed
it without changing the ≤2 gate. Chroma filtering therefore must not be
replaced by an arbitrary linear sampler in handoff B.

## Pixel gate

Actual FFmpeg 8.1.3 plugin process, both shared-memory and pipe transports.
Reference: default `FfmpegBackend` RGBA decode of the same fixture/time.
Each alpha sample is asserted equal. RGB errors below are 8-bit levels;
mean is over all RGB channels; differing count is pixels with any difference.
Both transports produce the same metrics. Output retained in
`/home/ubuntu/research/yuv-gate/plugin-gate-3.log`.

| Fixture | Size | Answer | Max | Mean | Differing pixels |
| --- | --- | --- | ---: | ---: | ---: |
| H.264 BT.709 limited | 1920×1080 | I420 | 2 | 0.292687 | 1,394,520 |
| H.264 untagged / BT.601 limited | 1920×1080 | I420 | 2 | 0.400221 | 1,721,668 |
| H.264 BT.709 full | 1920×1080 | I420 | 1 | 0.161292 | 1,003,367 |
| H.264 even size, odd chroma dimensions | 1918×1078 | I420 | 2 | 0.293076 | 1,391,230 |
| Raw native NV12, untagged | 128×72 | NV12 | 2 | 0.438874 | 7,799 |
| H.264 odd 4:4:4 | 1919×1079 | RGBA fallback | 0 | 0 | 0 |
| FFV1 odd I420 | 1919×1079 | RGBA fallback | 0 | 0 | 0 |

Fallback rows are exact frame-equality assertions, not YUV conversions.
An additional eight-frame H.264 sequence exercises real-process YUV prefetch,
offer changes, backward seeks and EOF over both transports. Each YUV frame
meets max 2; mean ranges 0.401837–0.454644 and differing counts 7,561–7,876.
Rotated, HDR and alpha-source fallback frames are asserted identical to RGBA.

## Checks

Rust tests cover legacy requests/responses, unoffered formats, malformed
payload/shared-slot layouts, materialization, prefetch offer identity, native
YUV, timestamp selection and bounded fallbacks. Final results:

```text
cargo test -p celesta-media
test result: ok. 148 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.36s

cargo test -p celesta-media -p celesta-renderer  # renderer results
test result: ok. 263 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.68s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s

cargo test -p celesta-codec-test --test process
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s

cargo test -p celesta-exporter --lib
test result: ok. 191 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s

VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json XDG_RUNTIME_DIR=/tmp cargo test --release -p celesta-gpu-renderer
test result: ok. 437 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 143.23s

node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg --locked
# 103 passed, 0 failed across the nonempty suite results:
# 56 + 1 + 4 + 13 + 14 + 3 + 1 + 6 + 5
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.80s

cargo clippy -p celesta-media -p celesta-codec-test -p celesta-renderer -p celesta-exporter -p celesta-gpu-renderer --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.34s

node scripts/plugins/run-cargo.mts plugins clippy -p celesta-codec-ffmpeg --locked --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.17s

cargo fmt --all -- --check
cargo fmt --manifest-path plugins/Cargo.toml --all -- --check
git diff --check
Formatting and diff checks: exit 0
```

FFmpeg runner environment: `PKG_CONFIG_PATH` points to
`/home/ubuntu/research/ffmpeg8-standard/lib/pkgconfig`; software-GPU checks
also use the Vulkan ICD and XDG settings shown above. An initial plugin/export
run emitted `error: XDG_RUNTIME_DIR not set in the environment.` because no
desktop runtime directory is provisioned. The final runs use the blueprint's
`/tmp` runtime setting and do not emit it. Clippy's mechanical modulo/chunk
suggestions were fixed. The first GPU suite exposed a changed malformed-RGBA
error classification from unconditional materialization; preparation now only
materializes YUV, preserving the original RGBA path. Its existing assertion
was not changed; the final GPU suite passes.

Logs remain under `/home/ubuntu/research/yuv-gate/`: `media-tests-final.log`,
`root-tests.log`, `process-tests.log`, `exporter-tests-final.log`,
`gpu-tests-final.log`, `plugin-tests-last.log`, `root-clippy-final.log`,
`plugin-clippy-last.log` and `plugin-gate-3.log`.

Native macOS/Windows execution is not checked locally. No screenshots or
visual check were performed in this handoff; there is no GPU YUV upload or
conversion implementation or opt-in here. No performance claim is made.

## Handoff B — GPU upload and conversion

The sections above record handoff A. Handoff B adds GPU-only offers at the
editor preview and exporter `MediaBackend` construction sites. The backend
default, CPU renderer and other callers remain RGBA-only. No composition API,
wire format, native-platform decode behavior or export pixel format changes.

`video_yuv.rs` / `video_yuv.wgsl` implement decoded-plane pooling and compute
conversion. NV12 uploads R8 + RG8; I420 uploads three R8 planes. `VideoUploads`
now stages arbitrary tightly packed plane rows through its existing mapped
buffer pool, including alignment padding, map failure fallback and asynchronous
remapping. Converted Rgba8Unorm output textures use the existing retained
layer-texture pool and ownership checks. Plane textures are reusable after a
conversion has been submitted: ordered copy/compute submissions precede any
subsequent overwrite. Output textures remain distinct while layers hold them.

The shader reproduces the handoff-A oracle: nearest 2×2 I420 chroma, the NV12
finite bicubic window and border normalization, BT.601/709, full/limited range,
floor/clip and exact opaque alpha. It writes nonlinear code values to
Rgba8Unorm, with no sRGB conversion. Effects, masks, blending and readback
continue to consume ordinary RGBA layer textures. Final-quality CPU mipmaps
and oversized-source CPU downsizing materialize via `to_rgba()` and retain
the existing path. Plane measurement uploads nothing; no GPU mips were added.

Tests in `tests/video_yuv.rs` cover both matrices/ranges/formats, odd dimensions,
padded rows, stable allocation counts, malformed frames, multiple queued
conversions, and exact CPU mip/downsize fallbacks. Real-plugin fixture tests
use `CELESTA_TEST_FFMPEG` (otherwise explicitly print a skip notice); local
runs set it to `plugins/target/debug/celesta-codec-ffmpeg`. Each end-to-end
scene uses a real shared-memory `MediaBackend`, switching only its offer.

### GPU gates (lavapipe)

All alpha samples match exactly. Means are over RGB channels; differing is
the number of pixels with any difference. Converted-texture readback vs
`to_rgba()` is **max 0, mean 0, differing 0** for each of the six YUV fixtures:
709 limited, untagged 601 limited, 709 full, 1918×1078 odd chroma, raw NV12,
and the first frame of the 128×72 sequential H.264 fixture.
The ≤1 requirement is retained. Synthetic cases also meet ≤1.

End-to-end render vs the RGBA-offer path (≤2 required):

| Fixture | Plain max / mean / differing | Effects + blend max / mean / differing |
| --- | --- | --- |
| 709 limited, 1920×1080 | 2 / 0.292687 / 1,394,520 | 2 / 0.159816 / 924,318 |
| Untagged, 1920×1080 | 2 / 0.400221 / 1,721,668 | 2 / 0.214545 / 1,055,326 |
| 709 full, 1920×1080 | 1 / 0.161292 / 1,003,367 | 1 / 0.083997 / 522,526 |
| 709 limited, 1918×1078 | 2 / 0.293076 / 1,391,230 | 2 / 0.160083 / 923,314 |
| NV12, 128×72 | 2 / 0.438874 / 7,799 | 2 / 0.244900 / 5,260 |
| Sequential H.264, 128×72 | 2 / 0.401837 / 7,561 | 2 / 0.219256 / 5,129 |

Effects scene: brightness 0.8, Screen blending, opacity 0.75 over the default
background. Screenshots of the same 709-limited scene were saved and visually
checked as a matched pair, not committed:

- `/home/ubuntu/research/yuv-shots/rgba-effects-false.png`
- `/home/ubuntu/research/yuv-shots/yuv-effects-false.png`
- `/home/ubuntu/research/yuv-shots/rgba-effects-true.png`
- `/home/ubuntu/research/yuv-shots/yuv-effects-true.png`

Gate output: `/home/ubuntu/research/yuv-gate/b-gpu-tests-final.log`.

### Completed-texture benchmark

FFmpeg 8.1.3 external plugin, shared memory asserted, `MediaBackend` sequential
decode with prefetch enabled in both cases. Four warmup + 120 timed frames;
three interleaved RGBA/YUV pairs at each workload. Medians below include
decode, staging upload, compute where applicable, GPU completion via
`Device::poll(Wait)`, and the stated CPU busy work. No readback copy is timed.
The benchmark is an ignored Rust test, `benchmark_completed_video_texture`;
set `CELESTA_TEST_FFMPEG` and `CELESTA_YUV_BENCH_INPUT`, then run it with
`cargo test --release -p celesta-gpu-renderer benchmark_completed_video_texture -- --ignored --nocapture`.

Adapter: **Vulkan lavapipe / llvmpipe**, Mesa 23.2.1, LLVM 15.0.7 (CPU).
These are software-GPU numbers, not evidence of a hardware-GPU speedup.

| Resolution | Host work | RGBA ms/frame (range) | YUV ms/frame (range) | RGBA slot bytes | YUV slot bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1920×1080 | 0 ms | 2.068 (2.054–2.105) | 9.437 (9.147–9.689) | 8,294,400 | 3,110,400 |
| 1920×1080 | 15 ms | 17.080 (17.057–17.125) | 24.329 (23.848–24.611) | 8,294,400 | 3,110,400 |
| 3840×2160 | 0 ms | 13.693 (12.996–14.043) | 33.332 (31.783–36.801) | 33,177,600 | 12,441,600 |
| 3840×2160 | 15 ms | 25.498 (24.693–25.800) | 49.533 (48.847–56.587) | 33,177,600 | 12,441,600 |

**YUV is slower on this software GPU**, despite 62.5% fewer slot bytes. The
compute work runs on the CPU in this configuration. No end-to-end speedup
claim is made, and no timed encoded-composition export was obtained. Logs:
`b-bench-1080.log` and `b-bench-4k.log` under the same research directory.
Native hardware GPU, macOS/Windows and interactive editor execution remain
unchecked. The saved images are actual offscreen GPU scene renders.

### Handoff-B checks

Environment: `XDG_RUNTIME_DIR=/tmp`, lavapipe ICD from the blueprint,
`CELESTA_TEST_FFMPEG` as above; plugin runner uses the FFmpeg 8.1.3 standard
prefix via `PKG_CONFIG_PATH` (x264 prefix configured separately).

| Command | Result |
| --- | --- |
| `cargo test --release -p celesta-gpu-renderer` (blueprint environment, real-plugin gate enabled) | 441 passed, 0 failed, 2 ignored (existing fixture writer and new benchmark) |
| `cargo test -p celesta-media` | 148 passed, 0 failed |
| `cargo test -p celesta-codec-test --test process` | 20 passed, 0 failed |
| `cargo test -p celesta-exporter --lib` | 191 passed, 0 failed |
| `node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg --locked` | 103 passed, 0 failed across nonempty suites |
| Targeted root Clippy (`celesta-media`, `celesta-codec-test`, `celesta-gpu-renderer`, `celesta-exporter`), all targets, `-D warnings` | passed |
| `cargo clippy -p celesta-editor --all-targets -- -D warnings` | passed |
| FFmpeg plugin runner Clippy, all targets, `-D warnings` | passed |
| Root/plugin rustfmt checks; `git diff --check` | passed |

Detailed outputs are `/home/ubuntu/research/yuv-gate/b-*-tests.log`,
`b-root-clippy.log`, and `b-plugin-clippy.log`. The first compile of the new
test helper caught fallible mapped-buffer access and a wrapper-borrow error;
both were fixed before gates ran. There were no pixel-gate failures.
The additional editor Clippy check initially found missing desktop system
development packages (`fontconfig.pc`, then `alsa.pc`). The standard pkg-config
search path was correct; installing `libfontconfig1-dev` and `libasound2-dev`
resolved this environment issue. Its final output is `b-editor-clippy-final.log`.
The sequential fixture was added in a follow-up commit so every fixture
answering YUV in A is included. The final complete GPU suite again passed
441 tests (0 failed, 2 ignored), followed by GPU Clippy and formatting checks.
Other source and benchmark inputs were unchanged; their earlier checks apply.

## Profiling follow-up

See [the lavapipe profile](2026-11-22-yuv-lavapipe-profile.md) for the measured
stage breakdown, no-dispatch control, binding/submission/chroma-reuse fixes,
updated benchmark table and checks. The historical measurements above are
retained; the follow-up uses newly paired before/after runs.
