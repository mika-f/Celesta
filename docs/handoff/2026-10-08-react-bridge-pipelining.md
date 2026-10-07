# React bridge: length-prefixed messages and pipelined frames (2026-10-08)

Two changes to how `celesta-react-bridge` moves frames from Node to Rust.

## Length-prefixed messages

`ReactBridge::spawn` passes `--length-prefixed` to the CLI. Every message
Node writes is then a little-endian `u32` byte count followed by that many
bytes of JSON. Node encodes it into a `Buffer` reused across messages;
Rust reads it with `read_exact` into a `Vec` reused across messages and
parses it with `serde_json::from_slice`. Before, Rust scanned an 8 KiB
`BufReader` for the line end into a new `String` per message.

Requests from Rust to Node stay JSON lines. Without the flag the CLI still
writes JSON lines, which the JavaScript tests and scripts read.

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

## Results

Apple M-series Mac, Node 26, release builds of the four combinations,
interleaved. The machine's load average was 13–17 during the export runs.

`protocol-bench` (300 frames from frame 60, mean ms per frame, median of
five rounds; `--pipelined` for the pipelined variants):

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| main | 3.32 | 1.47 | 0.93 |
| length prefix | −3% | ±0% | −3% |
| pipelining | 2.70 (−19%) | 1.32 (−10%) | 0.78 (−16%) |
| both | 2.65 (−20%) | 1.38 (−6%) | 0.78 (−16%) |

The length-prefix row is from a separate run of twelve rounds against
main (NEBULA 3.38 → 3.29 ms); five rounds could not separate it from noise.

`celesta-exporter --react … --preset ultrafast`, time the render thread
spent waiting for the next React scene (median of six rounds):

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| main | 1,494 ms | 1,090 ms | 691 ms |
| length prefix | 1,441 ms | 1,152 ms | 725 ms |
| pipelining | 155 ms | 752 ms | 571 ms |
| both | 158 ms | 678 ms | 427 ms |

The whole export did not get measurably faster on this machine (NEBULA
6.53 s both before and after, SPECTRA 5.09 → 4.95 s): rendering and
encoding set the pace here, and the load made the totals vary by up to
2×.

### Windows, RTX A4000

i7-12700K, RTX A4000 (driver 581.15, Vulkan), Windows 11, Node 26; CPU
load 1–10%. main, the length prefix alone, and both changes; medians of
five rounds.

`protocol-bench`, mean ms per frame:

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| main | 5.79 | 4.26 | 1.51 |
| length prefix | 5.68 (−2%) | 4.33 (+2%) | 1.55 (+3%) |
| both, `--pipelined` | 4.14 (−29%) | 3.97 (−7%) | 1.19 (−21%) |

Export wall time, and the render loop / the render thread's wait for
React:

| Entry, preset | main | both |
| --- | --- | --- |
| NEBULA, `ultrafast` | 5.23 s, 3,661 / 292 ms | 5.24 s, 3,559 / 25 ms |
| NEBULA, `medium` | 7.27 s, 5,292 / 672 ms | 7.20 s, 5,159 / 24 ms |
| SPECTRA, `ultrafast` | 4.56 s, 2,865 / 983 ms | 4.42 s, 2,729 / 804 ms |
| SPECTRA, `medium` | 6.26 s, 4,221 / 1,251 ms | 6.16 s, 4,055 / 918 ms |
| reel, `ultrafast` | 4.76 s, 1,715 / 451 ms | 4.68 s, 1,579 / 300 ms |
| reel, `medium` | 5.90 s, 2,697 / 500 ms | 5.89 s, 2,657 / 181 ms |

The length prefix alone changed neither table beyond noise. Pipelining
removes almost all of NEBULA's wait, but the render loop shrinks 2.5–3%
and the wall time not at all: rendering and encoding take the freed time
here too. SPECTRA still waits 0.8–0.9 s after pipelining, so its
evaluation remains partly the limit; reel spends much of its wall time
outside the render loop (start-up and audio).

Every frame's scene message from main and from both changes was
byte-identical for NEBULA (600 frames), SPECTRA (690, with 114 text
measurements) and reel (960). Decoded video frames could not be compared:
two exports from main already differ in 455 of NEBULA's 600 frames.

## Considered and dropped

- Sending only the layers that changed since the previous frame: 99.8% of
  NEBULA's layers and a median of 72% of SPECTRA's change every frame.
- Field-level patches and struct-of-arrays runs: NEBULA's frame shrinks by
  59% and 44%, but building them generically in JavaScript costs more than
  the `JSON.stringify` they save.
- Other transports: moving a 512 KiB frame through the pipe costs about
  0.2 ms. A Unix socket with 1 MiB buffers or a shared file would save
  about 0.15 ms of a 5–7 ms frame; macOS pipes cannot be enlarged.
