// Content plugins' JavaScript: `@celesta-plugin/<id>` resolves to the
// `module` a plugin's celesta-plugin.json names, found as celesta-plugin-host
// finds plugins (crates/plugin-host): each path of CELESTA_PLUGIN_PATH is a
// plugin directory or a directory of them, and the first plugin with an id
// wins. See docs/handoff/2026-11-01-plugin-system.md.

import * as fs from 'node:fs';
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

function readManifest(directory: string): FoundPlugin | undefined {
  let manifest: Manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(path.join(directory, MANIFEST), 'utf8')) as Manifest;
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

/** The plugins under `CELESTA_PLUGIN_PATH`, first of each id. */
export function findPlugins(pluginPath = process.env[PLUGIN_PATH_ENV] ?? ''): FoundPlugin[] {
  const found: FoundPlugin[] = [];
  const add = (plugin: FoundPlugin | undefined) => {
    if (plugin && !found.some((other) => other.id === plugin.id)) found.push(plugin);
  };
  for (const root of pluginPath.split(path.delimiter).filter(Boolean)) {
    if (fs.existsSync(path.join(root, MANIFEST))) {
      add(readManifest(root));
      continue;
    }
    let children: string[];
    try {
      children = fs.readdirSync(root, { withFileTypes: true }).filter((entry) => entry.isDirectory()).map((entry) => entry.name);
    } catch {
      continue;
    }
    for (const child of children.sort()) add(readManifest(path.join(root, child)));
  }
  return found;
}

/**
 * The module file `@celesta-plugin/<id>` stands for, or an error message
 * saying why there is none.
 */
export function resolvePluginModule(specifier: string): { path: string } | { error: string } {
  const id = PLUGIN_SPECIFIER.exec(specifier)?.[1];
  if (!id) return { error: `${specifier} is not a plugin module` };
  const plugin = findPlugins().find((candidate) => candidate.id === id);
  if (!plugin) {
    return { error: `no plugin "${id}" is installed (looked in ${PLUGIN_PATH_ENV})` };
  }
  if (!plugin.module) return { error: `plugin "${id}" has no JavaScript module` };
  return { path: plugin.module };
}
