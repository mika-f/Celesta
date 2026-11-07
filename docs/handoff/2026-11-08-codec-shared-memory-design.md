# Codec frames through shared memory: design (2026-11-08)

A design proposal. `2026-11-08-plugin-performance.md` removed the copies
around the codec plugin's pipe and left the pipe itself; this note works
out what moving frames through shared memory would take, what it would
gain, and what has to be decided first.

**Status.** Option B was built for decoded frames
(`2026-11-08-codec-shared-memory.md`). A review of this note found points
that the build does not follow yet, and that it should before it merges:
the generation check cannot see a slot written under its reader (C below),
the descriptor is moved to a fixed number 3 (Platform mechanisms), the
plugin leaves that descriptor open for processes it starts, every backend
gets a segment whatever it does (Segments), and on Windows the whole
section is committed at creation. Each is written below as the design
now says it.

## Goal and expected gain

`codec-bench` and `preview-bench` on an Apple M4 (Metal, release builds,
load average about 10), after `2026-11-08-plugin-performance.md`'s change.
This is a later run than that note's, so its numbers differ a little:
decode rows are medians of three runs, encode rows medians of four runs of
the corrected bench (which times the whole run of frames), 240 frames a run.

| per frame | in process | through the plugin | overhead |
| --- | --- | --- | --- |
| decode 1080p | 0.80 ms | 1.83 ms | 1.0 ms |
| decode 4K | 3.25 ms | 8.26 ms | 5.0 ms |
| encode 1080p (`ultrafast`) | 1.75 ms | 2.83 ms | 1.1 ms |
| encode 4K (`ultrafast`) | 7.40 ms | 14.15 ms | 6.6 ms |

What shared memory would leave of the decode overhead. Requests are
serial, so every copy on either side waits on the next frame:

- **Copy out of a slot** (option B): the plugin copies its frame into the
  slot and the host copies it out, each one `memcpy` of the frame
  (measured at 0.18–0.21 ms for 8.3 MB, 1080p, and 0.72–1.04 ms for 33 MB,
  4K). Left: about 0.4 ms (1080p) and 1.8 ms (4K), so B saves about
  0.6 ms and 3 ms a decoded frame (an estimate from those copies). The
  build measured 1.73 → 1.10 ms and 7.32 → 4.66 ms, which agrees.
- **Upload straight from the slot** (option D): no copy on the host; the
  plugin's copy stays. About 0.9 ms more than B at 4K.

Converting straight into the slot would not remove the plugin's copy
either: the slot is shared by every session of the process, so a codec
that answers a repeat request or holds its last frame past the end keeps a
copy of its own.

Against the rest of a 4K preview frame. `preview-bench` reports two
times: `node`, the React bridge evaluating the scene (about 0.3 ms at 4K),
and `gpu`, `render_preview`, which is decoding, uploading, and drawing
together (24.5–25.1 ms, three runs). Taking the plugin's decode (about
8 ms) off leaves about 17 ms of everything else; it is not a measurement
of uploading and drawing alone. Against that, B's 3 ms is about 12% of the
frame.

**Plainly: shared memory was not worth doing before the rest of the 4K
frame was looked at.** 4K at 30 fps already fit (25 ms of a 33 ms frame);
4K at 60 fps (16.7 ms) needed that rest cut whatever happened to the pipe.
At 1080p the whole preview frame is 6–10 ms, so 0.6 ms matters little.

Since then (`2026-11-08-plugin-profiling.md`), sampled rather than taken
by difference: the upload was about 7 ms of that rest (fresh staging and
texture memory) and now takes about 1.5 ms; 4K preview went from 24.6 to
21.2 ms a frame (19.3 ms in a quiet run). The plugin already decodes
ahead (ez-ffmpeg's frame export keeps the next frame ready on its own
threads; the plugin waits on a decoded frame 0.1% of the time), so its
wait is the transfer alone. A 4K preview frame then spent about 8.3 ms
waiting for the plugin's frame (4.4–4.9 ms of pipe in isolation, 7–8 ms
while the GPU upload and FFmpeg's decoding share the memory bandwidth),
about 6.6 ms waiting for the GPU, and about 2 ms uploading. That made the
transfer the largest single cost, and the case for B. The alternative
without a protocol change is the host asking for a sequential session's
next frame as soon as it has the current one, on a thread of its own, so
the transfer overlaps the render instead of being removed.

