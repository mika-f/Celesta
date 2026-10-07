# React bridge: pipelined frames (2026-10-08)

The exporter's React evaluator thread used to run Node's evaluation of a
frame and Rust's JSON decoding of it back to back, so Node sat idle while
Rust parsed. Now the two overlap.

## Pipelined frames

`ReactBridge` has `submit_frame`, `receive_frame` and `decode_frame`, the
three steps `evaluate_at` runs back to back. The exporter's evaluator
thread submits frame N + 1 after it receives frame N's bytes and before it
decodes them, so Node evaluates the next frame while Rust parses the
current one. Text measurements Node asks for in the meantime wait in the
pipe until the parse is done; `receive_frame` answers them.

`receive_frame` recognises a text measurement by its leading
`{"measureText":` key, so a frame's scene is parsed only once, when it is
decoded. The CLI writes that key first.

Every submitted frame must be received, in order, before any other
request. The exporter only submits the next frame when there is one, and
it drops the bridge after a failed or cancelled export.

Only sequential evaluation benefits. The editor preview asks for arbitrary
times one at a time, so it has no next frame to submit early.

`protocol-bench --pipelined` times the pipelined loop.

## Results

### Apple M-series Mac

Node 26, release builds, variants interleaved. The machine's load average
was 13–17 during the export runs.

`protocol-bench` (300 frames from frame 60, mean ms per frame, median of
five rounds):

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| main | 3.32 | 1.47 | 0.93 |
| pipelining (`--pipelined`) | 2.70 (−19%) | 1.32 (−10%) | 0.78 (−16%) |

`celesta-exporter --react … --preset ultrafast`, time the render thread
spent waiting for the next React scene (median of six rounds):

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| main | 1,494 ms | 1,090 ms | 691 ms |
| pipelining | 155 ms | 752 ms | 571 ms |

The whole export did not get measurably faster on this machine: rendering
and encoding set the pace, and the load made the totals vary by up to 2×.

### Windows, RTX A4000

i7-12700K, RTX A4000 (driver 581.15, Vulkan), Windows 11, Node 26; CPU
load 1–10%; medians of five rounds. These runs measured pipelining
together with the length prefix described below, which on its own changed
nothing beyond noise. A run of pipelining alone is pending.

`protocol-bench`, mean ms per frame:

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| main | 5.79 | 4.26 | 1.51 |
| pipelining + length prefix | 4.14 (−29%) | 3.97 (−7%) | 1.19 (−21%) |

Export wall time, and the render loop / the render thread's wait for
React:

| Entry, preset | main | pipelining + length prefix |
| --- | --- | --- |
| NEBULA, `ultrafast` | 5.23 s, 3,661 / 292 ms | 5.24 s, 3,559 / 25 ms |
| NEBULA, `medium` | 7.27 s, 5,292 / 672 ms | 7.20 s, 5,159 / 24 ms |
| SPECTRA, `ultrafast` | 4.56 s, 2,865 / 983 ms | 4.42 s, 2,729 / 804 ms |
| SPECTRA, `medium` | 6.26 s, 4,221 / 1,251 ms | 6.16 s, 4,055 / 918 ms |
| reel, `ultrafast` | 4.76 s, 1,715 / 451 ms | 4.68 s, 1,579 / 300 ms |
| reel, `medium` | 5.90 s, 2,697 / 500 ms | 5.89 s, 2,657 / 181 ms |

### Reading

Pipelining removes almost all of NEBULA's wait, but the render loop
shrinks 2.5–3% and the wall time not at all: rendering and encoding set
the pace on both machines and take the freed time.

What pipelining changes is the ceiling evaluation puts on an export. On
the A4000, NEBULA's render loop runs at about 164 frames/s with
`ultrafast`, just under the 173 frames/s serial evaluation allows;
pipelined evaluation allows about 241. Faster rendering or encoding (for
example hardware encoding) would otherwise hit the serial ceiling almost
at once. SPECTRA's render loop already runs about as fast as its
evaluation allows (4.2 ms per frame over all 690 frames, against 4.3 ms
serial and 4.0 ms pipelined over the 300 `protocol-bench` samples), which
is why it still waits 0.8–0.9 s: only faster evaluation helps it.

With the two overlapped, a frame costs the longer of Node's evaluation and
Rust's decoding, not their sum. Node's side (3–4 ms) is far longer than
the decode (about 0.65 ms for NEBULA), so further decoding work no longer
speeds exports up; Node-side work does.

reel spends much of its wall time outside the render loop (start-up and
audio): 3.0 of 4.8 s on the A4000.

Every frame's scene message from main and from this branch was
byte-identical for NEBULA (600 frames), SPECTRA (690, with 114 text
measurements) and reel (960). Decoded video frames could not be compared:
two exports from main already differ in 455 of NEBULA's 600 frames.

## Tried and reverted: length-prefixed messages

162b385 had the bridge spawn the CLI with `--length-prefixed`: every
message Node wrote was a little-endian `u32` byte count and the JSON,
encoded into a `Buffer` reused across messages, and Rust read it with
`read_exact` into a `Vec` reused across messages. Requests from Rust stayed
JSON lines, and without the flag the CLI still wrote JSON lines. A later
commit reverted it; the branch history keeps both.

It made no difference beyond noise:

| Machine | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| Mac, `protocol-bench`, 12 rounds | −2.9% (3.38 → 3.29 ms) | −0.1% | — |
| Mac, `protocol-bench`, 5 rounds | +4.6% | −1.2% | −3.3% |
| A4000, `protocol-bench` | −1.9% | +1.6% | +2.9% |
| Mac, render-thread wait | 1,494 → 1,441 ms | 1,090 → 1,152 ms | 691 → 725 ms |
| A4000, export wall time (`ultrafast`) | +1.5% | −0.9% | +0.7% |

That matches the transport estimate: moving a 512 KiB frame through the
pipe as it was costs about 0.18 ms, and length-prefixed reads into a
reused buffer save about 0.1 ms of it, a few percent of a NEBULA frame.
Worth bringing back only if the bridge needs to carry binary payloads or
the pipe becomes the limit.

## Considered and dropped

- Sending only the layers that changed since the previous frame: 99.8% of
  NEBULA's layers and a median of 72% of SPECTRA's change every frame.
- Field-level patches and struct-of-arrays runs: NEBULA's frame shrinks by
  59% and 44%, but building them generically in JavaScript costs more than
  the `JSON.stringify` they save.
- Other transports: a Unix socket with 1 MiB buffers or a shared file
  would save about 0.15 ms of a 5–7 ms frame; macOS pipes cannot be
  enlarged.
