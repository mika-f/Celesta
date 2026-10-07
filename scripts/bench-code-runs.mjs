// Compare the current Code with its implementation at a ref, using the same
// release renderer for both. Build @celesta/react first.
import { execFileSync } from 'node:child_process';
import { readFileSync, mkdirSync, writeFileSync, symlinkSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('..', import.meta.url));
const baseline = process.argv[2];
if (!baseline) throw new Error('usage: node scripts/bench-code-runs.mjs <baseline-ref> [samples]');
const samples = Number(process.argv[3] ?? 3);
if (!Number.isSafeInteger(samples) || samples < 1) throw new Error('samples must be a positive integer');
const directory = resolve(root, 'target/code-runs-baseline');
mkdirSync(directory, { recursive: true });
for (const file of ['index.ts', 'syntax.ts']) {
  writeFileSync(resolve(directory, file), execFileSync('git', ['show', `${baseline}:packages/code/src/${file}`], { cwd: root }));
}
if (!existsSync(resolve(directory, 'node_modules'))) symlinkSync(resolve(root, 'packages/code/node_modules'), resolve(directory, 'node_modules'));
const entry = resolve(root, 'packages/code/examples/benchmark.tsx');
writeFileSync(resolve(directory, 'benchmark.tsx'), readFileSync(entry, 'utf8').replace("'@celesta/code'", "'./index.ts'"));
// Observe the first actual compact Scene response and initial measurement RPCs.
// Only the first Scene writes a tiny probe file; steady-state timings do not
// parse or record responses.
const probe = resolve(directory, 'wire-probe.cjs');
writeFileSync(probe, `
  const fs = require('node:fs');
  const write = fs.writeSync;
  let measurements = 0, recorded = false;
  fs.writeSync = function(fd, chunk, ...args) {
    if (fd !== 1 || (typeof chunk !== 'string' && args[0] !== 0)) return write.call(this, fd, chunk, ...args);
    const line = typeof chunk === 'string' ? chunk : chunk.toString();
    if (line.startsWith('{"measureText":')) measurements++;
    if (!recorded && line.startsWith('{"scene":')) {
      recorded = true;
      const end = line.lastIndexOf(',"audio":');
      const scene = end < 0 ? JSON.stringify(JSON.parse(line).scene) : line.slice(9, end);
      fs.writeFileSync(process.env.CELESTA_CODE_WIRE_PROBE, JSON.stringify({ measurement_calls: measurements, scene_wire_bytes: Buffer.byteLength(scene) }));
    }
    return write.call(this, fd, chunk, ...args);
  };
`);
execFileSync('cargo', ['build', '--release', '-p', 'celesta-bench', '--example', 'code-runs'], { cwd: root, stdio: 'inherit' });
for (let sample = 0; sample < samples; sample++) {
  for (const multiline of [false, true]) {
    for (const [variant, path] of [['before', resolve(directory, 'benchmark.tsx')], ['after', entry]]) {
      const env = { ...process.env, CELESTA_CODE_WIRE_PROBE: resolve(directory, 'wire.json') };
      env.NODE_OPTIONS = `${process.env.NODE_OPTIONS ?? ''} --require=${JSON.stringify(probe)}`;
      if (multiline) env.CELESTA_CODE_BENCH_MULTILINE = '1';
      else delete env.CELESTA_CODE_BENCH_MULTILINE;
      const result = JSON.parse(execFileSync(resolve(root, 'target/release/examples/code-runs'), [path], { cwd: root, env, encoding: 'utf8' }));
      console.log(JSON.stringify({ sample, ...result, ...JSON.parse(readFileSync(env.CELESTA_CODE_WIRE_PROBE, 'utf8')), variant, workload: multiline ? 'multiline' : 'long-line' }));
    }
  }
}