Encoding:

- **The encoder already runs beside the host.** The FFmpeg plugin's
  `write_frame` (`plugins/ffmpeg/src/encoder.rs`) calls ez-ffmpeg's
  `VideoWriter::write_owned`, which returns once the frame is in its
  bounded queue; conversion and encoding happen on ez-ffmpeg's worker
  threads. So `frameEncoded` comes back early, and what the host waits for
  is the frame's way through the pipe (1.1 ms at 1080p, 6.6 ms at 4K with
  `ultrafast`). An earlier version of this note blamed waiting for
  `frameEncoded` and about 20 ms of extra plugin CPU a 4K frame; the CPU
  was the bench timing only the `encode_frame` calls, and the waiting
  matters only when that queue is full.
- **Pipelining is not cheap.** Sending the next frame before the last
  one's answer would overlap the transfer with the host's next render, but
  needs two encode slots or more and a protocol that pairs answers with
  requests: `PluginChild` has one request in flight and treats an answer
  out of order as a plugin out of step.
- **Each encoded frame is a new 33 MB `Vec` in the plugin** (the protocol
  reads it into one, and `write_owned` takes it), allocated, faulted in,
  and freed every frame; that may cost the plugin CPU (a guess, not
  profiled). `write(&[u8])`, which copies into ez-ffmpeg's own buffer pool,
  would drop the allocation, and with shared memory make the slot-to-`Vec`
  copy that copy instead.
- **Exports are dominated by libx264** (about 15 ms a 1080p frame and 60 ms
  a 4K one with `medium`), so neither changes an export's time by more than
  a few percent.

## Who holds a frame, and for how long

- **Requests are serial per process.** Each `MediaBackend` has its own
  codec process, and `PluginChild` has one request in flight. Frames of
  several layers (sessions) interleave, but never overlap.
- **The plugin keeps its last frame.** The FFmpeg codec's decoder keeps the
  frame it last served (an `Arc` share) to answer a repeat request or hold
  past the end.
- **The host keeps frames past the next request, briefly.** `VideoFrame`
  (`celesta-media`) is public, with `pixels: Arc<Vec<u8>>`. The GPU
  renderer prepares a plane's content twice (to measure it, then to draw
  it) but decodes its frame once and keeps it for the draw
  (`plane_video_frames`, `crates/gpu-renderer/src/prepare.rs`): one frame
  held a layer. It wraps the `Arc` in a `DecodedImage` and copies it into
  wgpu when it uploads; the CPU renderer reads it as it draws. Anything
  holding a `VideoFrame` (tests, tools) can keep it as long as it likes, so
  the host cannot know when a frame's memory is free unless the type tells
  it.
