import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

const script = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'build.mjs');

// A plugin with a process, a module and a license file, in a temporary
// folder. `CARGO` is Node, run in the plugin's folder, so `cargo metadata ...`
// and `cargo build ...` run the `metadata` and `build` files there: the first
// prints what Cargo would, the second writes the binary where Cargo would
// and leaves a mark that it ran. rustc is the real one, always given a
// --target, so the platform is the same on every host.
let tree: string;
let plugin: string;
let out: string;

const LINUX = 'x86_64-unknown-linux-gnu';
const MACOS = 'aarch64-apple-darwin';

function metadata(version: string) {
  writeFileSync(
    join(plugin, 'metadata'),
    `console.log(JSON.stringify(${JSON.stringify({
      workspace_root: tree,
      target_directory: join(tree, 'target'),
      packages: [{ name: 'demo-plugin', version, manifest_path: join(plugin, 'Cargo.toml'), dependencies: [] }],
    })}));\n`,
  );
}

function cargoBuild(body: string) {
  const mark = `require('node:fs').writeFileSync(${JSON.stringify(join(tree, 'ran-build'))}, '');\n`;
  writeFileSync(join(plugin, 'build'), `${mark}${body}`);
}

beforeEach(() => {
  tree = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-build-')));
  plugin = join(tree, 'demo');
  out = join(tree, 'dist', 'demo');
  mkdirSync(plugin);
  writeFileSync(join(tree, 'package.json'), '{ "type": "commonjs" }\n');
  writeFileSync(
    join(plugin, 'plugin.json'),
    JSON.stringify({
      id: 'demo',
      version: '0.1.0',
      cargoPackage: 'demo-plugin',
      module: 'js/index.ts',
      licenses: ['LICENSE-A'],
    }),
  );
  writeFileSync(join(plugin, 'Cargo.toml'), '[package]\nname = "demo-plugin"\nversion = "0.1.0"\n');
  writeFileSync(join(plugin, 'LICENSE-A'), 'license A\n');
  metadata('0.1.0');
  cargoBuild(
    "const { mkdirSync, writeFileSync } = require('node:fs');\n" +
      "const { join } = require('node:path');\n" +
      'const at = (flag) => process.argv[process.argv.indexOf(flag) + 1];\n' +
      `const folder = join(${JSON.stringify(join(tree, 'target'))}, at('--target'), 'release');\n` +
      'mkdirSync(folder, { recursive: true });\n' +
      "writeFileSync(join(folder, at('--bin')), at('--target'));\n",
  );
  // --skip-js keeps the module an earlier run built.
  mkdirSync(join(out, 'js'), { recursive: true });
  writeFileSync(join(out, 'js', 'index.js'), 'export {};\n');
  writeFileSync(join(out, 'js', 'index.d.ts'), 'export {};\n');
});

afterEach(() => {
  rmSync(tree, { recursive: true, force: true });
});

function build(target: string) {
  return spawnSync(process.execPath, [script, 'demo', '--out', 'dist', '--skip-js', '--target', target], {
    cwd: tree,
    encoding: 'utf8',
    env: { ...process.env, CARGO: process.execPath },
  });
}

const manifest = () => JSON.parse(readFileSync(join(out, 'celesta-plugin.json'), 'utf8'));

describe('build.mjs', () => {
  it('writes the manifest, the executable and the license files', () => {
    const result = build(LINUX);
    expect(result.status, result.stderr).toBe(0);
    const expected = {
      id: 'demo',
      version: '0.1.0',
      protocol: 1,
      executable: { 'linux-x86_64': 'bin/linux-x86_64/demo-plugin' },
      module: 'js/index.js',
      types: 'js/index.d.ts',
    };
    expect(readFileSync(join(out, 'celesta-plugin.json'), 'utf8')).toBe(`${JSON.stringify(expected, null, 2)}\n`);
    expect(readFileSync(join(out, 'bin', 'linux-x86_64', 'demo-plugin'), 'utf8')).toBe(LINUX);
    expect(readdirSync(join(out, 'licenses'))).toEqual(['LICENSE-A']);
  });

  it("keeps the first platform's executable on a run for another target", () => {
    expect(build(LINUX).status).toBe(0);
    const result = build(MACOS);
    expect(result.status, result.stderr).toBe(0);
    expect(manifest().executable).toEqual({
      'linux-x86_64': 'bin/linux-x86_64/demo-plugin',
      'macos-aarch64': 'bin/macos-aarch64/demo-plugin',
    });
  });

  it('drops an earlier executable whose file is gone', () => {
    mkdirSync(join(out, 'bin', 'macos-aarch64'), { recursive: true });
    writeFileSync(join(out, 'bin', 'macos-aarch64', 'demo-plugin'), '');
    writeFileSync(
      join(out, 'celesta-plugin.json'),
      JSON.stringify({
        executable: {
          'macos-aarch64': 'bin/macos-aarch64/demo-plugin',
          'windows-x86_64': 'bin/windows-x86_64/demo-plugin.exe',
        },
      }),
    );
    const result = build(LINUX);
    expect(result.status, result.stderr).toBe(0);
    expect(manifest().executable).toEqual({
      'macos-aarch64': 'bin/macos-aarch64/demo-plugin',
      'linux-x86_64': 'bin/linux-x86_64/demo-plugin',
    });
  });

  it('replaces licenses/ with the files plugin.json lists', () => {
    mkdirSync(join(out, 'licenses'), { recursive: true });
    writeFileSync(join(out, 'licenses', 'STALE'), '');
    expect(build(LINUX).status).toBe(0);
    expect(readdirSync(join(out, 'licenses'))).toEqual(['LICENSE-A']);
  });

  it("refuses a version that differs from the Cargo package's before Cargo builds", () => {
    metadata('0.2.0');
    const result = build(LINUX);
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("version 0.1.0 differs from the Cargo package demo-plugin's 0.2.0");
    expect(existsSync(join(tree, 'ran-build'))).toBe(false);
  });

  it.each(['index.js', 'index.d.ts'])(
    'with --skip-js, refuses a module whose %s was not built, before Cargo builds',
    (file) => {
      rmSync(join(out, 'js', file));
      const result = build(LINUX);
      expect(result.status).toBe(1);
      expect(result.stderr).toContain(`${join(out, 'js', file)} is missing`);
      expect(existsSync(join(tree, 'ran-build'))).toBe(false);
    },
  );

  it('names the plugin.json it cannot parse', () => {
    writeFileSync(join(plugin, 'plugin.json'), '{ "id": ');
    const result = build(LINUX);
    expect(result.status).toBe(1);
    expect(result.stderr).toContain(join(plugin, 'plugin.json'));
  });

  it('names the exit code Cargo failed with', () => {
    cargoBuild('process.exit(3);\n');
    const result = build(LINUX);
    expect(result.status).toBe(1);
    expect(result.stderr).toMatch(/build\.mjs: .* build --release .* failed \(exit code 3\)/);
  });

  it.skipIf(process.platform === 'win32')('names the signal Cargo died by', () => {
    cargoBuild("process.kill(process.pid, 'SIGKILL');\n");
    const result = build(LINUX);
    expect(result.status).toBe(1);
    expect(result.stderr).toMatch(/build\.mjs: .* build --release .* failed \(signal SIGKILL\)/);
  });
});
