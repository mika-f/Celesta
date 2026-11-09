// Builds plugins into plugin directories Celesta loads: for each, its manifest
// (celesta-plugin.json), its process when it has one, the component module
// entries import as `@celesta-plugin/<id>` and that module's declarations
// when it has one, and the license files it lists, under licenses/. Each
// plugin's plugin.json says what it has, its version and roles included. A
// plugin with a process gives the same version in its Cargo package, and one
// whose versions differ is refused before that plugin is built. Every
// plugin.json is checked first, so a plugin with a key plugin.json may not
// have, nothing to build, components but no module, no version, a license
// file that is not there, two license files sharing a name, or, with
// --skip-js, a module no earlier run built, fails the run before anything is
// built. The manifest's `celesta` range, the Celesta versions the plugin
// works with, comes from the Celesta this script belongs to (its root
// Cargo.toml's version): the one whose crates the plugin's process is built
// with and whose runtime its module is checked against.
//
//   node scripts/plugins/build.mts <plugin directory>...
//                                  [--out <dir>] [--target <triple>] [--skip-js]
//
// Each plugin directory resolves against the current directory, as does
// --out. When a plugin has a module and --skip-js is not given, run
// `pnpm install && pnpm run build:runtime` at the repository root first: the
// runtime supplies the declarations of `react` and `@celesta.studio/*`,
// `tsc`, and `esbuild`. A plugin with only a process, or a run with
// --skip-js, needs Cargo alone. A plugin directory goes to
// <out>/<id>, where <out> is by default the dist/ folder beside the plugin's
// folder (plugins/vrm goes to plugins/dist/vrm, examples/plugins/flag to
// examples/plugins/dist/flag); put <out> on CELESTA_PLUGIN_PATH or pass it as
// the exporter's --plugin-dir.
//
// Each run adds this platform's (or --target's) executable and keeps the
// others' whose files are still there, so builds on several platforms can
// fill one directory; the JavaScript is the same everywhere, so later builds
// may pass --skip-js. Both hold only while the earlier run wrote the same
// `celesta` range: a run for another range drops the other executables, and
// refuses --skip-js. Each run replaces licenses/ with the files plugin.json
// lists.
import { spawnSync } from 'node:child_process';
import type { SpawnSyncOptions } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { cargoMetadata } from './cargo.mts';
import { licenseFiles } from './licenses.mts';
import { buildsOn, celestaRange, checkPluginSource, pluginManifest, pluginVersion } from './manifest.mts';
import type { Built, PluginManifest, PluginSource } from './manifest.mts';
import { rustPlatform } from './platform.mts';
import { workspaceVersion } from './version.mts';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');
const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    out: { type: 'string' },
    target: { type: 'string' },
    'skip-js': { type: 'boolean', default: false },
  },
});

if (positionals.length === 0) {
  console.error(
    'usage: node scripts/plugins/build.mts <plugin directory>... [--out <dir>] [--target <triple>] [--skip-js]',
  );
  process.exit(2);
}

function fail(message: string): never {
  console.error(`build.mts: ${message}`);
  process.exit(1);
}

function run(command: string, args: string[], options: SpawnSyncOptions = {}): void {
  const line = `${command} ${args.join(' ')}`;
  console.log(`> ${line}`);
  const result = spawnSync(command, args, { stdio: 'inherit', ...options });
  if (result.error) fail(`${line} failed: ${result.error.message}`);
  if (result.status !== 0) {
    fail(`${line} failed (${result.status === null ? `signal ${result.signal}` : `exit code ${result.status}`})`);
  }
}

