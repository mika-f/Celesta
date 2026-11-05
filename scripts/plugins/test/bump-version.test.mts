import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

const scripts = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// A repository with what the bump reads and writes, and the script beside
// them. `CARGO` is Node, so `cargo update ...` runs the `update` file at the
// root: each test writes it to say what "Cargo" does.
const files: Record<string, string> = {
  'Cargo.toml': '[workspace.package]\nversion = "0.9.0"\n\n[workspace.dependencies]\nfoo = { path = "x", version = "=0.9.0" }\n',
  'Cargo.lock': 'root lock\n',
  'plugins/Cargo.lock': 'plugins lock\n',
  'plugins/ffmpeg/Cargo.toml':
    '[package]\nversion = "0.9.0"\n\n[dependencies]\nfoo = { path = "../../x", version = "=0.9.0" }\n',
  'plugins/ffmpeg/plugin.json': '{\n  "id": "ffmpeg",\n  "version": "0.9.0"\n}\n',
};

let root: string;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'bump-version-'));
  mkdirSync(join(root, 'scripts', 'plugins'), { recursive: true });
  for (const script of ['bump-version.mjs', 'version.mjs']) {
    copyFileSync(join(scripts, script), join(root, 'scripts', 'plugins', script));
  }
  writeFileSync(join(root, 'package.json'), '{ "type": "commonjs" }\n');
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

function bump(cargo: string) {
  writeFileSync(join(root, 'update'), cargo);
  return spawnSync(process.execPath, [join(root, 'scripts', 'plugins', 'bump-version.mjs'), '0.9.1'], {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, CARGO: process.execPath },
  });
}

const read = (path: string) => readFileSync(join(root, path), 'utf8');

describe('bump-version.mjs', () => {
  it('bumps every file, then lets Cargo refresh the lockfiles', () => {
    const result = bump('');
    expect(result.status, result.stderr).toBe(0);
    expect(read('Cargo.toml')).toBe(files['Cargo.toml'].replaceAll('0.9.0', '0.9.1'));
    expect(read('plugins/ffmpeg/Cargo.toml')).toBe(files['plugins/ffmpeg/Cargo.toml'].replaceAll('0.9.0', '0.9.1'));
    expect(read('plugins/ffmpeg/plugin.json')).toBe(files['plugins/ffmpeg/plugin.json'].replace('0.9.0', '0.9.1'));
  });

  it('puts every file back when Cargo fails', () => {
    // The root's update rewrites its lockfile; the plugins' fails.
    const result = bump(
      "if (process.argv.includes('--manifest-path')) process.exit(1);\n" +
        "require('node:fs').writeFileSync('Cargo.lock', 'rewritten\\n');\n",
    );
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('the bump was undone, every file is as it was');
    for (const [path, text] of Object.entries(files)) expect(read(path), path).toBe(text);
  });
});
