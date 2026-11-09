# Shared memory and pipelining, P2b (2026-11-08)

Phase P2b of the media components design
(`docs/superpowers/specs/2026-11-08-media-components-design.md`, #782;
plan `docs/superpowers/plans/2026-11-08-media-components-p2b.md`), on top
of P2a (`docs/handoff/2026-11-08-media-encoders.md`). An export's video
frames reach the video encoder's process through a ring of slots in
memory shared with it, as #775's decoded frames come back
(`docs/handoff/2026-11-08-codec-shared-memory.md`), and an export's encode
and write requests go several at a time through a send window, so
Celesta renders the next frames while the plugin encodes the earlier ones.
Without shared memory the window still pipelines, with the frames in each
request's payload. Exports are byte-identical to P2a's (see Checks).

## Specification

- **The ring** (`celesta_plugin::shared`). A segment of `slots` slots,
  laid out by `RingLayout { slots, slot_bytes }` (`slotBytes` on the
  wire): each slot is #775's seqlock region, a `SLOT_HEADER` of 64 bytes
  (an atomic `u64` generation, then a `u64` length) and `slot_bytes` of
  data; slot `i` starts at `i × stride`, where `stride = SLOT_HEADER +
  slot_bytes`, and the segment is `slots × stride` bytes.
  `SharedMemory::ring(layout)` sees a segment as a `Ring`, refusing a
  layout with no slots or no bytes, one longer than the segment, or one of
  more than one slot whose `slot_bytes` is not a multiple of 8 (slot `i`'s
  atomic generation would be misaligned). `Ring::write_slot` and
  `Ring::read_slot` are the single slot's seqlock writer and reader, per
  slot; bytes cross through raw pointers, and neither side holds a Rust
  reference into the segment.
- **Sizing** (`celesta_media::ring_layout`, `crates/media/src/ring.rs`). A
  slot holds the largest frame, at the export's size, of the formats in
  `PixelFormat::OFFERABLE` (`yuv420p`, `yuv420p10`, `rgba16`, `rgba8`) that
  the encoder takes, or an `rgba16` frame when none is known (deviation
  20), rounded up to a multiple of 8. A ring has `RING_SLOTS = 3` slots,
  fewer while the segment would exceed `MAX_RING_BYTES = 256 MiB`, and none
  when one slot does not fit (deviation 21): at 3840×2160 `rgba16`, three
  slots of 66,355,200 bytes; at 7680×4320 `rgba8`, two of 132,710,400; at
  7680×4320 `rgba16`, one of 265,420,800; at 8192×4320 `rgba16`, no ring.
  `PixelFormat::frame_bytes` saturates, so an absurd size gets no ring
  rather than overflowing (the export's size is resolved before its
  dimensions are validated).
- **Which formats size it** (`ExportSession::resolve`, Task 6's decision).
  A process started to describe its plugin during `resolve_output`, before
  the encoder is chosen, gets a ring sized from the union of its manifest
  video encoders' `inputFormats` (an encoder without them sizes for
  `rgba16`; a plugin with no video encoder gets none). A process started
  after `resolve`, as with a cached `describe`, gets one sized from the
  chosen encoder's described `inputFormats` (`ResolvedEncoder.input_formats`,
  ruling R3). The React path calls `resolve` after `bridge.metadata()`,
  since the ring needs the frame size, so its `output` progress event now
  follows the composition event.
- **Handing it over** (`MediaProcess::start_with`, `ProviderUse::Export {
  ring: Option<RingLayout> }`). A process started for an export with a
  ring is offered a segment of `layout.segment_bytes()` in `hello`, as
  #775's decode slot is; one that accepts it (`"frames": "sharedMemory"`)
  keeps it as a `FrameRing`. `Describe` processes and exports without a
  ring are offered none. `CELESTA_CODEC_SHARED_MEMORY=0`
  (`FrameTransport::from_environment`) offers no process any segment: no
  decode slot, no ring.
