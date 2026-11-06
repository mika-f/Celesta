import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { githubOutputs } from '../inputs.mts';

// affected.mts in the real checkout. Only HEAD is compared, which a depth-1
// checkout (the root Test job's) has.
const script = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'affected.mts');
const root = resolve(dirname(script), '..', '..');

/** The environment without the step output file a CI job sets. */
function environment(extra: Record<string, string> = {}) {
  const env: NodeJS.ProcessEnv = { ...process.env, ...extra };
  if (!('GITHUB_OUTPUT' in extra)) delete env.GITHUB_OUTPUT;
  return env;
}

function affected(args: string[], env = environment()) {
  return spawnSync(process.execPath, [script, ...args], { cwd: root, encoding: 'utf8', env });
}

describe('affected.mts', () => {
  let folder: string;

  beforeEach(() => {
    folder = mkdtempSync(join(tmpdir(), 'celesta-affected-'));
  });

  afterEach(() => {
    rmSync(folder, { recursive: true, force: true });
  });

  it('affects no plugin when HEAD is its own base', () => {
    const result = affected(['--base', 'HEAD']);
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toBe('{"plugins":[]}\n');
  }, 60_000);

  it('appends the step outputs for no plugin to $GITHUB_OUTPUT, with \\n line ends', () => {
    const output = join(folder, 'github-output');
    writeFileSync(output, '');
    const result = affected(['--base', 'HEAD', '--github-output'], environment({ GITHUB_OUTPUT: output }));
    expect(result.status, result.stderr).toBe(0);
    const written = readFileSync(output, 'utf8');
    expect(written).toBe(githubOutputs([]));
    expect(written).toBe('any=false\nplugins=[]\ndirectories=\njs=\ncargo<<CARGO_WORKSPACES\nCARGO_WORKSPACES\n');
    expect(written).not.toContain('\r');
  }, 60_000);

  it('refuses --head without --base, with a usage error', () => {
    const result = affected(['--head', 'HEAD']);
    expect(result.status).toBe(2);
    expect(result.stderr).toContain('affected.mts: --head needs --base');
  });

  it('fails --github-output without GITHUB_OUTPUT', () => {
    const result = affected(['--base', 'HEAD', '--github-output']);
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('affected.mts: GITHUB_OUTPUT is not set');
  });
});
