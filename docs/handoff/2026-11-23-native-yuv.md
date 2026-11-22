# Native decoded YUV — AVFoundation and Media Foundation

## Specification

One branch for AVFoundation now and Media Foundation in the next handoff.
No host/GPU/protocol changes. AVFoundation answers only NV12, and only when
offered, for even-sized, unrotated/unmirrored 8-bit 4:2:0 SDR H.264/HEVC Main
without alpha and with supported BT.601/709, range and left chroma siting.
Everything else retains the existing BGRA reader and RGBA conversion.

## Implementation

- `plugins/avfoundation/src/decoder.rs`: reads `avcC`/`hvcC` records,
  `ContainsAlphaChannel`, source colour/range/chroma extensions and the
  original preferred transform. `params.rs` checks AVC SPS chroma/depth and
  HEVC Main configuration plus SPS chroma/depth. Unknown records fall back.
- Requests `420v` or `420f` according to source range. NV12 output has no
  BT.709 output-colour dictionary. The delivered format, plane count,
  dimensions, strides and attachments are validated. Unexpected delivery
  returns an error rather than labelling converted or RGB bytes as NV12.
- Per-frame CVBuffer matrix/primaries/transfer/chroma attachments take
  precedence over source tags. Range is encoded by the verified actual
  CoreVideo pixel format; a FullRangeVideo attachment, if present, must
  agree. Untagged matrix is BT.601, untagged range limited, like FFmpeg.
  Only left/unspecified chroma is supported. No new HDR/alpha semantics.
- `celesta_media::pack_nv12`: shared safe positive-stride Y/UV packer for
  Media Foundation to reuse. Tests cover tight/padded planes, omitted final
  padding, empty/odd/truncated/short-stride and overflowing layouts.
- Sequential keys include the complete accepted-format vector. Shared-slot,
  pipe and host prefetch use existing protocol machinery unchanged.
- EN/JA plugin guides and agent skill describe current YUV-capable plugins.

## Native tests

`plugins/avfoundation/tests/decode.rs` includes `support/yuv_decode.rs`.
VideoToolbox generates eight-frame H.264 and optional HEVC 8-/10-bit files.
FFmpeg's h264_metadata bitstream filter creates truly unspecified and
full-range VUI/container tags without a CLI or x264 requirement. Its small
test-only C declarations match FFmpeg 8.1's public `libavcodec/bsf.h`, which
ffmpeg-sys-next does not include. ProRes covers odd dimensions and alpha;
the existing writer helper generates rotated H.264. Optional codec failures
print skip reasons; mandatory H.264 failures fail the test.

Both transports run with prefetch, changed/missing/RGBA-only offers, seeks
and EOF. NV12 bytes must exactly match FFmpeg's decoded I420 planes after
U/V interleaving (or its NV12), including colour metadata. NV12 `to_rgba()`
versus AVFoundation's own RGBA answer reports max/mean/differing and asserts
max <=4 with exact alpha. No tolerance relaxation or FFmpeg NV12 exception
is inherited. These native gates have **not run on Linux**; compatibility
numbers, Apple compilation and actual output remain unverified pending macOS.

## Validation

Linux checks:

```sh
cargo check --manifest-path plugins/Cargo.toml -p celesta-codec-avfoundation -p celesta-codec-mediafoundation --all-targets --locked
cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-avfoundation --lib --locked
cargo test -p celesta-media --locked
cargo clippy --manifest-path plugins/Cargo.toml -p celesta-codec-avfoundation -p celesta-codec-mediafoundation --all-targets --locked -- -D warnings
cargo clippy -p celesta-media --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo fmt --manifest-path plugins/Cargo.toml --all --check
git diff --check
```

Host all-target checks passed (native modules excluded). AVFoundation's
portable library tests: **24 passed**, including colour policy and SPS
eligibility. Media tests: **151 passed**, plus 0 doctests. Both Clippy
commands passed with warnings denied. Formatting and diff checks exited 0.

