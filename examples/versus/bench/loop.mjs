#!/usr/bin/env node
// Times the edit loop: change one color in the scene source, then get frame
// 300 as a PNG from the command line. fframes has to recompile the crate
// (incremental release build); Remotion re-bundles and opens Chromium; Celesta
// re-bundles and evaluates the entry.
//
//   node examples/versus/bench/loop.mjs [--runs 3]
//
// The color alternates between two values that look the same, and the
// original is restored at the end. Results go to loop.json next to this script.
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const exe = process.platform === 'win32' ? '.exe' : '';
const i = process.argv.indexOf('--runs');
const runs = i >= 0 ? Number(process.argv[i + 1]) : 3;
const out = path.join(here, 'out');
mkdirSync(out, { recursive: true });

const run = (cmd, args, cwd) => {
  const r = spawnSync(cmd, args, { cwd, stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')} failed:\n${r.stderr}`);
};
const remotionDir = path.join(here, 'remotion');
const fframesDir = path.join(here, 'fframes');
const tools = {
  fframes: {
    source: path.join(fframesDir, 'src/lib.rs'),
    frame: () => {
      run('cargo', ['build', '--release'], fframesDir);
      run(path.join(fframesDir, 'target/release', `nebula-fframes${exe}`), ['frame', '300', '-o', out], fframesDir);
    },
  },
  remotion: {
    source: path.join(remotionDir, 'src/Nebula.tsx'),
    frame: () => run(process.execPath, [path.join(remotionDir, 'node_modules/@remotion/cli/remotion-cli.js'),
      'still', 'Nebula', path.join(out, 'remotion-300.png'), '--frame=300', '--overwrite', '--log=error'], remotionDir),
  },
  celesta: {
    source: path.join(here, 'celesta/nebula.tsx'),
    frame: () => run(path.join(root, 'target/release', `celesta-exporter${exe}`),
      ['--no-ui', '--overwrite', '--react', path.join(here, 'celesta/nebula.tsx'), '--frame', '300',
        path.join(out, 'celesta-300.png')], root),
  },
};

const results = Object.fromEntries(Object.keys(tools).map((name) => [name, []]));
const colors = ['#8FB8FE', '#8FB8FF'];
try {
  for (let n = 0; n < runs; n++) {
    for (const [name, tool] of Object.entries(tools)) {
      const text = readFileSync(tool.source, 'utf8');
      writeFileSync(tool.source, text.replace(colors[(n + 1) % 2], colors[n % 2]));
      const start = process.hrtime.bigint();
      tool.frame();
      const seconds = Number(process.hrtime.bigint() - start) / 1e9;
      results[name].push(Math.round(seconds * 1000) / 1000);
      console.log(`${name.padEnd(9)} run ${n + 1}: ${seconds.toFixed(2)} s`);
    }
  }
} finally {
  for (const tool of Object.values(tools)) {
    writeFileSync(tool.source, readFileSync(tool.source, 'utf8').replace(colors[0], colors[1]));
  }
}
writeFileSync(path.join(here, 'loop.json'), `${JSON.stringify({ date: new Date().toISOString(), results }, null, 2)}\n`);
