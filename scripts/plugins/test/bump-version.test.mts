import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
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
  'plugins/avfoundation/Cargo.toml':
    '[package]\nversion = "0.9.0"\n\n[dependencies]\nfoo = { path = "../../x", version = "=0.9.0" }\n',
  'plugins/avfoundation/plugin.json': '{\n  "id": "avfoundation",\n  "version": "0.9.0"\n}\n',
  'plugins/ffmpeg/Cargo.toml':
    '[package]\nversion = "0.9.0"\n\n[dependencies]\nfoo = { path = "../../x", version = "=0.9.0" }\n',
  'plugins/ffmpeg/plugin.json': '{\n  "id": "ffmpeg",\n  "version": "0.9.0"\n}\n',
  'plugins/mediafoundation/Cargo.toml':
    '[package]\nversion = "0.9.0"\n\n[dependencies]\nfoo = { path = "../../x", version = "=0.9.0" }\n',
  'plugins/mediafoundation/plugin.json': '{\n  "id": "mediafoundation",\n  "version": "0.9.0"\n}\n',
};

let root: string;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'bump-version-'));
  mkdirSync(join(root, 'scripts', 'plugins'), { recursive: true });
  for (const script of ['bump-version.mts', 'version.mts']) {
    copyFileSync(join(scripts, script), join(root, 'scripts', 'plugins', script));
  }
  writeFileSync(join(root, 'package.json'), '{ "type": "commonjs" }\n');
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
});

afterEach(() => {
  for (const path of Object.keys(files)) chmodSync(join(root, path), 0o644);
  rmSync(root, { recursive: true, force: true });
});

// `fileSizeLimit` runs the script under bash's `ulimit -f` (in KiB), past
// which a write fails with EFBIG once the file is open: Node ignores SIGXFSZ.
function bump(
  cargo: string,
  { CARGO = process.execPath, to = '0.9.1', fileSizeLimit }: { CARGO?: string; to?: string; fileSizeLimit?: number } = {},
) {
  writeFileSync(join(root, 'update'), cargo);
  const script = [process.execPath, join(root, 'scripts', 'plugins', 'bump-version.mts'), to];
  const [command, ...args] =
    fileSizeLimit === undefined ? script : ['bash', '-c', `ulimit -f ${fileSizeLimit} && exec "$@"`, 'bash', ...script];
  return spawnSync(command, args, { cwd: root, encoding: 'utf8', env: { ...process.env, CARGO } });
}

const read = (path: string) => readFileSync(join(root, path), 'utf8');
const expectUnchanged = () => {
  for (const [path, text] of Object.entries(files)) expect(read(path), path).toBe(text);
};

// A read-only file cannot be written, except by root.
const writesAnyFile = process.getuid?.() === 0;

