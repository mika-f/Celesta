# YUV independent reference and nearest rounding — owner option 2

## Final implementation and local validation

After the preflight report below, the owner authorized changing both production
quantizers from floor to nearest. `VideoFrame::to_rgba()` now uses `round()`;
the WGSL shader rounds the reconstructed RGB before clamping and storing.

The first nearest shader run left one odd-chroma green channel +1 above the
reference: max 1, mean 0.000000161, one differing pixel, exact alpha. At
`(1201, 778)`, Y/Cb/Cr are 149/153/53; reference green is
189.4999979837452 (nearest 189), while the original f32 expression reaches
189.5 (rounded to 190). Reusing the unclamped reconstructed R/B values and
evaluating `G = (Y - Kr*R - Kb*B) / (1 - Kr - Kb)` fixes this boundary without
an epsilon, shifted threshold, format fallback or tolerance relaxation. This
is the same matrix identity, with no extra GPU resources, dispatch or submit.

Final independent-reference RGB max/mean/differing counts are **0 / 0 / 0**
for CPU and GPU separately on each of the six fixtures: 709 limited, untagged
601 limited, 709 full, odd chroma, native NV12 and sequence first frame.
Alpha is exact. This is measured on lavapipe, not a claim about all possible
YUV byte combinations or untested hardware GPUs.

Final swscale compatibility gates (max / mean / differing pixels), alpha exact:

| Fixture | Plugin RGBA / plain scene | Effects/blend scene |
|---|---|---|
| 709 limited | 3 / 0.517328 / 2,070,445 | 2 / 0.280139 / 1,536,971 |
| Untagged | 3 / 0.575934 / 2,068,408 | 2 / 0.316774 / 1,364,172 |
| 709 full | 2 / 0.451051 / 2,065,928 | 2 / 0.250076 / 1,466,440 |
| Odd chroma | 3 / 0.515477 / 2,062,973 | 2 / 0.279373 / 1,528,316 |
| NV12 | 3 / 0.372541 / 6,968 | 2 / 0.207393 / 4,914 |
| Sequence | 3 / 0.681894 / 9,131 | 2 / 0.372070 / 7,394 |

Plugin pipe/shared-memory figures agree; sequence uses prefetch's first frame.
Raw gate output: `nearest-conditioned-preflight.log` and
`nearest-plugin-final.log` under `/home/ubuntu/research/yuv-gate/`.

The same-scene RGBA/YUV plain and effects pairs were re-rendered and visually
checked under `/home/ubuntu/research/yuv-shots/nearest/`, named
`{rgba,yuv}-effects-{false,true}.png` (not committed). Before-rounding YUV
screenshots remain in the parent directory. No visible artifacts were found.

Final checks, with logs in `/home/ubuntu/research/yuv-gate/`:

```text
# nearest-gpu-full.log; blueprint GPU environment, actual debug FFmpeg plugin,
# CELESTA_YUV_SHOTS=/home/ubuntu/research/yuv-shots/nearest
# cargo test --release -p celesta-gpu-renderer
test result: ok. 446 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 141.12s

# nearest-media-renderer-tests.log; cargo test -p celesta-media -p celesta-renderer
test result: ok. 148 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.43s
test result: ok. 263 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.40s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

# nearest-plugin-final.log; plugin runner --locked (107 total, zero failures)
# final YUV suite:
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.62s

# nearest-root-clippy-final.log; media/renderer/gpu-renderer, all targets, -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.87s
# nearest-plugin-clippy-final.log; plugin runner --locked --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.11s
```

Root/plugin formatting checks and `git diff --check`: exit 0. CI and signing
results are reported after publication; the following sections preserve the
historical preflight and its old-head CI, not the final implementation status.

## Specification and implementation

Branch: `devin/1795326315-yuv-decode`.

The owner requires CPU `to_rgba()` and GPU converted-texture readback to match
an independent nearest-rounded f64 reference exactly (RGB max 0, exact alpha).
Plugin RGBA/swscale and RGBA-offer scene comparisons instead allow RGB max 4,
with exact alpha and max/mean/differing-pixel reporting on every platform.

`crates/media/src/tests/yuv_reference.rs` is included only by plugin and GPU
tests. It implements BT.601/709, limited/full range, I420 2x2 replication,
NV12 vertical bicubic reconstruction with the existing tap selection and
border normalization, and nearest rounding/clamping. Unit tests cover both
formats, matrices and ranges, colored/neutral samples, clamping, alpha
preservation, malformed frames and empty dimensions.

The plugin and GPU tests decode all six supported YUV fixture first frames
and compare CPU materialization to this reference; the GPU test also uploads
and reads back the converted layer texture. Diagnostics collect all six
fixtures before failing. Swscale and end-to-end scene limits are now 4.

## Historical exact gate: blocked before production changes

Lavapipe, FFmpeg 8.1.3. CPU and GPU independently produced **identical metrics**
against the reference for every row. Alpha is exact. `Below` counts RGB
channels below the reference; every row has zero channels above it.

| Fixture | CPU max | GPU max | Mean RGB error (both) | Differing pixels (both) | Below (both) |
|---|---:|---:|---:|---:|---:|
| 709 limited, 1920x1080 | 1 | 1 | 0.224640400 | 1,355,482 | 1,397,443 |
| Untagged/601 limited, 1920x1080 | 1 | 1 | 0.175713092 | 1,041,722 | 1,093,076 |
| 709 full, 1920x1080 | 1 | 1 | 0.289758391 | 1,759,632 | 1,802,529 |
| Odd chroma, 1918x1078 | 1 | 1 | 0.222400582 | 1,338,118 | 1,379,509 |
| Native NV12, 128x72 | 1 | 1 | 0.256727431 | 5,612 | 7,098 |
| Sequence first frame, 128x72 | 1 | 1 | 0.280056424 | 6,071 | 7,743 |

