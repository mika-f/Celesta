#!/usr/bin/env node
// Times a complete MP4 export of the NEBULA scene with each framework.
//
//   node examples/versus/bench/run.mjs [--runs 3] [--only celesta,remotion,fframes] [--out DIR]
//
// Run it from the repository root after building each implementation (see
// README.md). Every run is a cold CLI invocation, measured from process start
// to exit, as a user would experience it: Remotion's bundling and browser
// launch, Celesta's runtime startup and fframes' GPU context setup all count.
// Results are appended to results.json next to this script.
import { spawnSync } from 'node:child_process';
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
const runs = Number(option('runs', '3'));
const only = option('only', 'celesta,remotion,fframes').split(',');
const out = path.resolve(option('out', path.join(here, 'out')));
mkdirSync(out, { recursive: true });

const remotionDir = path.join(here, 'remotion');
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
    args: (file) => [path.join(remotionDir, 'node_modules/@remotion/cli/remotion-cli.js'), 'render', 'Nebula', file,
      '--overwrite', ...(option('remotion-flags', '') || '').split(' ').filter(Boolean)],
  },
  fframes: {
    label: 'fframes (Skia Vulkan)',
    cwd: path.join(here, 'fframes'),
    cmd: path.join(here, 'fframes/target/release', `nebula-fframes${exe}`),
    args: (file) => ['render', '-o', file],
  },
};

const ffprobe = (file) => {
  const r = spawnSync('ffprobe', ['-v', 'error', '-count_frames', '-select_streams', 'v:0',
    '-show_entries', 'stream=width,height,nb_read_frames,codec_name,pix_fmt', '-of', 'json', file], { encoding: 'utf8' });
  return r.status === 0 ? JSON.parse(r.stdout).streams[0] : null;
};

const results = [];
for (const name of only) {
  const p = pipelines[name];
  if (!p) throw new Error(`unknown pipeline ${name}`);
  for (let run = 1; run <= runs; run++) {
    const file = path.join(out, `${name}.mp4`);
    const start = process.hrtime.bigint();
    const r = spawnSync(p.cmd, p.args(file), { cwd: p.cwd, stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' });
    const seconds = Number(process.hrtime.bigint() - start) / 1e9;
    if (r.status !== 0) {
      console.error(r.stderr);
      throw new Error(`${name} failed with exit code ${r.status}`);
    }
    const probe = ffprobe(file);
    const entry = { name, label: p.label, run, seconds: Math.round(seconds * 1000) / 1000,
      fps: Math.round((600 / seconds) * 10) / 10, bytes: statSync(file).size, probe };
    results.push(entry);
    console.log(`${p.label.padEnd(24)} run ${run}: ${entry.seconds.toFixed(2)} s  (${entry.fps} fps)`);
  }
}

const gpu = spawnSync('nvidia-smi', ['--query-gpu=name,driver_version', '--format=csv,noheader'], { encoding: 'utf8' });
const record = {
  date: new Date().toISOString(),
  machine: {
    cpu: os.cpus()[0].model.trim(), threads: os.cpus().length,
    memoryGiB: Math.round(os.totalmem() / 2 ** 30), gpu: gpu.status === 0 ? gpu.stdout.trim() : 'unknown',
    os: `${os.type()} ${os.release()}`, node: process.version,
  },
  scene: '1920x1080, 60 fps, 600 frames, H.264 (libx264 medium, CRF 18), no audio',
  remotionFlags: option('remotion-flags', ''),
  results,
};
const file = path.join(here, 'results.json');
const all = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : [];
all.push(record);
writeFileSync(file, `${JSON.stringify(all, null, 2)}\n`);
