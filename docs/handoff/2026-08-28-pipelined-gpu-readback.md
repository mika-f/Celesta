# Pipelined GPU readback for export (2026-08-28)

Requested as a follow-up: "書き出し速度を速くしたい" (make export faster).
Profiled first rather than guessing — a temporary `CELESTA_EXPORT_PROFILE=1`
instrumentation (not committed) around `render_react_video`'s per-frame loop
showed, for both a tiny 640x360 `<Rect>`-heavy scene
(`homepage-demo.tsx`) and a 1920x1080 single-`<Text>` scene (`title.tsx`),
that **`GpuRenderer::render()` dominated at ~3.4-4.4ms/frame regardless of
resolution or scene complexity** — Node IPC was ~0.4-0.5ms/frame and the
FFmpeg pipe write scaled with resolution as expected, but the render cost
barely moved between the two very different scenes. That pointed at fixed
per-frame overhead rather than actual rendering work.

- **Root cause** (`crates/gpu-renderer/src/lib.rs`'s old `render()`): every
  single frame allocated a brand-new offscreen texture and readback buffer,
  submitted GPU work, then called `device.poll(wgpu::PollType::
  wait_indefinitely())` and blocked immediately for the CPU↔GPU round trip
  before doing anything else. Frame N+1 could not start until frame N's GPU
  work, readback, *and* the FFmpeg pipe write had all fully completed — a
  fully serial submit→wait→copy→write chain with no overlap between Node
  evaluation, GPU rendering, and FFmpeg encoding.
- **Fix**: `GpuRenderer::submit(&mut self, scene) -> Result<Option<GpuFrame>,
  GpuRenderError>` and `GpuRenderer::drain(&mut self) -> Result<Vec<GpuFrame>,
  GpuRenderError>`, alongside (not replacing) the existing synchronous
  `render()` used by the editor's one-off preview rendering and existing
  tests. `submit` encodes and submits a frame's GPU work and kicks off its
  `map_async` readback *without blocking*, reusing a ring of
  `PIPELINE_DEPTH = 3` texture/readback-buffer pairs (`ReadbackSlot`) instead
  of allocating fresh ones every call. Once `PIPELINE_DEPTH` frames are
  in flight, `submit` blocks to reclaim the *oldest* one (FIFO, so output
  order is always preserved) before submitting the new one, returning that
  reclaimed frame's pixels as `Some(GpuFrame)` (or `None` while the pipeline
  is still filling up). By the time a slot is reclaimed, the GPU has usually
  already finished it while the caller was busy with other frames' Node IPC/
  evaluation/FFmpeg writes, so the wait is short or free instead of a full
  submit-to-complete stall every frame. `drain()` blocks to collect whatever
  is still in flight, in order, once the caller is done submitting — needed
  since the last `PIPELINE_DEPTH - 1` frames never get returned by `submit`
  itself.
- **`celesta-exporter`**: `render_video` (plain project export) and
  `render_react_video` (React entry export) both now call `submit` per frame
  (writing its `Some(GpuFrame)` immediately when present) and `drain` after
  the loop (writing the remaining frames). A new `write_frame` helper
  de-duplicates the FFmpeg-stdin write shared by both call sites. Both
  export paths funnel through `GpuRenderer`, so both benefit from the same
  change without duplicating the pipelining logic.
- Verified: a new `celesta-gpu-renderer` unit test
  (`submit_and_drain_return_frames_in_submission_order_with_correct_content`)
  submits five distinctly-colored scenes (more than `PIPELINE_DEPTH`, so it
  exercises both the reclaim-while-submitting path and the final `drain`)
  and asserts every frame comes back in submission order with the exact
  color it was given — the concrete regression this pipelining could have
  introduced (slot mixups) is a shuffled or wrong-content frame, and this
  test is the direct check for that. Full workspace suite still green (125
  tests, up from 124). End to end: a release-build `celesta-exporter --react`
  A/B (git-stashed old code vs. new, `/usr/bin/time -p`, steady-state median
  of 3 runs) showed real time dropping from ~0.71s to ~0.58s for
  `homepage-demo.tsx` and ~0.84s to ~0.68s for `title.tsx` (~18-19%,
  meaningful but bounded by these exports' short frame counts and Node's own
  per-frame IPC being fast for such small compositions already; the fixed
  ~3-4ms/frame that was being fully serialized on every frame is now mostly
  hidden). Extracted a frame from both the old and new output of each
  composition and confirmed byte-identical raw pixel content
  (`ffmpeg -f rawvideo` + `cmp`), confirming the pipelining changed timing,
  not output. A plain (non-React) project export
  (`cargo run -p celesta-exporter -- examples/editor-demo.celesta.json`, 600
  frames) was also re-run end to end and still produces a valid h264+aac MP4.
- **Not done, deliberately scoped out for this pass**: overlapping the
  FFmpeg pipe *write* itself (currently still a blocking call on the same
  thread right after `submit`/`drain` return pixels) and overlapping Node
  IPC evaluation of frame N+1 with frame N's GPU work are both still
  possible further wins — the profiling data suggests they matter more for
  heavier/longer compositions than the ones measured here. `render_video`'s
  `mix_audio_graph_cancellable`/FFmpeg audio-mux stages, and `celesta-media`'s
  video decode path, were not profiled or touched in this pass.
