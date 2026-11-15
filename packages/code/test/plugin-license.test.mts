// The code plugin (plugins/code) bundles @twinkleplop/* into its module, so
// it ships their license as plugins/code/LICENSE-twinkleplop. Every
// @twinkleplop package the bundle can reach must have that same text, or the
// file must be updated (or split) with the dependency.

import assert from 'node:assert/strict';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const shipped = readFileSync(join(here, '../../../plugins/code/LICENSE-twinkleplop'), 'utf8');

/** The folder of package `name` as Node finds it from `from`: in the nearest node_modules above. */
function packageFolder(name: string, from: string): string {
  for (let directory = from; ; directory = dirname(directory)) {
    const candidate = join(directory, 'node_modules', name);
    if (existsSync(join(candidate, 'package.json'))) return realpathSync(candidate);
    if (dirname(directory) === directory) throw new Error(`${name} is not installed above ${from}`);
  }
}

/** Each @twinkleplop package reachable from the dependencies of the package in `folder`, by name, with its folder. */
function twinkleplopPackages(folder: string, found = new Map<string, string>()): Map<string, string> {
  const { dependencies = {} } = JSON.parse(readFileSync(join(folder, 'package.json'), 'utf8')) as {
    dependencies?: Record<string, string>;
  };
  for (const name of Object.keys(dependencies).filter((dependency) => dependency.startsWith('@twinkleplop/'))) {
    if (found.has(name)) continue;
    const own = packageFolder(name, folder);
    found.set(name, own);
    twinkleplopPackages(own, found);
  }
  return found;
}

test('every bundled @twinkleplop package has the license the plugin ships', () => {
  const packages = twinkleplopPackages(join(here, '..'));
  assert.ok(packages.size >= 5, `found ${[...packages.keys()].join(', ')}`);
  for (const [name, folder] of packages) {
    assert.equal(readFileSync(join(folder, 'LICENSE'), 'utf8'), shipped, `${name}'s LICENSE differs from plugins/code/LICENSE-twinkleplop`);
  }
});
