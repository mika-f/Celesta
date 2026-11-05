// What each first-party plugin (plugins/*/plugin.json) is built and tested
// from, and which plugins a change affects. A plugin's inputs come from the
// dependency graphs, never from a hand-kept list: its folder, the local Cargo
// packages its lockfile reaches from its Cargo package, the workspace packages
// pnpm lists for its js/ package, and the license files its plugin.json lists.
// Anything the graphs cannot place is an error, so CI fails rather than
// skipping a plugin.
import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, realpathSync } from 'node:fs';
import { isAbsolute, join, relative, sep } from 'node:path';

import { cargoMetadata } from './cargo.mjs';
import { licenseFiles } from './licenses.mjs';
import { localPackages, parseCargoLock } from './lockfile.mjs';

/**
 * Paths every plugin is tested from: the plugin job's workflow and setup, the
 * scripts it runs, and the workspaces' and toolchains' configuration.
 *
 * @type {string[]}
 */
export const SHARED_INPUTS = [
  '.github/workflows/ci.yml',
  '.github/workflows/actions.lock',
  '.github/actions/setup-ffmpeg',
  'scripts/build-ffmpeg-linux.sh',
  'scripts/plugins',
  'scripts/report-skipped-tests.mjs',
  '.cargo/config.toml',
  'Cargo.toml',
  'Cargo.lock',
  'plugins/Cargo.toml',
  'plugins/Cargo.lock',
  'package.json',
  'pnpm-lock.yaml',
  'pnpm-workspace.yaml',
  'turbo.json',
  'mise.toml',
  'mise.lock',
];

/**
 * A plugin and what it is built and tested from.
 *
 * @typedef {object} PluginInputs
 * @property {string} id
 * @property {string} directory The plugin's folder (`plugins/vrm`).
 * @property {string | null} workspace The folder of its Cargo workspace's root, or `null` without `cargoPackage`.
 * @property {string | null} cargoPackage
 * @property {string | null} jsPackage The name in its js/package.json.
 * @property {string[]} inputs Sorted, without duplicates; a folder covers every path under it.
 */

/**
 * An affected plugin, and the first changed path that affects it.
 *
 * @typedef {Omit<PluginInputs, 'inputs'> & { because: string }} Affected
 */

/**
 * `path` relative to `root`, with `/` separators, as `git diff` names paths.
 *
 * @param {string} root
 * @param {string} path
 * @returns {string}
 */
export function repositoryPath(root, path) {
  const from = relative(root, path);
  if (isAbsolute(from) || from === '..' || from.startsWith(`..${sep}`)) {
    throw new Error(`${path} is outside the repository ${root}`);
  }
  return from.split(sep).join('/');
}

/**
 * Whether the input `input` (a file or a folder) covers the path `path`.
 *
 * @param {string} input
 * @param {string} path
 * @returns {boolean}
 */
export function covers(input, path) {
  return path === input || path.startsWith(`${input}/`);
}

/**
 * Every plugins/<folder>/plugin.json under `root`, sorted by `id`, with its
 * inputs. `cargo metadata` runs once per Cargo workspace.
 *
 * @param {string} root The repository's folder.
 * @returns {PluginInputs[]}
 */
