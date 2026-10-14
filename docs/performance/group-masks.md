# Group masks: the `masks` workload

Group masks (#146) were added to the GPU renderer by PR #205. React and
`@celesta/web` drawing came later, with no change to the GPU renderer, so
scenes without masks take the same code paths as before. The `masks`
workload adds a mask to the benchmark, so later changes to the masking path
are measured. The perf workflow's instruction counts on each pull request
then check every workload against its base.

## Workload

`masks` (`crates/bench/src/workloads.rs`) draws two masked groups per frame,
as the design's examples do
(`docs/superpowers/specs/2026-10-09-group-mask-design.md`):

- 24 moving stripes seen only through the word CELESTA in 300 px text, an
  alpha mask of a text layer over the top half;
- a red rect seen through a disc whose radius changes every frame and
  whose edge is blurred (`blur: 24`), over the bottom half.

Each masked group draws onto three canvases: the mask, the children, and
the masked result. The blurred disc adds the blur's two passes.

## Measurements

Instruction counts come from the pull request's perf workflow
(`.github/workflows/perf.yml`, lavapipe under Cachegrind). That workflow
also compares every other workload with the base, at a 1% threshold.

Wall-clock times on the Apple M5 used for this change are not reported.
The machine was shared, with load averages between 40 and 75. Across nine
runs of `masks` and `text`, the within-run ratio ranged from 0.33 to 1.97,
which is too wide to say anything. Run the commands below on an idle
machine.

## Reproduce

```sh
pnpm install --frozen-lockfile
cargo run --release -p celesta-bench -- run --frames 600 --size 1920x1080 masks text shapes
python3 scripts/bench.py compare --base origin/main text shapes blur
```

On Linux with Valgrind, `python3 scripts/bench.py compare --mode
instructions` gives the counts the perf workflow uses. Add `--dump out/` to
the `run` command to save each workload's last frame and check the picture.
