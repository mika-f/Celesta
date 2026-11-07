# Decoded frames through shared memory (2026-11-08)

The codec plugin's decoded video frames reach Celesta through memory
shared with its process instead of its stdout pipe. This is option B of
`2026-11-08-codec-shared-memory-design.md`, which the user chose after
profiling (`2026-11-08-plugin-profiling.md`, on its own branch) showed the
pipe had become the largest single cost of 4K preview (about 8 of 19 ms
a frame). Decided with it, by the design's recommendations: decoded frames
only (encoding still goes through the pipe), an environment variable to
force the pipe, the cheap protection against a misbehaving plugin, and
content plugins left for later.

## Specification

- **The segment.** `celesta_plugin::shared::SharedMemory`: one segment a
  codec process, which Celesta creates, maps for the process's lifetime, and
  hands over; nothing else can open it by name, and its size cannot change
  once made.
  - Linux: a `memfd` (`MFD_CLOEXEC | MFD_ALLOW_SEALING`), sealed with
    `F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_SEAL`.
  - Other Unix (macOS): a POSIX shared memory object with a random name,
    `O_EXCL`, mode `0600`, unlinked at once, before the plugin starts.
    macOS refuses a second `ftruncate` of such an object (checked: shrinking
    and growing both fail with `EINVAL`), so a plugin cannot shrink it under
    Celesta.
  - Windows: an unnamed pagefile-backed section (`CreateFileMappingW` with
    no name), whose size is fixed.
- **Handing it over.** `PluginChild::start_sharing` prepares the command
  with the segment (`SharedMemory::prepare`): on Unix, a `pre_exec` that
  clears close-on-exec on the segment's own descriptor in the child only
  (so other children started meanwhile do not inherit it); on Windows
  nothing. After the spawn, `SharedMemory::handle_for` gives the handle the
  child maps it by: the descriptor's number (the same in the child), or a
  handle `DuplicateHandle`d into the child (not inheritable). A segment
  that cannot be handed over leaves the process without it. The descriptor
  is not moved to a fixed number: `dup2` onto 3 could close a descriptor
  std still uses after `pre_exec` (the pipe it reports a failed `exec`
  through), and in a process with only 0–2 open the segment itself is 3,
  where `dup2(3, 3)` leaves close-on-exec set. The plugin sets
  close-on-exec again once it has mapped the segment, so processes it
  starts do not inherit it.
- **Negotiation** (`hello`, no change to `protocol`): Celesta's `hello`
  request carries `"frames": { "sharedMemory": { "handle", "bytes" } }`. A
  codec that maps it answers `hello` with `"frames": "sharedMemory"`; one
  that does not know the field (every plugin built before this) ignores it,
  as the serve loop ignores unknown fields, and Celesta closes the segment.
  A codec that cannot map what is offered says so on stderr and declines.
  The `celesta` range need not change: either side falls back.
- **Frames.** A `videoFrame` answer from a codec that accepted carries
  `"slot": { "generation", "bytes" }` and no payload when the frame fits the
  slot, and the payload as before when it does not. Celesta accepts both
  every time. The slot is the segment after a 64-byte header holding its
  generation (an atomic `u64`) and length, used as a seqlock. Generations
  answered are even and grow by 2; the codec stores the odd one between
  (relaxed), issues a release fence, copies the pixels in, stores the new
  even generation with release ordering, then answers. Celesta checks the
  answer's generation is newer than the last it read, loads the slot's with
  acquire ordering and refuses an odd one or another, copies the pixels out
  into a new `Vec` (so `VideoFrame` is unchanged) through raw pointers,
  issues an acquire fence, and checks the generation again. The slot's
  offset and capacity are Celesta's own and the length is the frame's
  `width * height * 4`; the length in the header is only compared with it.
  A slot answer that was not offered, is not the frame's size, or whose
  generation does not match is `MediaError::CodecUnavailable`: the codec is
  out of step, and the backend starts another, as for any other.
- **Who writes when.** Requests to a codec process are one at a time, and
  Celesta copies a frame out before it sends the next request, so the codec
  never writes the slot while Celesta reads it. The seqlock catches a codec
  that breaks this, mid-write included.
- **Size.** `SHARED_FRAME_BYTES`: the header and 40 MiB, an RGBA frame up to
  4096x2560 (UHD and DCI 4K). Larger frames go through the pipe. The pages
  are reserved, and used only as frames are written.
- **Lifetime.** `CodecProcess` drops its `PluginChild` (which waits for the
  process to exit, or kills it) before it unmaps the segment, so the process
  is never writing into memory being unmapped. A killed or stopped request
  has no answer, so no slot is read. Each process (a restart included) has
  its own segment.
