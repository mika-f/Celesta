// Content plugins' JavaScript: `@celesta-plugin/<id>` resolves to the
// `module` a plugin's celesta-plugin.json names, found as celesta-plugin-host
// finds plugins (crates/plugin-host): each path of CELESTA_PLUGIN_PATH is a
// plugin directory or a directory of them, and the first plugin with an id
// wins. See docs/handoff/2026-11-01-plugin-system.md.

import { readFile, readdir, stat } from 'node:fs/promises';
import * as path from 'node:path';

export const PLUGIN_PATH_ENV = 'CELESTA_PLUGIN_PATH';
const MANIFEST = 'celesta-plugin.json';
export const PLUGIN_SPECIFIER = /^@celesta-plugin\/([^/]+)$/;

interface Manifest {
  id: string;
  module?: string;
  types?: string;
}

export interface FoundPlugin {
  id: string;
  directory: string;
  /** Absolute path of its JavaScript module, if it has one. */
  module?: string;
}

async function isFile(file: string): Promise<boolean> {
  try {
    return (await stat(file)).isFile();
  } catch {
    return false;
  }
}

/** The plugin in `directory`, or undefined when its manifest is missing or unreadable. */
async function readManifest(directory: string): Promise<FoundPlugin | undefined> {
  let manifest: Manifest;
  try {
    manifest = JSON.parse(await readFile(path.join(directory, MANIFEST), 'utf8')) as Manifest;
  } catch {
    return undefined;
  }
  if (typeof manifest.id !== 'string') return undefined;
  return {
    id: manifest.id,
    directory,
    ...(typeof manifest.module === 'string' ? { module: path.resolve(directory, manifest.module) } : {}),
  };
}

/** The plugins `root` holds: itself when it has a manifest, else its subdirectories', in name order. */
async function pluginsIn(root: string): Promise<(FoundPlugin | undefined)[]> {
  if (await isFile(path.join(root, MANIFEST))) return [await readManifest(root)];
  let children: string[];
  try {
    children = (await readdir(root, { withFileTypes: true }))
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
  } catch {
    return [];
  }
  return Promise.all(children.map((child) => readManifest(path.join(root, child))));
}

/** The plugins under `CELESTA_PLUGIN_PATH`, first of each id. */
export async function findPlugins(pluginPath = process.env[PLUGIN_PATH_ENV] ?? ''): Promise<FoundPlugin[]> {
  // Read concurrently, kept in path order so the first of an id still wins.
  const roots = await Promise.all(pluginPath.split(path.delimiter).filter(Boolean).map(pluginsIn));
  const found: FoundPlugin[] = [];
  for (const plugin of roots.flat()) {
    if (plugin && !found.some((other) => other.id === plugin.id)) found.push(plugin);
  }
  return found;
}

/**
 * The module file `@celesta-plugin/<id>` stands for among `plugins`, or an
 * error message saying why there is none.
 */
export function resolvePluginModule(specifier: string, plugins: readonly FoundPlugin[]): { path: string } | { error: string } {
  const id = PLUGIN_SPECIFIER.exec(specifier)?.[1];
  if (!id) return { error: `${specifier} is not a plugin module` };
  const plugin = plugins.find((candidate) => candidate.id === id);
  if (!plugin) {
    return { error: `no plugin "${id}" is installed (looked in ${PLUGIN_PATH_ENV})` };
  }
  if (!plugin.module) return { error: `plugin "${id}" has no JavaScript module` };
  return { path: plugin.module };
}
