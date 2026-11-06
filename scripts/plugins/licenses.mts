// The license files a plugin's plugin.json lists, which the build script
// copies by name into the plugin directory's licenses/. The script knows
// nothing of what they say: each plugin lists the texts it ships with.
import { statSync } from 'node:fs';
import { basename, resolve } from 'node:path';

import type { PluginSource } from './manifest.mts';

/**
 * The license files of the plugin `plugin` in the folder `source` (its
 * plugin.json read from `sourcePath`, which errors name): each `licenses`
 * entry resolved against `source`, in order, named by its base name. Every
 * entry must be a file, and no two may share a name, since they are copied
 * into one folder; names that differ only in case are one name, as they are
 * on macOS's and Windows's file systems.
 */
export function licenseFiles(source: string, plugin: PluginSource, sourcePath: string): { from: string; name: string }[] {
  const names = new Set<string>();
  return (plugin.licenses ?? []).map((entry) => {
    const from = resolve(source, entry);
    if (!statSync(from, { throwIfNoEntry: false })?.isFile()) {
      throw new Error(`${sourcePath}: license file ${entry} is not a file`);
    }
    const name = basename(from);
    const key = name.toLowerCase();
    if (names.has(key)) {
      throw new Error(`${sourcePath}: two license files are named ${name}`);
    }
    names.add(key);
    return { from, name };
  });
}
