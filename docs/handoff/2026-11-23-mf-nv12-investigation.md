# PR 888: Windows NV12 investigation

## Specification and outcome

Investigate the 64x36 H.264 exact-plane mismatch and the unsupported odd-sized
MJPEG fixture without relaxing either pixel gate. Log native decoder/reader
types and pitch in Windows CI, remove temporary diagnostics afterward, and
stop after two investigation rounds if the cause cannot be established.

The odd-size test now capability-skips only the known RGB32 negotiation error
`0xC00D36B4`; unexpected errors still fail. It exercises the fallback normally
on systems that can decode the fixture. An even-coded H.264 4:2:0 SPS crop
cannot express an odd display size because its crop units are even. Portable
tests cover odd width, odd height, both odd, zero dimensions, and valid even
dimensions. The tested predicate is used for both source and display-size
eligibility in the real decoder.

**The exact-plane mismatch is unresolved.** Investigation stopped at the
requested two-round limit. No fixture, equality assertion, own-RGBA tolerance,
or alpha assertion was changed. No speculative decoding workaround remains.
All temporary tracing, quality-setting experiments, direct diagnostic decode,
and the loop-filter-disabled reference helper have been removed.

## Round 1: native types, transforms and pitch

Signed diagnostic commit: `9dccc56f32186c4932451f4eca9b0b3520910538`.
Windows Plugins job: `137315526585`, Windows Server 2025, image
`windows-2025-vs2026` version `20261116.315.1`.

For `709-limited.mp4`, shared memory, prefetch enabled, t=0:

| Stage | Subtype | Frame size | Default stride | Minimum/geometric aperture |
| --- | --- | --- | --- | --- |
| Native compressed source | H264 | 64x36 | absent | both absent |
| Decoder output / reader after requesting NV12 | NV12 | 64x36 | 64 | both absent |
| Decoder output / reader after color negotiation | NV12 | 64x36 | 64 | both absent |
| Decoder output / available native type / reader after first sample | NV12 | 64x48 | 64 | both (0,0), 64x36 |

Matrix is 1 (709), range 2 (limited) throughout; negotiated chroma siting is
5 (MPEG2 left). The native aperture blobs after the initial stream change are
both `[0,0,0,0,0,0,0,0,64,0,0,0,36,0,0,0]`.

`IMFSourceReaderEx::GetTransformForStream` shows exactly one transform, category
`D6C02D4B-6833-45B4-971A-05A4B04BAB91` (`MFT_CATEGORY_VIDEO_DECODER`). Its input is
H264 and output is NV12. Index 1 returns `0xC00D36BF` (invalid index), both
before and after reading. There is no additional video processor/resizer.
The NV12 reader is opened without either basic or advanced video processing.

The actual lock/copy diagnostic is:

```text
MFTRACE Lock2DSize pitch=64 length=4608 offset=0 crop=Crop { width: 64, height: 36, coded_width: 64, coded_height: 48, left: 0, top: 0 }
```

The sample therefore already uses the decoder's native coded height. The
existing stream-change handler refreshes the layout before copying. The UV
plane starts at 64*48, not 64*36, and the copier strips the padding. These
observations contradict the proposed inserted-resizer/wrong-height explanation
for this run; requesting NV12 did not leave the sample at a scaled 64x36.

## Round 2: loop-filter comparison and quality controls

Signed diagnostic commit: `56ee23933edac361e407cfed884a043b067e256c`.
Windows Plugins job: `137319822386`, same OS/image version. The temporary
reference decoded the same input's first frame directly with libavcodec,
setting `AVCodecContext::skip_loop_filter = AVDISCARD_ALL`, then tightly packed
its original I420 planes as NV12. The real exact gate still used the ordinary
FFmpeg plugin answer, unchanged.

```text
MFTRACE FFmpeg skip_loop_filter=all max=0 mean=0.000000000 differing=0
planes ...\709-limited.mp4 SharedMemory t=0: max=4 mean=0.638310185 differing_bytes=1632
assertion `left == right` failed: Media Foundation/FFmpeg decoded planes differ
```

This establishes output equivalence to FFmpeg with its loop filter disabled
for this frame, not the internal MF setting or why it occurs. Quality/control
queries immediately after negotiation returned:

```text
quality: 0x80004005 "Decoder not initialized yet"
drop mode: 0x80004005 "Unspecified error"
CODECAPI_AVDecVideoFastDecodeMode: 0x80004001 "Not implemented"
CODECAPI_AVDecVideoThumbnailGenerationMode: VT_UI4, 0
CODECAPI_AVEnableInLoopDeblockFilter: 0x80004001 "Not implemented"
CODECAPI_AVDecVideoSWPowerLevel: 0x80004001 "Not implemented"
```

The conditional normal-quality/fast-mode experiment made no changes because
no degraded supported setting was returned. The quality level after decoder
initialization was **not checked**. Whether MF internally bypasses deblocking,
has a conformance issue at this size, or exposes a usable post-initialization
quality setting remains unverified. No root cause or successful fix is claimed.

## Gate results and validation

Both Windows rounds: Media Foundation **140 tests, 139 passed, 1 failed**.
The odd fixture reports its unsupported RGB32 capability skip. The 1080p exact
plane and own-RGBA test passes. For the failing 64x36 first frame, the own-RGBA
gate is not reached. Tagged/untagged/full-range later cases in that test are
not reached either.

| Plane | Maximum error | Mean error | Differing bytes |
| --- | ---: | ---: | ---: |
| Y | 4 | 0.903211806 | 1507 / 2304 |
| UV | 1 | 0.108506944 | 125 / 1152 |
| All | 4 | 0.638310185 | 1632 / 3456 |

The portable MF library run after adding the dimension policy tests reported
`70 passed; 0 failed` (0.31s). Windows API and gate-helper Clippy audits passed
for both diagnostic revisions; those isolated audits do not establish native
parity. Both native CI rounds passed plugin compilation and Clippy, macOS and
Linux plugin suites, formatting and the root Linux/macOS tests. Final cleanup
checks/CI are reported with the handoff rather than claimed green here.

Full diagnostic logs and signing records are retained outside the repository:
`/home/ubuntu/research/native-yuv/round{1,2}-windows-job.log`,
`round{1,2}-signing.log`, and `round{1,2}-signed.json`.

Recommended next authorized action: probe `IMFQualityAdvise` after the first
sample/initial stream change, then test normal-quality replay from the start
if it reports a degraded level, with the same exact gate. This is not yet
performed. Native before/after screenshots remain unavailable on this Linux
box; the outstanding Devin Review screenshot finding is not resolved.
