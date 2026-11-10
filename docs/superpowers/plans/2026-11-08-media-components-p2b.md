# Media components P2b: shared memory and pipelining — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An export's frames reach the video encoder through a ring of
shared-memory slots and its requests go to each plugin process several at
a time, with answers in order and a deadline per request, so rendering,
the slot copy, encoding, and muxing overlap; the output is byte-identical
to P2a's, and the export is not slower than `main`'s.

**Architecture:**
- **The ring** (`celesta-plugin`). One segment per export process holds
  `slots` seqlock regions laid out back to back, each #775's slot (a
  64-byte header with a generation and a length, then the bytes). Celesta
  sizes it before the process starts from the frame size and the largest
  pixel format the encoder may take, offers it at `hello` as today, and
  declares the layout in `openVideoEncoder`'s `ring`.
- **The send window** (`celesta-plugin-host`). `PluginChild` keeps a queue
  of requests in flight; answers come back in order, each checked against
  its request's expected kind, each with its own deadline, which for the
  streaming requests counts from the previous answer. `request` stays the
  window of one.
- **The pipeline** (`celesta-media`). `MediaProcess` places each frame in
  a free slot (or sends it as a payload), sends `encodeVideoFrame`,
  `encodeAudio`, and `writePackets` without waiting, and frees a slot when
  its `videoFrameAccepted` arrives. The serve loop copies a frame out of
  the named slot and hands it to the encoder as before; the FFmpeg and
  test-codec encoders are unchanged save for `transports`.
- **The export** (`celesta-exporter`). `ExportSession` keeps up to the
  window's depth of requests in flight per process, drains the oldest
  when the window is full, batches `writePackets`, drains everything
  before `finish`, and drops a process with requests in flight on abort.
  With `CELESTA_EXPORT_TIMING=1` it reports how long the mix took next to
  the render and whether the interleaver spilled.

**Tech stack:** Rust 2024 (MSRV 1.89), `libc` (POSIX shared memory,
`memfd`), Win32 sections through `celesta-plugin`'s own declarations,
serde/serde_json; `ffmpeg-sys-next` 8.1 in `plugins/ffmpeg` (unchanged).

**Spec:** `docs/superpowers/specs/2026-11-08-media-components-design.md`
(Transport; Protocol 2 → Handshake, Encoding, Deadlines and the window;
Capability discovery → at export start; Audio → the P2b measurement;
Testing → `celesta-plugin`, `celesta-plugin-host`, `celesta-media`,
Performance; Phases → P2b). Read it with this plan. P2a's plan
(`2026-11-08-media-components-p2a.md`) defines every interface this one
builds on; #775's note (`docs/handoff/2026-11-08-codec-shared-memory.md`)
describes the slot, the hand-over, and the escape hatch.

## Global constraints

- This PR stacks on P2a's. Every name below that P2a defines (`ExportSession`,
  `MediaProcess`, `ProviderUse`, the four provider traits, `Interleaver`,
  `VideoEncoderRequest`, `Encoded`, `TrackPacket`, `ComponentDescription`)
  is used as P2a's plan spells it; where this plan changes one, it says so.
- No Celesta crate except `celesta-codec-ffmpeg` links FFmpeg. The core's
  tests use `celesta-codec-test` (HANDOFF.md).
- Each slot is #775's seqlock region: `SLOT_HEADER = 64` bytes (an atomic
  `u64` generation, then a `u64` length), then the data. Generations written
  are even and grow by 2; the odd one between marks a write. A segment of
  `slots` slots is `slots × (SLOT_HEADER + slotBytes)` bytes, slot `i` at
  offset `i × (SLOT_HEADER + slotBytes)`.
- `slots` is **3** by default (`RING_SLOTS`). A ring is at most **256 MiB**
  (`MAX_RING_BYTES`); fewer slots fit a larger frame, and a frame whose one
  slot would exceed it gets no ring (deviation 21).
- Celesta writes a slot only after the plugin's `videoFrameAccepted` for the
  frame last placed there. A slot answer or request out of step is a plugin
  (or host) error, never undefined behaviour: bytes cross the segment
  through raw pointers under the seqlock, and neither side holds a Rust
  reference into it.
- Deadlines (the spec's table): `hello`, `describe` 30 s; the three `open`s
  60 s; `encodeVideoFrame`, `encodeAudio`, `writePackets` **60 s counted
  from when the request ahead of it in the window was answered** (from
  when it was sent, when nothing was ahead); `finishVideoEncoder` and
  `finishAudioEncoder` 10 min; `finishMuxer` 1 h; `abortEncoder` and
  `abortMuxer` 5 s, after which the process is killed. All others count from
  when the request was sent. An answer of another kind than its request
  expects, or a missed deadline, kills the process; an `error` answer leaves
  it running.
- `CELESTA_CODEC_SHARED_MEMORY=0` (`FrameTransport::from_environment`) offers
  no process any segment: no decode slot, no encode ring. Frames then go as
  payloads, still pipelined (deviation 23).
- Decoding keeps #775's single slot (`SHARED_FRAME_BYTES`), unchanged; a
  decoding process is never an export process.
- The output must be **byte-identical to P2a's** for the same export, and
  the measured exports **must not be slower than `main`'s** (release builds,
  Apple M4, medians of interleaved runs).
- JS/TS tests use Vitest in TypeScript; Rust tests stay in Rust (AGENTS.md).
  Nothing in `packages/` changes in P2b.
