// What each first-party plugin (plugins/*/plugin.json) is built and tested
// from, and which plugins a change affects. A plugin's inputs come from the
// dependency graphs, never from a hand-kept list: its folder, the local Cargo
// packages its lockfile reaches from its Cargo package, the workspace packages
// pnpm lists for its js/ package with the repository paths turbo.json gives
// their tasks, and the license files its plugin.json lists. Anything the
// graphs cannot place is an error, so CI fails rather than skipping a plugin.
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
 * The folders under plugins/ that hold no plugin: the built plugin
 * directories and the plugins workspace's build output.
 */
const NOT_PLUGINS = new Set(['dist', 'target']);

/**
 * Every plugins/<folder>/plugin.json under `root`, sorted by `id`, with its
 * inputs. Every folder under plugins/ but dist and target must have one, so
 * a plugin whose plugin.json is missing fails rather than goes untested.
 * `cargo metadata` runs once per Cargo workspace.
 *
 * @param {string} root The repository's folder.
 * @returns {PluginInputs[]}
 */
export function repositoryPlugins(root) {
  // Cargo and pnpm give real paths.
  const real = realpathSync(root);
  const workspaces = cargoWorkspaces(real);
  /** @type {TurboConfig | null | undefined} */
  let turbo;
  const turboConfig = () => {
    if (turbo === undefined) turbo = readTurboConfig(real);
    return turbo;
  };
  const pluginsFolder = join(real, 'plugins');
  return readdirSync(pluginsFolder, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && !NOT_PLUGINS.has(entry.name))
    .map(({ name }) => {
      const source = join(pluginsFolder, name);
      if (!existsSync(join(source, 'plugin.json'))) {
        throw new Error(
          `${repositoryPath(real, source)}: no plugin.json; every folder under plugins/ but dist and target is a plugin`,
        );
      }
      return pluginInputs(real, source, workspaces, turboConfig);
    })
    .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/**
 * A JSON file's value; a parse error names the file.
 *
 * @param {string} path
 * @param {string} name The file's repository path, for error messages.
 * @returns {any}
 */
function readJson(path, name) {
  const text = readFileSync(path, 'utf8');
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new Error(`${name}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

/**
 * @param {string} root The repository's real folder.
 * @param {string} source The plugin's folder.
 * @param {ReturnType<typeof cargoWorkspaces>} workspaces
 * @param {() => TurboConfig | null} turboConfig The root turbo.json, read once.
 * @returns {PluginInputs}
 */
function pluginInputs(root, source, workspaces, turboConfig) {
  const sourcePath = join(source, 'plugin.json');
  const sourceName = repositoryPath(root, sourcePath);
  /** @type {import('./manifest.mjs').PluginSource} */
  const plugin = readJson(sourcePath, sourceName);
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
    const packageName = repositoryPath(root, packageJson);
    /** @type {{ name: string, scripts?: Record<string, string> }} */
    const { name, scripts } = readJson(packageJson, packageName);
    // CI runs `pnpm --filter <name> test`, which passes over a package without the script.
    if (typeof scripts?.test !== 'string') {
      throw new Error(`${packageName}: no "test" script, so \`pnpm --filter ${name} test\` would test nothing`);
    }
    jsPackage = name;
    const packages = workspacePackages(root, name, packageName);
    inputs.push(...packages.map(({ folder }) => folder));
    inputs.push(...turboInputs(turboConfig(), packages.map((entry) => packageTasks(root, entry)), 'turbo.json'));
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
 * The workspace package `name` and the workspace packages it depends on,
 * devDependencies included, with their folders, as pnpm lists them (which
 * works before `pnpm install`).
 *
 * @param {string} root The repository's real folder.
 * @param {string} name
 * @param {string} packageJson The package's package.json, for error messages.
 * @returns {{ name: string, folder: string }[]}
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
  return listed.map((entry) => {
    if (typeof entry.path !== 'string') throw new Error(`pnpm ls gave a package without a path for ${name}`);
    if (typeof entry.name !== 'string') throw new Error(`pnpm ls gave a package without a name for ${name}`);
    return { name: entry.name, folder: repositoryPath(root, entry.path) };
  });
}

/**
 * The root turbo.json's task settings that inputs.mjs reads.
 *
 * @typedef {object} TurboConfig
 * @property {unknown[]} [globalDependencies]
 * @property {Record<string, { inputs?: unknown[], [setting: string]: unknown }>} [tasks]
 */

/**
 * The root turbo.json, or `null` without one. A turbo.jsonc is refused rather
 * than skipped, since JSON.parse cannot read it.
 *
 * @param {string} root The repository's real folder.
 * @returns {TurboConfig | null}
 */
function readTurboConfig(root) {
  if (existsSync(join(root, 'turbo.jsonc'))) throw new Error('turbo.jsonc: not read; use turbo.json');
  const path = join(root, 'turbo.json');
  return existsSync(path) ? readJson(path, 'turbo.json') : null;
}

