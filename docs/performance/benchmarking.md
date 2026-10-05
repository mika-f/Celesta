# Checking a change for performance regressions

Two checks use the same benchmark, `celesta-bench` (`crates/bench`):

- **On pull requests that touch the renderer, the React runtime, or the
  benchmark (CI):** instructions per frame on a GitHub-hosted runner, which
  has no GPU. Nearly deterministic: the check fails when a workload needs
  more than 1% more instructions per frame than the base, well above the
  run-to-run noise (0.4% at most).
- **On your machine:** wall-clock ms/frame on a real GPU. Measures what a
  user sees, but run to run noise is a few percent or more.

## Workloads

`celesta-bench` renders each workload through the GPU renderer's export path
(pipelined `submit`/`drain` with readback, `RenderQuality::Final`). It builds
every scene before it starts timing, so only rendering is measured.

| Workload | What it draws |
| --- | --- |
| `nebula` | NEBULA from [`examples/versus/bench`](../../examples/versus/bench/SCENE.md), the real React composition (`celesta/nebula.tsx`), evaluated by the React bridge: about 1,660 layers per frame |
| `rings` | NEBULA's 24 rotating stroked ellipses alone ([path-rasterization.md](path-rasterization.md)) |
| `blur` | six large blurred, screen-blended blobs plus cards with shadows and glows |
| `ribbons` | about 3,000 thin rotated rects that change every frame (`examples/afterimage`) |
| `text` | scaling text lines, a phrase-wrapped Japanese paragraph with a stroke, and a ticking counter |
| `shapes` | gradient-filled, stroked rounded rects in clipped, rotated groups with every blend mode |
| `images` | one 512x512 image drawn 16 times, scaled and rotated, with `cover` and `contain` |

The synthetic workloads are laid out at 1920x1080 and scaled to `--size`;
`nebula` is wrapped in a scaling group, with its blur, shadow, and glow
radii scaled to match. `--size` must be 16:9. `nebula` needs Node.js and a
built `packages/react`: `scripts/bench.py compare` builds it at both
revisions, but before running `celesta-bench` directly, build it yourself:

```sh
pnpm --dir packages/react install --frozen-lockfile
pnpm --dir packages/react run codegen --locked
pnpm --dir packages/react run build
```

```sh
cargo run --release -p celesta-bench -- list
cargo run --release -p celesta-bench -- run --frames 120 --size 1920x1080 nebula blur
cargo run --release -p celesta-bench -- run --frames 10 --dump out/ # also save each last frame as PNG
```

## In CI: instructions per frame

[`.github/workflows/perf.yml`](../../.github/workflows/perf.yml) runs on pull
requests that touch the renderer, the React runtime, or the benchmark. It
builds `celesta-bench` at the pull request's base and at its merge commit,
and runs each workload with both, at 640x360 for 6 frames after 2 warmup
frames:

- **No GPU needed.** The renderer runs on Mesa's software Vulkan driver,
  lavapipe, so the shaders run as JIT-compiled CPU code.
- **Counted, not timed.** Each run is under Valgrind's Cachegrind, which
  counts executed instructions. `celesta-bench` switches counting on just
  for the measured frames with Cachegrind's client requests
  (`crates/bench/src/cachegrind.rs`), so process start, device creation,
  the shader compiles of pipeline creation and warmup, and the React
  entry's evaluation are left out (shader variants lavapipe compiles inside
  a measured frame are counted; see below).
  Rayon and lavapipe each get one thread, so no idle thread spins for a
  scheduling-dependent number of instructions, and Mesa's shader cache is
  off: lavapipe compiles some shader variants on first use, possibly in a
  measured frame, and that compile is then always counted instead of
  depending on whether an earlier run cached it.
- **Both the CPU and the shader side.** The count includes the renderer's
  CPU work (text shaping and rasterization, path flattening and binning,
  planning, uploads, readback) and lavapipe's execution of every shader.

Repeated runs of the same binary agree within 0.02%, except `ribbons`
(3,000 layers), which varies by up to 0.4%. The job fails if a workload
needs more than 1% more instructions per frame than at the base. If the
base has no `crates/bench` yet, only the head is measured and the job
passes; the report says so. A workload the head removes is listed as
removed, and one it adds as new. Locally
(in an aarch64 Ubuntu 24.04 container on an M4), the whole comparison took
under 3 minutes after the build, and widening the blur kernel from 3σ to
3.3σ showed as +7.8% in `blur` and +6.9% in `nebula`, with the other
workloads within 0.4%.

The table appears on the job's summary page and as a comment on the pull
request, which each later run updates in place. The comment is posted by
[`perf-comment.yml`](../../.github/workflows/perf-comment.yml), a
`workflow_run` workflow: the measuring job runs the pull request's code with
a token that cannot write to pull requests from forks, so the report and the
pull request number go into the `perf-report` artifact (with the raw
numbers, `observations.csv`), and the second workflow, which runs from the
default branch and never runs the pull request's code, posts it. It only
comments if the pull request's head is still the measured commit; a newer
push gets its own run. Like every `workflow_run` workflow, it takes effect
once it is on the default branch.

What instruction counts cannot see: memory bandwidth, cache misses, GPU
occupancy and parallelism, driver-specific costs, and anything that is fast
on a GPU but slow on a CPU or the other way around. A shader that does more
work per pixel does show, as does more CPU work per frame; a change that
only moves data between passes may not. Use the local comparison below for
GPU-specific changes, and to confirm what CI flags.

When a regression is intended (a new feature that costs more), say so in
the pull request; the check is a signal, not a gate.

## On your machine: time on a real GPU

```sh
python3 scripts/bench.py compare                        # all workloads, base = merge base with origin/main
python3 scripts/bench.py compare nebula blur --rounds 7 # some workloads, more rounds
python3 scripts/bench.py compare --base v0.4.0 --head my-branch --csv out.csv
```

It builds both revisions (the head defaults to your working tree, including
uncommitted changes; the base is built in a worktree under
`target/bench-worktrees/`), then runs them in alternating order for
`--rounds` rounds (default 5) of 120 frames at 1920x1080 after 10 warmup
frames. The table gives medians and the head's range. A workload is marked
slower or faster only if the medians differ by more than `--threshold`
(default 5%) and the two sides' ranges do not overlap. `--csv` writes the
observations in the format of the other CSV files in this directory.

For results worth recording, close other GPU work, keep the machine on AC
power, and prefer more rounds over more frames.

`--mode instructions` runs the CI comparison locally. It needs Linux with
Valgrind 3.22 or later and lavapipe (`mesa-vulkan-drivers`) as the only
Vulkan driver, or a container that has them.