- **The trait decides the copies.** `CodecProvider::decode_video` returns a
  `VideoFrame` that owns its pixels, so a plugin copies into the slot;
  `encode_frame` takes an owned `Vec<u8>`, so a plugin copies out of the
  slot into a new `Vec` (two user-space copies with the host's, replacing
  the pipe's two kernel copies, for less gain than decoding). Changing the
  trait (borrowed pixels in, a destination to decode into) is an open
  question.

## Options

**A. A ring of slots with release messages.** The plugin writes a frame
into a free slot and answers with the slot's number; the host sends
`releaseFrames { slots }` (or adds the released slots to its next request)
once nothing holds the frame. `VideoFrame`'s pixels would need a handle that
releases its slot when dropped. Every frame held too long pins a slot, and a
leaked handle pins it for good; when no slot is free the plugin answers
through the pipe instead. It needs a type change to `VideoFrame` and the
bookkeeping of D, with an extra message.

**B. The host copies out at once (recommended).** One slot a direction a
process, for the directions a backend uses (Segments, below). The plugin
writes the frame into the slot and answers `videoFrame` naming it; the
host copies the pixels into a new `Vec<u8>` before it returns, so
`VideoFrame` and everything that holds one are unchanged. Since requests
are serial, the slot is free again as soon as the copy is done, before the
host's next request: no release message, no ring. It saves the pipe's two
kernel copies and its 64 KiB-at-a-time wake-ups, less the two copies it
adds (about 0.6 ms at 1080p, 3 ms at 4K, above).

**C. Generation counters, as a seqlock.** A safety check for B and the
lease bookkeeping of A and D. Each slot's header holds a generation:

- The writer stores an odd generation (the slot is being written) before
  it touches the data, writes, then stores the next even generation with
  release ordering, then answers naming that even generation.
- The reader loads the generation with acquire ordering and refuses the
  frame if it is odd or not the answer's; copies the data out; issues an
  acquire fence; loads the generation again and refuses the frame unless it
  is unchanged.

Without the odd step, a plugin that broke the protocol and began the next
frame while the host copied would leave the old generation in place, and
the host would pass a half-written frame. A refused frame means the codec
is out of step: the host starts another (B), or the lease is dropped
(A, D).

**D. Upload straight from the slot.** `VideoFrame`'s pixels would hold
either their own `Vec` or a mapped slot. Two problems:

- **Soundness.** A `&[u8]` into memory another process may write is
  undefined behaviour in Rust's memory model (a plugin with a bug may write
  it), and `write_texture` takes a `&[u8]`. D would have to copy from a raw
  pointer straight into wgpu's mapped staging memory (as `VideoUploads`
  keeps it), never forming a reference into the slot. B's host copy follows
  the same rule: raw-pointer copies, confined to the `unsafe` module that
  owns the mapping.
- **Slots and gain.** It needs as many slots as frames held at once (one a
  video layer), A's ring, and C's leases, falls back to the pipe when they
  run out, changes a public type every consumer reads, and saves only the
  host's copy (about 0.9 ms at 4K).

### Failure cases

| case | B | A / D |
| --- | --- | --- |
| The plugin crashes while writing | The host reads a slot only after its answer arrives, so a crash before the answer fails the request as today (`CodecUnavailable`, retried once on a new process with a new segment). | The same; slots it had leased are dropped with its segment once the frames holding them go. |
| The host is killed | The plugin's stdin closes and it exits, as today. The segment has no name by then (below), so the kernel frees it when the plugin's mapping goes. | The same. |
| A slot is written while read | Cannot happen while the plugin keeps to the protocol: it writes only while answering a request, and the host copies before sending the next. C's odd generation catches a plugin that breaks it. | Prevented by leases; C refuses a frame written under its reader. |
| The plugin shrinks the segment (`ftruncate`) | A host reading past the end would get `SIGBUS`. Linux: the memfd is sealed against shrinking and growing. Windows: a section's size is fixed. macOS: a POSIX shared memory object takes one `ftruncate` only (checked: a second, shrinking or growing, fails with `EINVAL`). | The same. |
| Cancel / kill switch (`2026-11-06-plugin-kill-switch.md`) | Killing the process ends the request it waits on (`RequestError::Stopped`, then `MediaError::CodecStopped`); no answer means no slot is read. The host unmapping its view does not touch the child's, so what matters is that the host never unmaps while it copies: `CodecProcess`'s requests take `&mut self`, so the copy is over before anything can drop the segment. A killed switch starts no process and so makes no segment. | Mapped frames hold an `Arc` of the mapping, so killing or replacing the codec never unmaps memory a frame still reads; the new process gets a new segment. |
| A slot answer the host did not offer | Out of step, as an answer out of order is: `CodecUnavailable`, and another codec is started. | The same. |
| A frame larger than a slot, or no slot free | The plugin answers through the pipe as today; the host takes either answer, every time. | The same. |

