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
const exportTimes = (name: string, flags: string) =>
  records.filter((r) => r.remotionFlags === flags || name !== 'remotion')
    .flatMap((r) => r.results).filter((r) => r.name === name).map((r) => r.seconds);

export const FRAMES = 600;

// Remotion with --gl=angle --concurrency=100%: slower and noisier than its defaults here.
export const TUNED = (() => {
  const t = exportTimes('remotion', '--gl=angle --concurrency=100%');
  return { min: Math.min(...t), max: Math.max(...t), median: median(t) };
})();

// Complete 600-frame MP4 export, median seconds.
export const EXPORT = {
  remotion: median(exportTimes('remotion', '')),
  fframes: median(exportTimes('fframes', '')),
  celesta: median(exportTimes('celesta', '')),
};

// Edit one value, then get one frame as a PNG, median seconds.
export const LOOP: Record<ToolId, number> = {
  remotion: median(loop.results.remotion),
  fframes: median(loop.results.fframes),
  celesta: median(loop.results.celesta),
};

const machine = records[records.length - 1].machine;
export const MACHINE = `${machine.cpu.replace(/^.*(i\d-\w+).*$/, 'Core $1')} · ${machine.gpu.split(',')[0].replace('NVIDIA GeForce ', '')} · ${machine.memoryGiB} GB · Windows 11`;

// Measured once while setting the projects up (see README.md).
export const SETUP = {
  remotion: { install: 'npm install', seconds: 16 },
  fframes: { install: 'cargo install cargo-fframes + first build', seconds: 25 + 88 },
} as const;

// Non-blank, non-comment lines of each implementation.
export const LINES = {
  remotion: { scene: 115, other: 3 + 5 + 2 + 19, files: ['src/Nebula.tsx', 'src/Root.tsx', 'src/index.ts', 'remotion.config.ts', 'package.json'] },
  fframes: { scene: 134, other: 36 + 23, files: ['src/lib.rs', 'src/main.rs', 'Cargo.toml'] },
  celesta: { scene: 138, other: 0, files: ['nebula.tsx'] },
} as const;

export const fmt = (s: number) => s.toFixed(1);
