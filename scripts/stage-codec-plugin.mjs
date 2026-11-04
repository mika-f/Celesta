#!/usr/bin/env node
// Lays out the FFmpeg codec plugin as a plugin folder, the way packages ship
// it in their bundled plugin folder:
//
//   node scripts/stage-codec-plugin.mjs <celesta-codec-ffmpeg program> <folder>
//
// <folder> gets `celesta-plugin.json`, the program under `bin/`, and the GPL
// text and source offer under `licenses/`: an FFmpeg built with libx264 makes
// the program, and only it, GPL licensed. Native libraries the program loads
// are the packaging script's to place beside it.

import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const [program, folder] = process.argv.slice(2).map((path) => path && resolve(path));
if (!program || !folder) {
  console.error('usage: stage-codec-plugin.mjs <celesta-codec-ffmpeg program> <folder>');
  process.exit(2);
}
if (!existsSync(program)) {
  console.error(`no program at ${program}; build it with cargo build --release -p celesta-codec-ffmpeg`);
  process.exit(1);
}
if (existsSync(folder)) {
  console.error(`${folder} already exists`);
  process.exit(1);
}

// The plugin's version is Celesta's: the workspace's.
const cargo = readFileSync(join(root, 'Cargo.toml'), 'utf8');
const version = cargo.match(/\[workspace\.package\][^[]*?\nversion = "([^"]+)"/)?.[1];
if (!version) throw new Error('no workspace.package version in Cargo.toml');

// Rust's names for this platform, which manifests use.
const os = { darwin: 'macos', win32: 'windows', linux: 'linux' }[process.platform];
const arch = { arm64: 'aarch64', x64: 'x86_64' }[process.arch];
if (!os || !arch) throw new Error(`unsupported platform ${process.platform}-${process.arch}`);

mkdirSync(join(folder, 'bin'), { recursive: true });
mkdirSync(join(folder, 'licenses'));
const executable = `bin/${basename(program)}`;
copyFileSync(program, join(folder, executable));
chmodSync(join(folder, executable), 0o755);
for (const [source, name] of [
  ['LICENSE-GPL-2.0', 'LICENSE-GPL-2.0'],
  ['packaging/GPL-SOURCE-OFFER.md', 'GPL-SOURCE-OFFER.md'],
]) {
  copyFileSync(join(root, source), join(folder, 'licenses', name));
}
writeFileSync(
  join(folder, 'celesta-plugin.json'),
  `${JSON.stringify(
    {
      id: 'ffmpeg',
      version,
      protocol: 1,
      roles: ['codec'],
      executable: { [`${os}-${arch}`]: executable },
    },
    null,
    2,
  )}\n`,
);
console.log(`Codec plugin: ${folder}`);