/**
 * A workspace package's name and script names, which are the tasks turbo
 * runs in it. A package's own turbo.json is refused: its inputs are not read.
 *
 * @param {string} root The repository's real folder.
 * @param {{ name: string, folder: string }} entry
 * @returns {{ name: string, scripts: string[] }}
 */
function packageTasks(root, { name, folder }) {
  for (const config of ['turbo.json', 'turbo.jsonc']) {
    if (existsSync(join(root, folder, config))) {
      throw new Error(`${folder}/${config}: a package configuration, whose task inputs are not read`);
    }
  }
  /** @type {{ scripts?: Record<string, string> }} */
  const manifest = readJson(join(root, folder, 'package.json'), `${folder}/package.json`);
  return { name, scripts: Object.keys(manifest.scripts ?? {}) };
}

/**
 * The repository paths turbo.json makes inputs of the tasks `packages` run:
 * its `globalDependencies`, and the `$TURBO_ROOT$/` entries of the `inputs`
 * of each package's scripts, under the `<script>` and `<package>#<script>`
 * keys. A task that `dependsOn` another task of the same package runs one of
 * its scripts, and `^` dependencies are packages in `packages` too, so those
 * add nothing; a `<package>#<task>` dependency on a package outside
 * `packages` is an error, since that task's inputs would be left out.
 * Package-relative entries stay in the package's folder, which is already an
 * input; `$TURBO_DEFAULT$` and `!` exclusions are skipped, which can only add
 * paths. A pattern that names no folder or file (any glob other than a
 * trailing `/**`, or a path outside the repository) is an error.
 *
 * @param {TurboConfig | null} turbo
 * @param {{ name: string, scripts: string[] }[]} packages
 * @param {string} turboName turbo.json's path, for error messages.
 * @returns {string[]} Sorted, without duplicates.
 */
export function turboInputs(turbo, packages, turboName) {
  if (turbo === null) return [];
  /** @type {Set<string>} */
  const inputs = new Set();
  for (const pattern of turbo.globalDependencies ?? []) {
    if (typeof pattern === 'string' && pattern.startsWith('!')) continue;
    inputs.add(repositoryPattern(pattern, `${turboName}: globalDependencies`));
  }
  const tasks = turbo.tasks ?? {};
  const names = new Set(packages.map(({ name }) => name));
  for (const { name, scripts } of packages) {
    for (const script of scripts) {
      for (const key of [script, `${name}#${script}`]) {
        if (!Object.hasOwn(tasks, key)) continue;
        const { dependsOn } = tasks[key];
        for (const dependency of Array.isArray(dependsOn) ? dependsOn : []) {
          if (typeof dependency !== 'string' || dependency.startsWith('^') || !dependency.includes('#')) continue;
          if (!names.has(dependency.slice(0, dependency.lastIndexOf('#')))) {
            throw new Error(
              `${turboName}: tasks[${JSON.stringify(key)}].dependsOn has ${dependency}, ` +
                "whose package is not among the plugin's packages",
            );
          }
        }
        const where = `${turboName}: tasks[${JSON.stringify(key)}].inputs`;
        for (const entry of tasks[key].inputs ?? []) {
          for (const pattern of taskGlobs(entry, where)) {
            if (pattern === '$TURBO_DEFAULT$' || pattern.startsWith('!')) continue;
            if (pattern.startsWith('$TURBO_ROOT$/')) {
              inputs.add(repositoryPattern(pattern.slice('$TURBO_ROOT$/'.length), where, pattern));
            } else if (pattern.includes('$TURBO_ROOT$') || pattern.split('/').includes('..')) {
              throw new Error(`${where} has ${pattern}, which names no repository folder or file`);
            }
          }
        }
      }
    }
  }
  return [...inputs].sort();
}

/**
 * The globs of one `inputs` entry: a string, or a deferred-hashing object
 * with `globs`.
 *
 * @param {unknown} entry
 * @param {string} where
 * @returns {string[]}
 */
function taskGlobs(entry, where) {
  if (typeof entry === 'string') return [entry];
  if (typeof entry === 'object' && entry !== null && 'globs' in entry && Array.isArray(entry.globs)) {
    if (entry.globs.every((glob) => typeof glob === 'string')) return entry.globs;
  }
  throw new Error(`${where} has ${JSON.stringify(entry)}, which is neither a glob nor an entry with globs`);
}

/**
 * A repository-relative glob as an input: a file, or a folder written
 * `<folder>/**`.
 *
 * @param {unknown} glob
 * @param {string} where
 * @param {unknown} [pattern] The glob as written, for error messages.
 * @returns {string}
 */
function repositoryPattern(glob, where, pattern = glob) {
  const path = typeof glob === 'string' && glob.endsWith('/**') ? glob.slice(0, -'/**'.length) : glob;
  const parts = typeof path === 'string' ? path.split('/') : [''];
  // `$` too: a `$TURBO_ROOT$` or `$TURBO_DEFAULT$` left here would match no path.
  if (parts.some((part) => part === '' || part === '.' || part === '..' || /[*?[\]{}!$]/.test(part))) {
    throw new Error(`${where} has ${String(pattern)}, which names no repository folder or file`);
  }
  return parts.join('/');
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
