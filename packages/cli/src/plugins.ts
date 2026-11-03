// Content plugins' JavaScript: `@celesta-plugin/<id>` resolves to the
// `module` a plugin's celesta-plugin.json names, found as celesta-plugin-host
// finds plugins (crates/plugin-host): each absolute path of
// CELESTA_PLUGIN_PATH is a plugin directory or a directory of them, the
// first plugin with an id wins, and a manifest with a bad id or a module
// outside its directory is skipped. See
// docs/handoff/2026-11-01-plugin-system.md.

import { readFile, readdir, realpath, stat } from 'node:fs/promises';
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

/** A plugin directory whose manifest could not be used, and why. */
export interface SkippedPlugin {
  directory: string;
  /** Its id, when that was not the problem. */
  id?: string;
  reason: string;
}

export interface Plugins {
  found: FoundPlugin[];
  skipped: SkippedPlugin[];
}

/**
 * Whether `id` can name a plugin: lowercase ASCII letters, digits, `-`, `_`,
 * and `.`, starting with a letter or digit (celesta-plugin-host's
 * `is_valid_id`).
 */
export function isValidPluginId(id: string): boolean {
  return /^[a-z0-9][a-z0-9._-]*$/.test(id);
}

/**
 * Whether a path a manifest names stays inside the plugin directory on every
 * platform: relative, with no `..` part, drive, or `:` (celesta-plugin-host's
 * `is_contained_path`).
 */
export function isContainedPath(file: string): boolean {
  return file !== '' && !/^[/\\]/.test(file) && !/[:\0]/.test(file) && file.split(/[/\\]/).every((part) => part !== '..');
}

async function isFile(file: string): Promise<boolean> {
  try {
    return (await stat(file)).isFile();
  } catch {
    return false;
  }
}

/** Whether `file`, links followed, is inside `directory`; true when it does not exist yet. */
async function staysInside(directory: string, file: string): Promise<boolean> {
  let resolved: string;
  try {
    resolved = await realpath(file);
  } catch {
    return true;
  }
  const root = await realpath(directory);
  return resolved.startsWith(root.endsWith(path.sep) ? root : root + path.sep);
}

/** The plugin in `directory`, why it was skipped, or undefined when it has no readable manifest. */
async function readManifest(directory: string): Promise<FoundPlugin | SkippedPlugin | undefined> {
  let manifest: Manifest;
  try {
    manifest = JSON.parse(await readFile(path.join(directory, MANIFEST), 'utf8')) as Manifest;
  } catch {
    return undefined;
  }
  if (typeof manifest.id !== 'string') return undefined;
  const { id } = manifest;
  if (!isValidPluginId(id)) {
    return { directory, reason: `its id ${JSON.stringify(id)} is not a valid plugin id` };
  }
  if (typeof manifest.module !== 'string') return { id, directory };
  if (!isContainedPath(manifest.module)) {
    return { directory, id, reason: `${JSON.stringify(manifest.module)} is not a relative path inside the plugin directory` };
  }
  const module = path.resolve(directory, manifest.module);
  if (!(await staysInside(directory, module))) {
    return { directory, id, reason: `${JSON.stringify(manifest.module)} leads outside the plugin directory` };
  }
  return { id, directory, module };
}

/** The plugins `root` holds: itself when it has a manifest, else its subdirectories', in name order. */
async function pluginsIn(root: string): Promise<(FoundPlugin | SkippedPlugin | undefined)[]> {
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

/**
 * The plugins under `CELESTA_PLUGIN_PATH`, first of each id, and the ones
 * skipped. A relative entry is skipped too: its folder would depend on where
 * the runtime happens to be started.
 */
export async function findPlugins(pluginPath = process.env[PLUGIN_PATH_ENV] ?? ''): Promise<Plugins> {
  const entries = pluginPath.split(path.delimiter).filter(Boolean);
  const skipped: SkippedPlugin[] = entries
    .filter((entry) => !path.isAbsolute(entry))
    .map((entry) => ({ directory: entry, reason: 'it is not an absolute path' }));
  // Read concurrently, kept in path order so the first of an id still wins.
  const roots = await Promise.all(entries.filter((entry) => path.isAbsolute(entry)).map(pluginsIn));
  const found: FoundPlugin[] = [];
  for (const plugin of roots.flat()) {
    if (!plugin) continue;
    if ('reason' in plugin) skipped.push(plugin);
    else if (!found.some((other) => other.id === plugin.id)) found.push(plugin);
  }
  return { found, skipped };
}

/**
 * The module file `@celesta-plugin/<id>` stands for among `plugins`, or an
 * error message saying why there is none.
 */
export function resolvePluginModule(specifier: string, plugins: Plugins): { path: string } | { error: string } {
  const id = PLUGIN_SPECIFIER.exec(specifier)?.[1];
  if (!id) return { error: `${specifier} is not a plugin module` };
  if (!isValidPluginId(id)) return { error: `${specifier} does not name a valid plugin id` };
  const plugin = plugins.found.find((candidate) => candidate.id === id);
  if (!plugin) {
    const skipped = plugins.skipped
      .filter((candidate) => candidate.id === id)
      .map((candidate) => `; ${candidate.directory} was skipped: ${candidate.reason}`)
      .join('');
    return { error: `no plugin "${id}" is installed (looked in ${PLUGIN_PATH_ENV})${skipped}` };
  }
  if (!plugin.module) return { error: `plugin "${id}" has no JavaScript module` };
  return { path: plugin.module };
}