describe('bump-version.mts', () => {
  it('bumps every file, then lets Cargo refresh the lockfiles', () => {
    const result = bump('');
    expect(result.status, result.stderr).toBe(0);
    expect(read('Cargo.toml')).toBe(files['Cargo.toml'].replaceAll('0.9.0', '0.9.1'));
    expect(read('plugins/avfoundation/Cargo.toml')).toBe(
      files['plugins/avfoundation/Cargo.toml'].replaceAll('0.9.0', '0.9.1'),
    );
    expect(read('plugins/avfoundation/plugin.json')).toBe(
      files['plugins/avfoundation/plugin.json'].replace('0.9.0', '0.9.1'),
    );
    expect(read('plugins/ffmpeg/Cargo.toml')).toBe(files['plugins/ffmpeg/Cargo.toml'].replaceAll('0.9.0', '0.9.1'));
    expect(read('plugins/ffmpeg/plugin.json')).toBe(files['plugins/ffmpeg/plugin.json'].replace('0.9.0', '0.9.1'));
    expect(read('plugins/mediafoundation/Cargo.toml')).toBe(
      files['plugins/mediafoundation/Cargo.toml'].replaceAll('0.9.0', '0.9.1'),
    );
    expect(read('plugins/mediafoundation/plugin.json')).toBe(
      files['plugins/mediafoundation/plugin.json'].replace('0.9.0', '0.9.1'),
    );
    expect(result.stdout).toContain('Bump packages/*/package.json and templates/package.json by hand.');
  });

  it('puts every file back when Cargo fails', () => {
    // The root's update rewrites its lockfile; the plugins' fails.
    const result = bump(
      "if (process.argv.includes('--manifest-path')) process.exit(1);\n" +
        "require('node:fs').writeFileSync('Cargo.lock', 'rewritten\\n');\n",
    );
    expect(result.status).toBe(1);
    expect(result.stderr).toContain(
      `${process.execPath} update --manifest-path plugins/Cargo.toml --workspace failed (exit code 1); the bump was undone, every file is as it was`,
    );
    expectUnchanged();
  });

  it('names why Cargo did not start, and puts every file back', () => {
    const missing = join(root, 'no-cargo');
    const result = bump('', { CARGO: missing });
    expect(result.status).toBe(1);
    expect(result.stderr).toMatch(new RegExp(`update --workspace failed: .*ENOENT.*; the bump was undone`));
    expectUnchanged();
  });

  it.skipIf(process.platform === 'win32')('names the signal Cargo died by', () => {
    const result = bump("process.kill(process.pid, 'SIGKILL');\n");
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('update --workspace failed (signal SIGKILL); the bump was undone');
    expectUnchanged();
  });

  // Only the script is sent SIGINT here, as by `kill -INT`; it passes it on
  // to Cargo, which stops, and puts the files back once Cargo has.
  it.skipIf(process.platform === 'win32')('passes a Ctrl-C on to Cargo and puts every file back once it stops', () => {
    const result = bump(
      "const fs = require('node:fs');\n" +
        "if (process.argv.includes('--manifest-path')) fs.writeFileSync('ran-plugins-update', '');\n" +
        'else {\n' +
        "  process.on('SIGINT', () => { fs.writeFileSync('cargo-got-sigint', ''); process.exit(130); });\n" +
        "  fs.writeFileSync('Cargo.lock', 'rewritten\\n');\n" +
        "  process.kill(process.ppid, 'SIGINT');\n" +
        '  setTimeout(() => {}, 3000);\n' +
        '}\n',
    );
    expect(result.status).toBe(130);
    expect(result.stderr).toContain('interrupted during');
    expect(result.stderr).toContain('the bump was undone, every file is as it was');
    expect(existsSync(join(root, 'cargo-got-sigint'))).toBe(true);
    expect(existsSync(join(root, 'ran-plugins-update'))).toBe(false);
    expectUnchanged();
  });

  // Cargo ignores SIGINT here and would rewrite its lockfile later; the
  // second Ctrl-C kills it, and the files are put back once it has exited.
  // Cargo shares the script's output, so the test would wait for a Cargo left
  // running.
  it.skipIf(process.platform === 'win32')('kills Cargo on a second Ctrl-C and puts every file back once it exits', () => {
    const result = bump(
      "const fs = require('node:fs');\n" +
        'const script = process.ppid;\n' +
        "process.on('SIGINT', () => {});\n" +
        "process.kill(script, 'SIGINT');\n" +
        "setTimeout(() => process.kill(script, 'SIGINT'), 300);\n" +
        "setTimeout(() => fs.writeFileSync('Cargo.lock', 'rewritten late\\n'), 1500);\n",
    );
    expect(result.status).toBe(130);
    expect(result.stderr).toContain('interrupted again during');
    expect(result.stderr).toContain('the bump was undone, every file is as it was');
    expectUnchanged();
  });

  it.skipIf(process.platform === 'win32')('counts a Cargo stopped by SIGINT alone as an interruption', () => {
    const result = bump("process.kill(process.pid, 'SIGINT');\n");
    expect(result.status).toBe(130);
    expect(result.stderr).toContain('interrupted during');
    expect(result.stderr).toContain('the bump was undone, every file is as it was');
    expectUnchanged();
  });

  it.skipIf(writesAnyFile)('names what was and was not written when a write fails, and undoes it', () => {
    chmodSync(join(root, 'plugins/ffmpeg/plugin.json'), 0o444);
    const result = bump("require('node:fs').writeFileSync('ran-update', '');\n");
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('could not write plugins/ffmpeg/plugin.json');
    expect(result.stderr).toContain(
      'written: Cargo.toml, plugins/avfoundation/Cargo.toml, plugins/avfoundation/plugin.json, plugins/ffmpeg/Cargo.toml; ' +
        'failed: plugins/ffmpeg/plugin.json; ' +
        'not written: plugins/mediafoundation/Cargo.toml, plugins/mediafoundation/plugin.json',
    );
    expect(result.stderr).toContain('the bump was undone, every file is as it was');
    expect(existsSync(join(root, 'ran-update'))).toBe(false);
    expectUnchanged();
  });

  // plugin.json is 1 KiB, the limit, and one byte longer once bumped to
  // 0.9.10, so its write fails after writing 1 KiB of it.
  it.skipIf(process.platform === 'win32')('puts back a file whose write failed after it was opened', () => {
    const head = '{\n  "id": "ffmpeg",\n  "version": "0.9.0",\n  "padding": "';
    const tail = '"\n}\n';
    const pluginJson = `${head}${'x'.repeat(1024 - head.length - tail.length)}${tail}`;
    writeFileSync(join(root, 'plugins/ffmpeg/plugin.json'), pluginJson);
    const result = bump("require('node:fs').writeFileSync('ran-update', '');\n", { to: '0.9.10', fileSizeLimit: 1 });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('could not write plugins/ffmpeg/plugin.json');
    expect(result.stderr).toContain('failed: plugins/ffmpeg/plugin.json');
    expect(result.stderr).toContain('the bump was undone, every file is as it was');
    expect(existsSync(join(root, 'ran-update'))).toBe(false);
    expect(read('plugins/ffmpeg/plugin.json')).toBe(pluginJson);
    expect(read('Cargo.toml')).toBe(files['Cargo.toml']);
    expect(read('plugins/ffmpeg/Cargo.toml')).toBe(files['plugins/ffmpeg/Cargo.toml']);
  });

  it.skipIf(writesAnyFile)('names what was and was not restored when putting a file back fails', () => {
    // Cargo fails after making plugin.json read-only.
    const result = bump("require('node:fs').chmodSync('plugins/ffmpeg/plugin.json', 0o444);\nprocess.exit(1);\n");
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('the bump was undone only in part');
    expect(result.stderr).toContain(
      'restored: Cargo.toml, plugins/avfoundation/Cargo.toml, plugins/avfoundation/plugin.json, ' +
        'plugins/ffmpeg/Cargo.toml, plugins/mediafoundation/Cargo.toml, plugins/mediafoundation/plugin.json, ' +
        'Cargo.lock, plugins/Cargo.lock',
    );
    expect(result.stderr).toMatch(/not restored: plugins\/ffmpeg\/plugin\.json \(.+\)/);
    expect(read('plugins/ffmpeg/plugin.json')).toBe(files['plugins/ffmpeg/plugin.json'].replace('0.9.0', '0.9.1'));
  });

  it("refuses a pin on Celesta's version the bump leaves, before writing anything", () => {
    // bumpCargoToml rewrites only the line with the path.
    writeFileSync(
      join(root, 'plugins/ffmpeg/Cargo.toml'),
      `${files['plugins/ffmpeg/Cargo.toml']}bar = { path = "../../y",\n  version = "=0.9.0" }\n`,
    );
    const before = read('plugins/ffmpeg/Cargo.toml');
    const result = bump("require('node:fs').writeFileSync('ran-update', '');\n");
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('plugins/ffmpeg/Cargo.toml: "=0.9.0" is left after the bump');
    expect(existsSync(join(root, 'ran-update'))).toBe(false);
    expect(read('plugins/ffmpeg/Cargo.toml')).toBe(before);
    expect(read('Cargo.toml')).toBe(files['Cargo.toml']);
  });
});