- **The escape hatch.** `CELESTA_CODEC_SHARED_MEMORY=0` offers no codec
  process shared memory (`FrameTransport::from_environment`);
  `CodecProcess::start_with` takes the transport explicitly, which tests
  use. User guide (en, ja) and the Agent Skill's plugin reference say so.
- **Code.** `celesta-plugin` keeps to its dependencies: the Windows
  functions are declared in `shared.rs`, as `protocol.rs` declares
  `SetStdHandle`. The serve loop gained a `hello` hook
  (`serve_requests_with_hello[_on]`), whose fields join the `hello` answer
  without replacing its own. `PluginChild::hello_with` sends extra fields in
  the request.

## Measurements

Apple M4, Metal, release builds, the same binaries with
`CELESTA_CODEC_SHARED_MEMORY` `0` and `1`, interleaved, load average about
15–25. `codec-bench`, 240 frames a run, four runs a side (medians):

| decode, per frame | in process | pipe | shared memory |
| --- | --- | --- | --- |
| 1080p | 0.76 ms | 1.73 ms | 1.10 ms |
| 4K | 3.00 ms | 7.32 ms | 4.66 ms |

Celesta's CPU for reading a 4K frame went from 3.9 to 1.7 ms. The plugin's
went up, from about 25 to 31 ms a 4K frame: it now copies each frame into
the slot itself (about half of its main thread's time, sampled), where the
pipe's copy was the kernel's. Decoding straight into the slot would remove
that copy, but ez-ffmpeg's frame export hands out its own buffers.

`preview-bench` (the editor's preview path; this branch does not have the
upload fix of `2026-11-08-plugin-profiling.md`), five runs of 150 frames:

| per frame | pipe | shared memory |
| --- | --- | --- |
| 1080p | 5.6 ms | 5.0 ms |
| 4K | 20.9 ms | 16.6 ms |

Exports of the same entries took 3.63 → 2.60 s (1080p) and 11.84 → 10.15 s
(4K) for 300 frames, one run each, and wrote byte-identical MP4s with
shared memory, with the pipe, and with the exporter before this change.

## Checks

- `celesta-plugin`: a segment written on one side reads back on the other
  (through a second handle, as a plugin maps it); a read under another
  generation or length, or past the slot, is refused; a slot being written
  (an odd generation in its header) is not read; an odd or zero generation
  is not written; bytes that do not fit are not written; a segment smaller
  than asked for is not mapped; a new segment reads as zero; on Unix, a
  descriptor inherited without close-on-exec has it once mapped. The
  `hello` hook's fields join the answer.
- `celesta-media`: frames come through the slot once `hello` accepts it,
  and a frame read from a slot written over since is refused; a frame
  larger than the slot goes through the pipe; a `hello` without an offer, or
  with one that cannot be mapped, keeps the pipe; slot answers that were
  not offered, are the wrong size, or carry a stale, wrong, or odd
  generation are `CodecUnavailable`.
- `celesta-codec-test`: the program, as a real process, accepts the segment
  (at the descriptor's own number) and serves exact frames out of order and a sequential run
  (repeats, holding past the end) byte-identical to the pipe and to the
  codec in process. The kill-switch tests pass with the segment offered.
- The FFmpeg plugin's process tests, which compare its program's frames
  with the in-process codec's, run through shared memory now (the default).
- `cargo test --workspace` (2,395 passed, 1 ignored as before; the codec
  plugin built from this branch on `CELESTA_PLUGIN_PATH`), `cargo clippy
  --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --
  --check`, `cargo test --manifest-path plugins/Cargo.toml --workspace
  --locked` (412 passed) with its clippy and `cargo fmt --check`, and
  `git diff --check`, on macOS arm64. Clippy for `celesta-plugin` and
  `celesta-plugin-host` on `x86_64-pc-windows-msvc` (type-checked from
  macOS, not run); Linux and Windows run in CI.

## Not done

- **Encoding** still sends frames through the pipe (1.1 ms a 1080p frame,
  6.6 ms a 4K one with `ultrafast`; little next to libx264's `medium`). The
  same slot, written by Celesta and read by the codec before it answers
  `frameEncoded`, would carry them.
- **Uploading straight from the slot** (option D) would save Celesta's copy
  out (about 1 ms a 4K frame) but changes `VideoFrame`.
- **Content plugins** do not use it.

## Review follow-ups

The review of the design note (#773) found three points this build did
not follow, now fixed: the generation check as a seqlock (it could not see
a slot being written under its reader), the descriptor kept at its own
number instead of `dup2`ed to 3, and the plugin setting close-on-exec on it
once mapped. Two others are open questions in the design note: giving a
segment only to backends that decode video or encode (every codec process
gets one now, probing and audio-only ones included; their pages are
untouched, so they cost address space, not memory, on Linux and macOS),
and the Windows commit charge (each codec process's 40 MiB section is
probably charged in full at creation; not verified).