- **Declaring it** (deviation 27). `openVideoEncoder` carries `"ring": {
  "slots", "slotBytes" }` only when the process accepted a segment and the
  chosen video encoder's described `transports` lists `sharedMemory`
  (`ResolvedEncoder.transports`). Otherwise `ExportSession` calls
  `Pipeline::decline_ring` before the open, and the frames go as payloads.
  The serving side (`celesta_media::protocol`) checks a declared layout
  against its segment through `SharedMemory::ring` before any arithmetic,
  and fails the open (`MediaError::Codec`, "the ring of … does not fit the
  shared segment of … bytes") when it does not fit, or when a ring is
  declared to a process that accepted no segment. The FFmpeg plugin's and
  the test codec's video encoders describe `transports: ["sharedMemory",
  "pipe"]`. The opt-out is the exporter's: a direct user of `MediaProcess`
  (the FFmpeg plugin's and the test codec's tests, `codec-bench`) gets the
  ring declared unless it calls `decline_ring`, which is right for every
  current caller, whose encoders all describe `sharedMemory`.
- **Frames in slots** (`FrameRing`, `crates/media/src/process.rs`).
  Celesta writes a frame into a free slot under that slot's next
  generation (its last + 2, so even, from 0), takes the slot, and sends
  `encodeVideoFrame` with `"slot": { "index", "generation", "bytes" }` and
  no payload. The slot is free again only once the answer to that request
  is received (`videoFrameAccepted` or an `error`, which means the plugin
  is done with the frame; any other failure stopped the process). A frame
  larger than a slot, or sent with no slot free, goes in the payload. On
  the serving side, the serve loop copies the frame out of the named slot
  through the seqlock's reader (deviation 19) and refuses a slot that does
  not hold that generation, an index past the ring, or a slot named
  without a declared ring, each as `MediaError::Codec`.
- **The send window** (`PluginChild`, `crates/plugin-host/src/child.rs`).
  `send(request, expected, timeout, from)` writes a request without
  waiting; `receive()` takes the answer to the oldest one in flight;
  `in_flight()` counts them; `request` is `send` then `receive`. Answers
  come back in order. Each request's deadline counts from its send
  (`DeadlineFrom::Sent`) or, for `encodeVideoFrame`, `encodeAudio`, and
  `writePackets`, from the later of its send and the previous answer
  (`DeadlineFrom::PreviousAnswer`; `last_answered` is when the host
  received it). An answer of another kind, or a missed deadline, kills the
  process, and every request still in flight then fails without waiting;
  an `error` answer leaves it running.
- **The pipeline** (`celesta_media::Pipeline`, `MediaProvider::pipeline`).
  `MediaProcess` sends `send_video_frame`, `send_audio`, and
  `send_packets` without waiting and answers them through `receive` as
  `Answered::{VideoFrame, Audio, Packets}`. `depth()` is the ring's
  `slots` once frames go through it, or, otherwise, `pipe_depth` of the opened encoder's
  frame: `clamp(MAX_RING_BYTES / frame_bytes, 1, PIPE_WINDOW)` with
  `PIPE_WINDOW = 3` (deviations 23 and 28): one for 8192×4320 `rgba16`,
  three for frames of up to about 89 MB. Every other request (`describe`,
  the `open`s, `finish`es, `abort`s, and the one-at-a-time encode and
  write methods) is refused while anything is in flight.
  `frames_in_ring()` says whether frames go through the ring: it was
  declared and the opened input's frame fits a slot (a declared ring
  whose slots are too small for the chosen input is not used, and frames
  go as payloads with the pipe's window).
- **The export's flow** (`ExportSession`, `crates/exporter/src/encode.rs`).
  One process per plugin. Frames, audio chunks, and batches of packets go
  up to each process's depth at a time (`make_room` receives answers until
  another fits); every other request to a process first receives all it
  owes (`drain`). Answers are taken in order: a frame's packets go to the
  interleaver, an audio chunk's wait for the rest of the mix, and each
  track keeps its first config. An in-process provider (no pipeline)
  answers as it is sent, its answers waiting in a queue, so the session
  treats both alike. The stage of each request in flight is kept with it,
  so an `error` answer is reported as "video encoding", "audio encoding",
  or "muxing" by what it answered.
- **Batching.** Ready packets go to the muxer as one `writePackets` per
  `WRITE_BATCH_PACKETS = 8` packets or `WRITE_BATCH_BYTES = 4 MiB`, and the
  rest at `finish`, so a window carries mostly frames. One request never
  exceeds P2a's per-request ceilings, renamed `WRITE_REQUEST_BYTES = 32 MiB`
  and `WRITE_REQUEST_PACKETS = 16 × 1024` (ruling R2; a single larger
  packet still goes alone); a pending batch is sent before adding packets
  would pass either.
- **Abort** (deviation 22). `ExportSession::abort` sends `abortEncoder` /
  `abortMuxer` only to processes with nothing in flight; a process with
  requests in flight is dropped (its stdin closes, and it is killed half a
  second later), its sessions with it. The staged output and the spill
  file are Celesta's own and are removed either way.
- **The timing line.** `CELESTA_EXPORT_TIMING=1` has an export print one
  line on stderr:

  ```text
  celesta: timing: render 4.50 s for 300 frames; mix 1.25 s, done at frame 87; staged peak 12.0 MB; spilled no; frames ring 3 deep
  ```

  The render loop's time from its first frame to the last one it sent to
  the encoder, and the frames sent; the mix's time and the frames sent when
  the render took it (`done after the render` when every frame was sent
  first; `mix none` without a mix); the interleaver's peak staged bytes over
  the whole export, `finish`'s packets included, in megabytes of 10^6
  bytes; whether it spilled; and (ruling R12) how the video encoder's
  process takes the frames and how many at a time: `ring` (the segment was
  accepted, the ring declared, and the opened input's frame fits a slot),
  `pipe` (payloads), or `in process` (1).
- **`codec-bench`** (`plugins/ffmpeg/examples/codec-bench.rs`) encodes
  through the plugin's ring, pipelined, and prints how the frames went
  (from `frames_in_ring()` after the open, ruling R8);
  `CELESTA_CODEC_SHARED_MEMORY=0` compares the pipe, pipelined. Its encode
  leaves CRF at the default 18, as P2a's does (`main`'s used 23).
- **Decoding is unchanged**: #775's single slot of `SHARED_FRAME_BYTES`,
  one request at a time. A decoding process is never an export process.

## Deviations from the spec

Numbered after P2a's 8–18 and 24–26; recorded in the spec's Transport,
Deadlines and the window, Audio, and Phases sections. P2c's start at 30.

19. The plugin copies each frame out of its slot before encoding, one
    `memcpy` through the seqlock's reader, rather than encoding straight
    from the mapping: libavcodec copies a non-refcounted frame on
    `avcodec_send_frame` anyway, so encoding from the mapping would save
    nothing without an `AVBufferRef` over shared memory. The slot is still
    held until `videoFrameAccepted`.
20. The ring is sized from the formats Celesta can offer at all
    (`PixelFormat::OFFERABLE`) intersected with the encoder's
    `inputFormats`, not from this export's renderer's offer, since the
    process starts (and may `describe`) before the renderer exists; an
    empty intersection, or an encoder without `inputFormats`, sizes for
    `rgba16`.
21. A ring has a ceiling, `MAX_RING_BYTES = 256 MiB`: slots drop from 3
    toward 1, and a frame whose one slot does not fit gets no ring and goes
    through the pipe. `slot_bytes` is rounded up to a multiple of 8 (ruling
    R6).
22. An abort with requests in flight drops the process instead of sending
    `abortEncoder` / `abortMuxer` behind them, whose 5 s deadlines would
    otherwise wait on up to `slots` encodes first.
23. Pipelining does not need the ring: without a segment or a declared
    ring, requests still go up to `PIPE_WINDOW = 3` deep with payload
    frames.
27. The video encoder's `transports` gate the ring (ruling R10): Celesta
    declares the ring in `openVideoEncoder` only when the chosen encoder
    lists `sharedMemory`, deciding at the open because the process may have
    started (with a segment) while its plugin was described. The spec takes
    the plugin's acceptance of the segment at `hello` as the whole
    negotiation.
28. The payload window is bounded by bytes (ruling R11): when frames do
    not go through the ring (none was declared, or the opened input's
    frame does not fit a slot), the window is `clamp(MAX_RING_BYTES / frame_bytes, 1,
    PIPE_WINDOW)` (`pipe_depth`), so frames deviation 21 sends through the
    pipe do not wait three deep, two of them in the writer's queue.

## Docs

The user guide's plugins page (en and ja), "Shared memory and other FFmpeg
builds": video frames cross memory shared with the plugin's process, the
decoded ones and, from this release, the ones an export sends a video
encoder, up to three ahead so rendering and encoding overlap (fewer for
very large frames such as 8K); decoded frames larger than 4096×2560 and
exported frames of more than about 33 megapixels go through the pipe, as
does every frame of a plugin or video encoder that does not support it;
`CELESTA_CODEC_SHARED_MEMORY=0` keeps every frame, decoded or exported, in
the pipe. The Agent Skill's `references/plugins.md` says the same.
HANDOFF.md's rows for `celesta-plugin` (the ring), `celesta-plugin-host`
(the send window), and `celesta-media` (pipelined export requests), and its
setup section (`CELESTA_EXPORT_TIMING=1`, with the line explained). The
spec records deviations 19–23, 27, and 28, the deadlines as built, the
timing line, and P2b's Phases row.

## Checks

On macOS 26.6.2, Apple M4 (10 cores, 32 GB), Metal, with `PKG_CONFIG_PATH`
at Homebrew's `ffmpeg@8` and this branch's FFmpeg plugin, staged by
`build.mts`, on `CELESTA_PLUGIN_PATH`, at this branch as it was when
measured, plus this note's commit (the commits were since rebased, so
the hashes recorded then no longer exist):

- `cargo build -p celesta-codec-test`, then `cargo test --workspace
  --no-fail-fast` (2,637 passed, 1 ignored as before; no flake this time),
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --all -- --check`, and `cargo clippy -p celesta-plugin -p
  celesta-plugin-host --target x86_64-pc-windows-msvc -- -D warnings`
  (type-checked from macOS, not run).