- Never amend commits; each task ends in a new commit. Commits are signed
  through 1Password; if signing fails with "failed to fill whole buffer",
  ask the user to unlock 1Password. Each commit message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Builds: `export PATH="/Users/natsuneko/Library/Application Support/mbx/bin:$HOME/.cargo/bin:$PATH"`;
  building or testing `plugins/` needs FFmpeg 8:
  `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`
  (Homebrew's default `ffmpeg` is 9 and breaks the build).
- **Builds between tasks.** Tasks 1–3 are additive or crate-internal, and
  the workspace builds after each. Task 4 turns `ProviderUse::Export` into
  `Export { ring }` and gives `MediaProcess::start_with` a `ProviderUse`,
  so `celesta-exporter` does not compile until Task 6, and the tests of
  `celesta-codec-test` (`tests/process.rs`) and `celesta-codec-ffmpeg`
  (`tests/process.rs`, `examples/codec-bench.rs`) until Tasks 5 and 7.
  Each task builds and tests only the crates it names; from Task 6 on,
  `cargo test --workspace` passes, and from Task 7 on, the plugins
  workspace too.

## Deviations from the spec (recorded in Task 8's spec edit)

Numbered after P2a's 8–18. P2c's start at 30.

19. **The plugin copies each frame out of its slot before encoding**, one
    `memcpy` through the seqlock's reader (as the decoder copies in),
    rather than encoding straight from the mapping as the spec expects.
    `celesta-plugin`'s rule that neither side holds a reference into the
    segment stands, and libavcodec copies a non-refcounted frame on
    `avcodec_send_frame` anyway, so encoding from the mapping would save
    nothing without an `AVBufferRef` over shared memory. The slot is still
    held until `videoFrameAccepted`, so Celesta's side is as the spec says.
20. **The ring is sized from the formats Celesta can offer at all**
    (`PixelFormat::OFFERABLE`: `yuv420p`, `yuv420p10`, `rgba16`, `rgba8`
    until P2c) intersected with the encoder's `inputFormats`, not from what
    this export's renderer offers: the process starts (and may `describe`)
    before the renderer exists. A software renderer's smaller offer costs
    address space, not memory. An empty intersection, or an encoder without
    `inputFormats`, sizes for `rgba16`, as the spec says of the unknown
    case.
21. **A ring has a ceiling, `MAX_RING_BYTES = 256 MiB`.** `ring_layout`
    drops slots from 3 toward 1 until the segment fits, and gives no ring
    when one slot does not (an 8192×4320 `rgba16` frame); those frames go
    through the pipe, pipelined. The spec sizes the ring for any frame.
    The ring's pages are all written within its first `slots` frames, so
    on Windows the section's commit charge is memory the export uses in
    any case; the ceiling bounds it. #775's open question about the
    decode slot's charge stays open and is not P2b's.
22. **An abort with requests in flight drops the process** (its stdin
    closes; it is killed half a second later) instead of sending
    `abortEncoder` / `abortMuxer` behind them, whose 5 s deadlines count
    from the send and would otherwise wait on up to `slots` encodes first.
    The abort messages are sent only when nothing is in flight. The staged
    output and the spill file are Celesta's own and are removed either way.
23. **Pipelining does not need the ring.** Without a segment (the plugin
    declined it, `CELESTA_CODEC_SHARED_MEMORY=0`, or deviation 21) requests
    still go `PIPE_WINDOW = 3` deep with payload frames. The spec ties the
    window to `slots` and says only that the variable turns the ring off.

## Decisions the spec left to this plan

- The wire shape of a frame in a slot: `encodeVideoFrame`'s
  `"slot": {"index", "generation", "bytes"}` (Task 3), mirroring the decode
  answer's `{"generation", "bytes"}` plus the slot's index.
- Which formats size the ring when the export's own process describes
  (deviation 20, Task 6): the union of that plugin's manifest video
  encoders' `inputFormats`, whether or not it ends up serving the video
  encoder; a plugin whose manifest has no video encoder gets no ring. With
  a cached `describe`, the chosen encoder's described `inputFormats`.
- `ProviderUse::Export { ring: Option<RingLayout> }` carries the layout to
  the factory (P2a's deviation 16, extended; Task 4).
- `writePackets` is batched: one request per `WRITE_BATCH_PACKETS = 8`
  ready packets or `WRITE_BATCH_BYTES = 4 MiB`, and everything at `finish`
  (Task 6), so the window carries mostly frames.
- The React path calls `ExportSession::resolve` after `bridge.metadata()`,
  since the ring needs the frame size (Task 6); the `output` progress event
  then follows the composition event.
- The mix-against-render measurement (Task 6): `CELESTA_EXPORT_TIMING=1`
  prints one stderr line per export with the render time, the mix time, the
  frame at which the mix arrived, the interleaver's peak staged bytes, and
  whether it spilled. Task 8 records it for the three example projects.
- The performance comparison (Task 8) runs `main`, P2a's tip, and P2b
  interleaved, so the gain is attributed; the requirement is against
  `main`.

## Review focus

The tests that pin each of these are in the task named.

1. **A plugin that accepts the ring and then never answers a frame**, or
   answers the wrong kind, is killed within its deadline and the export
   fails with `CodecStopped` / `CodecUnavailable` rather than hanging
   (Task 2, `a_deadline_from_send_counts_the_queue`,
   `a_wrong_kind_in_the_window_stops_the_plugin`; Task 5,
   `a_killed_switch_fails_the_window`).
2. **An encoder that picks a format larger than the ring was sized for**
   (described `inputFormats` wider than the manifest's, or deviation 20's
   `rgba16` guess undershooting a plugin's own format): the frame goes as
   a payload and the export succeeds (Task 4,
   `a_frame_larger_than_a_slot_is_not_placed`; Task 5,
   `frames_too_large_for_the_ring_go_through_the_pipe`).
3. **Cancel with three frames in flight** returns without waiting on the
   plugin and leaves no file (Task 6, `abort_with_frames_in_flight_does_not_wait`;
   P2a's `cancel_during_encoding_leaves_no_files`, rerun).
4. **An `error` answer to one frame among several in flight** fails that
   frame, frees its slot, and leaves the process running for a clean abort
   (Task 2, `an_error_answer_leaves_the_window_running`; Task 5,
   `an_error_in_the_window_leaves_the_rest_in_flight`).
5. **A frame size whose ring would exceed the ceiling** (8K, 16-bit) still
   exports, through the pipe (Task 3, `ring_layout_shrinks_to_the_cap`;
   Task 6, `no_ring_when_the_layout_is_none`).

---

### Task 1: A ring of slots in shared memory (`celesta-plugin`)

**Files:**
- Modify: `crates/plugin/src/shared.rs` (`RingLayout`, `Ring`, `SharedMemory::ring`; `write_slot`, `read_slot`, `slot_generation`, `slot_length` take an offset internally), `crates/plugin/src/lib.rs` (re-export `RingLayout`, `Ring`)
- Test: `crates/plugin/src/shared.rs` tests

**Interfaces:**
- Produces, in `celesta_plugin::shared`:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
  pub struct RingLayout { pub slots: usize, pub slot_bytes: usize }
  impl RingLayout {
      pub const fn stride(&self) -> usize;          // SLOT_HEADER + slot_bytes
      pub const fn segment_bytes(&self) -> usize;   // slots × stride
      pub const fn offset(&self, slot: usize) -> usize;   // slot × stride
  }
  impl SharedMemory {
      pub fn ring(&self, layout: RingLayout) -> io::Result<Ring<'_>>;   // InvalidInput when slots == 0, slot_bytes == 0, or segment_bytes() > len()
      // write_slot / read_slot / slot_bytes stay: slot 0 of RingLayout { slots: 1, slot_bytes: len - SLOT_HEADER }
  }
  pub struct Ring<'a> { memory: &'a SharedMemory, layout: RingLayout }
  impl Ring<'_> {
      pub fn layout(&self) -> RingLayout;
      pub fn write_slot(&self, slot: usize, bytes: &[u8], generation: u64) -> io::Result<()>;   // the seqlock writer at offset(slot); InvalidInput for slot >= slots, as for a bad generation or bytes that do not fit
      pub fn read_slot(&self, slot: usize, len: usize, generation: u64) -> Option<Vec<u8>>;     // the seqlock reader at offset(slot); None for slot >= slots
  }
  ```
- The writer and reader are #775's, per slot (the `write_slot` and
  `read_slot` bodies already in `shared.rs`, with `self.ptr.add(offset)`
  in place of `self.ptr`): relaxed store of the odd generation, release
  fence, copy, relaxed length store, release store of the even generation;
  acquire load and compare, copy out, acquire fence, relaxed load and
  compare again.

- [ ] **Step 1: Write failing tests** in `shared.rs`'s tests:
  - `a_layout_measures_its_segment`: `RingLayout { slots: 3, slot_bytes: 100 }` has `stride() == 164`, `segment_bytes() == 492`, `offset(2) == 328`.
  - `ring_slots_are_independent`: a segment of `RingLayout { slots: 3, slot_bytes: 16 }.segment_bytes()`; through a second handle (`duplicate_handle`, as the plugin maps it) write `[1; 8]` into slot 1 under generation 2 and `[2; 12]` into slot 2 under 4; `read_slot(1, 8, 2) == Some([1; 8])`, `read_slot(2, 12, 4) == Some([2; 12])`, `read_slot(0, 8, 2) == None`, `read_slot(1, 8, 4) == None`.
  - `a_ring_larger_than_the_segment_is_refused`: `SharedMemory::create(SLOT_HEADER + 1024)` and `ring(RingLayout { slots: 2, slot_bytes: 1024 })` is `Err`; `ring(RingLayout { slots: 0, slot_bytes: 1 })` is `Err`; `ring(RingLayout { slots: 1, slot_bytes: 1024 })` is `Ok`.
  - `a_slot_past_the_ring_is_refused`: on a 3-slot ring, `write_slot(3, &[1], 2)` is `Err` and `read_slot(3, 1, 2)` is `None`.
  - `the_one_slot_ring_is_the_old_slot`: `host.write_slot(&bytes, 8)` then `host.ring(RingLayout { slots: 1, slot_bytes: host.slot_bytes() }).unwrap().read_slot(0, bytes.len(), 8) == Some(bytes)`, and the reverse.
  - `a_slot_being_written_is_not_read_in_a_ring`: store 3 into slot 1's generation word directly (as the existing `a_slot_being_written_is_not_read` does for slot 0); `read_slot(1, 4, 2)` and `read_slot(1, 4, 3)` are `None`, while slot 0 written under 2 still reads.
- [ ] **Step 2:** `cargo test -p celesta-plugin shared` fails (`RingLayout` undefined).
- [ ] **Step 3: Implement.** The existing eight tests must pass unchanged.
- [ ] **Step 4:** `cargo test -p celesta-plugin` passes; `cargo clippy -p celesta-plugin --all-targets -- -D warnings` passes, and type-checks for `x86_64-pc-windows-msvc` (`cargo clippy -p celesta-plugin --target x86_64-pc-windows-msvc -- -D warnings`, as #775 did).
- [ ] **Step 5: Commit:** `feat(plugin): a ring of slots in shared memory`.

### Task 2: The send window on `PluginChild` (`celesta-plugin-host`)

**Files:**
- Modify: `crates/plugin-host/src/child.rs` (`PluginChild` fields, `send`, `receive`, `in_flight`, `request`, `stop`), `crates/plugin-host/src/lib.rs` (re-export `DeadlineFrom`)
- Test: `crates/plugin-host/src/child.rs` tests

**Interfaces:**
- Produces, in `celesta_plugin_host`:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum DeadlineFrom { Sent, PreviousAnswer }
  impl PluginChild {
      pub fn send(&mut self, request: Message, expected: &str, timeout: Duration, from: DeadlineFrom) -> Result<(), RequestError>;
      pub fn receive(&mut self) -> Result<Message, RequestError>;   // the oldest request's answer
      pub fn in_flight(&self) -> usize;
      pub fn request(&mut self, request: Message, expected: &str, timeout: Duration) -> Result<Message, RequestError>;   // send(…, DeadlineFrom::Sent) then receive(); unchanged behaviour with nothing in flight
  }
  ```
- `PluginChild` gains `pending: VecDeque<Pending { kind: String, expected: String, timeout: Duration, from: DeadlineFrom, sent: Instant }>`
  and `last_answered: Instant` (the process's start).
- `send`: with `requests` gone (the process was stopped), `Err(self.gone(kind, "it stopped reading requests"))` as today's `request`; otherwise the message goes to the writer thread and `Pending` is pushed.
- `receive`:
  - No request in flight: `Err(RequestError::Unavailable(format!("{name} is unavailable: no request is in flight")))`, nothing stopped.
  - The process already stopped: pop the head and fail it with `Stopped` when `killed`, else `Unavailable`, message `"it was stopped before it answered `{kind}`"` through `explain`.
  - Otherwise `start = match from { Sent => sent, PreviousAnswer => sent.max(last_answered) }` and `recv_timeout((start + timeout).saturating_duration_since(now))`. A timeout stops the process (`Stopped`, today's text `"it did not answer `{kind}` within {} s, so it was stopped"`); a stream error goes through `gone`; an answer sets `last_answered`, pops the head, and is `Refused(answer)` for `error`, `Ok` for `expected`, and otherwise stops the process with today's text `"it answered `{kind}` with `{found}`, not `{expected}`, so it was stopped"` (`Unavailable`).
- `hello_as` keeps using `request`, so `hello` is never pipelined.

- [ ] **Step 1: Write failing tests** in `child.rs`'s tests, with shell-script fakes whose canned answers are files written by `write_message` (as `hello_as_checks_the_answered_protocol` does) and a `kind(k)` helper making `Message::new(json!({"type": k}))`:
  - `answers_come_back_in_order_through_the_window`: the script `cat answers.bin; exec cat >/dev/null`, where `answers.bin` holds `x`, `y`, `z`; `send(kind("a"), "x", 5 s, Sent)`, `send(kind("b"), "y", …)`, `send(kind("c"), "z", …)`; `in_flight() == 3`; three `receive`s give kinds `x`, `y`, `z`; `in_flight() == 0`.
  - `a_wrong_kind_in_the_window_stops_the_plugin`: canned `x`, `z`; expected `x`, `y`, `w`: the first `receive` is `Ok`, the second `Unavailable` containing `` "with `z`, not `y`" ``, the plugin `exited`, and the third `Unavailable` containing `"stopped before it answered `c`"`.
  - `a_deadline_from_the_previous_answer_waits_for_the_queue`: the script `sleep 1; cat x.bin; sleep 0.5; cat y.bin; exec sleep 30`; `send(kind("a"), "x", 5 s, Sent)` and `send(kind("b"), "y", 800 ms, PreviousAnswer)` at once; both `receive`s are `Ok`, and the second returns more than 1.2 s after the sends (a deadline from the send would have passed).
  - `a_deadline_from_send_counts_the_queue`: the same script with `b`'s 800 ms as `Sent`: the second `receive` is `Stopped` containing `"did not answer `b`"` and the plugin `exited`.
  - `an_error_answer_leaves_the_window_running`: canned `error` (through `error_message("no")`), `y`; the first `receive` is `Refused` with message `"no"`, the second `Ok` with kind `y`, and `!exited`.
  - `receive_without_a_request_in_flight_is_an_error_not_a_kill`: a fresh child's `receive()` is `Unavailable` containing `"no request is in flight"` and `!exited`.
  - `killing_the_switch_fails_every_request_in_flight`: a script that touches a marker and sleeps; two sends; a thread kills the switch after 200 ms; both `receive`s are `Stopped`, within 10 s.
- [ ] **Step 2:** `cargo test -p celesta-plugin-host child` fails (`send` undefined).
- [ ] **Step 3: Implement.** The nine existing `child.rs` tests pass unchanged.
- [ ] **Step 4:** `cargo test -p celesta-plugin-host` and `cargo clippy -p celesta-plugin-host --all-targets -- -D warnings` pass.
- [ ] **Step 5: Commit:** `feat(plugin-host): a send window on PluginChild`.

### Task 3: Frame sizes, the ring's layout, and ring slots on the wire (`celesta-media`)

**Files:**
- Modify: `crates/media/src/provider.rs` (`PixelFormat::OFFERABLE`, `frame_bytes`), `crates/media/src/protocol.rs` (`SharedFrames` → `SharedSegment`; `open_video_request`, `encode_video_request`, `FrameLocation`; the `openVideoEncoder` and `encodeVideoFrame` arms), `crates/media/src/process.rs` (the two call sites pass `None` and `FrameLocation::Payload`), `crates/media/src/lib.rs` (`pub mod ring;` and re-exports)
- Create: `crates/media/src/ring.rs` (`ring_layout`, `RING_SLOTS`, `MAX_RING_BYTES`)
- Test: `crates/media/src/ring.rs` tests, `crates/media/src/protocol.rs` tests

**Interfaces:**
- Consumes: Task 1's `RingLayout`, `Ring`.
- Produces, in `celesta_media`:
  ```rust
  impl PixelFormat {
      pub const OFFERABLE: [PixelFormat; 4] = [Yuv420p, Yuv420p10, Rgba16, Rgba8];   // what Celesta can produce today; P2c adds Nv12, P010
      pub fn frame_bytes(self, width: u32, height: u32) -> usize;   // Rgba8 w×h×4; Rgba16 w×h×8; Yuv420p, Nv12 w×h + 2×⌈w/2⌉×⌈h/2⌉; Yuv420p10, P010 twice that
  }
  pub const RING_SLOTS: usize = 3;
  pub const MAX_RING_BYTES: usize = 256 << 20;
  pub fn ring_layout(formats: &[PixelFormat], width: u32, height: u32) -> Option<RingLayout>;
  pub(crate) enum FrameLocation { Payload(Vec<u8>), Slot { index: usize, generation: u64, bytes: usize } }
  pub(crate) fn open_video_request(request: &VideoEncoderRequest<'_>, ring: Option<RingLayout>) -> Result<Message, MediaError>;   // `ring` as {"slots","slotBytes"} when Some
  pub(crate) fn encode_video_request(session: SessionId, pts: i64, frame: FrameLocation) -> Message;   // Slot → "slot": {"index","generation","bytes"}, no payload
  ```
- The spec's "ring and pipelining against the test codec's program" tests
  live in `celesta-codec-test`'s `tests/process.rs` (Task 5), as P1's
  program tests do: `celesta-media` cannot depend on the test codec.
- `ring_layout`: `slot_bytes` is the largest `frame_bytes` over
  `formats ∩ OFFERABLE`, or `Rgba16.frame_bytes` when that set is empty;
  `slots` starts at `RING_SLOTS` and drops by one while
  `RingLayout { slots, slot_bytes }.segment_bytes() > MAX_RING_BYTES`;
  `None` when `slots` would reach 0.
- Serve side: `SharedSegment { memory: SharedMemory, generation: u64, ring: Option<RingLayout> }`
  replaces `SharedFrames` (decode answers keep writing slot 0 as a one-slot
  ring, unchanged). `openVideoEncoder` with `ring` and a segment:
  `memory.ring(layout)` must succeed, else `error`
  `"the ring of {slots} slots of {slotBytes} bytes does not fit the shared segment of {len} bytes"`;
  with `ring` and no segment: `error` `"a ring was declared but no shared memory was accepted"`;
  the layout is kept for the process's life. `encodeVideoFrame` with
  `slot`: no ring is `error` `"a slot was named but no ring was opened"`;
  `Ring::read_slot(index, bytes, generation)` giving `None` is `error`
  `"slot {index} does not hold the frame under generation {generation}"`;
  otherwise the copied `Vec<u8>` goes to `encode_video(session, pts, frame)`
  exactly as a payload does.

- [ ] **Step 1: Write failing tests.**
  - `ring.rs`: `frame_bytes_follow_the_format`: at 1920×1080, `Rgba8` 8 294 400, `Rgba16` 16 588 800, `Yuv420p` 3 110 400, `Yuv420p10` 6 220 800, `Nv12 == Yuv420p`, `P010 == Yuv420p10`; at 33×17, `Yuv420p` 867.
    `ring_layout_takes_the_largest_offerable_format`: `[Yuv420p, Rgba8]` at 1920×1080 → `Some({3, 8_294_400})`; `[Yuv420p]` → `{3, 3_110_400}`; `[]` → `{3, 16_588_800}`; `[Nv12, P010]` (nothing offerable) → `{3, 16_588_800}`.
    `ring_layout_shrinks_to_the_cap`: `[Rgba16]` at 3840×2160 → `{3, 66_355_200}`; `[Rgba8]` at 7680×4320 → `{2, 132_710_400}`; `[Rgba16]` at 7680×4320 → `{1, 265_420_800}`; `[Rgba16]` at 8192×4320 → `None`.
  - `protocol.rs` (the fake video encoder of P2a's Task 3 records frames and `pts`):
    `open_video_carries_the_ring`: `open_video_request(&request, Some(RingLayout { slots: 3, slot_bytes: 64 }))`'s header has `ring == {"slots": 3, "slotBytes": 64}`; with `None`, no `ring` key.
    `encoded_frames_come_out_of_ring_slots`: a segment of `RingLayout { slots: 2, slot_bytes: 64 }.segment_bytes()`; serve `hello` with the offer (accepted), `openVideoEncoder` with that `ring`; the host writes `A` (64 bytes) into slot 0 under generation 2 and `B` (48 bytes) into slot 1 under 2 through `Ring::write_slot`; `encode_video_request(session, 0, Slot { 0, 2, 64 })` and `(session, 1001, Slot { 1, 2, 48 })`; the fake recorded `[(0, A), (1001, B)]`, and both answers are `videoFrameAccepted`.
    `a_payload_frame_still_works_with_a_ring`: after that, `Payload(C)` is recorded too.
    `a_ring_that_does_not_fit_is_refused_at_open`: `ring: {3, 64}` on that 2-slot segment answers `error` containing `"does not fit"`.
    `a_ring_without_a_segment_is_refused`: `hello` without an offer, then `openVideoEncoder` with `ring` → `error` containing `"no shared memory was accepted"`.
    `a_slot_without_a_ring_is_refused`: `openVideoEncoder` without `ring`, then a `Slot` frame → `error` containing `"no ring was opened"`.
    `a_stale_slot_is_refused`: slot 0 holds generation 2; `Slot { 0, 4, 64 }` → `error` containing `"slot 0"`; the fake recorded nothing for it.
- [ ] **Step 2:** `cargo test -p celesta-media ring protocol` fails.
- [ ] **Step 3: Implement.** `process.rs` passes `None` and `FrameLocation::Payload(frame)` for now (Task 4 wires the ring).
- [ ] **Step 4:** `cargo test -p celesta-media` passes and `cargo build --workspace` succeeds.
- [ ] **Step 5: Commit:** `feat(media): frames through ring slots on the wire`.

### Task 4: `MediaProcess`'s pipeline and the export ring (`celesta-media`)

**Files:**
- Modify: `crates/media/src/provider.rs` (`ProviderUse`, `Pipeline`, `Answered`, `MediaProvider::pipeline`), `crates/media/src/process.rs` (`FrameRing`, `start_with`, `factory`, `open_video`, the `Pipeline` impl, the sync encode methods), `crates/media/src/registry.rs` (`describe` passes `ProviderUse::Describe`, unchanged; its `factories_learn_their_use` test matches the new variant), `crates/media/src/lib.rs`
- Test: `crates/media/src/process.rs` tests

**Interfaces:**
- Consumes: Task 2's `send` / `receive` / `DeadlineFrom`; Task 3's `FrameLocation`, `ring_layout`.
- Produces, in `celesta_media`:
  ```rust
  pub enum ProviderUse { Decode, Describe, Export { ring: Option<RingLayout> } }   // P2a's `Export` gains the ring
  pub enum Answered { VideoFrame(Encoded), Audio(Encoded), Packets }
  pub trait Pipeline {
      fn depth(&self) -> usize;        // the ring's slots, or PIPE_WINDOW without a ring
      fn in_flight(&self) -> usize;
      fn send_video_frame(&mut self, session: SessionId, pts: i64, frame: Vec<u8>) -> Result<(), MediaError>;
      fn send_audio(&mut self, session: SessionId, pts: i64, samples: &[f32]) -> Result<(), MediaError>;
      fn send_packets(&mut self, session: SessionId, packets: &[TrackPacket]) -> Result<(), MediaError>;
      fn receive(&mut self) -> Result<Answered, MediaError>;   // the oldest; MediaError::Codec("no request is in flight") when none
  }
  pub trait MediaProvider { /* P2a's */ fn pipeline(&mut self) -> Option<&mut dyn Pipeline> { None } }
  pub const PIPE_WINDOW: usize = 3;
  impl MediaProcess {
      pub fn start_with(executable: &Path, directory: &Path, kill: &PluginKillSwitch, transport: FrameTransport, purpose: ProviderUse) -> Result<Self, MediaError>;
      pub fn shares_frames(&self) -> bool;   // a decode slot or a ring was accepted
      pub fn ring(&self) -> Option<RingLayout>;
  }
  pub(crate) struct FrameRing { memory: SharedMemory, layout: RingLayout, generations: Vec<u64>, free: Vec<usize> }
  impl FrameRing {
      pub(crate) fn new(memory: SharedMemory, layout: RingLayout) -> Self;   // every slot free, generations 0
      pub(crate) fn place(&mut self, frame: &[u8]) -> Option<FrameLocation>;  // None when no slot is free or frame.len() > slot_bytes; else writes under generation + 2 and takes the slot
      pub(crate) fn release(&mut self, index: usize);
      pub(crate) fn free_slots(&self) -> usize;
      pub(crate) fn layout(&self) -> RingLayout;
  }
  ```
- `start_with`'s segment: `Decode` with `SharedMemory` → `SHARED_FRAME_BYTES`
  (today); `Export { ring: Some(layout) }` with `SharedMemory` →
  `SharedMemory::create(layout.segment_bytes())`, kept as a `FrameRing` when
  `hello` accepts it; `Describe`, `Export { ring: None }`, or `Pipe` → no
  segment. `factory` calls `start_with(…, FrameTransport::from_environment(), purpose)`
  with the `ProviderUse` it is given (P2a's `Describe`/`Export` → `Pipe`
  special case goes: those uses make no segment in any case).
  `MediaProcess::start` keeps `(…, FrameTransport::from_environment(), ProviderUse::Decode)`.
- `open_video` sends `open_video_request(request, self.ring())`. The
  `Pipeline` impl keeps `pending: VecDeque<Pending { kind: PendingKind, slot: Option<usize> }>`
  in step with the child's window: `send_video_frame` places the frame
  (`FrameLocation::Payload` when `place` is `None`) and sends with
  `"videoFrameAccepted"`, `ENCODE_TIMEOUT`, `DeadlineFrom::PreviousAnswer`;
  `send_audio` → `"audioAccepted"`; `send_packets` → `"packetsWritten"`
  (both `ENCODE_TIMEOUT`, `PreviousAnswer`). `receive` pops the head, calls
  `child.receive()` (errors through `media_error`), releases the head's slot
  whether the answer is `Ok` or `Err`, and decodes `Encoded` for the two
  encoder kinds. `depth()` is `ring.layout().slots` or `PIPE_WINDOW`.
- The sync `encode_video`, `encode_audio`, and `write_packets` of
  `MediaProcess` become `send_*` then `receive`, for callers with nothing in
  flight (an answer of another `Answered` kind is
  `MediaError::Codec("the plugin answered out of step")`). Every other
  request (`describe`, the `open`s, `finish`es, `abort`s) stays a `request`,
  so it is sent only when the caller has drained the window.

- [ ] **Step 1: Write failing tests** in `process.rs`'s tests (`FrameRing` on a `SharedMemory` in this process):
  - `a_ring_hands_out_each_slot_once_until_released`: `RingLayout { slots: 2, slot_bytes: 16 }`; placing 8-byte frames gives `Slot { index: 0, generation: 2, bytes: 8 }`, `Slot { index: 1, generation: 2, bytes: 8 }`, then `None`; `release(0)`; the next is `Slot { index: 0, generation: 4, bytes: 8 }`; `free_slots() == 1`.
  - `a_frame_larger_than_a_slot_is_not_placed`: a 17-byte frame is `None` and `free_slots()` is unchanged.
  - `placed_frames_read_back_under_their_generation`: through `memory.ring(layout).read_slot(index, bytes, generation)` on a second handle, each placed frame reads back equal.
  - The real-program cases (which `ProviderUse` gets a segment, the window through the program) are Task 5's.
- [ ] **Step 2:** `cargo test -p celesta-media process` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p celesta-media` passes (`celesta-exporter` and the two plugins' tests do not compile until Tasks 5–7).
- [ ] **Step 5: Commit:** `feat(media): pipelined requests and the export ring`.

### Task 5: The test codec's program through the ring and the window (`celesta-codec-test`)

**Files:**
- Modify: `crates/codec-test/src/components.rs` (`transports`), `crates/codec-test/src/lib.rs` (the video encoder refuses a frame of the wrong length), `crates/codec-test/tests/process.rs`
- Test: `crates/codec-test/tests/codec.rs`, `crates/codec-test/tests/process.rs`

**Interfaces:**
- Consumes: Tasks 3–4.
- Produces: the video encoder `video` describes `transports: ["sharedMemory", "pipe"]`;
  `encode_video` with a frame whose length is not `input.frame_bytes(width, height)`
  is `MediaError::Codec("a {w}x{h} {format} frame has {expected} bytes, not {given}")`
  and leaves the session open. `program_with(transport, purpose)` in
  `tests/process.rs` replaces `program_with(transport)`.

- [ ] **Step 1: Write failing tests.**
  - `tests/codec.rs`: `a_frame_of_the_wrong_length_is_refused` (the message above; the next frame of the right length is accepted); `describe_offers_shared_memory` (`transports == ["sharedMemory", "pipe"]`).
  - `tests/process.rs`, with a helper `encode_pipelined(provider: &mut dyn MediaProvider, session, frames: &[Vec<u8>], denominator: i64) -> Vec<Packet>` that keeps `in_flight() < depth()` by receiving before each send and collects `Answered::VideoFrame` packets (and the first `config`), then drains:
    `pipelined_frames_through_the_ring_match_the_pipe_byte_for_byte`: `program_with(SharedMemory, Export { ring: ring_layout(&[Rgba8], 32, 16) })` has `shares_frames()` and `ring() == Some({3, 2048})`; its pipeline's `depth() == 3`; after three sends with no receive `in_flight() == 3`; 7 frames of 32×16 `rgba8` (`input=rgba8`), finish, open the `file` muxer, write, finish; the same through `program_with(Pipe, Export { ring: ring_layout(…) })` (`!shares_frames()`, `depth() == 3`) and through `TestCodec` in process (the sync traits); the three files are byte-identical and `read` gives the 7 frames.
    `a_describing_or_ringless_export_process_gets_no_segment`: `program_with(SharedMemory, Describe)` and `program_with(SharedMemory, Export { ring: None })` have `!shares_frames()`; `describe` still answers.
    `frames_too_large_for_the_ring_go_through_the_pipe`: a ring sized for 32×16 `rgba8` (2048 bytes a slot), the encoder opened at 64×32 (8192-byte frames): 4 frames encode and read back equal.
    `an_error_in_the_window_leaves_the_rest_in_flight`: three frames sent, the second 4 bytes short; `receive` ×3 gives `Ok`, `Err(MediaError::Codec(..))` containing `"not 2044"`, `Ok`; `describe` on the same process still answers.
    `a_killed_switch_fails_the_window`: `hanging_program` with two frames sent; a thread kills the switch after 200 ms; both `receive`s are `CodecStopped`, within 10 s.
- [ ] **Step 2:** `cargo test -p celesta-codec-test` fails to compile (`program_with` and `ProviderUse::Export`).
- [ ] **Step 3: Implement.** Port the existing `process.rs` tests to `program_with(transport, ProviderUse::Decode)`.
- [ ] **Step 4:** `cargo test -p celesta-codec-test` passes.
- [ ] **Step 5: Commit:** `test(codec-test): the program through the ring and the window`.

### Task 6: `ExportSession` pipelines frames and packets (`celesta-exporter`)

**Files:**
- Modify: `crates/exporter/src/encode.rs` (`Plugin`, `resolve`, `ring_for`, `encode_frame`, `encode_audio`, `flush`, `drain`, `finish`, `abort`, `ExportTiming`, `timing_line`), `crates/exporter/src/interleave.rs` (`peak_staged_bytes`), `crates/exporter/src/exporter.rs` and `crates/exporter/src/react_entry.rs` (`resolve` with the frame size; the React path resolves after `bridge.metadata()`; the timing line), `crates/exporter/src/media.rs` (the factory passes `ProviderUse` through, as P2a built it)
- Test: `crates/exporter/src/encode.rs` tests, `crates/exporter/tests/integration/export.rs`

**Interfaces:**
- Consumes: Task 4's `Pipeline`, `Answered`, `ProviderUse::Export { ring }`; Task 3's `ring_layout`, `PixelFormat::from_str` for manifest names.
- Produces, in `encode.rs`:
  ```rust
  pub(crate) const WRITE_BATCH_PACKETS: usize = 8;
  pub(crate) const WRITE_BATCH_BYTES: usize = 4 << 20;
  pub(crate) const TIMING_VARIABLE: &str = "CELESTA_EXPORT_TIMING";
  struct Plugin { provider: Box<dyn MediaProvider>, ready: VecDeque<Answered> }   // one per started process
  impl Plugin {
      fn depth(&mut self) -> usize;       // provider.pipeline().depth(), or 1 for an in-process provider
      fn in_flight(&mut self) -> usize;   // the pipeline's, or ready.len()
      fn send_video_frame(&mut self, session: SessionId, pts: i64, frame: Vec<u8>) -> Result<(), ExportError>;   // the pipeline, or encode_video and ready.push_back(Answered::VideoFrame(..))
      fn send_audio(&mut self, …) -> Result<(), ExportError>;  fn send_packets(&mut self, …) -> Result<(), ExportError>;
      fn receive(&mut self) -> Result<Answered, ExportError>;  // the pipeline's, or ready.pop_front()
  }
  impl ExportSession {
      pub(crate) fn resolve(&mut self, choice: &OutputChoice, output: &Path, frame: (u32, u32)) -> Result<OutputPlan, ExportError>;   // P2a's, plus the frame size the ring is sized for
      fn ring_for(formats: Option<&[PixelFormat]>, frame: (u32, u32)) -> Option<RingLayout>;   // None for None; else ring_layout(formats, w, h)
      fn make_room(&mut self, plugin: usize) -> Result<(), ExportError>;   // while in_flight() >= depth(): drain_one
      fn drain_one(&mut self, plugin: usize) -> Result<(), ExportError>;   // receive: VideoFrame → packets to the interleaver, first config kept; Audio → audio_packets, first config kept; Packets → nothing
      fn drain(&mut self, plugin: usize) -> Result<(), ExportError>;       // until in_flight() == 0
      fn flush(&mut self, force: bool) -> Result<(), ExportError>;         // try_open_muxer; take_ready into pending_write; one send_packets (after make_room) when force, or ≥ WRITE_BATCH_PACKETS packets, or ≥ WRITE_BATCH_BYTES bytes wait
      pub(crate) fn frames_sent(&self) -> u64;
      pub(crate) fn timing(&self) -> SessionTiming;
  }
  pub(crate) struct SessionTiming { pub peak_staged_bytes: usize, pub spilled: bool }   // from the interleaver
  pub(crate) struct ExportTiming { pub render: Duration, pub frames: u64, pub mix: Option<Duration>, pub mix_done_at_frame: Option<u64>, pub peak_staged_bytes: usize, pub spilled: bool }
  pub(crate) fn timing_line(timing: &ExportTiming) -> String;
  // interleave.rs
  impl Interleaver { pub(crate) fn peak_staged_bytes(&self) -> usize; }
  ```
- Process starts. The describe callback (an uncached plugin) starts its
  process with `ProviderUse::Export { ring: ring_for(formats, frame) }`,
  where `formats` is `None` when the plugin's manifest has no video
  encoder, else the union of its video encoders' `input_formats` parsed as
  `PixelFormat` (unknown names dropped; an encoder with
  `input_formats: None` makes the union empty, so `rgba16` sizes it). A
  process started after `resolve` for the chosen video encoder's plugin
  uses `Some(&description.input_formats)`; for any other plugin, `None`.
- `encode_frame(frame)`: `make_room(video plugin)`, `send_video_frame(session, pts, frame.into_pixels())`,
  `frames_sent += 1`, `flush(false)`. `encode_audio`: per chunk
  `make_room` then `send_audio`; then `drain`, the sync `finish_audio`,
  the audio packets to the interleaver, `finish_audio` on it, `flush(false)`.
  `finish`: `drain` every plugin; the sync `finish_video`; `finish_video`
  on the interleaver; `flush(true)`; `drain` the muxer's plugin; the sync
  `finish_muxer`. `abort`: a plugin with `in_flight() == 0` gets P2a's abort
  requests; one with requests in flight is dropped (deviation 22).
- `timing_line` format, exactly:
  `celesta: timing: render {render:.2} s for {frames} frames; mix {mix:.2} s, done at frame {n}; staged peak {peak_mb:.1} MB; spilled {yes|no}`,
  with `mix none` and no `done at` when `mix` is `None` (a React entry
  without audio), and `done after the render` when `mix_done_at_frame ==
  Some(frames)`. The exporter prints it on stderr when `TIMING_VARIABLE`
  is `1`, for a project (the render loop records when the mix channel
  answered and `frames_sent()` then) and for a React entry.

- [ ] **Step 1: Write failing tests.**
  - `encode.rs` tests, with a fake plugin whose provider implements `pipeline()` (depth 3; it records every send, answers `receive` from a queue of canned `Encoded`s, one 100-byte packet and a config on the first frame, and counts receives) and a recording muxer fake in a second plugin:
    `frames_are_sent_up_to_the_depth_before_waiting`: three `encode_frame`s record three sends and no receive; the fourth records one receive first.
    `write_packets_are_batched`: after `no_audio()`, 20 frames, and `finish`, the muxer recorded every packet in dts order and at most `20 / 8 + 2` `writePackets` requests.
    `finish_drains_the_window_first`: after two frames, `finish` makes the fake's receive count equal its send count before it records `finishVideoEncoder`.
    `abort_with_frames_in_flight_does_not_wait`: a fake whose `receive` panics; after two frames, `abort` returns and the fake was dropped without a receive.
    `a_refused_frame_fails_the_export`: the fake's second `receive` answers `Err(MediaError::Codec("boom"))`; the third `encode_frame` (which drains it) returns `ExportError::Codec { stage: "video encoding", .. }` whose source contains `"boom"`.
    `the_ring_is_sized_from_the_plugins_formats`: a registry whose factories record their `ProviderUse`: plugin `v` (video encoder `inputFormats ["yuv420p", "rgba8"]`, describe cached) and `a` (audio only); `resolve(…, (64, 48))` then `open_video` start `v` with `Export { ring: Some(RingLayout { slots: 3, slot_bytes: 12_288 }) }` and `encode_audio` starts `a` with `Export { ring: None }`.
    `an_uncached_describe_sizes_the_ring_from_the_manifest`: `v` not cached, manifest `inputFormats ["yuv420p"]` → `Export { ring: Some({3, 4_608}) }` at `(64, 48)`; a plugin whose encoder has no `inputFormats` → `{3, 24_576}`.
    `no_ring_when_the_layout_is_none`: `(8192, 4320)` with `["rgba16"]` → `Export { ring: None }`, and the export runs (the fake has no segment to lose).
    `timing_line_formats_the_measurement`: `ExportTiming { render: 4.5 s, frames: 300, mix: Some(1.25 s), mix_done_at_frame: Some(87), peak_staged_bytes: 12_582_912, spilled: false }` → `"celesta: timing: render 4.50 s for 300 frames; mix 1.25 s, done at frame 87; staged peak 12.0 MB; spilled no"`; `mix_done_at_frame: Some(300)` → `"… mix 1.25 s, done after the render; …"`; `mix: None` → `"… mix none; …"`.
  - `tests/integration/export.rs`: `an_export_through_the_programs_ring_is_byte_identical_to_the_pipe`: the `<Video>` + `<Audio>` test entry exported twice by the `celesta-exporter` child with `TestCodecPlugin` on `--plugin-dir`, once with `CELESTA_CODEC_SHARED_MEMORY=1` and once `=0` in the child's environment; the two files are equal byte for byte. P2a's `cancel_during_encoding_leaves_no_files` and the other ported tests pass unchanged.
- [ ] **Step 2:** `cargo test -p celesta-exporter encode export` fails to compile.
- [ ] **Step 3: Implement.** `export_project_cancellable` passes `(project.settings.width, project.settings.height)` to `resolve`; `export_react_entry_impl` moves `ExportSession::start` and `resolve` after `bridge.metadata()` and passes `(metadata.width, metadata.height)`.
- [ ] **Step 4:** `cargo test --workspace` passes (`celesta-codec-test` built), with `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo fmt --all -- --check`.
- [ ] **Step 5: Commit:** `feat(exporter): pipeline frames and packets through the send window`.

### Task 7: The FFmpeg plugin's `transports`, its process test, and `codec-bench` (`plugins/ffmpeg`)

**Files:**
- Modify: `plugins/ffmpeg/src/components.rs` (`transports`), `plugins/ffmpeg/tests/process.rs`, `plugins/ffmpeg/examples/codec-bench.rs`
- Test: `plugins/ffmpeg/tests/encoders.rs` (`describe_names_every_manifest_component` also checks `transports`), `plugins/ffmpeg/tests/process.rs`

**Interfaces:**
- Consumes: Tasks 3–4; P2a's Task 13 `codec-bench`.
- Produces: the three video encoders describe `transports: ["sharedMemory", "pipe"]`.
  `codec-bench`'s `encode` opens `libx264` with `preset=ultrafast` through
  `VideoEncoder::open_video` (offered `[Rgba8]`), feeds frames through
  `provider.pipeline()` when it has one (`make_room`-style: receive when
  `in_flight() == depth()`), else `encode_video`; the plugin factory is
  called with `ProviderUse::Export { ring: ring_layout(&[Rgba8], width, height) }`;
  the line printed names the transport (`ring`, `pipe`, or `in process`) and
  the depth. Its doc comment says `CELESTA_CODEC_SHARED_MEMORY=0` compares
  the pipe, pipelined.

- [ ] **Step 1: Write failing tests.**
  - `tests/encoders.rs`: `describe_names_every_manifest_component` also asserts each video encoder's `transports == ["sharedMemory", "pipe"]`.
  - `tests/process.rs`: `frames_reach_the_encoder_through_the_ring`: the program started with `Export { ring: ring_layout(&[Rgba8], 32, 16) }` and `SharedMemory` has `shares_frames()`; ten 32×16 `rgba8` frames pipelined (three in flight), finished, muxed to MP4 through the program; the same with `Pipe`; both MP4s are byte-identical, and decoding one gives ten 32×16 frames.
- [ ] **Step 2:** `cargo test --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg` fails to compile.
- [ ] **Step 3: Implement.** Port the existing `process.rs` tests to `start_with(…, ProviderUse::Decode)`.
- [ ] **Step 4:** `cargo test --manifest-path plugins/Cargo.toml --workspace --locked`, its clippy with `-D warnings`, and `cargo fmt --check` in `plugins/` pass; `cargo build --release --manifest-path plugins/Cargo.toml -p celesta-codec-ffmpeg --example codec-bench` builds.
- [ ] **Step 5: Commit:** `feat(codec-ffmpeg): shared-memory frames for the encoders; codec-bench through the ring`.

### Task 8: Docs, the spec's deviations, the handoff note, the byte-identical check, and the measurements

**Files:**
- Modify: `packages/website/content/en/docs/05-(more)/05-plugins.md` (section "Shared memory and other FFmpeg builds") and `ja/docs/05-(more)/05-plugins.md` (「共有メモリーと別の FFmpeg ビルド」)
- Modify: `skills/celesta/references/plugins.md` (the shared-memory bullet at `:107-110`)
- Modify: `HANDOFF.md` rows for `celesta-plugin` (the shared segment: one slot, or a ring of slots), `celesta-plugin-host` (`PluginChild`'s send window), `celesta-media` (pipelined export requests); the validation section gains `CELESTA_EXPORT_TIMING=1`
- Modify: `docs/superpowers/specs/2026-11-08-media-components-design.md`: deviations 19–23 in Transport, Deadlines and the window, Audio (the measurement's place), and Phases → P2b
- Create: `docs/handoff/2026-11-08-media-pipelining.md`

- [ ] **Step 1: Write the docs.** The guide's section says: decoded frames and, from this release, the frames an export sends to a video encoder cross memory shared with the plugin's process (the export's in a ring of three slots, so rendering and encoding overlap); frames larger than 4096×2560 (decoding) or an export whose ring would take more than 256 MiB go through the pipe; `CELESTA_CODEC_SHARED_MEMORY=0` keeps every frame, decoded or exported, in the pipe. Keep en and ja in step; the Agent Skill's bullet says the same in two sentences.
- [ ] **Step 2: Verify.** Each passes: `cargo test --workspace`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo fmt --all -- --check`; `cargo clippy -p celesta-plugin -p celesta-plugin-host --target x86_64-pc-windows-msvc -- -D warnings`; with `PKG_CONFIG_PATH`: `cargo test --manifest-path plugins/Cargo.toml --workspace --locked` and its clippy; `pnpm --dir packages/website test`; `git diff --check`.
- [ ] **Step 3: The byte-identical check against P2a.** In temporary worktrees, build release `celesta-exporter` and the FFmpeg plugin (`node scripts/plugins/build.mts plugins/ffmpeg`) from P2a's tip (the commit this branch stacks on) and from this branch, each staged alone as P1's note did (own `CELESTA_PLUGIN_PATH`, empty `HOME`). Export with each: `examples/greenscreen/composite.tsx` as MP4; a `<Video>` (the green-screen footage) + `<Audio>` (`examples/assets/voices/001.wav`) entry as MP4, as MP4 with `--loop`, as MOV, and as WebM. `cmp` each pair: **identical**, and this branch's exports with `CELESTA_CODEC_SHARED_MEMORY=0` identical too. A difference is a bug (the frames reach libx264 in the same order with the same `pts`; the one legitimate cause would be a thread-count difference, which the same machine rules out). Record the SHA-256 of each.
- [ ] **Step 4: The mix-against-render measurement.** With `CELESTA_EXPORT_TIMING=1`, export `examples/editor-demo.celesta.json`, `examples/minimal.celesta.json`, and `examples/voiceroid.celesta.json` as MP4 at their full length, three runs each, and the `<Video>` + `<Audio>` React entry once. Record per project: render time, mix time, the frame the mix arrived at (as a fraction of the frames), peak staged bytes, and whether the interleaver spilled (expected: the mix finishes well before the render and nothing spills; the React entry stages everything by design).
- [ ] **Step 5: The performance comparison** (release builds, Apple M4, three builds interleaved, medians of three runs each, load average noted): `main`, P2a's tip, and this branch.
  - `codec-bench` encode at 1080p and 4K, each build's own `codec-bench` against its own plugin (`main`'s drives protocol 1); its decode half too, unchanged.
  - A 300-frame React entry (the entries #775's note timed, or a full-frame `<Video>` with animated text whose source goes in the note) at 1920×1080 and at 3840×2160 as MP4, with `preset=ultrafast` (`main`: `--preset ultrafast`; P2a and this branch: `--video-option preset=ultrafast`) and with the default `medium`.
  - The 4K entry as MOV (ProRes 4444), whose packets cross the pipe twice.
  - Also this branch with `CELESTA_CODEC_SHARED_MEMORY=0` for the 1080p and 4K `ultrafast` exports, to attribute the ring's share.

  This branch must not be slower than `main`: its median is at or below `main`'s within `main`'s own run spread, for every row. An excess is a blocker to fix before the PR. Record every number.
- [ ] **Step 6: Write `docs/handoff/2026-11-08-media-pipelining.md`** in the P1 note's format: the specification as built (the ring's layout and sizing, the send window and its deadlines, the pipeline, the serve loop's slot reads, the export's flow and batching, the abort rule, the escape hatch, the timing line), deviations 19–23, docs, checks (Steps 2–5's results: the SHA-256 table, the timing table, the performance table), and what is not done (encoding straight from the mapping, content plugins, the decode slot's Windows commit charge, a same-process packet shortcut).
- [ ] **Step 7: Commit:** `docs: shared memory and pipelining (P2b)`.
