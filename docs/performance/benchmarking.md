# Checking a change for performance regressions

Two checks use the same benchmark, `celesta-bench` (`crates/bench`):

- **On pull requests that touch the renderer, the React runtime, or the
  benchmark (CI):** instructions per frame on a GitHub-hosted runner, which
  has no GPU. Nearly deterministic: the check fails when a workload needs
  more than 1% more instructions per frame than the base, well above the
  run-to-run noise (0.4% at most).
- **On your machine:** wall-clock ms/frame on a real GPU. Measures what a
  user sees, but run to run noise is a few percent or more.

Neither check compares the pictures: a change that alters the output can
look like a speedup. See [Check the output too](#check-the-output-too).

## Workloads

`celesta-bench` renders each workload through the GPU renderer's export path
(pipelined `submit`/`drain` with readback, `RenderQuality::Final`). It builds
every scene before it starts timing, so only rendering is measured.

| Workload | What it draws |
| --- | --- |
| `nebula` | NEBULA from [`examples/versus/bench`](../../examples/versus/bench/SCENE.md), the real React composition (`celesta/nebula.tsx`), evaluated by the React bridge: about 1,660 layers per frame |
| `spectra` | SPECTRA from [`examples/spectra`](../../examples/spectra/README.md), a React reel of five chapters that uses blur, shadows, glows, blend modes, clips, paths, text, code, images and a camera. Its measured frames are split into five runs of consecutive frames, one in each chapter's settled part or crossfade: the first follows the warmup frames from frame 60, the others start at frames 140, 300, 410 and 600 whatever the warmup. It needs `--frames 5` or more |
| `rings` | NEBULA's 24 rotating stroked ellipses alone ([path-rasterization.md](path-rasterization.md)) |
| `blur` | six large blurred, screen-blended blobs plus cards with shadows and glows |
| `ribbons` | about 3,000 thin rotated rects that change every frame (`examples/afterimage`) |
| `text` | scaling text lines, a phrase-wrapped Japanese paragraph with a stroke, and a ticking counter |
| `shapes` | gradient-filled, stroked rounded rects in clipped, rotated groups with every blend mode |
| `images` | one 512x512 image drawn 16 times, scaled and rotated, with `cover` and `contain` |

The synthetic workloads are laid out at 1920x1080 and scaled to `--size`;
`nebula` and `spectra` are wrapped in a scaling group, with their blur,
shadow, and glow radii scaled to match. `--size` must be 16:9. `nebula` and
`spectra` need Node.js and a built `packages/react`: `scripts/bench.py
compare` builds it at both revisions, but before running `celesta-bench`
directly, build it yourself:

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
and runs each workload with both, at 1280x720 for 6 frames after 2 warmup
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

**The size.** The synthetic workloads are laid out at 1920x1080 and scaled
to the measured size, so a smaller size is a different scene: at 640x360
the 3,000 `ribbons` rects are mostly under one pixel, and text scale, mip
selection, path tile counts and blur, shadow and glow radii all shrink,
while the pixel count falls to 1/9 and the CPU side (planning, uploads,
text) weighs more against the shaders. CI therefore measures at 1280x720,
a ratio of 2/3, which keeps those effects close to 1080p at a 2.25x pixel
cost. The local comparison uses 1920x1080. The change in #166 was about the
same at both 640x360 and 1280x720 (`shapes` -33% and -37%, `ribbons` -25%
at both, `text` -9% and -11%, `images` -10% at both), but a change to
sub-pixel drawing, mips or text scale can still depend on the size; confirm
such a change locally at 1920x1080. CI does not measure two sizes or a size
per workload: that doubles or complicates the job for effects the local
comparison already covers.

**Read a decrease as fewer instructions, not a faster GPU.** The report says
"fewer instructions" for a drop. In #166, `text` and `images` needed about
10% fewer instructions with a pixel-identical output, yet on a real GPU
(Apple M4, Metal, 1920x1080, 10 rounds) they did not change (+2.1% and
-0.4%, intervals spanning zero), while `shapes` was faster on both (-37% in
instructions, -40% in time). Probably lavapipe generated different code for
a changed shader. A pull request that claims a speedup from the instruction
count must confirm it on a real GPU.

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
`target/bench-worktrees/`), then runs them for `--rounds` rounds (default 10)
of 120 frames at 1920x1080 after 10 warmup frames. The table gives the medians
and the min–max range of each side. `--csv` writes the observations in the
format of the other CSV files in this directory.

Each round runs both sides in a random order, from a seed the report prints
(`--seed` repeats a run), after one discarded round: a freshly built
executable is slow on its first run. A workload is marked slower or faster
only if the medians differ by more than `--threshold` (default 5%) and the
95% bootstrap interval of the median's change, shown in brackets, excludes
zero. With the default 10 rounds and run-to-run noise of 7%, a true 10%
change is found about 7 times in 10 and no change is reported about 1 time
in 20 (simulated); a smaller change needs more rounds. `ribbons` and `text`
in #166 are the kind of change that needs them.

For results worth recording, close other GPU work, keep the machine on AC
power, and prefer more rounds over more frames.

### On a GitHub-hosted macOS runner

The `Time per frame (macOS)` job of
[`perf.yml`](../../.github/workflows/perf.yml) runs this comparison on
`macos-latest` for every pull request that runs the performance check, and by
hand (Actions, Performance, Run workflow, with a `base`). Its adapter is
`Apple Paravirtual device (Metal)`: a GPU, reached through virtualization.
`perf-comment.yml` adds its table to the pull request's comment, below the
instruction counts: medians and min–max ranges of both sides, the 95%
bootstrap interval of the change, the adapter and the seed.

It is a reference, not a check. A shared VM is much noisier than a desk machine
(intervals of +-20% or more for workloads under 2 ms), so it only sees large
changes; it never fails the workflow (`continue-on-error`), and a run without
its table still gets the comment. macOS arm64 runners can queue for a while,
and the comment waits for the whole workflow.

## Check the output too

The benchmark measures time, not pictures. A change can look faster because it
draws less, or draw something else (in #166, `ribbons` and `shapes` changed
on 12.8% and 10.0% of the pixels, including thin rects that got too dark,
which the numbers did not show). Before comparing performance, render the
same frames at the base and the head, with `--dump`, and compare the PNGs:

```sh
cargo run --release -p celesta-bench -- run --frames 1 --warmup 0 --dump out-head/ shapes ribbons
# the same at the base, then compare out-base/ and out-head/
```

A pull request that changes the output says so and shows that the difference
is intended; one that claims not to change it should show no difference. There
is no automatic check yet: it is a step for the author and the reviewer.

## Not measured

The benchmark covers the export path only (`submit`/`drain`,
`RenderQuality::Final`, a target the size of the scene). The preview path
(`RenderQuality::Draft`, `render_to_target` scaling to a viewport) has no
workload; a bug there has been caught by a test instead (#166). Adding one is
left open.

`--mode instructions` runs the CI comparison locally. It needs Linux with
Valgrind 3.22 or later and lavapipe (`mesa-vulkan-drivers`) as the only
Vulkan driver, or a container that has them.
