# FFmpeg encode directly from shared slots

## Specification

Remove the plugin-side shared-slot-to-Vec allocation and full-frame copy
when FFmpeg can copy the negotiated input directly into its owned AVFrame.
Keep the host and wire protocol unchanged, validate generations before and
after the copy, and preserve the Vec path for other plugins and conversions.

## Implementation

- `crates/plugin/src/shared.rs`: unsafe `Ring::copy_slot_with` allows only
  synchronous raw-pointer copies into owned memory, never references into
  the mapping or retained source pointers. Bounds, length, even generation,
  acquire load/fence and final generation check match `read_slot`.
- `crates/media/src/provider.rs`, `lib.rs`, `protocol.rs`: the plugin-side
  dispatcher passes `VideoFrameSlot` to a defaulted `VideoEncoder` method.
  Its default materializes the same Vec as before; AVFoundation,
  MediaFoundation, and the test codec need no changes. Generation failures
  retain the existing error text. No requests, replies, host logic, ring
  reservation, publication or acknowledgement timing change.
- `plugins/ffmpeg/src/{lib,video_encoder}.rs`: both profiles share the
  override. For same-format inputs, `av_image_fill_arrays` computes source
  planes and `av_image_copy` copies directly into a writable, owned AVFrame.
  Only after the second generation check succeeds is the frame submitted
  to libavcodec. On failure the unsubmitted contents are discarded; the
  writable buffer can be fully overwritten by a later valid frame.
- Direct inputs today: libx264 YUV420P, YUV420P10, NV12. RGBA conversion and
  P010 repacking keep the Vec/swscale path: `Scaler` builds a reference-
  counted source AVFrame and is deliberately not given mapped memory.
  Standard FFmpeg ProRes and VP9 currently negotiate RGBA16, so they retain
  the conversion path. Pipe input is unchanged. This is not zero-copy:
  there remains one plugin-side copy into the native frame.
- `plugins/ffmpeg-x264/examples/codec-bench.rs`: optional
  `CELESTA_BENCH_INPUT=yuv420p` exercises same-format input. The default is
  still RGBA8; the negotiated input is included in the transport label.

## Checks

Rust coverage:

- Shared-memory copies equal Vec reads, including empty data. Invalid
  indices, capacity, stored length, odd/stale generations are refused.
  A deterministic rewrite during the callback proves the second generation
  check rejects the copied data.
- Media preserves the existing generation error after such a rewrite;
  existing dispatch and test-codec process tests cover default Vec behavior.
- Real x264 process tests compare packets and track config byte-for-byte
  against pipe/owned input, pipelined three deep, for YUV420P, YUV420P10,
  NV12 and fallback P010/RGBA8/RGBA16. Empty same-format input is rejected,
  and valid frames can still be encoded afterwards.

Native libraries: FFmpeg 8.1.3 built with the repository script into separate
`~/research/ffmpeg8-standard` and `~/research/ffmpeg8-x264` prefixes. Select
them with `PKG_CONFIG_PATH` and `FFMPEG_X264_PKG_CONFIG_PATH` respectively;
the standalone codec-bench links the x264 prefix. Linux execution only;
Apple/Windows plugins are source-compatible via the default method but
were not run. No rendering/UI behavior changes or visual checks.

Executed root checks:

```text
cargo test -p celesta-media -p celesta-plugin
test result: ok. 145 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.04s
test result: ok. 51 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
cargo test --release -p celesta-codec-test --test process
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
cargo clippy -p celesta-media -p celesta-plugin --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.92s
```

Both workspace `cargo fmt --all -- --check` commands and `git diff --check`
completed without errors. With the prefixes above and lavapipe selected:

```sh
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json XDG_RUNTIME_DIR=/tmp \
  node scripts/plugins/run-cargo.mts plugins test \
  -p celesta-codec-ffmpeg -p celesta-codec-ffmpeg-x264 --locked
node scripts/plugins/run-cargo.mts plugins clippy \
  -p celesta-codec-ffmpeg -p celesta-codec-ffmpeg-x264 --all-targets --locked -- -D warnings
```

All standard-plugin tests passed (97 tests across 8 nonempty suites), as did
all x264 tests (33 across 4 suites), with no failures or ignored tests.
The x264 process suite includes the new direct/fallback byte-equivalence
and empty-frame recovery cases:

```text
test empty_slot_frame_is_refused_without_consuming_an_encoder_frame ... ok
test slot_direct_yuv_packets_match_the_owned_path ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
```

Plugin Clippy finished without warnings for both packages:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 25.64s
Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.53s
```

Checks were targeted to the changed crates and the requested plugin/process
suites, not the full root workspace or native macOS/Windows suites.

## Measurements

Baseline executables were built from an untouched detached worktree at
`origin/main`, `a56c182a9bf0091972ed6135d23aaa90cea85318`, with the same two
FFmpeg prefixes. The after executables were staged from this branch.
Both were driven by the same release `codec-bench` harness (with the new
optional input selector); the in-process encoder never uses the slot
override. All measurements below are the **external encode** line, not
decode or in-process timing. No compilations/tests ran during measurement.

120 timed frames per run, one warmup, ultrafast x264, synthetic 30 fps
fixtures at 1920x1080 and 3840x2160. Three interleaved before/after runs per
size and format; independent medians below. Every run reported
`(Rgba8, ring, 3 deep)` or `(Yuv420p, ring, 3 deep)`. Wall timing includes
host frame cloning and finish; plugin CPU includes startup and warmup,
as the existing benchmark documents. RGBA and YUV use different patterns;
compare before/after within a row, not performance across formats.

| Input | Size | Main wall ms/frame | After wall ms/frame | Main plugin CPU ms/frame | After plugin CPU ms/frame |
| --- | --- | ---: | ---: | ---: | ---: |
| RGBA8 (Vec/swscale unchanged) | 1920x1080 | 3.90 | 3.80 | 14.13 | 14.12 |
| RGBA8 (Vec/swscale unchanged) | 3840x2160 | 13.21 | 13.14 | 55.29 | 57.08 |
| YUV420P (direct slot copy) | 1920x1080 | 1.73 | 1.77 | 6.98 | 6.88 |
| YUV420P (direct slot copy) | 3840x2160 | 5.98 | 5.88 | 29.36 | 28.86 |

This demonstrates removal of a known copy/allocation, **not a significant
end-to-end throughput improvement** on this machine. YUV wall medians are
2.3% worse at 1080p and 1.7% better at 4K, with overlapping run ranges:
1080p main 1.73–1.83 / after 1.70–1.94; 4K main 5.92–6.11 / after 5.58–6.48
ms/frame. Plugin CPU medians improve by 0.10 and 0.50 ms/frame respectively,
also small relative to noise and encoder work. The unchanged RGBA path
shows measurement variation too.

Reproduction (paths outside the repository are machine-local):

```sh
FFMPEG_VARIANT=standard FFMPEG_JOBS=8 scripts/build-ffmpeg-linux.sh "$HOME/research/ffmpeg8-standard"
FFMPEG_VARIANT=x264 FFMPEG_JOBS=8 scripts/build-ffmpeg-linux.sh "$HOME/research/ffmpeg8-x264"
export PKG_CONFIG_PATH="$HOME/research/ffmpeg8-standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$HOME/research/ffmpeg8-x264/lib/pkgconfig"
node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264
PKG_CONFIG_PATH="$FFMPEG_X264_PKG_CONFIG_PATH" cargo build --release \
  --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg-x264 --example codec-bench --locked
# Generate each fixture with size=1920x1080 or size=3840x2160:
ffmpeg -y -f lavfi -i testsrc2=size=1920x1080:rate=30 -t 5 \
  -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$HOME/research/input-1080.mp4"
# DIST is either the main-worktree or after-build plugins/dist directory.
CELESTA_CODEC_SHARED_MEMORY=1 CELESTA_BENCH_INPUT=yuv420p \
  plugins/target/release/examples/codec-bench \
  "$DIST/ffmpeg/bin/linux-x86_64/celesta-codec-ffmpeg" \
  "$DIST/ffmpeg-x264/bin/linux-x86_64/celesta-codec-ffmpeg-x264" \
  "$HOME/research/input-1080.mp4" 120
# Repeat with CELESTA_BENCH_INPUT=rgba8 and the 4K fixture.
```

Full build, test and benchmark logs remain under `~/research/` as
`ffmpeg-{standard,x264}-build.log`, `main-plugins-build.log`,
`slot-{plugins-build,bench-build,bench-results,root-tests,codec-test,root-clippy,plugin-tests,plugin-clippy}.log`.