export function repositoryPlugins(root) {
  // Cargo and pnpm give real paths.
  const real = realpathSync(root);
  const workspaces = cargoWorkspaces(real);
  const pluginsFolder = join(real, 'plugins');
  return readdirSync(pluginsFolder)
    .map((name) => join(pluginsFolder, name))
    .filter((source) => existsSync(join(source, 'plugin.json')))
    .map((source) => pluginInputs(real, source, workspaces))
    .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/**
 * @param {string} root The repository's real folder.
 * @param {string} source The plugin's folder.
 * @param {ReturnType<typeof cargoWorkspaces>} workspaces
 * @returns {PluginInputs}
 */
function pluginInputs(root, source, workspaces) {
  const sourcePath = join(source, 'plugin.json');
  const sourceName = repositoryPath(root, sourcePath);
  /** @type {import('./manifest.mjs').PluginSource} */
  const plugin = JSON.parse(readFileSync(sourcePath, 'utf8'));
  const directory = repositoryPath(root, source);
  const packageJson = join(source, 'js', 'package.json');
  const hasJs = existsSync(packageJson);
  if (plugin.module !== undefined && !hasJs) {
    throw new Error(`${sourceName}: its module needs a js/package.json that declares what the module uses`);
  }

  const inputs = [directory];
  let workspace = null;
  if (plugin.cargoPackage !== undefined) {
    const { metadata, local } = workspaces.closure(source, plugin.cargoPackage);
    workspace = repositoryPath(root, metadata.workspaceRoot);
    inputs.push(...local);
  }
  let jsPackage = null;
  if (hasJs) {
    /** @type {{ name: string }} */
    const { name } = JSON.parse(readFileSync(packageJson, 'utf8'));
    jsPackage = name;
    inputs.push(...workspacePackages(root, name, repositoryPath(root, packageJson)));
  }
  for (const { from } of licenseFiles(source, plugin, sourceName)) inputs.push(repositoryPath(root, from));

  return {
    id: plugin.id,
    directory,
    workspace,
    cargoPackage: plugin.cargoPackage ?? null,
    jsPackage,
    inputs: [...new Set(inputs)].sort(),
  };
}

/**
 * The Cargo workspaces the plugins are in, each read once with
 * `cargo metadata`, and the root's, whose members are most local packages.
 *
 * @param {string} root The repository's real folder.
 */
function cargoWorkspaces(root) {
  /** @type {Map<string, import('./cargo.mjs').CargoMetadata>} */
  const byRoot = new Map();
  /** @type {import('./cargo.mjs').CargoMetadata | null | undefined} */
  let rootWorkspace;

  /** @param {string} folder @returns {import('./cargo.mjs').CargoMetadata} */
  const read = (folder) => {
    const metadata = cargoMetadata(join(folder, 'Cargo.toml'), { cwd: folder });
    const known = byRoot.get(metadata.workspaceRoot);
    if (known) return known;
    byRoot.set(metadata.workspaceRoot, metadata);
    return metadata;
  };
  const rootMetadata = () => {
    if (rootWorkspace === undefined) rootWorkspace = existsSync(join(root, 'Cargo.toml')) ? read(root) : null;
    return rootWorkspace;
  };

  /**
   * The workspace whose member `cargoPackage` is in `source`, and the
   * folders of the local packages its lockfile reaches from it.
   *
   * @param {string} source
   * @param {string} cargoPackage
   */
  const closure = (source, cargoPackage) => {
    const directory = repositoryPath(root, source);
    /** @param {import('./cargo.mjs').CargoMetadata} metadata */
    const member = ({ versions, directories }) => {
      const folder = directories.get(cargoPackage);
      return versions.has(cargoPackage) && folder !== undefined && repositoryPath(root, folder) === directory;
    };
    rootMetadata();
    const metadata = [...byRoot.values()].find(member) ?? read(source);
    if (!member(metadata)) throw new Error(`${directory}/Cargo.toml is in no workspace with the package ${cargoPackage}`);
    const lockfile = join(metadata.workspaceRoot, 'Cargo.lock');
    const lockName = repositoryPath(root, lockfile);
    const names = localPackages(parseCargoLock(readFileSync(lockfile, 'utf8'), lockName), cargoPackage, lockName);
    // The plugin's own workspace first, then the root's, then the others'.
    const lookups = [metadata, rootMetadata(), ...byRoot.values()].filter((known) => known !== null);
    const local = names.map((name) => {
      const folder = lookups.map(({ directories }) => directories.get(name)).find((found) => found !== undefined);
      if (folder === undefined) throw new Error(`${lockName}: no folder is known for the local package ${name}`);
      return repositoryPath(root, folder);
    });
    return { metadata, local };
  };

  return { closure };
}

/**
 * The folders of the workspace package `name` and of the workspace packages
 * it depends on, devDependencies included, as pnpm lists them (which works
 * before `pnpm install`).
 *
 * @param {string} root The repository's real folder.
 * @param {string} name
 * @param {string} packageJson The package's package.json, for error messages.
 * @returns {string[]}
 */
function workspacePackages(root, name, packageJson) {
  const args = ['--filter', `${name}...`, 'ls', '--depth', '-1', '--json'];
  // pnpm is a .cmd shim on Windows, which only a shell runs.
  const windows = process.platform === 'win32';
  const result = windows
    ? spawnSync(`pnpm ${args.map((arg) => `"${arg}"`).join(' ')}`, { cwd: root, encoding: 'utf8', shell: true })
    : spawnSync('pnpm', args, { cwd: root, encoding: 'utf8' });
  if (result.error) throw new Error(`pnpm ls failed: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`pnpm ls failed for ${name}: ${result.stderr.trim()}`);
  /** @type {{ name?: string, path?: string }[]} */
  const listed = JSON.parse(result.stdout);
  if (!Array.isArray(listed) || !listed.some((entry) => entry.name === name)) {
    throw new Error(`${packageJson}: pnpm lists no workspace package ${name}`);
  }
  return listed.map(({ path }) => {
    if (typeof path !== 'string') throw new Error(`pnpm ls gave a package without a path for ${name}`);
    return repositoryPath(root, path);
  });
}

/**
 * The plugins `changed` affects. `null` (no base revision to compare with)
 * affects every plugin. Each plugin's `because` is the first changed path
 * that a shared input or one of its own inputs covers.
 *
 * @param {PluginInputs[]} plugins
 * @param {string[] | null} changed Repository-relative paths, in `git diff` order.
 * @param {string[]} [shared]
 * @returns {Affected[]}
 */
export function affectedPlugins(plugins, changed, shared = SHARED_INPUTS) {
  /** @type {Affected[]} */
  const affected = [];
  for (const { inputs, id, directory, workspace, cargoPackage, jsPackage } of plugins) {
    const because =
      changed === null
        ? 'no base revision'
        : changed.find((path) => [...shared, ...inputs].some((input) => covers(input, path)));
    if (because !== undefined) affected.push({ id, directory, workspace, cargoPackage, jsPackage, because });
  }
  return affected.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/**
 * The plugin job's step outputs, in `$GITHUB_OUTPUT`'s format: `any`,
 * `plugins` (the entries as JSON), `directories`, `js` (pnpm filters), and
 * `cargo` (one `<workspace> -p <package>...` line per Cargo workspace).
 *
 * @param {Affected[]} affected
 * @returns {string}
 */
export function githubOutputs(affected) {
  const byId = [...affected].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  const directories = byId.map(({ directory }) => directory).sort();
  const js = byId.flatMap(({ jsPackage }) => (jsPackage === null ? [] : [`--filter ${jsPackage}`]));
  /** @type {Map<string, string[]>} */
  const workspaces = new Map();
  for (const { workspace, cargoPackage } of byId) {
    if (workspace === null || cargoPackage === null) continue;
    workspaces.set(workspace, [...(workspaces.get(workspace) ?? []), cargoPackage]);
  }
  const cargo = [...workspaces.keys()]
    .sort()
    .map((workspace) => [workspace, ...(workspaces.get(workspace) ?? []).sort().map((name) => `-p ${name}`)].join(' '));
  return [
    `any=${affected.length > 0}`,
    `plugins=${JSON.stringify(byId)}`,
    `directories=${directories.join(' ')}`,
    `js=${js.join(' ')}`,
    'cargo<<CARGO_WORKSPACES',
    ...cargo,
    'CARGO_WORKSPACES',
    '',
  ].join('\n');
}
