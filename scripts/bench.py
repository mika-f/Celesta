#!/usr/bin/env python3
"""Compare the GPU renderer's performance between two revisions.

Builds `celesta-bench` (crates/bench) at a base revision and at the head
(by default the working tree, uncommitted changes included), runs its
workloads with both, and prints a Markdown table. Two modes:

- `time` (default): wall-clock ms/frame on this machine's GPU. Runs
  base and head in a seeded random order for `--rounds` rounds (after one
  discarded round) and the table gives medians with a bootstrap interval.
  Use this on a machine with a real GPU.
- `instructions`: instructions per frame, counted by Valgrind's Cachegrind
  while Mesa's software Vulkan driver (lavapipe) renders, so shader work is
  counted as CPU instructions. Deterministic enough to compare on a shared
  CI runner without a GPU. Linux only.

  scripts/bench.py compare [--base REV] [--head REV] [--mode time|instructions] [workload ...]
"""

import argparse
import concurrent.futures
import csv
import json
import os
from pathlib import Path
import random
import re
import shutil
import statistics
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
BENCH_CRATE = "crates/bench/Cargo.toml"
# Cachegrind's `summary:` line holds the total instruction count.
SUMMARY = re.compile(r"^summary:\s+(\d+)", re.MULTILINE)


def git(*arguments, cwd=ROOT):
    return subprocess.run(
        ["git", *arguments], check=True, cwd=cwd, text=True, stdout=subprocess.PIPE,
    ).stdout.strip()


def build(revision, label, target_dir, worktrees, react):
    """Builds celesta-bench at `revision` (None: the working tree) and returns
    a copy of the binary, or None if that revision has no benchmark. With
    `react`, also builds that revision's React runtime for `nebula`."""
    if revision is None:
        source = ROOT
    else:
        if subprocess.run(
            ["git", "cat-file", "-e", f"{revision}:{BENCH_CRATE}"],
            cwd=ROOT, stderr=subprocess.DEVNULL,
        ).returncode != 0:
            return None
        # A worktree that stays at the same path keeps Cargo's incremental
        # state for the workspace crates between invocations.
        source = worktrees / label
        if (source / ".git").is_file():
            git("checkout", "--quiet", "--detach", revision, cwd=source)
        elif source.exists():
            sys.exit(f"{source} exists but is not a worktree; remove it or pass --worktrees")
        else:
            git("worktree", "prune")
            git("worktree", "add", "--quiet", "--detach", str(source), revision)
    print(f"building celesta-bench for {label} ({describe(revision)})", file=sys.stderr)
    # Shared artifacts can reuse a path dependency from the other revision.
    cargo_target = target_dir / label
    env = {**os.environ, "CARGO_TARGET_DIR": str(cargo_target)}
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "--quiet", "-p", "celesta-bench"],
        check=True, cwd=source, env=env,
    )
    if react:
        # As in CI: the bridge spawns the compiled packages/cli/dist/cli.js.
        # Revisions from before the package split build packages/react alone,
        # whose dist/cli.js their bridge spawns instead.
        pnpm = shutil.which("pnpm") or sys.exit("the nebula workload needs pnpm")
        build = (["run", "--silent", "build:runtime"] if (source / "packages/cli").is_dir()
                 else ["--dir", "packages/react", "run", "--silent", "build"])
        for arguments in (["install", "--frozen-lockfile", "--silent"],
                          ["--dir", "packages/react", "run", "--silent", "codegen", "--locked"],
                          build):
            subprocess.run([pnpm, *arguments],
                           check=True, cwd=source, env=env, stdout=subprocess.DEVNULL)
    suffix = ".exe" if os.name == "nt" else ""
    binary = target_dir / "bench-bin" / f"celesta-bench-{label}{suffix}"
    binary.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(cargo_target / "release" / f"celesta-bench{suffix}", binary)
    return binary


def describe(revision):
    if revision is None:
        dirty = git("status", "--porcelain", "--untracked-files=no")
        return git("rev-parse", "--short", "HEAD") + (" + uncommitted changes" if dirty else "")
    return git("rev-parse", "--short", revision)


def run_bench(binary, arguments, env=None):
    output = subprocess.run(
        [str(binary), "run", *arguments], check=True, text=True,
        stdout=subprocess.PIPE, env=env,
    ).stdout
    return [json.loads(line) for line in output.splitlines() if line.startswith("{")]


