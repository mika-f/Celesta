// Bumps Celesta's version: the root Cargo.toml's [workspace.package] version
// and its crates' pins (`path` plus `version = "=X.Y.Z"`), and for each plugin that follows
// Celesta's version (FOLLOWS_CELESTA in version.mts) its Cargo package's
// version, its pins, and its plugin.json's version. Every file is checked
// before any is written, so a file that gives another version than Celesta's,
// or a Cargo.toml that would keep a pin on it (one the line-by-line rewrite
// cannot reach), fails the run with nothing changed. Then both lockfiles are
// refreshed with `cargo update --workspace` (`$CARGO` when set). When a write
// fails (a file it fails in the middle of included), Cargo fails, or the run
// is interrupted, every file written is put back as it was, and the run says
// which files it could not restore. A Ctrl-C is passed on to Cargo and the
// files are put back once Cargo stops; a second one kills Cargo, and the files
// are put back once it has exited, so it cannot write a lockfile after them.
// On Windows, where a process cannot be sent Ctrl-C, Cargo is stopped outright
// the first time. A Cargo stopped by SIGINT sent to it alone counts as an
// interruption too. An interrupted run exits with 130.
//
//   node scripts/plugins/bump-version.mts <X.Y.Z>
//
// packages/*/package.json and templates/package.json are not bumped; edit
// them by hand.
import { spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { FOLLOWS_CELESTA, bumpCargoToml, bumpPluginJson, workspaceVersion } from './version.mts';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** The exit code of a run stopped by SIGINT, as a shell gives it. */
const INTERRUPTED = 130;

function fail(message: string, code = 1): never {
  console.error(`bump-version.mts: ${message}`);
  process.exit(code);
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The command running now, for the SIGINT listener to pass a Ctrl-C on to. */
let running: { child: ChildProcess; command: string } | undefined;

/**
 * Runs `command` at the root. It runs asynchronously, so a Ctrl-C reaches the
 * SIGINT listener while it runs.
 *
 * @returns `interrupted` when SIGINT stopped it (a Ctrl-C sent to it alone
 *   included), why it failed, as `failed: <error>` or `failed (<signal or
 *   exit code>)`, or undefined when it succeeded.
 */
function run(command: string, args: string[]): Promise<'interrupted' | string | undefined> {
  const line = `${command} ${args.join(' ')}`;
  console.log(`> ${line}`);
  return new Promise((resolve) => {
    const child = spawn(command, args, { cwd: root, stdio: 'inherit' });
    running = { child, command: line };
    child.on('error', (error) => {
      running = undefined;
      resolve(`failed: ${error.message}`);
    });
    child.on('close', (code, signal) => {
      running = undefined;
      if (code === 0) resolve(undefined);
      else if (signal === 'SIGINT') resolve('interrupted');
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

/** Whether the file at `path` (relative to the root) reads as `text`. */
function holds(path: string, text: string): boolean {
  try {
    return readFileSync(join(root, path), 'utf8') === text;
  } catch {
    return false;
  }
}

/**
 * Puts each of `paths` back as it was, going on past one that cannot be
 * written, and fails with `reason`, what was restored, and `code`. A file
 * still as it was is left alone, so one the bump could not open is restored
 * even when it cannot be written.
 */
function undo(reason: string, paths: string[], code = 1): never {
  const restored: string[] = [];
  const notRestored: string[] = [];
  for (const path of paths) {
    const original = originals.get(path) as string;
    try {
      if (!holds(path, original)) writeFileSync(join(root, path), original);
      restored.push(path);
    } catch (error) {
      notRestored.push(`${path} (${messageOf(error)})`);
    }
  }
  if (notRestored.length === 0) fail(`${reason}; the bump was undone, every file is as it was`, code);
  return fail(
    `${reason}; the bump was undone only in part, put the rest back by hand (git checkout -- <file>). ` +
      `restored: ${list(restored)}; not restored: ${notRestored.join(', ')}`,
    code,
  );
}

// The listener runs only while Cargo does: the writes and the undo are
// synchronous, so a Ctrl-C never stops one in the middle. The first is passed
// on to Cargo (a terminal's reaches it too), and the bump is undone once
// Cargo stops; a second kills Cargo, and the bump is undone once Cargo has
// exited too. `kill` only sends the signal (and on Windows the process ends
// some time after), so undoing before `run` resolves on Cargo's `close` could
// let a lockfile write already under way land after the undo.
let interrupted = false;
let interruptedAgain = false;
process.on('SIGINT', () => {
  if (interrupted) {
    interruptedAgain = true;
    running?.child.kill('SIGKILL');
    return;
  }
  interrupted = true;
  running?.child.kill('SIGINT');
});

const written: string[] = [];
for (const [path, text] of changes) {
  try {
    writeFileSync(join(root, path), text);
  } catch (error) {
    const notWritten = changes.map(([other]) => other).filter((other) => other !== path && !written.includes(other));
    // A write that fails once the file is open leaves it written in part.
    undo(
      `could not write ${path}: ${messageOf(error)}; written: ${list(written)}; failed: ${path}; ` +
        `not written: ${list(notWritten)}`,
      [...written, path],
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
  if (interrupted || failure === 'interrupted') {
    undo(`interrupted${interruptedAgain ? ' again' : ''} during ${command}`, [...originals.keys()], INTERRUPTED);
  }
  if (failure !== undefined) undo(`${command} ${failure}`, [...originals.keys()]);
}

console.log(`\nBumped Celesta from ${from} to ${to}:`);
for (const path of [...changes.map(([path]) => path), ...LOCKFILES]) console.log(`  ${path}`);
console.log('Bump packages/*/package.json and templates/package.json by hand.');
