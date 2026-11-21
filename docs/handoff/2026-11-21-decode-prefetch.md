# Host-side sequential decode prefetch

## Specification

Overlap one predicted video decode with host rendering, only for external
`MediaProcess` requests carrying a sequential session. Keep the wire format,
single shared decode slot, plugins, non-sequential decoding and in-process
providers unchanged. `CELESTA_DECODE_PREFETCH=0` disables it for newly
started processes; `set_decode_prefetch` permits per-process control without
mutating global environment variables in concurrent tests.

## Implementation

`crates/media/src/process.rs` tracks the last requested time and step per
session/path (also decoder and fallback frame rate, to avoid cache reuse
across different decoding parameters). Two equal positive steps, compared
with `timestamps_match`, predict the next time. Zero/backward/changing steps
do not predict. History is bounded to 16 entries.

After copying the real frame out, `PluginChild::send` starts one predicted
decode with the normal 60-second deadline, measured from send time. The
next operation drains that answer and copies the shared slot before sending
anything else. Successful predictions go into a four-frame FIFO stash;
matching real requests consume them without IPC and can start the next
prediction. A session asking for another time invalidates its other stashed
frame. Both synchronous `request`/`idle` and pipelined `send` drain first.

Speculative codec/file/timestamp errors are discarded. Unavailable/stopped
process errors propagate so the existing `Processes::retrying` logic drops
or retries the provider as today. The process child is still dropped before
its shared memory, including when a prediction is outstanding. No thread,
mapping reference, new dependency, protocol field, or plugin edit is needed.

`crates/codec-test/tests/process.rs` covers byte equality on/off through real
test-codec processes, shared memory and pipes: +1, +2, changing cadence,
interleaved sessions on different files, a six-session stash-pressure case,
pause/repeats, backward seek immediately after prediction, past-end frames,
and interleaved audio/probe. Logged real-codec requests verify speculative
sends and cache hits, and no sends for non-sequential/disabled requests.
A Rust test-only relay (`crates/codec-test/src/bin/decode-prefetch-relay.rs`,
invoked by the Unix failure test) refuses or hangs on the first predicted request to
exercise harmless-error discard/retry, a killed hung prediction, and drop
with a request outstanding. No production plugin behavior is modified.

## FFmpeg idle-session caveat

A strict guarantee of no additional idle-session evictions is impossible
with this design and unchanged plugins/protocol. The existing dispatch
passes sequential requests to FFmpeg's sequential backend, whose
`decode_sequential` (`plugins/ffmpeg/src/decoder.rs`) increments
`sequential_requests` on every request, updates that session's `last_used`,
then calls `close_idle_video_sessions`. That retains sessions only while
`now - last_used <= IDLE_SESSION_FRAMES * sessions` (30 times session count).
The plugin has no indication that a request was speculative.

Consumed predictions replace real IPC requests, so ordinary round-robin
active layers remain far inside that window. However, a missed prediction,
terminal lookahead, or reordered request near the idle threshold can age
another session sooner. A later request still seeks/decodes correctly but
may cost more. A host cannot subtract requests from the plugin's counter.
Preventing this for arbitrary schedules would require disabling speculation
or changing plugin eviction accounting. This limitation is not represented
as a no-eviction guarantee; the opt-out remains available.

## Checks and measurements

Commands and results on Linux:

```text
cargo test -p celesta-media
test result: ok. 144 passed; 0 failed; 0 ignored
cargo test --release -p celesta-codec-test --test process
test result: ok. 19 passed; 0 failed; 0 ignored
cargo test -p celesta-exporter --lib
test result: ok. 191 passed; 0 failed; 0 ignored
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json XDG_RUNTIME_DIR=/tmp cargo test --release -p celesta-gpu-renderer
test result: ok. 436 passed; 0 failed; 1 ignored
cargo clippy -p celesta-media -p celesta-codec-test -p celesta-exporter --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.74s
cargo fmt --all -- --check
git diff --check
```

Formatting and diff checks exited 0 with no output. Exporter tests initially
needed `cargo build -p celesta-codec-test` (debug fixture executable). The
first GPU run failed the emoji fallback test; the same isolated failure
was reproduced on untouched main `a56c182a`. `fc-match emoji` selected
DejaVu Sans. Installing `fonts-noto-color-emoji` selected Noto Color Emoji,
and the full GPU command above passed. No test was changed for this.
Exporter initially logged `XDG_RUNTIME_DIR not set in the environment`;
the blueprint's XDG/Vulkan environment was used for remaining GPU checks.

### Backend benchmark

`crates/codec-test/examples/decode-prefetch.rs` runs through
`MediaBackend::decode_frame_for`, with a new backend/process per case,
four warm-up frames and 120 timed frames. Each frame includes 0, 5 or
15 ms of monotonic-clock busy host work. Three interleaved off/on runs,
sequential playback, same unchanged FFmpeg plugin built from main with
the existing FFmpeg 8.1.3 standard prefix. Shared memory acceptance is
asserted, not inferred from the environment. Both inputs are 150-frame,
30 fps, H.264/YUV420P MP4s. Only host prefetch is toggled.

```sh
cargo build --release -p celesta-codec-test --example decode-prefetch
PLUGIN=/home/ubuntu/research/celesta-main/plugins/dist/ffmpeg/bin/linux-x86_64/celesta-codec-ffmpeg
target/release/examples/decode-prefetch "$PLUGIN" /home/ubuntu/research/input-1080.mp4
target/release/examples/decode-prefetch "$PLUGIN" /home/ubuntu/research/input-4k.mp4
```

Wall ms/frame, median (min–max), including simulated work:

| Resolution | Host work ms | Prefetch off | Prefetch on | Wall-time reduction |
|---|---:|---:|---:|---:|
| 1920×1080 | 0 | 1.553 (1.499–1.607) | 1.500 (1.500–1.632) | 3.4% |
| 1920×1080 | 5 | 6.865 (6.710–6.882) | 5.605 (5.603–5.674) | 18.4% |
| 1920×1080 | 15 | 16.860 (16.625–17.229) | 15.635 (15.584–15.851) | 7.3% |
| 3840×2160 | 0 | 6.522 (6.373–11.493) | 6.322 (6.162–6.748) | 3.1% |
| 3840×2160 | 5 | 11.397 (10.881–15.089) | 8.509 (8.257–12.144) | 25.3% |
| 3840×2160 | 15 | 21.812 (21.150–22.415) | 18.238 (18.013–18.867) | 16.4% |

No useful 0 ms improvement is claimed: ranges overlap. The 4K/5 ms case
is noisy (one on run was slower). The 1080p/5 and /15 ms and 4K/15 ms
cases have disjoint ranges, showing useful overlap beyond noise. These
are synthetic backend measurements, not end-to-end render/export claims.
Raw output and test logs: `/home/ubuntu/research/prefetch-*.log`.

An end-to-end preview/export number was not obtained cheaply: the existing
`preview-bench` needs the built React/CLI runtime, which is absent in this
checkout. No visual/rendering algorithm change or visual check was made;
native AVFoundation/MediaFoundation execution is unavailable here. The
real 60-second speculative timeout was not waited out; the hung-process
test kills the request and verifies the stopped-provider error, while
deadline errors retain the same existing `media_error` mapping.
