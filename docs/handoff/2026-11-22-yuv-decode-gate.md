# YUV decode handoff A: parity preflight

## Superseding acceptance gate: owner option 2

The historical <=2 swscale gate in this note is no longer the owner's target.
The new gate is exact alpha and RGB max 0 for both `to_rgba()` and GPU texture
readback against an independent f64 BT.601/709 reference, supporting limited
and full range, I420 2x2 replication and NV12 vertical bicubic with matching
border normalization, nearest rounding and clamping. Comparisons with plugin
RGBA/swscale and end-to-end RGBA-offer scenes instead permit max 4/255 with
exact alpha; max/mean/differing counts remain required on every platform.

CPU and shader now use nearest rounding. All six YUV fixture first frames
match the independent reference exactly on lavapipe, with exact alpha.
The shader evaluates green as `(Y - Kr*R - Kb*B) / Kg`, using unclamped R/B,
to avoid one f32 half-integer boundary error. No epsilon or relaxed tolerance
is used. See `2026-11-22-yuv-independent-reference.md` for measurements.

The swscale CPU-path controls are retained at
`/home/ubuntu/research/yuv-gate/cpu-flags-report.md`: identical native payloads
produce plugin RGBA C-vs-auto differences up to 3 (I420) and 4 (NV12).
The macOS odd-chroma failure matches C fallback for width 1918, which cannot
use the 16-pixel-aligned NEON unscaled path. Windows NV12 matches the C
aggregate signature, without a native function-pointer capture. This finding
motivates separating deterministic correctness from swscale compatibility.

Everything below is the historical preflight, not the current gate policy.

## Specification

Handoff A proposes per-request RGBA8/NV12/I420 decode negotiation, validated
owned frames with host RGBA materialization, format-aware shared-memory and
prefetch handling, and native FFmpeg YUV output. No GPU opt-in yet. Existing
platform plugins retain RGBA. The acceptance gate is exact alpha and maximum
RGB error at most 2/255 against the existing FFmpeg RGBA decode, including
odd dimensions. The owner requested stopping rather than relaxing that gate.

## State

Started from main `67450afd166bc153d03e900574d8d7eb85b44f6f` on branch
`devin/1795326315-yuv-decode`. No production implementation or repository test
changes. This note records a failed preflight, not a completed handoff A.

The standalone Rust experiment and its fixtures are retained outside the
repository at `/home/ubuntu/research/yuv-gate/`. Its Cargo dependencies point
to the current FFmpeg plugin and media crates. `main.rs` decodes the RGBA
reference through `FfmpegBackend::decode_frame`, obtains native planes through
the same FFmpeg 8.1 libraries, and compares a nearest-chroma matrix converter
against the reference. It also converts those exact native planes with
libswscale as a control. FFmpeg 4.4's installed CLI only generates encoded
fixtures; it does not produce the RGB reference or the measured native decode.

## Checks and results

Command (exit 0; this experiment prints metrics, not a passing test assertion):

```sh
PKG_CONFIG_PATH=/home/ubuntu/research/ffmpeg8-standard/lib/pkgconfig \
CARGO_TARGET_DIR=/home/ubuntu/repos/Celesta/plugins/target \
cargo run --manifest-path /home/ubuntu/research/yuv-gate/Cargo.toml --offline
```

Full final output: `/home/ubuntu/research/yuv-gate/results-final.log`.
The following results use matrix conversion followed by floor/clamp, without
an extra rounding bias. Mean is absolute error per RGB channel in 8-bit units;
differing pixels count any unequal RGB channel. Alpha is exact throughout.

| Fixture | Native size | Maximum RGB error | Mean RGB error | Differing pixels |
| --- | --- | ---: | ---: | ---: |
| H.264 tagged BT.709 limited | 1920x1080 | 2 | 0.292687 | 1394520 / 2073600 |
| H.264 untagged, BT.601 limited fallback | 1920x1080 | 2 | 0.400221 | 1721668 / 2073600 |
| H.264 tagged BT.709 full | 1920x1080 | 1 | 0.161292 | 1003367 / 2073600 |
| H.264 BT.709 limited, odd chroma dimensions | 1918x1078 | 2 | 0.293076 | 1391230 / 2067604 |
| FFV1 BT.709 limited, genuinely odd dimensions | 1919x1079 | **255** | 0.851418 | 908557 / 2070601 |

The odd-sized FFV1 fixture is an additional probe, not a substitute claimed
to satisfy the requested odd-sized H.264 fixture. Rounding-bias variants
0.5, -0.5 and -1 also have maximum error 255 on that odd-sized I420 fixture.
Using libswscale on the same packed native planes instead gives **maximum
error 0 against the plugin for every fixture**. This control rules out native
decoder output differences or plane packing as explanations for this failure.

FFmpeg 8.1.3's `libswscale/swscale_unscaled.c:2413-2419` gates its unscaled
YUV420P-to-RGB converter on even destination height (`!(dstH & 1)`). The
extractor uses default bicubic conversion. The odd-height sample therefore
cannot use that converter. Matching its output requires more than changing
matrix coefficients or rounding; the general chroma reconstruction remains
unimplemented in the standalone converter. This does not prove that a more
complete host converter cannot meet the gate.

The installed x264 encoder rejects the requested truly odd 4:2:0 H.264 size:

```text
[libx264 @ ...] width not divisible by 2 (1919x1079)
Error initializing output stream 0:0 -- Error while opening encoder for output stream #0:0 - maybe incorrect parameters such as bit_rate, rate, width or height
```

Generating 1919x1079 H.264 as yuv444p succeeds; ffprobe prints:

```text
1919,1079,yuv444p
```

That source must retain RGBA under the agreed unsupported-format rule; it
cannot exercise native I420/NV12 materialization without forbidden conversion.
The full-range source decodes as deprecated YUVJ420P (I420 layout plus full
range), another case whose layout-versus-format eligibility needs making
explicit when implementing the raw sink.

## Decision needed

Recommended bounded first version: keep true odd-height/odd-width sources on
the existing RGBA path; use 1918x1078 H.264 to test odd chroma dimensions, and
1919x1079 H.264 4:4:4 to test unsupported-format fallback. Keep the same <=2
gate for every actual YUV answer. Do not silently treat an RGBA fallback test
as proof of odd-sized YUV materialization parity.

Alternatively, retain a genuinely odd-sized YUV gate using a codec such as
FFV1 and implement a matching general chroma resampler in both host and GPU
paths before enabling that source shape.

The requested media/process/renderer/exporter/plugin tests, Clippy and fmt
were not run: production code was not changed. No GPU screenshots were taken;
the renderer remains unchanged. Nothing was pushed.