**Bounds.** The host takes a slot's offset and capacity from its own table
(what it created) and the frame's length from `width * height * 4` of the
answer, as `decode_video_frame` checks a payload's length today; it never
reads an offset or length it acts on from the shared header. A length the
writer stores there is only compared with the expected one.

## Platform mechanisms

The segment belongs to the host: it creates it, gives the plugin a handle,
and maps it for the process's lifetime. A segment a process: a new one for
each start, never shared between processes or users.

- **Linux.** `memfd_create("celesta-codec", MFD_CLOEXEC | MFD_ALLOW_SEALING)`,
  `ftruncate` to the size, then `F_ADD_SEALS` with `F_SEAL_SHRINK |
  F_SEAL_GROW | F_SEAL_SEAL`. It has no name anywhere, so only processes
  holding the descriptor can map it (or one allowed to read the host's
  `/proc/<pid>/fd`, which is the same user with ptrace rights).
- **macOS.** No memfd. `shm_open` with a random name (at most 31 bytes,
  `PSHMNAMLEN`), `O_CREAT | O_EXCL | O_RDWR`, mode `0600`, `ftruncate`, then
  `shm_unlink` at once, before the plugin starts: from then on it has no
  name, and only the inherited descriptor reaches it. A host killed in the
  microseconds between create and unlink leaks one empty-named object until
  reboot; the random name and `0600` keep other users out during that time.
- **Passing it on (Unix).** In the child only, a `CommandExt::pre_exec`
  clears `FD_CLOEXEC` on the segment's own descriptor, and the `hello`
  request names that descriptor's number (the same in the child, which
  `fork` copied). Moving it to a fixed number instead (`dup2` to 3) has two
  problems: a process with only descriptors 0–2 open gets 3 from
  `memfd_create` or `shm_open`, where `dup2(3, 3)` does nothing and leaves
  close-on-exec set, so the segment silently does not arrive; and
  descriptor 3 in the child may be one std still uses after `pre_exec` (the
  pipe it reports an `exec` failure through), which `dup2` would close.
  Clearing close-on-exec in the parent instead would race other threads
  starting processes. `pre_exec` makes std use `fork` + `exec` instead of
  `posix_spawn`, which `PluginChild` can afford once a process.
- **In the plugin (Unix).** Once mapped, the plugin sets `FD_CLOEXEC` on the
  descriptor, or closes it (the mapping outlives it), so processes the
  plugin starts do not inherit the segment.
- **Windows.** `CreateFileMappingW(INVALID_HANDLE_VALUE, NULL,
  PAGE_READWRITE, …)` with no name, so no other process can open it by
  name; the size is fixed at creation. After the child starts, the host
  `DuplicateHandle`s the section into it (the host holds its child's
  process handle with `PROCESS_DUP_HANDLE`) and names the duplicated
  handle's value in `hello`; no change to how `Command` inherits handles.
  Both sides `MapViewOfFile`; the section is freed when both handles close.
- **Where the code lives.** The mapping type (open a handle, map it, slot
  layout and generation headers, `unsafe` confined there) goes in
  `celesta-plugin`, so codec plugins use the same code; it keeps to its
  dependencies (serde, serde_json, thiserror, libc), declaring the few
  Windows functions it calls itself, as `protocol.rs` already does for
  `SetStdHandle`. Creating the segment and handing it to the child goes in
  `celesta-plugin-host`'s `PluginChild`; `celesta-media`'s `CodecProcess`
  asks for it and reads the slots.

**Memory.** Pages are not resident until written, but not free either:

- On Linux and macOS a page, once written, stays resident for the
  segment's life: one 8K frame (133 MB) keeps that much for the process's
  lifetime. Linux can give pages back (`fallocate` with
  `FALLOC_FL_PUNCH_HOLE`, or `madvise(MADV_REMOVE)`) after a run of frames;
  macOS has no equivalent for a shared object short of a new segment.
