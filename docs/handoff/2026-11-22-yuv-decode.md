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