def listed_workloads(binary):
    return [line.split("\t")[0] for line in subprocess.run(
        [str(binary), "list"], check=True, text=True, stdout=subprocess.PIPE,
    ).stdout.splitlines()]


def workloads_of(binary, options):
    """The requested workloads (default: all) that `binary` has, so a
    workload added by the head is measured on the head only, and one the
    head removed on the base only."""
    listed = listed_workloads(binary)
    return [name for name in options.workloads or listed if name in listed]


def measure_time(binaries, options):
    """{label: {workload: [ms/frame per round]}}, plus the adapter name."""
    samples = {label: {} for label in binaries}
    adapter = None
    workloads = {label: workloads_of(binary, options) for label, binary in binaries.items()}
    arguments = {label: ["--frames", str(options.frames), "--warmup", str(options.warmup),
                         "--size", options.size, *workloads[label]]
                 for label in binaries}
    # A side with none of the requested workloads is skipped: celesta-bench
    # would run all of its workloads when given none.
    labels = [label for label in binaries if workloads[label]]
    rng = random.Random(options.seed)
    # Round 0 is not recorded: a freshly built executable is slow on its
    # first run (page cache, code signing checks), and so is a cold GPU.
    for round_index in range(options.rounds + 1):
        # A random order per round, from the reported seed, keeps drift
        # (thermals, clocks) and any first-in-round effect from lining up
        # with one side.
        order = rng.sample(labels, len(labels))
        for label in order:
            kind = "discarded" if round_index == 0 else f"{round_index}/{options.rounds}"
            print(f"round {kind}: {label}", file=sys.stderr)
            for result in run_bench(binaries[label], arguments[label]):
                if round_index > 0:
                    samples[label].setdefault(result["workload"], []).append(result["ms_per_frame"])
                adapter = f'{result["adapter"]} ({result["backend"]})'
    return samples, adapter


