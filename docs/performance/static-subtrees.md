# Static subtrees: still content walked, sent and parsed once

Decision: `<Static deps>` caches a subtree's layers in the walk and in the
Rust bridge. Frames refer to it by key; its layers cross the pipe only
until the bridge has them. See
[the design](../superpowers/specs/2026-10-12-static-subtrees-design.md).

## Results

Apple M4, Node v26.10.0, release builds, production React. The entries are
`crates/react-bridge/examples/static-100k.tsx` (100,000 still rects in one
`<Static>`, 100 moving rects outside) and `static-100k-memo.tsx` (the same
still rects in a `React.memo` component instead). `protocol-bench`, 120
frames after 30 warmup frames, three alternating runs:

| Entry | Median per frame |
| --- | ---: |
| `static-100k-memo.tsx` | 84.1–84.9 ms |
| `static-100k.tsx` | **6.6–6.7 ms** |

With `React.memo`, React no longer re-renders the still rects, but the
walk, `JSON.stringify` (17 MB per frame), and Rust's parse still repeat.
With `<Static>`, a frame carries one `staticRef` layer for them; what is
left is the moving rects and copying the 100,000 cached layers into each
scene on the Rust side (about 3 ms).

### The first frame

A `<Static>` frame first walks and caches its subtree, then sends it. In
one process, first frame (`renderAt` plus `JSON.stringify`), five runs
each:

| Entry | First frame |
| --- | ---: |
| `React.memo` | 56–65 ms |
| `<Static>` | 55–59 ms |

The first measurement put `<Static>` at 70–93 ms, all of it in
`JSON.stringify` (32 ms against 17 ms). Definitions were then an object
keyed by number (`{ "7": [...] }`). V8 holds numeric keys as indexed
elements, and one such object puts the whole frame's `JSON.stringify` on
its slow path: 15.4 ms against 8.5 ms for the same 100,000 layers in a
list, in isolation. Definitions are now a list of `{ key, layers }`, and a
test pins that shape.

Because the bridge renders one frame ahead, the second frame carries the
definition again; from the third on, only the reference crosses the pipe.

## No regressions

Base is the tip of the generic-layers branch (`2116a9d`), built in a
separate worktree; runs alternate.

| Workload | Base | New | Change |
| --- | ---: | ---: | ---: |
| NEBULA, `protocol-bench` (4 runs) | 2.798 ms | 2.782 ms | −0.6% |
| SPECTRA, `protocol-bench` | 0.780 ms | 0.764 ms | −2.0% |
| reel, `protocol-bench` | 0.909 ms | 0.919 ms | +1.1% |

`celesta-bench` on all nine workloads (30 frames, 3 runs) stays within
±2%, and the last frame of each is byte-identical to the base's PNG.

## Reproduce

```sh
pnpm install --frozen-lockfile
pnpm --dir packages/react run codegen
pnpm run build:runtime
cargo run --release -p celesta-react-bridge --example protocol-bench -- \
  crates/react-bridge/examples/static-100k.tsx 120
cargo run --release -p celesta-react-bridge --example protocol-bench -- \
  crates/react-bridge/examples/static-100k-memo.tsx 120
```
