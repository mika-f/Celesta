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
2×. Machines where the React evaluation paces the export, like the
RTX 4070 in `docs/performance/spectra-tuning.md`, are where it should
show.

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
