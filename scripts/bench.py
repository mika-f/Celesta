#!/usr/bin/env python3
"""Compare the GPU renderer's performance between two revisions.

Builds `celesta-bench` (crates/bench) at a base revision and at the head
(by default the working tree, uncommitted changes included), runs its
workloads with both, and prints a Markdown table. Two modes:

- `time` (default): wall-clock ms/frame on this machine's GPU. Runs
  alternate base and head for `--rounds` rounds and the table gives medians.
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
        if (source / ".git").exists():
            git("checkout", "--quiet", "--detach", revision, cwd=source)
        else:
            shutil.rmtree(source, ignore_errors=True)
            git("worktree", "prune")
            git("worktree", "add", "--quiet", "--detach", str(source), revision)
    print(f"building celesta-bench for {label} ({describe(revision)})", file=sys.stderr)
    env = {**os.environ, "CARGO_TARGET_DIR": str(target_dir)}
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "--quiet", "-p", "celesta-bench"],
        check=True, cwd=source, env=env,
    )
    if react:
        # As in CI: the bridge spawns the compiled packages/react/dist/cli.js.
        pnpm = shutil.which("pnpm") or sys.exit("the nebula workload needs pnpm")
        for arguments in (["install", "--frozen-lockfile", "--silent"],
                          ["run", "--silent", "codegen", "--locked"],
                          ["run", "--silent", "build"]):
            subprocess.run([pnpm, "--dir", "packages/react", *arguments],
                           check=True, cwd=source, env=env, stdout=subprocess.DEVNULL)
    suffix = ".exe" if os.name == "nt" else ""
    binary = target_dir / "bench-bin" / f"celesta-bench-{label}{suffix}"
    binary.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(target_dir / "release" / f"celesta-bench{suffix}", binary)
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


def workloads_of(binary, options):
    """The requested workloads (default: all) that `binary` has, so a
    workload added by the head is measured on the head only."""
    listed = [line.split("\t")[0] for line in subprocess.run(
        [str(binary), "list"], check=True, text=True, stdout=subprocess.PIPE,
    ).stdout.splitlines()]
    return [name for name in options.workloads or listed if name in listed]


def measure_time(binaries, options):
    """{label: {workload: [ms/frame per round]}}, plus the adapter name."""
    samples = {label: {} for label in binaries}
    adapter = None
    arguments = {label: ["--frames", str(options.frames), "--warmup", str(options.warmup),
                         "--size", options.size, *workloads_of(binary, options)]
                 for label, binary in binaries.items()}
    labels = list(binaries)
    for round_index in range(options.rounds):
        # Alternate the order so drift (thermals, clocks) hits both sides.
        order = labels if round_index % 2 == 0 else labels[::-1]
        for label in order:
            print(f"round {round_index + 1}/{options.rounds}: {label}", file=sys.stderr)
            for result in run_bench(binaries[label], arguments[label]):
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
    probe = run_bench(first, ["--frames", "1", "--warmup", "0", "--size", "16x16", "rings"])[0]
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


def report(samples, options, adapter, revisions):
    """Returns the Markdown report and whether a workload regressed."""
    unit = "ms/frame" if options.mode == "time" else "instructions/frame"
    lines = [
        f"### celesta-bench: {options.mode}",
        "",
        f"Base `{revisions['base']}`, head `{revisions['head']}`. "
        f"{adapter}, {options.size}, {options.frames} frames after {options.warmup} warmup, "
        f"{options.rounds} round(s). Threshold {options.threshold:g}%.",
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
    for workload in head:
        if workload not in base:
            lines.append(f"| {workload} | — | {fmt(statistics.median(head[workload]))} | | new |")
            continue
        before, after = base[workload], head[workload]
        change = (statistics.median(after) / statistics.median(before) - 1) * 100
        # Wall-clock samples overlap when the change is within the noise.
        separated = options.mode != "time" or min(after) > max(before) or max(after) < min(before)
        if change > options.threshold and separated:
            verdict, regressed = "🔴 slower", True
        elif change < -options.threshold and separated:
            verdict = "🟢 faster"
        else:
            verdict = ""
        spread = ""
        if options.mode == "time" and len(before) > 1:
            spread = f" ({fmt(min(after))}–{fmt(max(after))})"
        lines.append(
            f"| {workload} | {fmt(statistics.median(before))} | "
            f"{fmt(statistics.median(after))}{spread} | {change:+.2f}% | {verdict} |"
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
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    compare = commands.add_parser("compare", help="compare a base revision with the head")
    compare.add_argument("workloads", nargs="*", help="workloads to run (default: all)")
    compare.add_argument("--base", help="base revision (default: merge base with origin/main)")
    compare.add_argument("--head", help="head revision (default: the working tree)")
    compare.add_argument("--mode", choices=["time", "instructions"], default="time")
    compare.add_argument("--rounds", type=int, help="runs per side (default: 5 time, 1 instructions)")
    compare.add_argument("--frames", type=int, help="measured frames (default: 120 time, 6 instructions)")
    compare.add_argument("--warmup", type=int, help="unmeasured frames first (default: 10 time, 2 instructions)")
    compare.add_argument("--size", help="WxH (default: 1920x1080 time, 640x360 instructions)")
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
    for name, (time_default, instructions_default) in {
        "rounds": (5, 1), "frames": (120, 6), "warmup": (10, 2),
        "size": ("1920x1080", "640x360"), "threshold": (5.0, 1.0),
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

    measure = measure_instructions if instructions else measure_time
    samples, adapter = measure(binaries, options)
    markdown, regressed = report(samples, options, adapter, revisions)
    print(markdown)
    if options.markdown:
        with open(options.markdown, "a") as file:
            file.write(markdown)
    if options.csv:
        write_csv(options.csv, samples, options)
    if regressed and options.fail_on_regression:
        sys.exit(1)


if __name__ == "__main__":
    main()