- On Windows a pagefile-backed section made with the default `SEC_COMMIT`
  is probably charged against the commit limit in full when it is created
  (to be verified), physical pages or not. `SEC_RESERVE`, committing the
  view with `VirtualAlloc(MEM_COMMIT)` as frames need it, may avoid that
  (to be verified).

A frame larger than its slot goes through the pipe.

## Segments: which backend, which direction, how large

A `MediaBackend` has one codec process. The editor alone makes backends in
`audio.rs`, `view.rs`, `media_probe.rs`, `react_audio.rs`, and
`add_element.rs`; the exporter keeps decoding and encoding in separate
backends (`crates/exporter/src/render.rs`). So:

- a backend that only probes or decodes audio must not get a segment;
- one that decodes video needs the decode slot only, and the exporter's
  encoder the encode slot only;
- the slot's size is fixed at `hello`, before any frame's size is known,
  and a sealed memfd cannot grow: a size for the largest frame expected
  (4K, say), and the pipe beyond it.

The backend should say what it is for (decoding video, encoding, neither)
when it is made, and the codec process create only that segment. Giving
pages back after use (above) is part of the same question.

## Protocol

- **Negotiated in `hello`, so either side falls back.** The host's `hello`
  request gains `"frames": { "sharedMemory": { "handle", "bytes" } }` (with
  a direction, once a backend can have either). A plugin that maps it
  answers with `"frames": "sharedMemory"` in its `hello`; one that does not
  (every plugin today) ignores the field, and answers without it, and the
  host closes the segment and uses the pipe. A plugin that does not get the
  field (an older Celesta) uses the pipe.
- **A way for the plugin's code to see `hello`.** `serve_requests_on`
  answers `hello` inside `celesta-plugin` and never shows it to the
  plugin's closure, so a plugin has no way to accept. `celesta-plugin`
  needs, in its public API: a hook that sees the `hello` request and adds
  fields to the answer (the build added `serve_requests_with_hello` and
  `serve_requests_with_hello_on`, leaving the old functions as they were);
  a way to hand what the hook mapped to the closure that answers requests
  (shared state both closures capture, in the codec role); and no new
  `Answer` variant, since a slot answer is a header without a payload.
  `celesta-plugin` is versioned with Celesta and published; these are
  additions, which a 0.x minor release carries.
- **Per message, not per session.** `videoFrame` carries either a payload,
  as today, or `"slot": { "generation", "bytes" }` and an empty payload.
  `encodeFrame` the same the other way: the host writes the frame into its
  encode slot and names it, and the plugin must be done reading before it
  answers `frameEncoded`. Both sides accept both forms always, so a full
  slot, a large frame, or a test forcing the pipe needs no renegotiation.
- **The numbers.** `protocol` stays 1: the wire format is unchanged and the
  feature is optional on both sides, like the `codec` role and scene content
  before it. The manifest's `celesta` range need not change for
  compatibility; a plugin build that uses it will say `>=0.10.0` anyway,
  because it is built against that Celesta's `celesta-plugin`.
- **Ordering.** C's sequence: the plugin stores an odd generation, writes
  the pixels, stores the next even generation with release ordering, then
  writes its answer to the pipe; the host reads the answer, loads the
  generation with acquire ordering, checks it, copies, fences, and checks
  it again.

## Determinism and testing

- **Byte-identical to the pipe.** `celesta-codec-test` (the lossless test
  codec, in process and as a program) gains shared-memory support; tests
  ask it for the same frames, exact and sequential, out of order, through
  the pipe and through shared memory, and compare bytes. The FFmpeg
  plugin's `tests/process.rs`, which already compares its process's frames
  with the in-process codec's, runs both ways. An export through each path
  must write byte-identical MP4s (as `2026-11-08-plugin-performance.md`
  checked for its change).
