# Opaque VP9 GPU YUV readback

## Specification and implementation

Reuse the existing GPU YUV420P readback for opaque FFmpeg VP9 exports, without
changing the media protocol, shaders, default alpha policy, or ProRes.
Base: `origin/main` at `a1402a9ac3a825885c4b30621de69c4d0df23c7e`.

- `plugins/ffmpeg/plugin.json` and `src/components.rs` advertise VP9's
  `yuv420p`, then `rgba16`, in both the manifest and runtime description.
- `src/video_encoder.rs::plan` uses the existing validated `alpha` setting.
  Opaque VP9 prefers YUV420P; default Keep alpha still requires RGBA16 and
  encodes YUVA420P. Opaque fallback remains RGBA16. ProRes is unchanged.
- Matching YUV input reuses `VideoSession::encode`'s existing AVFrame plane
  copy, exactly as x264 does: no scaler is allocated. It remains 8-bit 4:2:0
  VP9 profile 0 (`vp09.00.10.08`). The existing color application is unchanged;
  export supplies BT.709 limited range. Tests check emitted tags against RGBA.
- `crates/exporter/src/encode.rs::offered_formats_with` makes the host's
  capability/software/environment policy directly testable without a physical
  GPU. Supported non-software renderers offer the existing YUV formats first.
  Software or unsupported renderers retain the existing RGBA offers.
- **`CELESTA_EXPORT_YUV=0` disables YUV offers for all GPU-export encoders.**
  VP9 falls back to RGBA16; 8-bit x264 to RGBA8, 10-bit x264 to RGBA16.
  Preserving both RGBA offers also preserves RGBA8-only third-party encoders.
  Only exactly `0` opts out; unset/empty/other values preserve negotiation.
  Decoding is unaffected. EN/JA guides and the skill reference document it.

## Rust checks

Run from the repository root, with FFmpeg **8.1.3** at the existing standard
prefix (libavcodec **62.28.103**):

```sh
PKG_CONFIG_PATH=/home/ubuntu/research/ffmpeg8-standard/lib/pkgconfig \
  cargo test --release --locked --manifest-path plugins/Cargo.toml \
  -p celesta-codec-ffmpeg --lib --test encoders --test manifest -- --nocapture
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json XDG_RUNTIME_DIR=/tmp \
  cargo test --release --locked -p celesta-exporter --lib
PKG_CONFIG_PATH=/home/ubuntu/research/ffmpeg8-standard/lib/pkgconfig \
  cargo clippy --release --locked --manifest-path plugins/Cargo.toml \
  -p celesta-codec-ffmpeg --all-targets -- -D warnings
cargo clippy --release --locked -p celesta-exporter --all-targets -- -D warnings
cargo fmt --all -- --check
cargo fmt --all --manifest-path plugins/Cargo.toml -- --check
git diff --check
```

Results:

```text
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 224 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s
```

Both Clippy commands completed successfully (59.45s plugin, 1m 00s exporter);
both formatting checks and the whitespace check exited 0.

Coverage includes direct host policy with simulated hardware/software support
and exact-zero/other/unset settings (without mutating process environment),
opaque preference/fallback, the validated alpha default, unsupported/empty
offers, unchanged ProRes input, no scaler for VP9 YUV, malformed frame rejection,
empty-session finish, and actual YUV encoding/decoding with FFmpeg 8.
The manifest and runtime description must agree.

The image test generates ten RGB gradients and independent BT.709 limited-range
4:2:0 planes with box-averaged chroma, comparing decoded VP9 against the same
images fed as RGBA16 through production swscale. It checks profile 0, all decoded
color fields against RGBA, BT.709 matrix/limited range, and PSNR >=40 dB for every
frame. Observed **47.290–51.852 dB**; existing alpha encode coverage also passes.

## CPU-side measurements

Artifacts, runner, raw CSVs and full logs:
`/home/ubuntu/research/vp9-yuv-readback/`.
The earlier temporary instrumentation lives separately in
`/home/ubuntu/research/gpu-export-profile-source/`; none is committed here.
Its encoder/description/manifest were synchronized with this implementation,
then stage timings were reinserted. No production profiling code was added.

