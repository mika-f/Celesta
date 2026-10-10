// Numbers shown in the film, taken from the benchmark's own output files so
// the video always matches the last measurement (bench/run.mjs, bench/loop.mjs).
import loop from './bench/loop.json';
import runs from './bench/results.json';
import { ORDER, type ToolId } from './constants';

type Run = { name: string; seconds: number; remotionFlags?: string };
type BenchRecord = { machine: { cpu: string; threads: number; gpu: string; memoryGiB: number; os: string };
  remotionFlags?: string; results: Run[] };

const records = runs as BenchRecord[];
const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
// Interleaved batches tag each Remotion run with its flags; older batches set
// them once for the whole record.
const flagsOf = (record: BenchRecord, run: Run) => run.remotionFlags ?? record.remotionFlags ?? '';
const TUNED_FLAGS = '--gl=angle --concurrency=100%';
const runsOf = (record: BenchRecord, name: string, flags = '') =>
  record.results.filter((r) => r.name === name && (name !== 'remotion' || flagsOf(record, r) === flags)).map((r) => r.seconds);
// The latest run.mjs invocation that measured `name` with these Remotion
// flags. Batches are never pooled, so each median is over one batch.
const latest = (name: string, flags = '') =>
  [...records].reverse().find((r) => runsOf(r, name, flags).length);
const batch = (name: string, flags = '') => {
  const record = latest(name, flags);
  return record ? runsOf(record, name, flags) : [];
};
const required = (values: number[], what: string) => {
  if (!values.length) throw new Error(`no measurements for ${what}; run bench/run.mjs and bench/loop.mjs first`);
  return median(values);
};

export const FRAMES = 600;

// Remotion with --gl=angle --concurrency=100%, or null when it was not measured.
// `baseline` is default Remotion from the same batch when there is one, so the
// comparison is not skewed by drift between sessions.
export const TUNED = (() => {
  const record = latest('remotion', TUNED_FLAGS);
  if (!record) return null;
  const t = runsOf(record, 'remotion', TUNED_FLAGS);
  const same = runsOf(record, 'remotion');
  return { min: Math.min(...t), max: Math.max(...t), median: median(t), baseline: same.length ? median(same) : null };
})();

// Complete 600-frame MP4 export, median seconds of the latest batch.
export const EXPORT: Record<ToolId, number> = {
  remotion: required(batch('remotion'), 'the Remotion export'),
  fframes: required(batch('fframes'), 'the fframes export'),
  celesta: required(batch('celesta'), 'the Celesta export'),
};

// Medians closer than this are reported as a tie, not a win. The measured
// run-to-run spread within one tool is of the same size, and the ranking of
// the two fastest tools differed between machines.
export const TIE = 0.05;
export const FASTEST = ORDER.reduce((best, id) => EXPORT[id] < EXPORT[best] ? id : best);
export const TIED_WITH_FASTEST = ORDER.filter((id) => id !== FASTEST && EXPORT[id] <= EXPORT[FASTEST] * (1 + TIE));

// Seconds saved by Remotion's tuned flags as a fraction of default Remotion,
// compared within one batch when possible.
export const TUNING_GAIN = TUNED ? 1 - TUNED.median / (TUNED.baseline ?? EXPORT.remotion) : null;

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
