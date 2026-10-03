// Numbers shown in the film, taken from the benchmark's own output files so
// the video always matches the last measurement (bench/run.mjs, bench/loop.mjs).
import loop from './bench/loop.json';
import runs from './bench/results.json';
import type { ToolId } from './constants';

type Run = { name: string; seconds: number };
type BenchRecord = { machine: { cpu: string; threads: number; gpu: string; memoryGiB: number; os: string };
  remotionFlags: string; results: Run[] };

const records = runs as BenchRecord[];
const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
// The latest run.mjs invocation that measured `name` with these Remotion
// flags. Batches are never pooled, so each median is over one batch.
const latest = (name: string, flags = '') =>
  [...records].reverse().find((r) => (name !== 'remotion' || r.remotionFlags === flags)
    && r.results.some((x) => x.name === name));
const batch = (name: string, flags = '') =>
  latest(name, flags)?.results.filter((r) => r.name === name).map((r) => r.seconds) ?? [];
const required = (values: number[], what: string) => {
  if (!values.length) throw new Error(`no measurements for ${what}; run bench/run.mjs and bench/loop.mjs first`);
  return median(values);
};

export const FRAMES = 600;

// Remotion with --gl=angle --concurrency=100%, or null when it was not measured.
export const TUNED = (() => {
  const t = batch('remotion', '--gl=angle --concurrency=100%');
  return t.length ? { min: Math.min(...t), max: Math.max(...t), median: median(t) } : null;
})();

// Complete 600-frame MP4 export, median seconds of the latest batch.
export const EXPORT: Record<ToolId, number> = {
  remotion: required(batch('remotion'), 'the Remotion export'),
  fframes: required(batch('fframes'), 'the fframes export'),
  celesta: required(batch('celesta'), 'the Celesta export'),
};

// How many runs the medians above summarize, for the footnotes.
const summary = (counts: number[]) => {
  const n = Math.min(...counts);
  const most = Math.max(...counts);
  if (most === 1) return 'single run';
  return n === most ? `median of ${n}` : `median of ${n}–${most}`;
};
export const EXPORT_RUNS = summary(['remotion', 'fframes', 'celesta'].map((id) => batch(id).length));

// Edit one value, then get one frame as a PNG, median seconds.
export const LOOP: Record<ToolId, number> = {
  remotion: required(loop.results.remotion, 'the Remotion edit loop'),
  fframes: required(loop.results.fframes, 'the fframes edit loop'),
  celesta: required(loop.results.celesta, 'the Celesta edit loop'),
};
export const LOOP_RUNS = summary(Object.values(loop.results).map((r) => r.length));

const osName = ({ os }: BenchRecord['machine']) => {
  const win = /^Windows_NT 10\.0\.(\d+)/.exec(os);
  if (win) return Number(win[1]) >= 22000 ? 'Windows 11' : 'Windows 10';
  return os.replace('Darwin', 'macOS');
};
const machine = latest('celesta')!.machine;
export const MACHINE = `${machine.cpu.replace(/^.*(i\d-\w+).*$/, 'Core $1')} · ${machine.gpu.split(',')[0].replace('NVIDIA GeForce ', '')} · ${machine.memoryGiB} GB · ${osName(machine)}`;

// Measured once while setting the projects up (see README.md).
export const SETUP = {
  remotion: { install: 'npm install', seconds: 16 },
  fframes: { install: 'first cargo build --release', seconds: 88 },
} as const;

// Non-blank, non-comment lines of each implementation.
export const LINES = {
  remotion: { scene: 115, other: 3 + 5 + 2 + 19, files: ['src/Nebula.tsx', 'src/Root.tsx', 'src/index.ts', 'remotion.config.ts', 'package.json'] },
  fframes: { scene: 134, other: 36 + 23, files: ['src/lib.rs', 'src/main.rs', 'Cargo.toml'] },
  celesta: { scene: 138, other: 0, files: ['nebula.tsx'] },
} as const;

export const fmt = (s: number) => s.toFixed(1);
