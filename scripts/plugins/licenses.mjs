// The license files a plugin's plugin.json lists, which the build script
// copies by name into the plugin directory's licenses/. The script knows
// nothing of what they say: each plugin lists the texts it ships with.
import { statSync } from 'node:fs';
import { basename, resolve } from 'node:path';

/**
 * The license files of the plugin `plugin` in the folder `source` (its
 * plugin.json read from `sourcePath`, which errors name): each `licenses`
 * entry resolved against `source`, in order, named by its base name. Every
 * entry must be a file, and no two may share a name, since they are copied
 * into one folder.
 *
 * @param {string} source The plugin's folder.
 * @param {import('./manifest.mjs').PluginSource} plugin
 * @param {string} sourcePath
 * @returns {{ from: string, name: string }[]}
 */
export function licenseFiles(source, plugin, sourcePath) {
  /** @type {Set<string>} */
  const names = new Set();
  return (plugin.licenses ?? []).map((entry) => {
    const from = resolve(source, entry);
    if (!statSync(from, { throwIfNoEntry: false })?.isFile()) {
      throw new Error(`${sourcePath}: license file ${entry} is not a file`);
    }
    const name = basename(from);
    if (names.has(name)) {
      throw new Error(`${sourcePath}: two license files are named ${name}`);
    }
    names.add(name);
    return { from, name };
  });
}