- **A switch to force the pipe**, such as `CELESTA_CODEC_SHARED_MEMORY=0`,
  for those tests and as a user's escape hatch.
- **Failure tests**, Unix and Windows: a plugin answering a slot out of
  range, a wrong or odd generation, or more bytes than the slot (the
  request fails and another codec is started, as for any answer out of
  step); a slot answer that was not offered; a plugin that dies while
  writing; killing the switch while a frame is written; a frame larger
  than its slot falling back to the pipe; the Linux seals refusing
  `ftruncate`; a host whose only open descriptors are 0–2 still handing
  the segment over.
- **Benchmarks.** `codec-bench` gains a third row (plugin, shared memory)
  for decode and encode at 1080p and 4K; `preview-bench` at both sizes;
  one export each. Expected, by the estimate above: decode overhead from
  1.0 to about 0.4 ms (1080p) and 5.0 to about 1.8 ms (4K).

## Recommendation

Option B, after the cheaper steps:

1. Profile what a 4K preview frame spends besides decoding, and the
   plugin's encode. Done: the upload is fixed
   (`2026-11-08-plugin-profiling.md`), and the encode overhead is the
   pipe's.
2. Decode-ahead in the plugin: already there, nothing to build.
3. Then B (or, without a protocol change, the host prefetching a
   sequential session's next frame; it hides the transfer behind the
   render rather than removing it, costs a background thread in
   `celesta-media`, and still moves 33 MB a 4K frame through the pipe),
   with C as a seqlock: one slot for the direction a backend uses, the
   host copying out, `VideoFrame` unchanged; Linux memfd with seals, macOS
   `shm_open` unlinked before spawn, the descriptor passed at its own
   number; Windows an unnamed section duplicated into the child;
   negotiated in `hello`, per message, with the pipe always accepted.
4. D only if 4K at 60 fps still needs the last millisecond after the rest,
   and only through raw-pointer copies into wgpu's staging memory.

Rough size of B: the mapping type in `celesta-plugin` for three platforms
with its tests (the larger part, mostly `unsafe` FFI and its checks),
`PluginChild` creating and passing the segment, the `hello` hook,
`CodecProcess` and the codec role's serve loop using slots, the test codec
and the FFmpeg plugin writing into them, the fallback and failure tests,
the bench rows, and this note's spec in the plugin system's handoff. About
three to five days, most of it Windows and CI on three platforms; D would
add the `VideoFrame` type change through `celesta-media`, both renderers,
and their tests.

## Open questions for the user

1. **Order.** Decided: profile first, done; decode-ahead turned out to be
   there already; then B, built for decoded frames.
2. **Option.** B (copy out, `VideoFrame` unchanged), or D (no host copy,
   a change to the public `VideoFrame` type and a slot ring)? Decided: B.
3. **Scope.** Decode only first, or decode and encode (export) together?
   Built: decode only.
4. **How far to trust a plugin.** Plugins are code the user approved and
   run as the user. Should a plugin bug that resizes or scribbles over the
   segment be survivable (the seals and macOS's one-`ftruncate` rule, and
   C's seqlock), or is crashing on a misbehaving plugin acceptable?
5. **The escape hatch.** Is an environment variable to force the pipe
   acceptable, or should it be a setting?
6. **Content plugins.** Should the same mechanism be designed for content
   plugins' textures and frames now, or left until one needs it?
7. **Segments by use.** Should a backend say what it is for, so that only
   video-decoding and encoding backends get a segment, of one direction?
   And should pages be given back after use where the platform allows?
8. **The trait.** Keep `CodecProvider` (a copy into the slot on decode,
   a copy out of it into a new `Vec` on encode), or change it to borrow
   pixels on encode and decode into a given destination?
9. **Windows commit.** Accept a 40 MiB commit charge a decoding process,
   or verify and use `SEC_RESERVE` with commits as frames need them?
