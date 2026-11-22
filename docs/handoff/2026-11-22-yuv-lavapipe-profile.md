# YUV lavapipe slowdown: profile and follow-up

## Request and method

Explain the completed-texture regression from handoff B, splitting host decode,
upload submission, conversion and completion, and fix avoidable recurring work.
Keep the existing pixel gates. Do not infer hardware-GPU timing from lavapipe.

The existing ignored Rust benchmark now records nonoverlapping CPU intervals
and texture/staging pool misses, pipeline creation, bind-group creation and
queue-submit counts. Instrumentation and the no-dispatch diagnostic are test-only.
The benchmark still uses FFmpeg 8.1.3, asserted shared memory, prefetch on,
4 warmup + 120 measured frames, 0/15 ms host work and three interleaved pairs.
Adapter: Vulkan lavapipe/llvmpipe, Mesa 23.2.1 / LLVM 15.0.7, a CPU device.

Fresh measurements of the original implementation reproduce the regression
(1080p/0 ms: 1.977 vs 8.448 ms). These are new runs, not an invented retrospective
split of the earlier 2.068 vs 9.437 ms observation. All comparisons below use
the new paired before/after runs. Each breakdown row is the mean of the 120
frames in the run whose total is the median of its three repeats.

`decode_frame_for` timing is remaining host-visible latency with prefetch,
not isolated plugin CPU time. Completion wait includes pending upload/copy,
conversion and scheduling; it is not a timestamp-query measurement of the
shader alone. No extra synchronization was inserted between those stages.

## Evidence and diagnosis

The benchmark asserts that the YUV offer actually returns I420, then calls
`upload_yuv`, which dispatches the conversion, followed by the benchmark's
completion poll. This path executed in the reported software-GPU configuration.

To try to disprove shader execution as the main cost, the test-only
`CELESTA_YUV_BENCH_NO_DISPATCH=1` control retains decoding, copies, pipeline and
bind-group setup, the compute pass, both original queue submits and completion
poll, but omits dispatch. At 1080p/0 ms its YUV total is **1.059 ms**, completion
wait **0.098 ms**, against **8.448 / 7.346 ms** with dispatch. It is a diagnostic
with an unwritten output texture, not a usable rendered-frame benchmark.

This establishes that dispatch execution dominates the regression; it is not
primarily texture/staging pool misses or slow plugin packing. In particular,
the no-dispatch YUV cycle is faster than RGBA's 1.951 ms in that control run.
It does not establish standalone plugin decode/pack CPU cost. Both original
paths had zero timed texture/staging pool misses and pipeline creations.
There was no extra poll in YUV: the benchmark polls once per frame in both.

There was also verified avoidable work: original YUV creates a bind group and
output view per frame, obtains the pipeline layout again, and submits a second
command buffer. I420 redundantly fetches the same chroma for all four pixels
in a 2×2 block. These paths were reached on every timed YUV frame.

## Fix

- `video_yuv.rs`: cache the layout and conversion bindings/views by actual input
  and output texture identities; evict unused entries with the pools. Cache
  keys own GPU handles, not `LayerTexture` Arcs that would prevent pool reuse.
- `texture.rs`: record conversion after upload copies in the same encoder,
  then submit once. Staging remapping is requested after submission. The
  mapped-buffer failure fallback also records conversion and flushes writes.
- `video_yuv.wgsl`: one I420 invocation writes a 2×2 block, sharing its two
  chroma reads; dispatch dimensions account for this. Odd edges are guarded.
  NV12's bicubic reconstruction is unchanged.
- Tests assert warmed binding reuse and one submit, exercise queued
  conversions with different matrix/range values to check uniform ordering,
  and force unavailable staging mappings for RGBA, NV12 and I420.

In 120 timed frames: YUV bind groups **120 → 0**, submits **240 → 120**.
Both before and after have **0 pipeline creations and 0 texture/staging pool
misses**. RGBA submits 120 times throughout. Small CPU metadata vectors still
exist, including inside staging; the measured preparation/pool bookkeeping
outside staging is around 1–3 microseconds per upload. This is not a claim
to have timed every allocation separately. The pipeline is created only cold.