Additional audits, outside the repository at
`/home/ubuntu/research/native-yuv/`:

- Linux compiled and linted the actual native gate helper using a scaffold
  for its parent module (not AVFoundation execution). The same retag helper
  was executed against the existing FFmpeg sequence fixture: all three
  matrix/range combinations matched expectations and all decoded plane
  bytes were unchanged (**1 audit test passed**).
- Apple-target metadata-only API audit included the actual decoder,
  params, colour policy and media frame types against real objc2 0.3.2
  bindings; its check and Clippy (`-D warnings`) passed. This uses lightweight media error/request/function
  scaffolding and `DOCS_RS=1` to skip objc2-exception-helper's native C
  build, as its build script documents. It is **not** a full plugin build,
  framework link or runtime test.

No macOS SDK is configured on this box. Added CoreVideo/CoreMedia API names
and safety contracts were also checked against objc2 registry sources.
No before/after screenshots can be rendered here with AVFoundation; this
decode feature is visually unverified. No PR is opened in step A.

On macOS, run the actual suites using the repository runner (which builds
the independent FFmpeg sibling programs first):

```sh
node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-avfoundation --locked -- --nocapture
```

Native gate numbers must be inspected there before claiming pixel parity.

## Step B: Media Foundation

### PR 888 review correction: primaries and transfer

Confirmed the PR head was cff8ed5 before changing it. Native NV12 now
requires primaries absent/BT.709/SMPTE170M/BT470BG, and transfer absent/
BT.709/SMPTE170M. P3, linear, sRGB, gamma and unknown colour descriptions
use the original RGBA path, preserving its colour conversion. Both native
plugins share this policy; Media Foundation also checks the SPS VUI when
attributes omit its source colour. No pixel gate is weakened.

Portable tests cover accepted, absent and rejected tags and AVC/HEVC VUI
descriptions. Native fallback fixtures retag both MP4 metadata and H.264
VUI to P3-D65 or linear using the linked FFmpeg h264_metadata filter,
verify those tags with FFmpeg, then compare NV12 offers with legacy RGBA
over both transports. EN/JA guides and the skill document the restriction.

Review-fix checks on Linux: AVFoundation portable tests 25 passed;
Media Foundation portable tests 69 passed. Both-plugin Clippy, native-test
helper metadata checks, Windows API audit (with a live decoder entry
point), plugin formatting and diff checks passed. Fixture-only tests for
each helper verified the new P3/linear tags with linked FFmpeg; native
fallback execution is deferred to PR 888 CI.

The same branch adds offered NV12 for even-sized, upright, alpha-free
8-bit 4:2:0 SDR H.264/HEVC Main. Unknown headers, unsupported source colour,
range or chroma siting retain the RGB32 path. No host/GPU/protocol changes.

- `plugins/mediafoundation/src/nv12.rs` reuses existing `annexb`, `params`
  and `vui` readers to check all SPSs in Annex B or avcC/hvcC headers.
  Auxiliary HEVC layers, 10-bit/non-420 SPSs, HDR and unsupported matrices
  or chroma locations are not native. Untagged matrix is 601, range limited.
  VUI range is read even without a colour description; source attributes
  take priority, with VUI matrix/range as a fallback when unspecified.
- `mf/decoder.rs` probes as before. An NV12 offer first opens its reader
  without advanced video processing (basic processing is off by default),
  and requests NV12 with source-consistent nominal range, matrix and MPEG2
  left chroma siting. The current type must confirm those attributes and
  NV12; the response color is read from it. Unexpected negotiation errors
  rather than returning falsely labelled pixels. RGB32 setup/conversion
  for ineligible offers/sources is retained; an ineligible NV12 candidate
  reopens the processed reader, so eligible NV12 opens only one reader.