- `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`
  (466 passed), its clippy with `-D warnings`, and its `cargo fmt --check`.
- `pnpm --dir packages/website test` (52 passed), `pnpm run test:runtime`
  (17 packages, 1,046 tests, all passed; `@celesta.studio/cli` 119,
  `@celesta.studio/components` 179), and `git diff --check`.

Unit and integration tests cover the rest (Tasks 1–7 and the final
review's fixes): the ring's layout, alignment, and per-slot seqlock; the
send window's order, deadlines (from the send and from the previous
answer), wrong kinds, and kills with requests in flight; `ring_layout`,
`pipe_depth`, and `frame_bytes`'s saturation; the wire's `ring` and
`slot`, and the serving side's refusals; `FrameRing`'s slots and
generations; the pipeline's depth, refusals while in flight, and slot
release on failure; the export's window, batching, stages, abort, the
transports gate, and the timing line; the test codec's program through the
ring and the window (a frame too large for the ring through the pipe, a
declined ring sending payloads, the window shrinking for large frames,
stopped programs); and the FFmpeg plugin's encoders through its program,
ring and pipe, byte-identical. The integration test
`an_export_through_the_programs_ring_is_byte_identical_to_the_pipe` asserts
`frames ring 3 deep` with shared memory and `frames pipe 3 deep` without.

