import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

const script = fileURLToPath(new URL('../run-cargo.mts', import.meta.url));
function run(args: string[], failure: string) {
  const tree = mkdtempSync(join(tmpdir(), 'cargo-packages-'));
  try {
    const stub = `const fs = require('node:fs');
      const action = require('node:path').basename(process.argv[1]);
      const args = process.argv.slice(2);
      const name = args[args.indexOf('-p') + 1];
      fs.appendFileSync('calls.jsonl', JSON.stringify({action, name, args}) + '\\n');
      process.exit(action + ':' + name === process.env.FAILURE ? 1 : 0);`;
    for (const action of ['build', 'test']) writeFileSync(join(tree, action), stub);
    const result = spawnSync(process.execPath, [script, '.', 'test', ...args], {
      cwd: tree, encoding: 'utf8', env: { ...process.env, CARGO: process.execPath, FAILURE: failure },
    });
    const calls = readFileSync(join(tree, 'calls.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
    return { ...result, calls };
  } finally { rmSync(tree, { recursive: true, force: true }); }
}

it('runs later packages after a test failure and returns failure overall', () => {
  const { status, calls, stderr } = run(['-p', 'celesta-codec-ffmpeg-x264', '-p', 'celesta-codec-ffmpeg', '-p', 'celesta-plugin-vrm', '--no-fail-fast'], 'test:celesta-codec-ffmpeg');
  expect(status, stderr).toBe(1);
  expect(calls.filter((call) => call.action === 'test').map((call) => call.name)).toEqual([
    'celesta-codec-ffmpeg', 'celesta-plugin-vrm', 'celesta-codec-ffmpeg-x264',
  ]);
  expect(stderr).toContain('Cargo failed for: celesta-codec-ffmpeg');
});

it('builds prerequisite programs in the requested profile and target without test arguments', () => {
  const { status, calls, stderr } = run(['-p', 'celesta-codec-ffmpeg-x264', '--locked', '--release', '--target', 'a-target', 'a_test_filter', '--', '--show-output'], '');
  expect(status, stderr).toBe(0);
  const builds = calls.filter((call) => call.action === 'build');
  expect(builds.map((call) => call.name)).toEqual(['celesta-codec-ffmpeg', 'celesta-codec-ffmpeg-x264']);
  for (const call of builds) {
    expect(call.args).toEqual(['-p', call.name, '--locked', '--release', '--target', 'a-target']);
  }
});

it('refuses tests with failed prerequisites and still reports independent package results', () => {
  const { status, calls, stderr } = run(['-p', 'celesta-codec-ffmpeg-x264', '-p', 'other'], 'build:celesta-codec-ffmpeg');
  expect(status, stderr).toBe(1);
  expect(calls.some((call) => call.action === 'test' && call.name === 'other')).toBe(true);
  expect(calls.some((call) => call.action === 'test' && call.name === 'celesta-codec-ffmpeg-x264')).toBe(false);
  expect(stderr).toContain('prerequisite build failed');
});