Production CPU conversion explicitly uses `value.floor()` at
`crates/media/src/types.rs:221`; the shader uses `floor(rgb)` at
`crates/gpu-renderer/src/video_yuv.wgsl:52`. The reference uses `round()`.
Thus the existing policy disagrees with nearest quantization before any
f32-versus-f64 question is resolved. CPU arithmetic is already f64; a GPU-only
f32 explanation cannot explain the CPU failures. As a countercheck to that
hypothesis, CPU and GPU counts are identical and all differences are downward.
Whether switching to nearest leaves any f32 boundary discrepancies is
**unverified**. No production conversion code has been changed, and no claim
is made that nearest alone will satisfy the GPU max-0 gate.

Reporting this checkpoint precedes any production quantization change.
Commit, push and re-signing are pending resolution; current local/remote HEAD
remains `04cb609be5f2fbfdb93dc6c17e630caa30dc76b2`.

## Swscale compatibility gates

Cells are max / mean / differing pixels, with exact alpha. Plugin figures
agree for pipe and shared memory. The sequence plugin figure is the existing
prefetch test's first-frame result. Production conversion is unchanged.

| Fixture | Plugin RGBA | Plain scene | Effects/blend scene |
|---|---|---|---|
| 709 limited | 2 / 0.292687 / 1,394,520 | 2 / 0.292687 / 1,394,520 | 2 / 0.159816 / 924,318 |
| Untagged | 2 / 0.400221 / 1,721,668 | 2 / 0.400221 / 1,721,668 | 2 / 0.214545 / 1,055,326 |
| 709 full | 1 / 0.161292 / 1,003,367 | 1 / 0.161292 / 1,003,367 | 1 / 0.083997 / 522,526 |
| Odd chroma | 2 / 0.293076 / 1,391,230 | 2 / 0.293076 / 1,391,230 | 2 / 0.160083 / 923,314 |
| NV12 | 2 / 0.438874 / 7,799 | 2 / 0.438874 / 7,799 | 2 / 0.244900 / 5,260 |
| Sequence | 2 / 0.401837 / 7,561 | 2 / 0.401837 / 7,561 | 2 / 0.219256 / 5,129 |

The dispatch investigation and numbers remain in
`/home/ubuntu/research/yuv-gate/cpu-flags-report.md`. Native YUV bytes are
identical across tested CPU masks while plugin RGBA changes by up to 3
(I420) and 4 (NV12). The macOS 1918-wide case cannot take the width-aligned
NEON unscaled path and matches C-path metrics. Windows NV12 matches C-path
aggregate metrics; exact native dispatch remains uninstrumented.

## Checks

Final logs under `/home/ubuntu/research/yuv-gate/`:

- `nearest-plugin-check.log`: runner command
  `node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-ffmpeg --locked -- --nocapture`.
  Standard FFmpeg pkg-config prefix `/home/ubuntu/research/ffmpeg8-standard`;
  x264 prefix `/home/ubuntu/research/ffmpeg8-x264`.
  Earlier suites pass (98 tests); final YUV suite:
  `test result: FAILED. 8 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.11s`.
  Sole failure: `native_yuv_matches_independent_nearest_reference`.
- `nearest-gpu-check.log`: `cargo test --release -p celesta-gpu-renderer video_yuv -- --nocapture`,
  `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json`,
  `XDG_RUNTIME_DIR=/tmp`,
  `CELESTA_TEST_FFMPEG=/home/ubuntu/repos/Celesta/plugins/target/debug/celesta-codec-ffmpeg`.
  `test result: FAILED. 8 passed; 1 failed; 1 ignored; 0 measured; 438 filtered out; finished in 6.66s`.
  Sole failure: `ffmpeg_fixtures_match_independent_nearest_reference`.
- `nearest-plugin-clippy-check.log`: plugin runner clippy with
  `--locked --all-targets -- -D warnings`:
  `Finished dev profile [unoptimized + debuginfo] target(s) in 6.79s`.
- `nearest-gpu-clippy-check.log`: `cargo clippy -p celesta-gpu-renderer --all-targets -- -D warnings`:
  `Finished dev profile [unoptimized + debuginfo] target(s) in 2.73s`.
- Root and plugin `cargo fmt --all --check`, and `git diff --check`: exit 0.

The reference tests pass, including normal, empty and malformed cases. The
new exact production gates deliberately remain failing rather than being
weakened. No new rendered appearance exists because production code is
unchanged; no new visual verification was performed.

## Existing remote CI and Devin Review

`git_pr_checks(wait_mode="all")` settled at 8 passed, 2 failed, 0 pending,
5 skipped, on the unchanged remote HEAD, not on these uncommitted tests.
Logs still show the old max-2 swscale failures:

- macOS job `137152134713`: `yuv-odd-chroma.mp4: max RGB difference 3`.
- Windows job `137152134606`: `yuv-native-nv12.nut: max RGB difference 3`.

AVFoundation: 78 passed; MediaFoundation: 127 passed. All root platform jobs
pass. These are historical-head CI results, not validation of the max-4 update.

`git_view_pr` reports Devin Review at `2026-11-22T06:48:38+00:00`, state
`COMMENTED`: **No Issues Found**. The other bot comment is monitoring controls,
not a code finding. No review finding was acted on.