### Byte-identical to P2a

P2a's tip then (since rebased; P2a's tip is now `1247585b`) was built in a
temporary `git worktree`, and this branch at the time of measurement in
its own checkout. Release `celesta-exporter` and the
FFmpeg plugin (`node scripts/plugins/build.mts plugins/ffmpeg --out …`) of
each were staged alone: each exporter in its
own folder with the same React runtime (`runtime/node`, `runtime/react`),
only its own plugin folder on `CELESTA_PLUGIN_PATH`, an empty `HOME`, and
`env -i`. Each exported `examples/greenscreen/composite.tsx` as MP4, and an
entry with a `<Video>` (the green-screen footage,
`examples/greenscreen/assets/footage.mp4`) and an `<Audio>`
(`examples/assets/voices/001.wav`), 640×360 at 30 fps for 60 frames, as
MP4, as MP4 with `--loop`, as MOV, and as WebM; this branch exported each
again with `CELESTA_CODEC_SHARED_MEMORY=0`. With
`CELESTA_EXPORT_TIMING=1`, every one of this branch's exports said
`frames ring 3 deep`, and every one with the variable at `0` said `frames
pipe 3 deep`. `cmp`:

| Output | P2a = P2b | P2a = P2b, `=0` | SHA-256 (all three) | Bytes |
| --- | --- | --- | --- | --- |
| green screen, MP4 | identical | identical | `bfc7be676037b6fd0e0d8a2f3bee11fe2f10e7511e93a3c449743ee1db3a547e` | 322,920 |
| `<Video>` + `<Audio>`, MP4 | identical | identical | `51103796c26302e5d0d676b5b26a65a1c2c13c83c980e30c71fa0c4f111fff2e` | 210,981 |
| the same with `--loop` | identical | identical | `83febef6423621d303e6959f0125d1c9e2120b4188335ba02d9a1368614b46bf` | 212,184 |
| the same as MOV | identical | identical | `35bd75332e27d244969bf5d6f69cd97a83ceea529f9207e351ac105f6a0a4e4b` | 13,009,211 |
| the same as WebM | identical but for four 8-byte UIDs | the same | `731947af06362c26d09dcb0bf72e4b1010f600566bdfabc8c75469d0d3d2cb08` (UIDs zeroed) | 184,422 |