function orFail<T>(step: () => T): T {
  try {
    return step();
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
}

/** The JSON in the file at `path`; a failure names the file. */
function readJson(path: string): any {
  return orFail(() => {
    try {
      return JSON.parse(readFileSync(path, 'utf8'));
    } catch (error) {
      throw new Error(`${path}: ${error instanceof Error ? error.message : String(error)}`);
    }
  });
}

// The Celesta the plugins are built against, and the range its manifests give.
const celestaVersion = orFail(() => workspaceVersion(readFileSync(join(root, 'Cargo.toml'), 'utf8')));
const range = orFail(() => celestaRange(celestaVersion));

// Every plugin.json, checked before anything is built, and with --skip-js the
// module an earlier run built for the same range.
const plugins = positionals.flatMap((directory) => {
  const source = resolve(directory);
  const sourcePath = join(source, 'plugin.json');
  const plugin: PluginSource = readJson(sourcePath);
  orFail(() => checkPluginSource(plugin, sourcePath));
  const { os } = orFail(() => rustPlatform({ target: values.target, cwd: source }));
  if (!buildsOn(plugin, os)) {
    console.log(`${plugin.id}: skipped, it is built for ${plugin.os?.join(', ')} only`);
    return [];
  }
  orFail(() => pluginVersion({ version: plugin.version }, undefined, sourcePath));
  const licenses = orFail(() => licenseFiles(source, plugin, sourcePath));
  const out = join(values.out === undefined ? join(dirname(source), 'dist') : resolve(values.out), plugin.id);
  const manifestPath = join(out, 'celesta-plugin.json');
  const previous: Partial<PluginManifest> = existsSync(manifestPath) ? readJson(manifestPath) : {};
  if (values['skip-js'] && plugin.module !== undefined) {
    for (const file of ['index.js', 'index.d.ts']) {
      const built = join(out, 'js', file);
      if (!existsSync(built)) fail(`${built} is missing; build ${source} once without --skip-js`);
    }
    // A module built against another Celesta must not get this one's range.
    if (previous.celesta !== range) {
      fail(
        `${join(out, 'js')} was built for Celesta ${previous.celesta ?? '(no range)'}, not ${range}; build ${source} once without --skip-js`,
      );
    }
  }
  return [{ source, sourcePath, plugin, licenses, out, manifestPath, previous }];
});

const runtime = join(root, 'packages', 'cli');
const projectTypes = join(runtime, 'dist', 'project-types');
const buildsModules = !values['skip-js'] && plugins.some(({ plugin }) => plugin.module !== undefined);
if (buildsModules && !existsSync(join(projectTypes, 'tsconfig.json'))) {
  fail(`${projectTypes} is missing; run pnpm install && pnpm run build:runtime in ${root}`);
}
const fromRuntime = createRequire(join(runtime, 'package.json'));

/**
 * The plugin's process: built with Cargo in the workspace the plugin's
 * Cargo.toml belongs to and copied under bin/<os>-<arch>/. Cargo runs in the
 * plugin's folder, so it reads the same configuration (`.cargo/config.toml`)
 * wherever the script is started.
 *
 * @param source The plugin's folder.
 * @param out The plugin directory.
 * @param targetDirectory Where that workspace builds, as Cargo says.
 */
function buildExecutable(source: string, out: string, cargoPackage: string, targetDirectory: string): Record<string, string> {
  const { os, arch } = orFail(() => rustPlatform({ target: values.target, cwd: source }));
  const binary = os === 'windows' ? `${cargoPackage}.exe` : cargoPackage;
  const cargo = process.env.CARGO ?? 'cargo';
  const manifest = join(source, 'Cargo.toml');
  const args = ['build', '--release', '--locked', '--manifest-path', manifest, '--bin', cargoPackage];
  if (values.target) args.push('--target', values.target);
  run(cargo, args, { cwd: source });
  const built = join(targetDirectory, ...(values.target ? [values.target] : []), 'release', binary);
  if (!existsSync(built)) fail(`${built} was not built`);
  mkdirSync(join(out, 'bin', `${os}-${arch}`), { recursive: true });
  copyFileSync(built, join(out, 'bin', `${os}-${arch}`, binary));
  return { [`${os}-${arch}`]: `bin/${os}-${arch}/${binary}` };
}

/**
 * The module as one ES module, and its declarations, under js/.
 *
 * @param source The plugin's folder.
 * @param out The plugin directory.
 * @param module The module's entry, relative to `source`.
 */
async function buildModule(source: string, out: string, module: string): Promise<void> {
  const entry = join(source, module);
  // Declarations, checked against the runtime's: a tsconfig extending the
  // project TypeScript support, which maps `react` and `@celesta.studio/*`.
  const types = join(out, '.types');
  rmSync(types, { recursive: true, force: true });
  mkdirSync(types, { recursive: true });
  writeFileSync(
    join(types, 'tsconfig.json'),
    JSON.stringify({
      extends: join(projectTypes, 'tsconfig.json'),
      compilerOptions: {
        noEmit: false,
        declaration: true,
        emitDeclarationOnly: true,
        rootDir: dirname(entry),
        outDir: join(types, 'out'),
      },
      files: [entry],
    }),
  );
  run(process.execPath, [fromRuntime.resolve('typescript/bin/tsc'), '-p', join(types, 'tsconfig.json')]);
  mkdirSync(join(out, 'js'), { recursive: true });
  copyFileSync(join(types, 'out', 'index.d.ts'), join(out, 'js', 'index.d.ts'));
  rmSync(types, { recursive: true, force: true });

  // `react`, `@celesta.studio/*`, and Node's modules stay imports: the CLI points
  // them at its own runtime when it bundles the module into an entry.
  await fromRuntime('esbuild').build({
    entryPoints: [entry],
    outfile: join(out, 'js', 'index.js'),
    bundle: true,
    format: 'esm',
    platform: 'node',
    external: ['react', 'react/*', '@celesta.studio/*'],
    logLevel: 'warning',
  });
}

for (const { source, sourcePath, plugin, licenses, out, manifestPath, previous } of plugins) {
  mkdirSync(out, { recursive: true });
  // The other platforms' executables an earlier run copied here, while they
  // are, and only when it built them against a Celesta with the same range.
  const sameRange = previous.celesta === range;
  const kept = Object.fromEntries(
    Object.entries(previous.executable ?? {}).filter(([, path]) => sameRange && existsSync(join(out, path))),
  );
  if (!sameRange && Object.keys(previous.executable ?? {}).length > 0) {
    console.log(`${plugin.id}: dropping the executables built for Celesta ${previous.celesta ?? '(no range)'}, not ${range}`);
  }

  let built: Built = { celestaVersion };
  if (plugin.cargoPackage !== undefined) {
    const { cargoPackage } = plugin;
    const cargoToml = join(source, 'Cargo.toml');
    const { targetDirectory, versions } = orFail(() => cargoMetadata(cargoToml, { cwd: source }));
    const cargoVersion = versions.get(cargoPackage);
    if (cargoVersion === undefined) fail(`${cargoToml} is in no workspace with the package ${cargoPackage}`);
    orFail(() => pluginVersion(plugin, cargoVersion, sourcePath));
    const executable = buildExecutable(source, out, cargoPackage, targetDirectory);
    built = { celestaVersion, cargoVersion, executable: { ...kept, ...executable } };
  }
  if (!values['skip-js'] && plugin.module !== undefined) await buildModule(source, out, plugin.module);

  rmSync(join(out, 'licenses'), { recursive: true, force: true });
  if (licenses.length > 0) {
    mkdirSync(join(out, 'licenses'));
    for (const { from, name } of licenses) copyFileSync(from, join(out, 'licenses', name));
  }

  const manifest = orFail(() => pluginManifest(plugin, built, sourcePath));
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  const executables = manifest.executable ? ` (executables: ${Object.keys(manifest.executable).join(', ')})` : '';
  console.log(`${plugin.id}: ${out}${executables}`);
}
