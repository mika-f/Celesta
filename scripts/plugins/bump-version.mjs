// Bumps Celesta's version: the root Cargo.toml's [workspace.package] version
// and its crates' pins (`path` plus `version = "=X.Y.Z"`), and for each plugin that follows
// Celesta's version (FOLLOWS_CELESTA in version.mjs) its Cargo package's
// version, its pins, and its plugin.json's version. Every file is checked
// before any is written, so a file that gives another version than Celesta's
// fails the run with nothing changed. Then both lockfiles are refreshed with
// `cargo update --workspace` (`$CARGO` when set); when that fails, every file
// is put back as it was.
//
//   node scripts/plugins/bump-version.mjs <X.Y.Z>
//
// packages/*/package.json are not bumped; edit them by hand.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { FOLLOWS_CELESTA, bumpCargoToml, bumpPluginJson, workspaceVersion } from './version.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/**
 * @param {string} message
 * @returns {never}
 */
function fail(message) {
  console.error(`bump-version.mjs: ${message}`);
  process.exit(1);
}

/**
 * @param {string} command
 * @param {string[]} args
 * @returns {boolean} Whether it succeeded.
 */
function run(command, args) {
  console.log(`> ${command} ${args.join(' ')}`);
  return spawnSync(command, args, { cwd: root, stdio: 'inherit' }).status === 0;
}

const [to, ...rest] = process.argv.slice(2);
if (to === undefined || rest.length > 0) {
  console.error('usage: node scripts/plugins/bump-version.mjs <X.Y.Z>');
  process.exit(2);
}
if (!/^\d+\.\d+\.\d+$/.test(to)) fail(`${to} is not a version X.Y.Z`);

/**
 * @param {string} path Relative to the root.
 * @returns {string}
 */
function read(path) {
  return readFileSync(join(root, path), 'utf8');
}

/**
 * Each file to write, relative to the root, with its text bumped from `from`
 * to `to`.
 *
 * @param {string} from
 * @param {string} to
 * @returns {[string, string][]}
 */
function bumped(from, to) {
  return [
    ['Cargo.toml', bumpCargoToml(read('Cargo.toml'), from, to, 'Cargo.toml')],
    ...FOLLOWS_CELESTA.flatMap((folder) => {
      const cargoToml = `${folder}/Cargo.toml`;
      const pluginJson = `${folder}/plugin.json`;
      return /** @type {[string, string][]} */ ([
        [cargoToml, bumpCargoToml(read(cargoToml), from, to, cargoToml)],
        [pluginJson, bumpPluginJson(read(pluginJson), from, to, pluginJson)],
      ]);
    }),
  ];
}

/**
 * @template T
 * @param {() => T} step
 * @returns {T}
 */
function orFail(step) {
  try {
    return step();
  } catch (error) {
    return fail(error instanceof Error ? error.message : String(error));
  }
}

const LOCKFILES = ['Cargo.lock', 'plugins/Cargo.lock'];

const from = orFail(() => workspaceVersion(read('Cargo.toml')));
if (to === from) fail(`Celesta's version is already ${from}`);
const changes = orFail(() => bumped(from, to));
// What the bump writes or Cargo rewrites, as it was, to put back when Cargo fails.
const originals = orFail(() => [...changes.map(([path]) => path), ...LOCKFILES].map((path) => [path, read(path)]));

for (const [path, text] of changes) writeFileSync(join(root, path), text);
const cargo = process.env.CARGO ?? 'cargo';
for (const args of [
  ['update', '--workspace'],
  ['update', '--manifest-path', 'plugins/Cargo.toml', '--workspace'],
]) {
  if (!run(cargo, args)) {
    for (const [path, text] of originals) writeFileSync(join(root, path), text);
    fail(`${cargo} ${args.join(' ')} failed; the bump was undone, every file is as it was`);
  }
}

console.log(`\nBumped Celesta from ${from} to ${to}:`);
for (const path of [...changes.map(([path]) => path), ...LOCKFILES]) console.log(`  ${path}`);
console.log('Bump packages/*/package.json by hand.');
