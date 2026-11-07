# Codec frames through shared memory: design (2026-11-08)

A design proposal, not built. `2026-11-08-plugin-performance.md` removed
the copies around the codec plugin's pipe and left the pipe itself; this
note works out what moving frames through shared memory would take, what
it would gain, and what has to be decided first.

## Goal and expected gain

Measured as in `2026-11-08-plugin-performance.md` (Apple M4, Metal,
release builds, `codec-bench` and `preview-bench`, load average about 10,
after that note's changes):

| per frame | in process | through the plugin | overhead |
| --- | --- | --- | --- |
| decode 1080p | 0.80 ms | 1.83 ms | 1.0 ms |
| decode 4K | 3.25 ms | 8.26 ms | 5.0 ms |
| encode 1080p (`ultrafast`) | 1.75 ms | 2.83 ms | 1.1 ms |
| encode 4K (`ultrafast`) | 7.40 ms | 14.15 ms | 6.6 ms |

(The encode rows time the whole run of frames; see
`2026-11-08-plugin-performance.md` for why an earlier version of the bench
timed less.)

What shared memory would leave of that, by the options below:

- **Copy out of a slot** (option B): one `memcpy` of the frame, measured at
  0.18–0.21 ms (8.3 MB, 1080p) and 0.72–1.04 ms (33 MB, 4K), fresh buffer
  or reused. It saves about 0.8 ms (1080p) and 4 ms (4K) a decoded frame.
- **Upload straight from the slot** (option D): no copy on the host at all;
  about 1 ms more at 4K than B.

Against the rest of a 4K preview frame: the editor's preview path takes
about 25 ms a frame for a full-frame 4K video (`preview-bench`, three runs:
24.8–25.4 ms), of which the plugin's decode is about 8 ms and uploading and
drawing the frame about 17 ms. Shared memory takes at most 4–5 ms off the
25 (16–20%); the upload and draw cost is about three and a half times what
it could save.

**Plainly: shared memory is not worth doing before the 4K upload cost is
looked at.** 4K at 30 fps already fits (25 ms of a 33 ms frame); 4K at 60
fps (16.7 ms) needs the upload and draw cost cut whatever happens to the
pipe, and shared memory alone does not get there. At 1080p the whole
preview frame is 6–10 ms, so 0.8 ms matters little.

Since then (`2026-11-08-plugin-profiling.md`): the upload was most of
that 17 ms, about 7 ms of fresh staging and texture memory, and now takes
about 1.5 ms; 4K preview went from 24.6 to 21.2 ms a frame (19.3 ms in a
quiet run). The plugin already decodes ahead (ez-ffmpeg's frame export
keeps the next frame ready on its own threads; the plugin waits on a
decoded frame 0.1% of the time), so the plugin's wait is the transfer
alone. What a 4K preview frame spends now: about 8.3 ms waiting for the
plugin's frame, which is the pipe (4.4–4.9 ms in isolation, 7–8 ms while
the GPU upload and FFmpeg's decoding share the memory bandwidth), about
6.6 ms waiting for the GPU, about 2 ms uploading. **The transfer is now
the largest single cost of 4K preview, about 40% of the frame**, which
makes the case for shared memory: it would replace about 8 ms with a copy
of about 1 ms. The alternative that needs no protocol change is the host
asking for the next frame of a sequential session as soon as it has the
current one, on a thread of its own, so the transfer overlaps the render
and the GPU wait instead of being removed.

Also:

- **The encode overhead is the pipe.** An earlier version of this note
  said the plugin spent about 20 ms more CPU a 4K frame encoding than the
  same encode in process; that was the bench timing only the
  `encode_frame` calls. Timed whole, the CPU is about the same either way,
  and the overhead is wall-clock time: 1.1 ms (1080p) and 6.6 ms (4K), the
  frame's way through the pipe. Sending the next frame before the last
  one's answer (pipelining) could hide at most that transfer, less with
  the default `medium` preset, where libx264 is the bottleneck; it would
  change the protocol (several requests in flight, or an unanswered
  `encodeFrame`), so it is not built (`2026-11-08-plugin-profiling.md`).
  For exports, shared memory and pipelining aim at the same few
  milliseconds; neither matters much while the encoder dominates.
- **Exports are dominated by libx264** (about 15 ms a 1080p frame and 60 ms
  a 4K one with `medium`), so shared memory would change an export's time
  by a few percent.

## Who holds a frame, and for how long

- **Requests are serial per process.** Each `MediaBackend` has its own
  codec process, and `PluginChild` has one request in flight. Frames of
  several layers (sessions) interleave, but never overlap.
- **The plugin keeps its last frame.** The FFmpeg codec's decoder keeps the
  frame it last served (an `Arc` share) to answer a repeat request or hold
  past the end.
- **The host keeps frames past the next request, briefly.** `VideoFrame`
  (`celesta-media`) is public, with `pixels: Arc<Vec<u8>>`. The GPU
  renderer keeps a plane's frame for the rest of a render
  (`plane_video_frames`, decoded once to measure and once to draw), wraps
  the `Arc` in a `DecodedImage`, and copies it into wgpu at `write_texture`;
  the CPU renderer reads it as it draws. Anything holding a `VideoFrame`
  (tests, tools) can keep it as long as it likes, so the host cannot know
  when a frame's memory is free unless the type tells it.

## Options

**A. A ring of slots with release messages.** The plugin writes a frame
into a free slot and answers with the slot's number; the host sends
`releaseFrames { slots }` (or adds the released slots to its next request)
once nothing holds the frame. `VideoFrame`'s pixels would need a handle that
releases its slot when dropped. Every frame held too long pins a slot, and a
leaked handle pins it for good; when no slot is free the plugin answers
through the pipe instead. It needs a type change to `VideoFrame` and the
bookkeeping of D, with an extra message.

**B. The host copies out at once (recommended).** One slot a direction
(decode, encode) a process. The plugin writes the frame into the slot and
answers `videoFrame` naming it; the host copies the pixels into a new
`Vec<u8>` before it returns, so `VideoFrame` and everything that holds one
are unchanged. Since requests are serial, the slot is free again as soon as
the copy is done, before the host's next request: no release message, no
ring. It saves the pipe's two kernel copies and its 64 KiB-at-a-time
wake-ups (about 0.8 ms at 1080p, 4 ms at 4K), not the copy out (0.2 ms, 0.9
ms). On the plugin side the decoder keeps its own last frame and copies it
into the slot (the same 0.2–0.9 ms), or later converts straight into the
slot.

**C. Leases with generation counters.** A refinement of A or D, not an
alternative: each slot's header holds a generation the plugin bumps when it
writes, and the answer names `(slot, generation)`. The host checks the
header's generation (acquire load) before and after it reads; a mismatch
means the slot was written under it, and the frame is refused (asked again
through the pipe) instead of shown torn. B uses this as a cheap safety
check; A and D use it to make stale releases harmless (a release of an old
generation does nothing).

**D. Upload straight from the slot.** `VideoFrame`'s pixels become
`Pixels::Owned(Arc<Vec<u8>>) | Pixels::Mapped(Arc<MappedFrame>)`, readable
as `&[u8]`. The GPU renderer's `write_texture` copies into wgpu's staging
memory at once, so a mapped frame is released when the renderer drops it,
usually within the render. It saves the copy out too (about 1 ms more at
4K), but changes a public type every consumer reads, needs A's ring and C's
generations, and needs as many slots as frames held at once (one a video
layer, two for a plane's measure and draw), falling back to the pipe when
they run out.

### Failure cases

| case | B | A / D |
| --- | --- | --- |
| The plugin crashes while writing | The host reads a slot only after its answer arrives, so a crash before the answer fails the request as today (`CodecUnavailable`, retried once on a new process with a new segment). | The same; slots it had leased are dropped with its segment once the frames holding them go. |
| The host is killed | The plugin's stdin closes and it exits, as today. The segment has no name by then (below), so the kernel frees it when the plugin's mapping goes. | The same. |
| A slot is reused while read | Cannot happen: the plugin writes only while answering a request, and the host copies before sending the next. The generation check catches a plugin that breaks this. | Prevented by leases; the generation check refuses a frame written under its reader. |
| The plugin shrinks the segment (`ftruncate`) | A host reading past the end gets `SIGBUS`. Linux: seal the memfd against shrinking and growing. Windows: a section's size is fixed. macOS: `shm_open` objects reportedly take one `ftruncate` only, which must be verified; otherwise this is the plugin trusting question below. | The same. |
| Cancel / kill switch (`2026-11-06-plugin-kill-switch.md`) | Killing the process ends the request it waits on (`RequestError::Stopped`, then `MediaError::CodecStopped`); no answer means no slot is read. The host unmaps the segment after the child has exited, so the process is never mid-write into memory being unmapped. A killed switch starts no process and so makes no segment. | Mapped frames hold an `Arc` of the mapping, so killing or replacing the codec never unmaps memory a frame still reads; the new process gets a new segment. |
| A frame larger than a slot, or no slot free | The plugin answers through the pipe as today; the host takes either answer, every time. | The same. |

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
- **Passing it on (Unix).** The plugin inherits the descriptor at a fixed
  number: `CommandExt::pre_exec` `dup2`s it to descriptor 3 in the child
  only, so other children spawned meanwhile do not inherit it (clearing
  `FD_CLOEXEC` in the parent would race them). `pre_exec` makes std use
  `fork` + `exec` instead of `posix_spawn`, which `PluginChild` can afford
  once a process. The `hello` request names the descriptor and the size.
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

Memory: the segment's size is reserved, not resident. Pages are allocated
as they are first written, so two 4K slots (decode and encode, 33 MB each)
cost nothing until used, and an 8K slot (133 MB) only when an 8K frame
arrives. A frame larger than its slot goes through the pipe.

## Protocol

- **Negotiated in `hello`, so either side falls back.** The host's `hello`
  request gains `"frames": { "sharedMemory": { "handle": 3, "size": N,
  "slots": [{ "offset", "bytes", "direction": "decode" | "encode" }] } }`.
  A plugin that maps it answers with `"frames": "sharedMemory"` in its
  `hello`; one that does not (every plugin today) ignores the field, as
  `serve_requests_on` already ignores what it does not know, and answers
  without it, and the host closes the segment and uses the pipe. A plugin
  that does not get the field (an older Celesta) uses the pipe.
- **Per message, not per session.** `videoFrame` carries either a payload,
  as today, or `"slot": { "index", "generation", "bytes" }` and an empty
  payload. `encodeFrame` the same the other way: the host writes the frame
  into its encode slot and names it, and the plugin must be done reading
  before it answers `frameEncoded`. Both sides accept both forms always, so
  a full slot, a large frame, or a test forcing the pipe needs no
  renegotiation.
- **The numbers.** `protocol` stays 1: the wire format is unchanged and the
  feature is optional on both sides, like the `codec` role and scene content
  before it. The manifest's `celesta` range need not change for
  compatibility; a plugin build that uses it will say `>=0.10.0` anyway,
  because it is built against that Celesta's `celesta-plugin`.
- **Ordering.** The plugin writes the pixels, then stores the slot's
  generation with release ordering, then writes its answer to the pipe; the
  host reads the answer, loads the generation with acquire ordering, checks
  it, copies, and checks it again.

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
  range, a wrong generation, or more bytes than the slot (the request
  fails and the plugin is killed, as for any answer out of step); a plugin
  that dies while writing; killing the switch while a frame is written;
  a frame larger than its slot falling back to the pipe; the host dropping
  the backend unmaps only after the child exited; the Linux seals refusing
  `ftruncate`.
- **Benchmarks.** `codec-bench` gains a third row (plugin, shared memory)
  for decode and encode at 1080p and 4K; `preview-bench` at both sizes;
  one export each. Expected, by the measurements above: decode overhead
  from 1.0 to about 0.2 ms (1080p) and 5.0 to about 1 ms (4K).

## Recommendation

Option B, after the cheaper steps:

1. Profile 4K preview's upload and draw, and the plugin's encode. Done:
   the upload is fixed (`2026-11-08-plugin-profiling.md`), and the
   encode overhead is the pipe's.
2. Decode-ahead in the plugin: already there, nothing to build.
3. Then B (or, without a protocol change, the host prefetching a
   sequential session's next frame; it hides the transfer behind the
   render rather than removing it, costs a background thread in
   `celesta-media`, and still moves 33 MB a 4K frame through the pipe) with C's generation check: one slot a direction a process, the
   host copying out, `VideoFrame` unchanged; Linux memfd with seals, macOS
   `shm_open` unlinked before spawn, Windows an unnamed section duplicated
   into the child; negotiated in `hello`, per message, with the pipe always
   accepted.
4. D only if 4K at 60 fps still needs the last millisecond after the rest.

Rough size of B: the mapping type in `celesta-plugin` for three platforms
with its tests (the larger part, mostly `unsafe` FFI and its checks),
`PluginChild` creating and passing the segment, `CodecProcess` and the
codec role's serve loop using slots, the test codec and the FFmpeg plugin
writing into them, the fallback and failure tests, the bench rows, and this
note's spec in the plugin system's handoff. About three to five days,
most of it Windows and CI on three platforms; D would add the `VideoFrame`
type change through `celesta-media`, both renderers, and their tests.

## Open questions for the user

1. **Order.** Decided: profile first, done; decode-ahead turned out to be
   there already. Next: shared memory (B), host-side prefetch, or neither
   for now? (The GPU's 6.6 ms at 4K is the other open cost.)
2. **Option.** B (copy out, `VideoFrame` unchanged), or D (no host copy,
   a change to the public `VideoFrame` type and a slot ring)?
3. **Scope.** Decode only first, or decode and encode (export) together?
4. **How far to trust a plugin.** Plugins are code the user approved and
   run as the user. Should a plugin bug that resizes or scribbles over the
   segment be survivable (seals on Linux, verifying macOS's one-`ftruncate`
   rule or falling back to the pipe there), or is crashing on a misbehaving
   plugin acceptable?
5. **The escape hatch.** Is an environment variable to force the pipe
   acceptable, or should it be a setting?
6. **Content plugins.** Should the same mechanism be designed for content
   plugins' textures and frames now, or left until one needs it?
