// Lists the first-party plugins (plugins/*/plugin.json) a change affects, for
// CI to test those alone. A plugin is affected when a changed path is one of
// its inputs (its folder, the local Cargo packages its lockfile reaches, the
// workspace packages its js/ package uses and the root paths turbo.json gives
// their tasks, its license files) or a path every plugin shares (inputs.mjs's
// SHARED_INPUTS). Without --base every plugin is affected, as on pushes to
// main. Anything the dependency graphs cannot place fails the run rather than
// shrinking the list.
//
//   node scripts/plugins/affected.mjs [--base <rev>] [--head <rev>] [--github-output]
//
// The changed paths are `git diff --name-only --no-renames <base> <head>`
// (<head> is HEAD by default; a moved file's old path counts too). It prints
// one JSON line, {"plugins": [...]}, each entry's `because` naming the first
// changed path that affects it. --github-output also appends the step
// outputs any, plugins, directories, js, and cargo to $GITHUB_OUTPUT.
// Locally, after committing:
//
//   node scripts/plugins/affected.mjs --base "$(git merge-base origin/main HEAD)"
import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { affectedPlugins, githubOutputs, repositoryPlugins } from './inputs.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');
const usage = 'usage: node scripts/plugins/affected.mjs [--base <rev>] [--head <rev>] [--github-output]';

/** @type {{ base?: string, head?: string, 'github-output'?: boolean }} */
let values;
try {
  ({ values } = parseArgs({
    strict: true,
    allowPositionals: false,
    options: {
      base: { type: 'string' },
      head: { type: 'string' },
      'github-output': { type: 'boolean', default: false },
    },
  }));
} catch (error) {
  console.error(`affected.mjs: ${error instanceof Error ? error.message : String(error)}`);
  console.error(usage);
  process.exit(2);
}
if (values.head !== undefined && values.base === undefined) {
  console.error('affected.mjs: --head needs --base');
  console.error(usage);
  process.exit(2);
}

/**
 * @param {string} message
 * @returns {never}
 */
function fail(message) {
  console.error(`affected.mjs: ${message}`);
  process.exit(1);
}

const output = process.env.GITHUB_OUTPUT;
if (values['github-output'] && !output) fail('GITHUB_OUTPUT is not set');

/** @type {string[] | null} */
let changed = null;
if (values.base !== undefined) {
  const args = ['-C', root, 'diff', '--name-only', '--no-renames', '-z', values.base, values.head ?? 'HEAD', '--'];
  const result = spawnSync('git', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (result.error) fail(`git diff failed: ${result.error.message}`);
  if (result.status !== 0) fail(`git diff failed: ${result.stderr.trim()}`);
  changed = result.stdout.split('\0').filter((path) => path !== '');
}

let affected;
try {
  affected = affectedPlugins(repositoryPlugins(root), changed);
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}

console.log(JSON.stringify({ plugins: affected }));
if (values['github-output'] && output) appendFileSync(output, githubOutputs(affected));
