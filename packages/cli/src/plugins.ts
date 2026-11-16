// Content plugins' JavaScript: `@celesta-plugin/<id>` resolves to the
// `module` a plugin's celesta-plugin.json names, found as celesta-plugin-host
// finds plugins (crates/plugin-host): each absolute path of
// CELESTA_PLUGIN_PATH is a plugin directory or a directory of them, the
// first plugin with an id wins, and a manifest with a bad id or a module
// outside its directory is skipped. A plugin listed in
// CELESTA_WAITING_PLUGIN_PATH instead waits for the user's approval, and none
// of its code runs: when CELESTA_WAITING_PLUGIN_IMPORTS is `stand-in` (the
// app), its import resolves to a stand-in; otherwise (command-line tools and
// the MCP server) the import fails, saying how to allow it. See
// docs/handoff/2026-11-01-plugin-system.md,
// docs/handoff/2026-11-02-plugin-waiting-stand-in.md, and
// docs/handoff/2026-11-08-plugin-command-line-approval.md.

import { readFile, readdir, realpath, stat } from 'node:fs/promises';
import * as path from 'node:path';

export const PLUGIN_PATH_ENV = 'CELESTA_PLUGIN_PATH';
export const WAITING_PLUGIN_PATH_ENV = 'CELESTA_WAITING_PLUGIN_PATH';
export const WAITING_IMPORTS_ENV = 'CELESTA_WAITING_PLUGIN_IMPORTS';
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

/**
 * The file `file`, a path a manifest names, is under `directory`: split on
 * either separator, as `isContainedPath` reads it, and joined with this
 * platform's own, so that `js\\index.js` is `js/index.js` on macOS and Linux
 * (celesta-plugin-host's `manifest_path`).
 */
