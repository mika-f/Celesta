# React bridge: moving frames from Node to Rust

Decision: pipeline sequential frame evaluation (Node evaluates frame N + 1
while Rust decodes frame N), and send the CLI's messages length-prefixed.
Keep JSON and the stdio pipe. Do not send per-frame deltas, field patches
or struct-of-arrays layers, and do not change the transport.

Exports did not get faster on either machine measured, because rendering
and encoding set their pace. What this changes is how fast evaluation can
go, which caps any export once rendering or encoding get faster.

## Where a frame's time goes

NEBULA (about 1,660 layers, 458 KiB of JSON per frame) on an Apple M-series
Mac, Node 26, release build of `f143b62` (before #187); per frame, from a
CPU profile of the CLI and from timing the bridge's own steps on captured
frames:

| Step | Side | Time |
| --- | --- | ---: |
| React render and scene walk (`renderFrame`) | Node | 1.8 ms |
| `JSON.stringify` of the frame | Node | 1.15 ms |
| `Buffer.from` + `fs.writeSync` to the pipe | Node | 0.2 ms |
| `read_line` + `serde_json` into `Scene` | Rust | 0.93 ms, 0.67 ms after #187 |

Serial evaluation pays the sum. `JSON.stringify` of the live scene objects
is about three times slower than of the same data built by `JSON.parse`;
#188 addresses that.

## Measurements

`protocol-bench` (300 frames from frame 60, mean ms per frame) times
evaluation alone. Exports were timed with `--preset ultrafast` and the
default preset; the render-thread figures come from a temporary timer (see
[Reproduce](#reproduce)). Variants ran alternately, round by round, and
the percentages compare within one run.

### Windows, RTX A4000

i7-12700K, RTX A4000 (driver 581.15, Vulkan), Windows 11, Node 26.10,
rustc 1.99; CPU load under 10%. Seven rounds:

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| `main` | 5.72 | 4.27 | 1.55 |
| length prefix, serial | −0.6% | −0.1% | −0.6% |
| pipelining alone | 4.62 (−19%) | 4.05 (−5%) | 1.35 (−13%) |
| **pipelining + length prefix** | **4.11 (−28%)** | **3.97 (−7%)** | **1.20 (−22%)** |

The length prefix on top of pipelining is worth −11.3% for NEBULA (95% CI
−13.4% to −9.8%) and −10.9% for reel (−13.0% to −8.8%); SPECTRA's −1.5%
is not resolved. Serial, its effect is within noise for all three.

Exports, pipelining + length prefix against `main`, five rounds: wall time
/ render loop / render thread waiting for React.

| Entry, preset | `main` | This change |
| --- | --- | --- |
| NEBULA, `ultrafast` | 5.23 s / 3,661 / 292 ms | 5.24 s / 3,559 / 25 ms |
| NEBULA, default | 7.27 s / 5,292 / 672 ms | 7.20 s / 5,159 / 24 ms |
| SPECTRA, `ultrafast` | 4.56 s / 2,865 / 983 ms | 4.42 s / 2,729 / 804 ms |
| SPECTRA, default | 6.26 s / 4,221 / 1,251 ms | 6.16 s / 4,055 / 918 ms |
| reel, `ultrafast` | 4.76 s / 1,715 / 451 ms | 4.68 s / 1,579 / 300 ms |
| reel, default | 5.90 s / 2,697 / 500 ms | 5.89 s / 2,657 / 181 ms |

### Apple M-series Mac

Node 26; the load average was 13–17 during the export runs, so only the
render thread's wait is reported for them. Five rounds:

| Variant | NEBULA | SPECTRA | reel |
| --- | ---: | ---: | ---: |
| `main` | 3.32 | 1.47 | 0.93 |
| length prefix, serial | −3% | ±0% | −3% |
| pipelining alone | 2.70 (−19%) | 1.32 (−10%) | 0.78 (−16%) |
| pipelining + length prefix | 2.65 (−20%) | 1.38 (−6%) | 0.78 (−16%) |

Render thread waiting for React during an `ultrafast` export, six rounds:
NEBULA 1,494 → 158 ms, SPECTRA 1,090 → 678 ms, reel 691 → 427 ms. The
wall time did not change beyond the load's noise.

Here the length prefix adds nothing to pipelining beyond the noise of five
rounds.

## Reading the results

**Exports are paced by rendering and encoding.** Pipelining removes almost
all of NEBULA's wait, but the A4000's render loop shrinks 2–3% and its wall
time not at all: rendering and encoding take the freed time.

**Evaluation caps an export.** On the A4000, NEBULA's render loop runs at
about 164 frames/s with `ultrafast`, just under the 175 frames/s serial
evaluation allows; pipelined evaluation allows about 243. Without it,
faster rendering or encoding (hardware encoding, for one) would hit the
serial cap almost at once. SPECTRA's render loop already runs about as fast
as its evaluation allows (4.2 ms per frame over its 690 frames, against
4.3 ms serial and 4.0 ms pipelined over `protocol-bench`'s 300), which is
why it still waits 0.8–0.9 s: only faster evaluation helps it.

**After pipelining, Node's side is what to make faster.** A pipelined frame
costs the longer of Node's work and Rust's decoding, not their sum. Node's
side (3–4 ms for NEBULA on the Mac) is far longer than the decode
(0.67 ms), so faster decoding no longer shortens a frame and faster
JavaScript does.

**The length prefix pays off only pipelined, and only on Windows.** Why is
not established. Line-delimited reads go through an 8 KiB `BufReader`
into a growing `String`; length-prefixed ones are a few large `read_exact`
calls into a reused `Vec`, and Node encodes into a reused `Buffer`.
Windows pipe reads may cost more than macOS ones, and pipelining may put
the transfer, rather than the decode, on the critical path.

**reel spends most of an export outside the render loop**: 3.0 of 4.8 s on
the A4000, in start-up and audio. That, not the bridge, is where reel's
export time is.

## Considered and dropped

Measured on NEBULA and SPECTRA frames captured from the bridge, on the Mac:

- **Sending only changed layers.** 99.8% of NEBULA's layers and a median of
  72% of SPECTRA's (counting nested layers) change every frame, so the
  frame does not shrink; NEBULA's even grows (458 → 494 KiB) with ids.
- **Field-level patches** (JSON merge patch per changed layer): NEBULA's
  frame shrinks 59% (458 → 187 KiB) and Rust would decode it in about
  0.38 ms instead of 0.93 ms. But finding the changes generically takes
  5.3 ms in Node and encoding the patch 1.7 ms, against 1.15 ms for the
  whole frame. Only worth revisiting with change tracking built into the
  scene walk and patch types on the Rust side.
- **Struct-of-arrays runs** (consecutive layers of one shape as columns):
  NEBULA shrinks 44% and Rust parses it as JSON values in 0.36 ms instead
  of 1.1 ms, but building it in Node costs 1.7 ms. SPECTRA's layers are too
  varied to form runs.
- **Rounding numbers** to 1/1000: 23% smaller, slower to encode.
- **Other transports.** Moving a 512 KiB frame through the pipe as it was
  costs about 0.18 ms on the Mac. Length-prefixed reads into reused buffers
  cut that to about 0.065 ms; a Unix socket with 1 MiB buffers (0.036 ms)
  or a memory-mapped file (0.026 ms) would save a further 0.03–0.04 ms.
  macOS pipes cannot be enlarged; Unix sockets need 1 MiB buffers to beat
  them. A Node native addon or an embedded JS engine would remove the
  transfer but not the conversion between V8 and Rust values, at a high
  build and packaging cost. Text-measurement round trips are 5–8 µs each,
  and SPECTRA asks for 114 over its 690 frames.
- **Other formats** (CBOR, MessagePack): see
  `docs/handoff/2026-10-08-tagged-enum-deserialization.md`.

## Correctness

Every frame's scene message is byte-identical to `main`'s for NEBULA (600
frames), SPECTRA (690, with 114 text measurements) and reel (960). Decoded
video frames could not be compared: two exports of `main` already differ
in 455 of NEBULA's 600 frames.
`pipelined_frames_match_serial_evaluation_with_text_measured_meanwhile_when_node_is_available`
checks pipelined evaluation against serial evaluation with text measured
every frame.

## Reproduce

Build each variant in its own worktree:

```sh
cd packages/react && pnpm install --frozen-lockfile && pnpm run codegen && pnpm run build && cd ../..
cargo build --release -p celesta-exporter -p celesta-react-bridge --bins --examples
python3 examples/reel/make-music.py   # reel's music.wav, once
```

`--examples` alone builds only the examples, leaving the exporter stale.
Run the variants alternately, round by round, and compare within a run.

Evaluation only; `--pipelined` needs this change:

```sh
target/release/examples/protocol-bench examples/versus/bench/celesta/nebula.tsx 300 60
target/release/examples/protocol-bench --pipelined examples/versus/bench/celesta/nebula.tsx 300 60
```

Exports: `target/release/celesta-exporter --overwrite --no-ui --preset
ultrafast --react <entry> out.mp4`. For the render-thread figures, add a
temporary timer to `render_react_video` in
`crates/exporter/src/render.rs`: an `Instant` before `thread::scope` for
the render loop, and an `Instant` around `frames.recv()` summed into a
static `AtomicU64` for the wait, both printed with `eprintln!` before
`finish_encode`.

To compare scene messages, put a `node` wrapper first on `PATH` that runs
the real Node with its stdout piped through `tee` to a file, export with
each variant, and compare the scene messages in order (JSON lines on
`main`, length-prefixed with this change).
