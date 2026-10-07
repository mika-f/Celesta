# React bridge: pipelined frames and length-prefixed messages (2026-10-08)

Measurements, their reading and what was dropped are in
[docs/performance/react-bridge.md](../performance/react-bridge.md).

## Pipelined frames

`ReactBridge` has `submit_frame`, `receive_frame` and `decode_frame`, the
three steps `evaluate_at` runs back to back. The exporter's evaluator
thread submits frame N + 1 after it receives frame N's bytes and before it
decodes them, so Node evaluates the next frame while Rust parses the
current one. Text measurements Node asks for in the meantime wait in the
pipe until the parse is done; `receive_frame` answers them.

- `receive_frame` recognises a text measurement by its leading
  `{"measureText":` key, so a frame's scene is parsed only once, when it is
  decoded. The CLI writes that key first; `text-metrics.test.mjs` checks
  it.
- Every submitted frame must be received, in order, before any other
  request. The exporter only submits the next frame when there is one, and
  it drops the bridge after a failed or cancelled export.
- Only sequential evaluation benefits. The editor preview asks for
  arbitrary times one at a time, so it has no next frame to submit early.
- `protocol-bench --pipelined` times the pipelined loop.

## Length-prefixed messages

`ReactBridge::spawn` passes `--length-prefixed` to the CLI. Every message
Node writes is then a little-endian `u32` byte count followed by that many
bytes of JSON, encoded into a `Buffer` reused across messages; Rust reads
it with `read_exact` into a `Vec` reused across messages and parses it with
`serde_json::from_slice`. Requests from Rust stay JSON lines. Without the
flag the CLI still writes JSON lines, which the JavaScript tests read.

When reading JSON lines, parse the `String` with `from_str`: `read_line`
has already checked its UTF-8, and `from_slice` checks it again (9% slower
on NEBULA frames).

## History

The length prefix (162b385) was reverted (8d87ac2) after it measured
within noise on its own on both machines, then restored (84df80c) when an
interleaved run on the Windows RTX A4000 showed it worth a further 11% on
top of pipelining for NEBULA and reel. 84df80c's message says the Mac
showed no such difference; a longer interleaved re-run there found about
4%. Three lessons:

- A change that does nothing alone can still matter combined with another;
  measure the combination before reverting.
- Results from separate sessions on the same machine drifted by up to 6%
  for `main` alone; compare variants only within one interleaved run.
- On the Mac, absolute times climb as it heats (NEBULA's export 2.9 → 5.2 s
  over five rounds), and an export pushes the load average past 10 by
  itself. Compare base and head run back to back in the same round.
