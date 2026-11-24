import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

const script = fileURLToPath(new URL('../run-cargo.mts', import.meta.url));

it('prints usage and handles malformed arguments', () => {
  for (const flag of ['--help', '-h']) {
    const { status, stdout, stderr } = spawnSync(process.execPath, [script, flag], { encoding: 'utf8' });
    expect(status, stderr).toBe(0);
    expect(stderr).toBe('');
    expect(stdout).toContain('Usage: node scripts/plugins/run-cargo.mts <workspace> <command> -p <package>');
    expect(stdout).toContain('Use nextest for cargo nextest run');
  }

  for (const [args, message] of [
    [['--bogus'], 'unexpected option "--bogus"; expected a workspace directory'],
    [[], 'expected a workspace directory and Cargo command'],
    [['plugins', 'test'], 'expected at least one -p <package>'],
    [['plugins', 'test', '-p', '--locked'], '-p needs a package name'],
    [['plugins', 'test', '-p', 'demo', '--target'], '--target needs a value'],
  ] as const) {
    const { status, stdout, stderr } = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
    expect(status).toBe(2);
    expect(stdout).toBe('');
    expect(stderr.trim().split(/\r?\n/)).toEqual([
      `run-cargo.mts: ${message}`,
      'Run node scripts/plugins/run-cargo.mts --help for usage.',
    ]);
  }
});

// CARGO is Node.js, so `cargo <action> ...` runs the stub file named <action>;
// `cargo nextest run ...` runs `nextest`, which writes a JUnit report as the
// ci profile does, naming the package in it (unless it is NO_REPORT).
interface Options { command?: string; noReport?: string; setup?: (tree: string) => void; inspect?: (tree: string) => void }
function run(args: string[], failure: string, { command = 'test', noReport = '', setup, inspect }: Options = {}) {
  const tree = mkdtempSync(join(tmpdir(), 'cargo-packages-'));
  try {
    const stub = `const fs = require('node:fs');
      const action = require('node:path').basename(process.argv[1]);
      const args = process.argv.slice(2);
      const name = args[args.indexOf('-p') + 1];
      fs.appendFileSync('calls.jsonl', JSON.stringify({action, name, args}) + '\\n');
      if (action === 'nextest' && name !== process.env.NO_REPORT) {
        fs.mkdirSync('target/nextest/ci', { recursive: true });
        fs.writeFileSync('target/nextest/ci/junit.xml', name);
      }
      process.exit(action + ':' + name === process.env.FAILURE ? 1 : 0);`;
    for (const action of ['build', 'test', 'nextest']) writeFileSync(join(tree, action), stub);
    setup?.(tree);
    const result = spawnSync(process.execPath, [script, '.', command, ...args], {
      cwd: tree, encoding: 'utf8', env: { ...process.env, CARGO: process.execPath, FAILURE: failure, NO_REPORT: noReport },
    });
    const calls = readFileSync(join(tree, 'calls.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
    inspect?.(tree);
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

it('forwards help after the Cargo command', () => {
  const { status, calls, stderr } = run(['-p', 'demo', '--', '--help'], '');
  expect(status, stderr).toBe(0);
  expect(calls).toEqual([{ action: 'test', name: 'demo', args: ['-p', 'demo', '--', '--help'] }]);
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

it.each(['--jobs', '-j'])('forwards negative %s values to Cargo and prerequisite builds', (flag) => {
  const { status, calls, stderr } = run(['-p', 'celesta-codec-ffmpeg-x264', flag, '-1'], '');
  expect(status, stderr).toBe(0);
  expect(calls.map((call) => call.action)).toEqual(['build', 'build', 'test']);
  for (const call of calls) expect(call.args.slice(-2)).toEqual([flag, '-1']);
});

it('refuses tests with failed prerequisites and still reports independent package results', () => {
  const { status, calls, stderr } = run(['-p', 'celesta-codec-ffmpeg-x264', '-p', 'other'], 'build:celesta-codec-ffmpeg');
  expect(status, stderr).toBe(1);
  expect(calls.some((call) => call.action === 'test' && call.name === 'other')).toBe(true);
  expect(calls.some((call) => call.action === 'test' && call.name === 'celesta-codec-ffmpeg-x264')).toBe(false);
  expect(stderr).toContain('prerequisite build failed');
});

it('runs nextest per package and keeps each package\'s JUnit report', () => {
  const { status, calls, stderr } = run(
    ['-p', 'celesta-codec-ffmpeg', '-p', 'celesta-plugin-vrm', '--locked', '--profile', 'ci'],
    'nextest:celesta-codec-ffmpeg',
    { command: 'nextest', inspect: (tree) => {
      const reports = join(tree, 'target', 'nextest', 'ci');
      expect(existsSync(join(reports, 'junit.xml'))).toBe(false);
      for (const name of ['celesta-codec-ffmpeg', 'celesta-plugin-vrm']) {
        expect(readFileSync(join(reports, `junit-${name}.xml`), 'utf8')).toBe(name);
      }
    } },
  );
  expect(status, stderr).toBe(1);
  expect(calls.map((call) => [call.action, call.args.slice(0, 2)])).toEqual([
    ['nextest', ['run', '-p']],
    ['nextest', ['run', '-p']],
  ]);
  expect(calls.map((call) => call.args[2])).toEqual(['celesta-codec-ffmpeg', 'celesta-plugin-vrm']);
});

it('builds nextest prerequisites with Cargo\'s profile, not nextest\'s', () => {
  const { status, calls, stderr } = run(
    ['-p', 'celesta-codec-ffmpeg-x264', '--profile', 'ci', '--cargo-profile', 'release', '--locked'],
    '',
    { command: 'nextest' },
  );
  expect(status, stderr).toBe(0);
  for (const call of calls.filter((call) => call.action === 'build')) {
    expect(call.args).toEqual(['-p', call.name, '--profile', 'release', '--locked']);
  }
  expect(calls.at(-1)?.args).toEqual(['run', '-p', 'celesta-codec-ffmpeg-x264', '--profile', 'ci', '--cargo-profile', 'release', '--locked']);
});

it('drops a report an earlier run left when a package writes none', () => {
  const stale = (tree: string) => join(tree, 'target', 'nextest', 'ci', 'junit-celesta-plugin-vrm.xml');
  const { status } = run(['-p', 'celesta-plugin-vrm', '--profile', 'ci'], '', {
    command: 'nextest',
    noReport: 'celesta-plugin-vrm',
    setup: (tree) => {
      mkdirSync(join(tree, 'target', 'nextest', 'ci'), { recursive: true });
      writeFileSync(stale(tree), 'stale');
    },
    inspect: (tree) => expect(existsSync(stale(tree))).toBe(false),
  });
  expect(status).toBe(0);
});
