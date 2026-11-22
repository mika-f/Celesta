# PR 888: exact MF decoder references

## Specification and reason

Owner decision: the MF plane gate must require exact equality to either the
normal FFmpeg decode or a decode with `skip_loop_filter=all`. Log the match.
No intermediate tolerance is allowed. AVFoundation stays exact against normal
FFmpeg. Every eligible MF fixture must still pass the independent comparison
against MF's own RGBA answer, max <=4/channel with exact alpha, including
64x36. Stop and report if that RGBA gate fails. No quality investigation.

Windows jobs `137315526585` and `137319822386` showed a single decoder, no
inserted processor, and a correct 64x48 coded allocation cropped to 64x36.
The first 64x36 MF frame differed from normal FFmpeg by max 4, mean
0.638310185, 1632/3456 bytes, but matched FFmpeg with loop filtering disabled
at max 0, mean 0, differing 0. The RGB32 path uses the same decoder. The owner
therefore attributes this difference to the decoder, not our packing/crop.

## Implementation

- Only `plugins/mediafoundation/tests/support/yuv_decode.rs` changes the gate.
  The existing FFmpeg plugin answer supplies the normal planes and unchanged
  dimension/color checks. A test-only libavcodec decoder sets
  `AVCodecContext::skip_loop_filter = AVDISCARD_ALL` before opening, drains
  packets and delayed frames, selects the displayed frame by timestamp with
  end-of-stream clamping, and packs its original I420 planes into tight NV12.
  Full-range YUVJ420P is also accepted without conversion.
- Both complete byte vectors are compared exactly. Normal is logged when both
  match. Otherwise only exact equality to the unfiltered vector passes. Tests
  cover either match, normal preference, mixed/nonmatching bytes, empty data
  and wrong length. The RGBA/alpha gate and both call sites remain required.
- `plugins/.config/nextest.toml` exposes successful output only for the MF
  H.264, optional HEVC, and 1080p gate tests, so their fixture/transport/time
  match logs are visible in CI rather than hidden in unuploaded JUnit output.
- No production decoder, AVFoundation test, pixel conversion, protocol,
  fixture, or RGBA tolerance changes.

## Checks

Local helper validation uses the existing Linux audit with real FFmpeg/media
crates and native-codec scaffolding. Its additional check compares the raw
normal decoder against the FFmpeg plugin across limited/untagged/full-range
and sequential fixtures, timestamps and EOF. This does not establish Windows
parity. Local nextest is not installed; Cargo/helper tests are used locally,
and the profile override is exercised by native CI. Final native gate results,
reference matches and signed SHA are reported in the handoff response.