def count_instructions(binary, workload, options, out):
    env = {
        **os.environ,
        # One worker each, so idle threads spinning between jobs do not add
        # instructions that depend on scheduling.
        "RAYON_NUM_THREADS": "1",
        "LP_NUM_THREADS": "1",
        # lavapipe compiles some shader variants on first use, which can be
        # in a measured frame. Without its disk cache that compile is
        # always counted, instead of depending on which run went first.
        "MESA_SHADER_CACHE_DISABLE": "true",
    }
    # celesta-bench switches instrumentation on for the measured frames only.
    result = subprocess.run(
        ["valgrind", "--tool=cachegrind", "--cache-sim=no", "--branch-sim=no",
         "--instr-at-start=no", "--smc-check=all-non-file", f"--cachegrind-out-file={out}",
         str(binary), "run", "--frames", str(options.frames), "--warmup", str(options.warmup),
         "--size", options.size, workload],
        env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    if result.returncode != 0:
        sys.exit(f"{binary.name} {workload} failed under valgrind:\n{result.stderr[-4000:]}")
    return int(SUMMARY.search(out.read_text()).group(1)) / options.frames


def measure_instructions(binaries, options):
    """{label: {workload: [instructions/frame per round]}}, plus the adapter name."""
    first = next(iter(binaries.values()))
    probe = run_bench(first, ["--frames", "1", "--warmup", "0", "--size", "32x18", "rings"])[0]
    adapter = f'{probe["adapter"]} ({probe["backend"]})'
    if "llvmpipe" not in probe["adapter"]:
        sys.exit(f"instructions mode needs lavapipe (llvmpipe), but the adapter is {adapter}")
    workloads = {label: workloads_of(binary, options) for label, binary in binaries.items()}
    samples = {label: {workload: [] for workload in workloads[label]} for label in binaries}
    with tempfile.TemporaryDirectory() as scratch, \
            concurrent.futures.ThreadPoolExecutor(options.jobs) as pool:
        jobs = [
            (label, workload, pool.submit(
                count_instructions, binary, workload, options,
                Path(scratch) / f"{label}-{workload}-{run}.out",
            ))
            for run in range(options.rounds)
            for label, binary in binaries.items()
            for workload in workloads[label]
        ]
        for label, workload, job in jobs:
            samples[label][workload].append(job.result())
    return samples, adapter


def median_change_interval(before, after, seed, resamples=5000, confidence=0.95):
    """Percentile-bootstrap confidence interval (in percent) of the change
    of the median from `before` to `after`."""
    rng = random.Random(seed)
    changes = sorted(
        (statistics.median(rng.choices(after, k=len(after)))
         / statistics.median(rng.choices(before, k=len(before))) - 1) * 100
        for _ in range(resamples)
    )
    tail = (1 - confidence) / 2
    return changes[int(tail * resamples)], changes[int((1 - tail) * resamples) - 1]


def report(samples, options, adapter, revisions):
    """Returns the Markdown report and whether a workload regressed."""
    unit = "ms/frame" if options.mode == "time" else "instructions/frame"
    lines = [
        f"### celesta-bench: {options.mode}",
        "",
        f"Base `{revisions['base']}`, head `{revisions['head']}`. "
        f"{adapter}, {options.size}, {options.frames} frames after {options.warmup} warmup, "
        f"{options.rounds} round(s)" + (f", seed {options.seed}" if options.mode == "time" else "")
        + f". Threshold {options.threshold:g}%.",
        "",
    ]
    regressed = False
    base, head = samples.get("base"), samples["head"]
    fmt = (lambda value: f"{value:.3f}") if options.mode == "time" else \
        (lambda value: f"{value / 1e6:,.2f} M")
    if base is None:
        lines += ["The base revision has no `crates/bench`, so only the head was measured.", "",
                  f"| Workload | Head ({unit}) |", "| --- | ---: |"]
        for workload, values in head.items():
            lines.append(f"| {workload} | {fmt(statistics.median(values))} |")
        return "\n".join(lines) + "\n", False

    lines += [f"| Workload | Base ({unit}) | Head ({unit}) | Change | |",
              "| --- | ---: | ---: | ---: | --- |"]
    for workload in base:
        if workload not in head:
            lines.append(f"| {workload} | {fmt(statistics.median(base[workload]))} | — | | "
                         "removed: no longer measured |")
    for workload in head:
        if workload not in base:
            lines.append(f"| {workload} | — | {fmt(statistics.median(head[workload]))} | | new |")
            continue
        before, after = base[workload], head[workload]
        change = (statistics.median(after) / statistics.median(before) - 1) * 100
        interval = ""
        separated = True
        if options.mode == "time":
            # Noise decides a wall-clock change: report it only if the 95%
            # bootstrap interval of the median's change excludes zero. With
            # fewer than 3 samples a side the interval means nothing, so
            # there is neither an interval nor a verdict.
            if len(before) > 2 and len(after) > 2:
                low, high = median_change_interval(before, after, options.seed)
                separated = low > 0 or high < 0
                interval = f" [{low:+.1f}, {high:+.1f}]"
            else:
                separated = False
        if change > options.threshold and separated:
            verdict, regressed = "🔴 slower" if options.mode == "time" else "🔴 more instructions", True
        elif change < -options.threshold and separated:
            # Instruction counts are not speed on a GPU.
            verdict = "🟢 faster" if options.mode == "time" else "🟢 fewer instructions"
        else:
            verdict = ""
        spread = before_spread = ""
        if options.mode == "time" and len(before) > 1:
            spread = f" ({fmt(min(after))}–{fmt(max(after))})"
            before_spread = f" ({fmt(min(before))}–{fmt(max(before))})"
        lines.append(
            f"| {workload} | {fmt(statistics.median(before))}{before_spread} | "
            f"{fmt(statistics.median(after))}{spread} | {change:+.2f}%{interval} | {verdict} |"
        )
    if options.mode == "instructions":
        lines += ["", "Instructions are counted with Cachegrind while lavapipe renders on the "
                  "CPU, so they track both the renderer's CPU work and its shader work, but "
                  "not GPU-specific costs such as memory bandwidth or occupancy. Confirm a "
                  "flagged change on a real GPU with `scripts/bench.py compare`."]
    return "\n".join(lines) + "\n", regressed


def write_csv(path, samples, options):
    metric, unit = ("gpu_submit_drain", "ms/frame") if options.mode == "time" else \
        ("instructions", "instructions/frame")
    with open(path, "w", newline="") as file:
        writer = csv.writer(file)
        writer.writerow(["workload", "version", "run", "frames", "metric", "value", "unit"])
        for label, workloads in samples.items():
            for workload, values in workloads.items():
                for run, value in enumerate(values, 1):
                    writer.writerow([workload, label, run, options.frames, metric,
                                     f"{value:.4f}" if options.mode == "time" else round(value),
                                     unit])


def main():
    # The report has en dashes; Windows consoles default to a legacy code page.
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    compare = commands.add_parser("compare", help="compare a base revision with the head")
    compare.add_argument("workloads", nargs="*", help="workloads to run (default: all)")
    compare.add_argument("--base", help="base revision (default: merge base with origin/main)")
    compare.add_argument("--head", help="head revision (default: the working tree)")
    compare.add_argument("--mode", choices=["time", "instructions"], default="time")
    compare.add_argument("--rounds", type=int, help="runs per side (default: 10 time, 1 instructions)")
    compare.add_argument("--seed", type=int, help="seed of the run order and the bootstrap (default: random; "
                         "the report shows it)")
    compare.add_argument("--frames", type=int, help="measured frames (default: 120 time, 6 instructions)")
    compare.add_argument("--warmup", type=int, help="unmeasured frames first (default: 10 time, 2 instructions)")
    compare.add_argument("--size", help="WxH (default: 1920x1080 time, 1280x720 instructions)")
    compare.add_argument("--threshold", type=float,
                         help="percent change reported as slower or faster (default: 5 time, 1 instructions)")
    compare.add_argument("--jobs", type=int, default=os.cpu_count(),
                         help="parallel Valgrind runs in instructions mode")
    compare.add_argument("--target-dir", type=Path,
                         help="Cargo target directory (default: target/ in the repository)")
    compare.add_argument("--worktrees", type=Path,
                         help="where to check out the base (default: target/bench-worktrees)")
    compare.add_argument("--markdown", type=Path, help="also append the report to this file")
    compare.add_argument("--csv", type=Path, help="write raw observations to this CSV file")
    compare.add_argument("--fail-on-regression", action="store_true",
                         help="exit with status 1 if a workload got slower")
    options = parser.parse_args()

    instructions = options.mode == "instructions"
    if options.seed is None:
        options.seed = random.SystemRandom().randrange(2**32)
    for name, (time_default, instructions_default) in {
        "rounds": (10, 1), "frames": (120, 6), "warmup": (10, 2),
        "size": ("1920x1080", "1280x720"), "threshold": (5.0, 1.0),
    }.items():
        if getattr(options, name) is None:
            setattr(options, name, instructions_default if instructions else time_default)

    # Resolved here: `HEAD~1` means something else inside a worktree.
    commit = lambda revision: git("rev-parse", "--verify", "--quiet", f"{revision}^{{commit}}")
    base = commit(options.base) if options.base else git("merge-base", "HEAD", "origin/main")
    options.head = options.head and commit(options.head)
    if instructions and shutil.which("valgrind") is None:
        sys.exit("instructions mode needs valgrind (3.22 or later)")
    target_dir = (options.target_dir or ROOT / "target").resolve()
    worktrees = (options.worktrees or target_dir / "bench-worktrees").resolve()
    revisions = {"base": describe(base), "head": describe(options.head)}
    react = not options.workloads or "nebula" in options.workloads
    binaries = {"base": build(base, "base", target_dir, worktrees, react),
                "head": build(options.head, "head", target_dir, worktrees, react)}
    if binaries["head"] is None:
        sys.exit("the head revision has no crates/bench")
    binaries = {label: binary for label, binary in binaries.items() if binary is not None}
    known = {name for binary in binaries.values() for name in listed_workloads(binary)}
    unknown = [name for name in options.workloads if name not in known]
    if unknown:
        sys.exit(f"unknown workload(s): {', '.join(unknown)} (known: {', '.join(sorted(known))})")

    measure = measure_instructions if instructions else measure_time
    samples, adapter = measure(binaries, options)
    markdown, regressed = report(samples, options, adapter, revisions)
    print(markdown)
    if options.markdown:
        with open(options.markdown, "a", encoding="utf-8") as file:
            file.write(markdown)
    if options.csv:
        write_csv(options.csv, samples, options)
    if regressed and options.fail_on_regression:
        sys.exit(1)


if __name__ == "__main__":
    main()
