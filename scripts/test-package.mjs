import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const packageDirectory = resolve(process.argv[2]);
const mac = process.platform === 'darwin';
const runtime = mac ? join(packageDirectory, 'Contents/Resources') : join(packageDirectory, 'runtime');
const node = mac ? join(packageDirectory, 'Contents/Helpers/node') : join(runtime, 'node.exe');
const exporter = join(packageDirectory, mac ? 'Contents/MacOS/Celesta-export' : 'Celesta-export.exe');
const directory = mkdtempSync(join(tmpdir(), 'Celesta package 日本語 '));
const entry = join(directory, 'test.tsx');
const env = { ...process.env, PATH: mac ? '/usr/bin:/bin' : join(process.env.SystemRoot, 'System32') };
for (const name of Object.keys(env)) {
  if (name.startsWith('DYLD_')) delete env[name];
}
delete env.NODE_PATH;
delete env.NODE_OPTIONS;
delete env.ESBUILD_BINARY_PATH;

function run(executable, args, input) {
  const result = spawnSync(executable, args, {
    cwd: directory, env, input, encoding: 'utf8', timeout: 120_000, windowsHide: true,
  });
  assert.equal(result.status, 0, `${result.error ?? ''}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

function assertNoLinks(directory) {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    const stat = lstatSync(path);
    assert.ok(!stat.isSymbolicLink(), `Package depends on a link: ${path}`);
    if (stat.isDirectory()) assertNoLinks(path);
  }
}

try {
  assertNoLinks(runtime);
  writeFileSync(entry, `
import { useState } from 'react';
import { Composition, Text, useCurrentFrame } from '@celesta/react';
function Content() {
  const [text] = useState('Celesta');
  const frame = useCurrentFrame();
  return <Text x={0} y={0}>{text + frame}</Text>;
}
export default function Root() {
  return <Composition width={64} height={64} fps={1} durationInFrames={1}><Content /></Composition>;
}
`);
  const protocol = run(node, [join(runtime, 'react/dist/cli.js'), entry],
    '{"time":{"value":0,"timescale":1}}\n');
  const messages = protocol.trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.ok(messages[0].config);
  assert.ok(messages[1].scene);
  const output = join(directory, 'test.mp4');
  run(exporter, ['--react', entry, output]);
  const mp4 = readFileSync(output);
  assert.equal(mp4.toString('ascii', 4, 8), 'ftyp');
  assert.ok(mp4.length > 100);
  assert.ok(!existsSync(join(runtime, 'react/.tmp')), 'Runtime must not write into the install directory');
  assert.ok(
    existsSync(join(runtime, 'react/dist/project-types/version.json')),
    'Package must include the TypeScript support template',
  );
  console.log('Package smoke test passed: bundled React hooks and MP4 export, without Node.js on PATH.');
} finally {
  assert.ok(directory.startsWith(join(tmpdir(), 'Celesta package 日本語 ')));
  rmSync(directory, { recursive: true, force: true });
}
