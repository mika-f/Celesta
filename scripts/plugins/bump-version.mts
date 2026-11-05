// Bumps Celesta's version: the root Cargo.toml's [workspace.package] version
// and its crates' pins (`path` plus `version = "=X.Y.Z"`), and for each plugin that follows
// Celesta's version (FOLLOWS_CELESTA in version.mts) its Cargo package's
// version, its pins, and its plugin.json's version. Every file is checked
// before any is written, so a file that gives another version than Celesta's,
// or a Cargo.toml that would keep a pin on it (one the line-by-line rewrite
// cannot reach), fails the run with nothing changed. Then both lockfiles are
// refreshed with `cargo update --workspace` (`$CARGO` when set). When a write
// fails, Cargo fails, or the run is interrupted (Ctrl-C), every file written
// is put back as it was, and the run says which files it could not restore.
//
//   node scripts/plugins/bump-version.mts <X.Y.Z>
//
// packages/*/package.json are not bumped; edit them by hand.
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { FOLLOWS_CELESTA, bumpCargoToml, bumpPluginJson, workspaceVersion } from './version.mts';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

function fail(message: string): never {
  console.error(`bump-version.mts: ${message}`);
  process.exit(1);
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Runs `command` at the root. It runs asynchronously, so a Ctrl-C reaches the
 * SIGINT listener while it runs.
 *
 * @returns Why it failed, as `failed: <error>` or `failed (<signal or exit
 *   code>)`, or undefined when it succeeded.
 */
function run(command: string, args: string[]): Promise<string | undefined> {
  console.log(`> ${command} ${args.join(' ')}`);
  return new Promise((resolve) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
    child.on('error', (error) => resolve(`failed: ${error.message}`));
    child.on('close', (code, signal) => {
      if (code === 0) resolve(undefined);
      else resolve(`failed (${signal !== null ? `signal ${signal}` : `exit code ${code}`})`);
    });
  });
}

const [to, ...rest] = process.argv.slice(2);
if (to === undefined || rest.length > 0) {
  console.error('usage: node scripts/plugins/bump-version.mts <X.Y.Z>');
  process.exit(2);
}
if (!/^\d+\.\d+\.\d+$/.test(to)) fail(`${to} is not a version X.Y.Z`);

/** The file at `path`, relative to the root. */
function read(path: string): string {
  return readFileSync(join(root, path), 'utf8');
}

/**
 * Each file to write, relative to the root, with its text bumped from `from`
 * to `to`.
 */
function bumped(from: string, to: string): [string, string][] {
  return [
    ['Cargo.toml', bumpedCargoToml('Cargo.toml', from, to)],
    ...FOLLOWS_CELESTA.flatMap((folder): [string, string][] => {
      const cargoToml = `${folder}/Cargo.toml`;
      const pluginJson = `${folder}/plugin.json`;
      return [
        [cargoToml, bumpedCargoToml(cargoToml, from, to)],
        [pluginJson, bumpPluginJson(read(pluginJson), from, to, pluginJson)],
      ];
    }),
  ];
}

/**
 * The Cargo.toml at `path` (relative to the root) bumped from `from` to `to`,
 * refused when a pin on `from` is left outside its comments: bumpCargoToml
 * rewrites a pin only on the line with its `path`, so one split over lines
 * would stay behind.
 */
function bumpedCargoToml(path: string, from: string, to: string): string {
  const text = bumpCargoToml(read(path), from, to, path);
  const left = text.split('\n').some((line) => !/^\s*#/.test(line) && line.includes(`"=${from}"`));
  if (left) {
    throw new Error(`${path}: "=${from}" is left after the bump; write each pin on one line with its path`);
  }
  return text;
}

function orFail<T>(step: () => T): T {
  try {
    return step();
  } catch (error) {
    return fail(messageOf(error));
  }
}

function list(paths: string[]): string {
  return paths.length === 0 ? 'none' : paths.join(', ');
}

const LOCKFILES = ['Cargo.lock', 'plugins/Cargo.lock'];

const from = orFail(() => workspaceVersion(read('Cargo.toml')));
if (to === from) fail(`Celesta's version is already ${from}`);
const changes = orFail(() => bumped(from, to));
// What the bump writes or Cargo rewrites, as it was, to put back when the bump stops.
const originals = new Map(
  orFail(() => [...changes.map(([path]) => path), ...LOCKFILES].map((path): [string, string] => [path, read(path)])),
);

/**
 * Puts each of `paths` back as it was, going on past one that cannot be
 * written, and fails with `reason` and what was restored.
 */
function undo(reason: string, paths: string[]): never {
  const restored: string[] = [];
  const notRestored: string[] = [];
  for (const path of paths) {
    try {
      writeFileSync(join(root, path), originals.get(path) as string);
      restored.push(path);
    } catch (error) {
      notRestored.push(`${path} (${messageOf(error)})`);
    }
  }
  if (notRestored.length === 0) fail(`${reason}; the bump was undone, every file is as it was`);
  return fail(
    `${reason}; the bump was undone only in part, put the rest back by hand (git checkout -- <file>). ` +
      `restored: ${list(restored)}; not restored: ${notRestored.join(', ')}`,
  );
}

// A Ctrl-C stops the bump between steps, not in the middle of a write or of
// the undo; Cargo gets it too.
let interrupted = false;
process.on('SIGINT', () => {
  interrupted = true;
});

const written: string[] = [];
for (const [path, text] of changes) {
  try {
    writeFileSync(join(root, path), text);
  } catch (error) {
    const notWritten = changes.map(([path]) => path).filter((path) => !written.includes(path));
    undo(
      `could not write ${path}: ${messageOf(error)}; written: ${list(written)}; not written: ${list(notWritten)}`,
      written,
    );
  }
  written.push(path);
}
const cargo = process.env.CARGO ?? 'cargo';
for (const args of [
  ['update', '--workspace'],
  ['update', '--manifest-path', 'plugins/Cargo.toml', '--workspace'],
]) {
  const failure = await run(cargo, args);
  const command = `${cargo} ${args.join(' ')}`;
  if (interrupted) undo(`interrupted during ${command}`, [...originals.keys()]);
  if (failure !== undefined) undo(`${command} ${failure}`, [...originals.keys()]);
}

console.log(`\nBumped Celesta from ${from} to ${to}:`);
for (const path of [...changes.map(([path]) => path), ...LOCKFILES]) console.log(`  ${path}`);
console.log('Bump packages/*/package.json by hand.');