The MP4 and MOV hashes are the ones P2a's note recorded for its branch.
The WebM files differ from each other in 32 bytes at offsets 286–293,
376–383, 508–515, and 561–568 (1-based): the 8-byte values of the two
tracks' `TrackUID` (EBML `0x73C5`) and of their copies in the tags
(`TagTrackUID`, `0x63C5`), which FFmpeg's Matroska muxer draws at random
for each file. Two runs of P2a's exporter differ from each other in the
same 32 bytes, and two of `main`'s (`14d0c5ab`) likewise, so this is not
P2b's: with those values zeroed, P2a's, this branch's, this branch's with
`=0`, and repeat runs of each are identical (the hash above), and
`ffmpeg -c copy -f framemd5` gives the same packets for all three
(the SHA-256 of its output without the comment lines is
`96998d03cd2baaa47a82df1e4109146b5f017b1361b79457d661a836925eea85`).
The three WebM files as written:

| WebM | SHA-256 |
| --- | --- |
| P2a | `307942d20df627ea0029ad64595b74d13782584b1f78e16e68c9bc51e8d4d6ca` |
| P2b | `3721ff83d041cdf641090dedac6b637bfa081537e64bc8a0aa85aa4338942530` |
| P2b, `=0` | `98d53f497b831e343c5e3d13a48560d83570558f3455cef9f6e86c9359ebfd8e` |

See Not done.

### The mix against the render

This branch's exporter, release, with `CELESTA_EXPORT_TIMING=1`, each
project exported as MP4 at its full length, three runs each (1-minute load
average 10 at the start and the end):

| Project | Frames | Render | Mix | The mix arrived at | Staged peak | Spilled | Frames |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `examples/editor-demo.celesta.json` | 600 | 0.77, 0.81, 0.91 s | 0.02, 0.03, 0.03 s | frame 1, 36, 39 (0.2%, 6.0%, 6.5%) | 0.0 MB | no | ring 3 deep |
| `examples/voiceroid.celesta.json` | 480 | 1.31, 1.46, 1.60 s | 0.08, 0.07, 0.08 s | frame 49 each run (10.2%) | 0.0 MB | no | ring 3 deep |
| the `<Video>` + `<Audio>` entry (React) | 60 | 0.10 s | 0.07 s | after the render | 0.1 MB | no | ring 3 deep |

`examples/minimal.celesta.json` has no tracks, so it cannot be exported:
each run reported `empty_timeline` ("cannot export an empty timeline"), as
any build does. As expected, the mix finishes long before the render (by
its frame 49 of 480 at the latest), so little video waits for the muxer
(under 0.05 MB at the peak) and nothing spills; the React entry stages its
whole
video by design (deviation 11's flow) and here it is small. The React
exports timed below stage 0.9–110.7 MB (1080p `medium` to 4K ProRes), with
no mix and no spill.

### Performance

Release builds of `main` (`14d0c5ab`) and P2a's tip then (now `1247585b`
after a rebase), each in a temporary `git worktree`, and of this branch at
the time of measurement, each exporter
staged alone as above and each `codec-bench` against its own plugin
(`main`'s drives protocol 1). Interleaved: six rounds, the order of the
builds rotating each round (`main`, P2a, P2b, P2b with
`CELESTA_CODEC_SHARED_MEMORY=0`, and the reverse and two other orders),
then four rounds of `codec-bench` alone; medians with each one's range and
run count. The machine had an IDE, a chat app, and another project's
`esbuild` watcher open. The 1-minute load average read 9, 62, and 72 at
the starts of the first three rounds, 48, 69, and 57 at the next three, 78,
59, 42, and 37 at the `codec-bench` rounds, and 42 at the end; much of it
is the exports' own threads (macOS counts runnable threads, and the 4K
exports keep all ten cores busy), but it makes every figure noisier than on
an idle machine.