export function manifestPath(directory: string, file: string): string {
  return path.join(directory, ...file.split(/[/\\]/));
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
  const module = manifestPath(directory, manifest.module);
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

const PROJECT_CONFIG = 'celesta.config.json';
const PROJECT_LOCK = 'celesta.lock';

/**
 * What the project an entry belongs to says about its registry plugins,
 * read only to say what to do when an import finds no plugin: Celesta
 * itself (crates/plugin-host) decides which plugins run.
 */
export interface ProjectPlugins {
  /** The project folder. */
  root: string;
  /** The ids `plugins.dependencies` in its `celesta.config.json` declares. */
  declared: string[];
  /** The ids its `celesta.lock` names, `'none'` without one, `'unreadable'` when it is not a lock. */
  locked: string[] | 'none' | 'unreadable';
}

/** The keys of `value`'s object `key`, or none when it is not an object. */
function keysOf(value: unknown, key: string): string[] {
  const inner = typeof value === 'object' && value !== null ? (value as Record<string, unknown>)[key] : undefined;
  return typeof inner === 'object' && inner !== null && !Array.isArray(inner) ? Object.keys(inner) : [];
}

/**
 * The registry plugins of the project `directory` (an entry's folder)
 * belongs to: the nearest folder from `directory` up that holds
 * `celesta.config.json` (celesta-plugin-host's `project_root`). `undefined`
 * when no folder does, or when the file is not JSON (Celesta reports that).
 */
export async function readProjectPlugins(directory: string): Promise<ProjectPlugins | undefined> {
  let root = path.resolve(directory);
  while (!(await isFile(path.join(root, PROJECT_CONFIG)))) {
    const parent = path.dirname(root);
    if (parent === root) return undefined;
    root = parent;
  }
  let config: unknown;
  try {
    config = JSON.parse(await readFile(path.join(root, PROJECT_CONFIG), 'utf8'));
  } catch {
    return undefined;
  }
  const plugins = typeof config === 'object' && config !== null ? (config as Record<string, unknown>).plugins : undefined;
  const declared = keysOf(plugins, 'dependencies');
  let locked: ProjectPlugins['locked'];
  try {
    locked = keysOf(JSON.parse(await readFile(path.join(root, PROJECT_LOCK), 'utf8')), 'plugins');
  } catch (error) {
    locked = (error as NodeJS.ErrnoException).code === 'ENOENT' ? 'none' : 'unreadable';
  }
  return { root, declared, locked };
}

/**
 * Why plugin `id`, which `project` declares, is not installed, and what to
 * do; `undefined` when `project` does not declare it.
 */
function projectHint(id: string, project: ProjectPlugins): string | undefined {
  if (!project.declared.includes(id)) return undefined;
  const install = `run \`celesta plugin install\` in ${project.root}`;
  if (project.locked === 'none') {
    return `${PROJECT_CONFIG} declares it, but the project has no ${PROJECT_LOCK}; choose Update Plugins… in the This project section of the Celesta app's Plugins tab, or ${install}`;
  }
  if (project.locked === 'unreadable') {
    return `${PROJECT_CONFIG} declares it, but the project's ${PROJECT_LOCK} could not be read; restore it from version control`;
  }
  if (!project.locked.includes(id)) {
    return `${PROJECT_CONFIG} declares it, but ${PROJECT_LOCK} does not lock it yet; choose Update Plugins… in the This project section of the Celesta app's Plugins tab, or ${install}`;
  }
  return `${PROJECT_LOCK} names it, but no copy that can run is on this computer; in the Celesta app, trust the project and choose Download (Problems says why when a copy cannot run), or ${install}`;
}

/**
 * The module file `@celesta-plugin/<id>` stands for among `plugins`, or an
 * error message saying why there is none. With the importing entry's
 * `project`, a plugin it declares says what would install it.
 */
export function resolvePluginModule(specifier: string, plugins: Plugins, project?: ProjectPlugins): { path: string } | { error: string } {
  const id = PLUGIN_SPECIFIER.exec(specifier)?.[1];
  if (!id) return { error: `${specifier} is not a plugin module` };
  if (!isValidPluginId(id)) return { error: `${specifier} does not name a valid plugin id` };
  const plugin = plugins.found.find((candidate) => candidate.id === id);
  if (!plugin) {
    const skipped = plugins.skipped
      .filter((candidate) => candidate.id === id)
      .map((candidate) => `; ${candidate.directory} was skipped: ${candidate.reason}`)
      .join('');
    const hint = project && projectHint(id, project);
    if (hint) return { error: `no plugin "${id}" is installed: ${hint}${skipped}` };
    return { error: `no plugin "${id}" is installed (looked in ${PLUGIN_PATH_ENV})${skipped}` };
  }
  if (!plugin.module) return { error: `plugin "${id}" has no JavaScript module` };
  return { path: plugin.module };
}

/** A waiting plugin whose import a stand-in answers. */
export interface StandIn {
  id: string;
  /** Its module, read for the names it exports and never run. */
  module: string;
}

/**
 * What `@celesta-plugin/<id>` resolves to: the module of a plugin in
 * `plugins`; else, for a plugin in `waiting` (the user has not allowed it
 * yet) with a module, a stand-in when `standIns` (the app) or an error saying
 * it waits and how to allow it; else an error message saying why there is
 * neither (a waiting plugin without a module says so, as an allowed one does;
 * one the importing entry's `project` declares says what would install it).
 */
export function resolvePluginImport(
  specifier: string,
  plugins: Plugins,
  waiting: Plugins,
  standIns: boolean,
  project?: ProjectPlugins,
): { path: string } | { standIn: StandIn } | { error: string } {
  const resolved = resolvePluginModule(specifier, plugins, project);
  if (!('error' in resolved)) return resolved;
  const id = PLUGIN_SPECIFIER.exec(specifier)?.[1];
  // A plugin that may run but cannot be imported is its own error.
  if (!id || plugins.found.some((plugin) => plugin.id === id)) return resolved;
  const plugin = waiting.found.find((candidate) => candidate.id === id);
  if (!plugin) return resolved;
  // Allowing it would not make it importable, so that is the error to show.
  if (!plugin.module) return { error: `plugin "${id}" has no JavaScript module` };
  if (!standIns) {
    // The MCP server shows this too, so what only the exporter takes comes last.
    return {
      error:
        `plugin "${id}" waits for your approval in the Celesta app, and does not run until you allow it ` +
        `(${plugin.directory}): allow it in the app's Plugins tab, or name its folder in ${PLUGIN_PATH_ENV}; ` +
        `celesta-exporter also takes --allow-plugin ${id} and --plugin-dir`,
    };
  }
  return { standIn: { id, module: plugin.module } };
}

/**
 * The source of the module that stands in for waiting plugin `id`, which
 * exports `names`. A capitalized export (or a default one) is taken for a
 * component and draws nothing of its own, but keeps what it wraps; any other
 * throws, when called, that the plugin waits for approval.
 */
export function standInSource(id: string, names: string[]): string {
  const message = `plugin "${id}" waits for your approval in the Celesta app, and does not run until you allow it`;
  const lines = [
    `const waiting = () => { throw new Error(${JSON.stringify(message)}); };`,
    'const component = (props) => props?.children ?? null;',
  ];
  names.forEach((name, index) => {
    const binding = name === 'default' || /^[A-Z]/.test(name) ? 'component' : 'waiting';
    const exported = /^[A-Za-z_$][\w$]*$/.test(name) ? name : JSON.stringify(name);
    lines.push(`const standIn${index} = ${binding};`, `export { standIn${index} as ${exported} };`);
  });
  return `${lines.join('\n')}\n`;
}