```sh
python3 /home/ubuntu/research/vp9-yuv-readback/measure.py
```

Same earlier four GPU-produced rings images, now comparing **RGBA16** against
YUV420P generated from those exact RGBA16 inputs using FFmpeg 8 and production
swscale flags (bicubic, automatic threads, BT.709 full RGB to limited YUV).
Three paired trials, alternating format order, **48 frames per trial**, opaque
VP9/default CRF20/row-mt, actual shared-memory plugin process and three slots.
No build, rendering, or other benchmark ran concurrently. Means, ms/frame:

| CPU feed/encode wall | 1080p | 4K |
| --- | ---: | ---: |
| RGBA16 | 85.884 | 301.723 |
| YUV420P | 82.794 | 235.657 |
| Saved | 3.090 | 66.066 |
| Reduction | **3.60%** | **21.90%** |
| Individual paired reductions | -0.93%, 6.23%, 5.43% | 25.96%, 14.15%, 24.93% |

Service times below overlap and **must not be summed**:

| Stage | 1080p RGBA16 | 1080p YUV | 4K RGBA16 | 4K YUV |
| --- | ---: | ---: | ---: | ---: |
| Source Vec clone | 2.575 | 0.374 | 50.147 | 2.138 |
| Host ring write + plugin ring read | 4.366 | 0.747 | 62.328 | 3.941 |
| swscale | 1.958 | 0 | 7.494 | 0 |
| YUV AVFrame copy | 0 | 0.244 | 0 | 1.500 |
| Encode API/receive + amortized finish/frame pool | 80.623 | 81.479 | 230.107 | 229.965 |

Main output:

```text
SUMMARY width=1920 rgba16_ms=85.884268 yuv420p_ms=82.794005 saved_ms=3.090262 reduction_pct=3.598 paired_pct=[-0.9329343899720668, 6.2306182682980396, 5.428367757230348]
SUMMARY width=3840 rgba16_ms=301.723031 yuv420p_ms=235.656932 saved_ms=66.066099 reduction_pct=21.896 paired_pct=[25.962988200173807, 14.14913367104559, 24.934029944276322]
MEASUREMENTS_PASS: 12 runs; 48 packets each; stage counts correct; all paired packet digests match
```

Every YUV run recorded **zero swscale events**. All corresponding RGBA16/YUV
runs produced equal packet counts and aggregate packet-byte digests. This is
an integrity control using CPU-preconverted matching input, not a claim that
GPU box-filtered/dithered YUV is bit-exact to swscale.

**Limits:** lavapipe/llvmpipe executes GPU work on CPU, and production deliberately
uses RGBA there. Timed runs therefore isolate the CPU side and explicitly offer
each format. They include source cloning as the earlier harness did, exclude
render/readback/GPU-conversion/mux/audio, and are not complete export timings.
The unusually expensive 4K RGBA16 allocation/copies on this VM dominate the gain;
do not promise 22% on hardware GPUs or other allocators. 1080p is noisy. GPU
conversion/readback performance and full hardware export improvement remain
unverified. Swscale still shares the plugin request thread with encode submission;
codec workers, readback and other frames can overlap it.

## Before/after scene images

Inspected matching 1080p rings readbacks, generated once after timing finished:

- `/home/ubuntu/research/vp9-yuv-readback/visual/before-rgba16.png`
- `/home/ubuntu/research/vp9-yuv-readback/visual/after-gpu-yuv420p.png`

The after image forces the existing GPU YUV shader through the renderer API on
llvmpipe (normal host negotiation would refuse YUV on this software adapter).
The before image uses the captured RGBA16 readback of the same scene. These are
pre-encode readback images, not screenshots of a hardware export or desktop UI;
hardware GPU visuals remain unverified. System FFmpeg 4.4 only converted raw
readbacks to PNG; all encoding tests and timings above use FFmpeg 8 libraries.
No examples were superseded; no PR was opened.