`codec-bench` (`plugins/ffmpeg/examples/codec-bench.rs`, 120 frames of a
`testsrc2` H.264 file, the same files P2a's note timed; ms per frame; ten
runs each). This branch's plugin rows go through the ring three deep, and
with `=0` through the pipe three deep (as the bench printed every run);
`=0` also keeps decoded frames in the pipe:

| | `main` | P2a | P2b | P2b, `=0` |
| --- | --- | --- | --- | --- |
| decode in process, 1080p | 0.78 (0.59–1.17) | 0.74 (0.59–1.03) | 0.79 (0.56–1.28) | 0.74 (0.55–0.86) |
| decode through the plugin, 1080p | 1.39 (0.89–1.85) | 1.45 (0.90–2.80) | 1.33 (0.88–1.94) | 2.09 (1.42–3.57) |
| decode in process, 2160p | 3.17 (2.49–3.65) | 3.24 (2.60–3.69) | 3.23 (2.58–3.57) | 3.29 (2.84–10.34) |
| decode through the plugin, 2160p | 4.68 (4.17–5.14) | 4.71 (4.14–6.37) | 4.75 (4.21–5.45) | 8.37 (7.08–11.44) |
| encode in process, 1080p | 2.54 (1.60–4.47) | 2.53 (1.68–6.13) | 2.46 (1.67–3.03) | 2.12 (1.65–2.63) |
| encode through the plugin, 1080p | 4.51 (2.69–8.10) | 3.46 (2.70–5.44) | **2.42** (1.82–3.09) | 3.69 (2.53–4.16) |
| encode in process, 2160p | 10.27 (7.49–14.69) | 9.61 (7.88–10.91) | 9.09 (8.00–11.20) | 8.36 (7.18–20.46) |
| encode through the plugin, 2160p | 16.20 (13.12–25.13) | 15.60 (12.96–19.80) | **10.73** (8.61–12.19) | 15.46 (12.82–24.52) |

The bench's encode differs in one parameter: `main`'s uses CRF 23, P2a's
and this branch's the default 18 (P2a's note found no difference).

Exports of a React entry of 300 frames (24 moving rectangles and a
`Frame N` text over a full-frame background, the entry P2a's note timed:
its source is below), seconds of wall time including start-up, six runs
each; `main` with `--preset ultrafast`, P2a and this branch with
`--video-option preset=ultrafast`:

| | `main` | P2a | P2b | P2b, `=0` |
| --- | --- | --- | --- | --- |
| 1920×1080 MP4, x264 `ultrafast` | 0.962 (0.903–1.229) | 0.948 (0.792–1.066) | **0.699** (0.589–0.768) | 0.899 (0.700–1.009) |
| 1920×1080 MP4, x264 `medium` | 2.936 (2.392–3.318) | 2.981 (2.378–3.134) | 2.928 (2.468–3.202) | |
| 3840×2160 MP4, x264 `ultrafast` | 3.047 (2.369–3.364) | 3.117 (2.587–3.379) | **2.508** (1.816–2.672) | 2.995 (2.466–3.487) |
| 3840×2160 MP4, x264 `medium` | 9.534 (7.683–10.273) | 9.597 (7.938–10.011) | 9.241 (7.469–9.670) | |
| 3840×2160 MOV, ProRes 4444 | 24.089 (21.475–25.638) | 24.138 (23.731–27.738) | 23.221 (19.849–24.393) | |

**Every row's median for this branch is at or within `main`'s range, and
every export and encode row's median is at or below `main`'s.** The first
three rounds alone had this branch's 4K decode through the plugin at 4.80
ms against `main`'s 4.70 (4.39–4.73), just outside `main`'s range; decoding
runs no code P2b changed, and with ten runs each it is 4.75 against 4.68
(4.17–5.14). The two decode-in-process rows, also unchanged code, are
within 2% of `main`'s. Where the encoder takes the frames as fast as they
come (`ultrafast`, and the bench's plugin encode), the ring and the window
save 27% at 1080p and 18% at 4K end to end against `main`, and 34–46% of
the plugin encode's time per frame (about 30% against P2a's); the pipe,
pipelined (`=0`), saves less (7% and 2%), so most of the gain is the
ring's. With `medium` and ProRes the
encoder dominates and the rows are within the noise.