## Breakdown, 0 ms host work

All columns are ms/frame. "Conversion CPU" includes its separate submit before
the fix; afterward it is recording only, with the combined submit charged to
"Upload CPU". Compare their sum, not just that column. "Other" is the residual
including bookkeeping and instrumentation. Completion is a wait, not pure GPU
time. CPU intervals and device execution can overlap.

| Size | Path | State | Decode | Upload CPU | Conversion CPU | Completion wait | Other | Total |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1080p | RGBA | before | 0.606 | 0.560 | 0 | 0.808 | 0.003 | 1.977 |
| 1080p | YUV | before | 0.203 | 0.234 | 0.659 | 7.346 | 0.006 | 8.448 |
| 1080p | RGBA | after | 0.678 | 0.549 | 0 | 0.796 | 0.003 | 2.026 |
| 1080p | YUV | after | 0.233 | 0.298 | 0.011 | 5.309 | 0.005 | 5.856 |
| 4K | RGBA | before | 6.292 | 2.876 | 0 | 3.061 | 0.002 | 12.231 |
| 4K | YUV | before | 0.814 | 0.839 | 1.208 | 23.185 | 0.007 | 26.053 |
| 4K | RGBA | after | 5.929 | 3.069 | 0 | 3.249 | 0.003 | 12.250 |
| 4K | YUV | after | 0.808 | 0.853 | 0.013 | 16.902 | 0.006 | 18.582 |

## Updated complete benchmark table

Wall time includes simulated host work. Slot bytes are unchanged: 1080p
RGBA/YUV = 8,294,400 / 3,110,400; 4K = 33,177,600 / 12,441,600.

| Size | Host work | Before RGBA | Before YUV | After RGBA | After YUV |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1080p | 0 ms | 1.977 | 8.448 | 2.026 | 5.856 |
| 1080p | 15 ms | 17.041 | 23.678 | 17.140 | 21.053 |
| 4K | 0 ms | 12.231 | 26.053 | 12.250 | 18.582 |
| 4K | 15 ms | 24.090 | 41.254 | 24.300 | 33.886 |

After YUV run ranges: 1080p/0 = 5.575–5.870, 1080p/15 = 20.835–21.601,
4K/0 = 17.887–18.681, 4K/15 = 33.180–35.044 ms/frame.
**YUV remains slower on lavapipe.** Conversion executes on CPU shader workers
in this configuration. No hardware GPU was available: a claim that this pass
takes well under 1 ms on hardware is unverified and is not made here.

Raw logs in `/home/ubuntu/research/yuv-gate/`: `profile-before-{1080,4k}.log`,
`profile-after-{1080,4k}.log`, `profile-no-dispatch-1080.log`. They include every
repeat's stage means, host work and counters, including 15 ms workloads.

## Validation

All six YUV fixtures still read back **max 0 / mean 0 / differing 0** against
`to_rgba()`. Plain and effects/blend scene max/mean/differing counts are unchanged
from handoff B, including exact alpha and the unchanged ≤1/≤2 gates.

```text
cargo test --release -p celesta-gpu-renderer
test result: ok. 442 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 155.85s
cargo test -p celesta-exporter --lib
test result: ok. 191 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
cargo clippy -p celesta-gpu-renderer --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.61s
```

FFmpeg plugin runner: 103 passed, 0 failed. Root fmt and diff checks exited 0.
Media and codec-process sources are unchanged; their preceding handoff checks
remain applicable. No native macOS/Windows or hardware-GPU run was performed.

Same-scene before/after screenshots (not committed), visually checked:

- Before: `/home/ubuntu/research/yuv-shots/yuv-effects-true.png`
- After: `/home/ubuntu/research/yuv-shots/profile-after/yuv-effects-true.png`

The new screenshot directory also contains the plain scene and RGBA references.
