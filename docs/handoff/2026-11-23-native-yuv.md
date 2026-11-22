# Native decoded YUV — step A: AVFoundation

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