CPU time, from `/usr/bin/time -l` (user and system, medians of six; the
exporter and the plugin process it waits for): the ring moves the frame
copies out of the kernel, so system time falls (4K `ultrafast` 2.7 s for
`main`, 2.2 P2a, 0.9 P2b, 2.5 P2b `=0`; 4K ProRes 13.6, 12.8, 2.2) while
user time rises (4K `ultrafast` 14.6 s, 13.4, 18.5, 14.1; 4K ProRes 197,
196, 202): about 2 s more in all for the 4K `ultrafast` export's 300
frames, which the overlap more than pays for in wall time.

The entry (`scene2160.tsx` has `W = 3840` and `H = 2160`):

```tsx
import { Composition, Rect, Text, useCurrentFrame } from '@celesta.studio/react';

const W = 1920;
const H = 1080;
const COLORS = ['#e4572e', '#29335c', '#f3a712', '#a8c686', '#669bbc', '#6c4b5e'];

function Scene() {
  const frame = useCurrentFrame();
  return (
    <>
      <Rect x={0} y={0} width={W} height={H} fill="#101820" />
      {Array.from({ length: 24 }, (_, i) => (
        <Rect
          key={i}
          x={(i * 97 + frame * (3 + (i % 5))) % W}
          y={(i * 53 + frame * 2) % H}
          width={W / 8}
          height={H / 8}
          fill={COLORS[i % COLORS.length]}
        />
      ))}
      <Text
        x={W / 2}
        y={H / 2}
        anchorX={0.5}
        anchorY={0.5}
        style={{ fontFamily: 'sans-serif', fontSize: H / 10, fill: { type: 'solid', color: '#ffffff' }, align: 'center' }}
      >
        {`Frame ${frame}`}
      </Text>
    </>
  );
}

export default function Root() {
  return (
    <Composition width={W} height={H} fps={30} durationInFrames={300}>
      <Scene />
    </Composition>
  );
}
```

Not run: Linux and Windows (CI), a software renderer's RGBA path, an 8K
export (the ring's ceiling and `pipe_depth` are covered by unit and
test-codec tests), and the editor by hand.

## Not done

- **An abort with requests in flight** drops the muxer's process without
  reading `abortMuxer`'s files. Harmless today: Celesta's own staged output
  and spill file go with their guards. A `sequence: true` muxer (P4) must
  remove the names Celesta chose, as the spec says, or drain `writePackets`
  with a short deadline before aborting.
- **The React path resolves the output after the bridge is up** (the ring
  needs the frame size), so output errors (`unsupported_output`,
  `invalid_option`, `component_unavailable`, ...) arrive only after Node
  starts and the entry loads, and the `output` progress event follows
  `composition`.
- **Encoding straight from the mapping** (deviation 19): the plugin copies
  each frame out of its slot, and Celesta copies each into one. An
  `AVBufferRef` over the shared memory would save the plugin's copy;
  reading back from the GPU into the slot would save Celesta's.
- **Content plugins** still use the pipe only.
- **The decode slot's Windows commit charge** (#775's open question):
  whether each process's 40 MiB section is charged in full at creation is
  still not verified. Related, from the final review: a process started
  during `resolve` with a manifest-sized ring that ends up serving only the
  muxer keeps a section it never writes, which on Windows may be charged.
  The export ring's section is charged in full too (`CreateFileMappingW`
  with `PAGE_READWRITE` commits it up front), yet only part of it is
  written: its slots are sized for the largest offerable format (libx264's
  `inputFormats` include `rgba16`), while the GPU's `yuv420p`, which x264
  8-bit chooses, writes 12,441,600 of a 4K slot's 66,355,200 bytes
  (18.75%). At 4K that is about 199 MB charged for about 37 MB touched,
  capped by `MAX_RING_BYTES`. Sizing the ring after the open, from the
  input chosen, would need the process to start later or the segment to
  be replaced.
