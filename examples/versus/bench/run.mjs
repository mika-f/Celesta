#!/usr/bin/env node
// Times a complete MP4 export of the NEBULA scene with each framework.
//
//   node examples/versus/bench/run.mjs [--runs 15] [--only celesta,remotion,remotion-tuned,fframes]
//     [--remotion-flags FLAGS] [--out DIR] [--build-notes TEXT]
//
// Run it from the repository root after building each implementation (see
// README.md). Every run is a cold CLI invocation, measured from process start
// to exit, as a user would experience it: Remotion's bundling and browser
// launch, Celesta's runtime startup and fframes' GPU context setup all count.
//
// The configurations are interleaved: each round runs every configuration once
// in a rotated order, so drift over the session (thermals, background load) is
// spread across the tools instead of landing on whichever ran last. A warm-up
// round runs first. Its times are stored as `firstRuns` and kept out of
// `results`, since a newly built executable pays a one-off launch cost.
// Results are appended to results.json next to this script.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const exe = process.platform === 'win32' ? '.exe' : '';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const runs = Number(option('runs', '15'));
if (!Number.isInteger(runs) || runs < 1) throw new Error(`--runs must be a positive integer, got ${option('runs', '15')}`);
const only = option('only', 'celesta,remotion,remotion-tuned,fframes').split(',');
const out = path.resolve(option('out', path.join(here, 'out')));
const tunedFlags = option('remotion-flags', '--gl=angle --concurrency=100%');
mkdirSync(out, { recursive: true });

const remotionDir = path.join(here, 'remotion');
const remotion = (flags) => (file) => [path.join(remotionDir, 'node_modules/@remotion/cli/remotion-cli.js'), 'render', 'Nebula', file,
  '--overwrite', ...flags.split(' ').filter(Boolean)];
const pipelines = {
  celesta: {
    label: 'Celesta (wgpu)',
    cwd: root,
    cmd: path.join(root, 'target/release', `celesta-exporter${exe}`),
    args: (file) => ['--no-ui', '--overwrite', '--react', path.join(here, 'celesta/nebula.tsx'), file],
  },
  remotion: {
    label: 'Remotion (Chromium)',
    cwd: remotionDir,
    cmd: process.execPath,
    args: remotion(''),
    remotionFlags: '',
  },
  'remotion-tuned': {
    label: 'Remotion (tuned)',
    cwd: remotionDir,
    cmd: process.execPath,
    args: remotion(tunedFlags),
    remotionFlags: tunedFlags,
  },
  fframes: {
    label: 'fframes (Skia Vulkan)',
    cwd: path.join(here, 'fframes'),
    cmd: path.join(here, 'fframes/target/release', `nebula-fframes${exe}`),
    args: (file) => ['render', '-o', file],
  },
};
for (const name of only) if (!pipelines[name]) throw new Error(`unknown pipeline ${name}`);

const ffprobe = (file) => {
  const r = spawnSync('ffprobe', ['-v', 'error', '-count_frames', '-select_streams', 'v:0',
    '-show_entries', 'stream=width,height,nb_read_frames,codec_name,pix_fmt', '-of', 'json', file], { encoding: 'utf8' });
  return r.status === 0 ? JSON.parse(r.stdout).streams[0] : null;
};

const measure = (name, run) => {
  const p = pipelines[name];
  const file = path.join(out, `${name}.mp4`);
  const start = process.hrtime.bigint();
  const r = spawnSync(p.cmd, p.args(file), { cwd: p.cwd, stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' });
  const seconds = Number(process.hrtime.bigint() - start) / 1e9;
  if (r.error) throw new Error(`could not start ${name} (${p.cmd}): ${r.error.message}`);
  if (r.status !== 0) {
    console.error(r.stderr);
    throw new Error(`${name} failed with exit code ${r.status}`);
  }
  const probe = ffprobe(file);
  if (probe && Number(probe.nb_read_frames) !== 600) throw new Error(`${name} wrote ${probe.nb_read_frames} frames, expected 600`);
  const entry = { name, label: p.label, run, seconds: Math.round(seconds * 1000) / 1000,
    fps: Math.round((600 / seconds) * 10) / 10, bytes: statSync(file).size, probe };
  if (name.startsWith('remotion')) entry.remotionFlags = p.remotionFlags;
  console.log(`${p.label.padEnd(24)} ${run === 0 ? 'warm-up' : `run ${run}`}: ${entry.seconds.toFixed(2)} s  (${entry.fps} fps)`);
  return entry;
};

const firstRuns = [];
const results = [];
// Round 0 is the warm-up; rounds 1..runs are measured. The start of the order
// rotates by one position each round.
for (let round = 0; round <= runs; round++) {
  const order = only.map((_, i) => only[(i + round) % only.length]);
  for (const name of order) (round === 0 ? firstRuns : results).push(measure(name, round));
}

const median = (v) => {
  const s = [...v].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
console.log('');
for (const name of only) {
  const t = results.filter((r) => r.name === name).map((r) => r.seconds);
  console.log(`${pipelines[name].label.padEnd(24)} median ${median(t).toFixed(2)} s  range ${Math.min(...t).toFixed(2)}–${Math.max(...t).toFixed(2)} s`);
}

const git = (...a) => spawnSync('git', a, { cwd: root, encoding: 'utf8' }).stdout?.trim() ?? '';
const sha256 = (file) => existsSync(file) ? createHash('sha256').update(readFileSync(file)).digest('hex').slice(0, 16) : null;
const gpu = spawnSync('nvidia-smi', ['--query-gpu=name,driver_version', '--format=csv,noheader'], { encoding: 'utf8' });
const record = {
  date: new Date().toISOString(),
  machine: {
    cpu: os.cpus()[0].model.trim(), threads: os.cpus().length,
    memoryGiB: Math.round(os.totalmem() / 2 ** 30), gpu: gpu.status === 0 ? gpu.stdout.trim() : 'unknown',
    os: `${os.type()} ${os.release()}`, node: process.version,
  },
  scene: '1920x1080, 60 fps, 600 frames, H.264 (libx264 medium, CRF 18), no audio',
  // What was measured: the checkout, whether it had uncommitted changes, and
  // the first 16 hex digits of each executable's sha256.
  build: {
    revision: git('rev-parse', '--short', 'HEAD'),
    dirty: git('status', '--porcelain').length > 0,
    sha256: Object.fromEntries(['celesta', 'fframes'].filter((n) => only.includes(n)).map((n) => [n, sha256(pipelines[n].cmd)])),
    notes: option('build-notes', ''),
  },
  interleaved: true,
  rounds: runs,
  firstRuns,
  results,
};
const file = path.join(here, 'results.json');
const all = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : [];
all.push(record);
writeFileSync(file, `${JSON.stringify(all, null, 2)}\n`);