- Locks the original single sample buffer, preferring IMF2DBuffer2's bounded
  Lock2DSize, then IMF2DBuffer::Lock2D, then IMFMediaBuffer::Lock. Positive
  pitches and extents are checked; every successful lock is unlocked on
  success and error. Legacy 2D memory is conservatively bounded by
  GetMaxLength; an insufficient bound errors instead of guessing.
- `MF_MT_FRAME_SIZE` supplies allocation dimensions. Minimum display
  aperture, then geometric aperture supplies the crop; if absent, the
  probed display size is used. The UV offset is pitch times **coded height**,
  not display height. Fractional/odd/invalid apertures are rejected. The
  probe now observes display apertures as well. The shared pack_nv12 strips
  pitch and allocation padding. Linux tests explicitly exercise 1088 rows
  cropped to 1080 and nonzero even crop offsets.
- Accepted formats join the sequential session key. Each buffered sample
  captures its native layout so a later media-type change cannot reinterpret
  an already buffered frame with the next frame's layout.

Windows-only tests in `plugins/mediafoundation/tests/support/yuv_decode.rs`
generate tagged/untagged/full-range H.264, optional HEVC 8-/10-bit, rotated
H.264, an odd 63x35 uncompressed AVI, and alpha ProRes 4444. HEVC reports
encoding/decoding capability skips (including a missing codec extension);
10-bit tries the existing hardware-test size, 160x90. ProRes alpha reports
a skip when Windows has no decoder for it. Odd AVI and H.264 are mandatory
when the suite's existing Media Foundation capability gate is available.

Both transports, prefetch, offer changes, RGBA-only/I420-only/empty offers, seeks
and EOF follow step A. A separate 1920x1080 fixture exercises decoder
alignment. Gates remain exact plane/color equality with linked FFmpeg,
and max <=4 against this plugin's own RGBA with exact alpha; max/mean/
differing are printed. The new gate helper also fixes the missing Decoder
trait import in step A's helper. EN/JA guides and the skill now describe
both native plugins.

### Step B validation on this Linux box

- Plugin all-target check: passed (1.35s).
- Media Foundation portable library tests: **67 passed**, 0 failed (0.12s).
- Both-plugin all-target Clippy, `-D warnings`: passed (0.70s).
- Plugin formatting and `git diff --check`: passed.
- The unchanged media crate retains step A's **151 passed** result and
  media Clippy pass (4.46s); root formatting and the unchanged AVFoundation
  portable library retain their step A passes. These were not rerun.
- An isolated Windows-target audit includes the **actual MF decoder and
  helpers** with real `windows 0.62.2` bindings; Clippy including its test
  configuration passed with `-D warnings` (0.55s). This substitutes shared
  media scaffolding and does not compile/link the full plugin, start MF,
  or validate any Windows runtime result. Checked the new attribute values
  against vendored bindings; MF_MT_ALPHA_MODE uses DXGI_ALPHA_MODE per
  Microsoft documentation (0 unspecified, 3 ignore).
- An isolated Linux audit compiles the actual new gate helper against the
  real media/FFmpeg crates, substituting the native codec with FFmpeg and
  parent native-only helpers with scaffolding. Clippy passed.
  A separate fixture-only test passed: odd AVI is readable with the intended
  dimensions/pixels, and all three H.264 retags preserve planes and set the
  expected color (1 test, 0 failures, 0.03s).

Logs/audit sources: `/home/ubuntu/research/native-yuv/mf-*` and
`/home/ubuntu/research/native-yuv/windows-api/`.

Full Windows builds/runtime and native gate numbers require a Windows
runner (the full cross-target dependency build needs MSVC's lib.exe/SDK).
Neither Windows pixel parity nor HEVC codec-extension behavior has been
measured here. No native screenshots; visually unverified. No PR is opened.

Native runner command:

```sh
node scripts/plugins/run-cargo.mts plugins test -p celesta-codec-mediafoundation --locked -- --nocapture
```