- **A same-process packet shortcut**: when the video encoder and the muxer
  are one plugin, each packet still crosses the pipe twice (out with the
  encode's answer, back in `writePackets`), which the ProRes row pays.
- **Rings of one or two slots** (large frames) narrow the window to their
  slots; the pipe's window shrinks likewise (deviation 28).
- **The ring is an opt-out for direct `MediaProcess` users** (ruling R10):
  the exporter declines it for an encoder that does not describe
  `sharedMemory`; another caller must call `Pipeline::decline_ring`
  itself. Every current caller's encoder describes it.
- **`last_answered`** is when the host received the previous answer, not
  when it arrived on the pipe, which errs on the lenient side.
- **WebM files are not reproducible byte for byte**: FFmpeg's Matroska
  muxer writes random `TrackUID`s (on `main` too). Setting
  `AVFMT_FLAG_BITEXACT`, or fixed UIDs, in the plugin's WebM muxer would
  make them so; it would change the files, so it belongs with a change
  that is allowed to.

## Review follow-ups

The rulings that shaped the code, from the session's ledger:

- R2: P2a's `interleave::WRITE_BATCH_BYTES` (32 MiB, a request's budget)
  became `WRITE_REQUEST_BYTES`, and its packet cap `WRITE_REQUEST_PACKETS`,
  so the plan's `WRITE_BATCH_BYTES = 4 MiB` is the flush threshold.
- R3: `ResolvedEncoder.input_formats` keeps the chosen encoder's described
  formats, to size the ring of a process started after `resolve`.
- R4: a refused frame fails the export from the `encode_frame` that drains
  its answer (the fifth, at depth 3), with stage "video encoding".
- R6: `slot_bytes` is rounded up to a multiple of 8, and
  `SharedMemory::ring` refuses a multi-slot layout that is not.
- R8: `codec-bench` names the transport the plugin accepted and its CPU
  column leaves out the probe's process.
- R10, R11, R12 (the final review's Important findings): the encoder's
  `transports` gate the ring (deviation 27); the payload window is bounded
  by bytes (deviation 28); the timing line says how the frames went, and
  the tests that pin the ring assert it was used.

Fixed after PR review: `open_video` marks the ring in use only when the
opened input's frame fits a slot (`frame_bytes <= slot_bytes`); a larger
frame goes as a payload with the pipe's window, and the timing line says
`frames pipe N deep`. `decodeVideo` is refused on a process that has a
ring, so a decoded frame is never written across ring slots.

Fixed after the final review as well: `PixelFormat::frame_bytes`
saturates, since `resolve` runs before the dimensions are validated; the
staged peak counts the packets `finish` receives; the timing line's
megabytes are decimal; stale docs on `MediaProvider`, `PluginChild`'s
kills, and `ExportTiming::render`; the guide and the Agent Skill say fewer
frames go ahead for very large frames and give the pipe's threshold. The
spec's new deviations were numbered 24 and 25, which P2a's use; they are 27
and 28.

Fixed after the second PR review:

- A unit test (`one_pipelined_process_muxes_what_one_at_a_time_does`)
  runs a project-shaped export through one fake process serving the video
  encoder, the audio encoder, and the muxer three deep (the FFmpeg
  plugin's shape): the mix arrives after twenty frames, and its packets
  and the frames' are written while frames are in flight. The muxer gets
  exactly the packets, in the order, that the one-at-a-time fake gets;
  an `error` answering an `encodeAudio` or a `writePackets` is reported as
  "audio encoding" or "muxing" (`a_refused_answer_names_its_stage`).
  `the_test_codecs_program_writes_what_it_writes_in_process` runs the same
  flow through the test codec's program (three deep, through the ring
  where shared memory is available) and through the test codec in
  Celesta's process (one at a time), and the two files are byte-identical.
  It drives `ExportSession` directly, without the GPU, so it is a unit
  test; the exporter binary has no depth-1 setting for a plugin process
  short of a frame over half of `MAX_RING_BYTES`.
- The FFmpeg plugin's and the test codec's ring tests assert
  `frames_in_ring()` after the open; the docs say a declared ring carries
  the frames only when the opened input's frame fits a slot.
- A frame's `error` answer names the frame (`frame N: …`, N from 0), since
  it arrives requests after the frame was sent (R4).
- A shared segment the host cannot create or hand over is reported once
  per run on stderr (`celesta: frames go through the pipe: …`), for the
  export ring and the decode slot alike.
- `RingLayout::segment_bytes()` is checked (`Option<usize>`) and replaces
  the three hand-written checked sizes; `PluginChild::request`
  debug-asserts that nothing is in flight.
