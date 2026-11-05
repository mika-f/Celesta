// Builds plugins into plugin directories Celesta loads: for each, its manifest
// (celesta-plugin.json), its process when it has one, the component module
// entries import as `@celesta-plugin/<id>`, and that module's declarations.
// Each plugin's plugin.json says what it has.
//
//   node scripts/plugins/build.mjs <plugin directory>...
//                                  [--out <dir>] [--target <triple>] [--skip-js]
//
// Each plugin directory resolves against the current directory, as does
// --out. Run `pnpm install && pnpm run build:runtime` at the repository root
// first: the runtime supplies the declarations of `react` and
// `@celesta.studio/*`, `tsc`, and `esbuild`. A plugin directory goes to
// <out>/<id>, where <out> is by default the dist/ folder beside the plugin's
// folder (plugins/vrm goes to plugins/dist/vrm, examples/plugins/flag to
// examples/plugins/dist/flag); put <out> on CELESTA_PLUGIN_PATH or pass it as
// the exporter's --plugin-dir.
//
// Each run adds this platform's (or --target's) executable and keeps the
// others', so builds on several platforms can fill one directory; the
// JavaScript is the same everywhere, so later builds may pass --skip-js.
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { pluginManifest } from './manifest.mjs';
import { rustPlatform } from './platform.mjs';

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
    'usage: node scripts/plugins/build.mjs <plugin directory>... [--out <dir>] [--target <triple>] [--skip-js]',
  );
  process.exit(2);
}

/**
 * @param {string} message
 * @returns {never}
 */
function fail(message) {
  console.error(`build.mjs: ${message}`);
  process.exit(1);
}

/**
 * @param {string} command
 * @param {string[]} args
 * @param {import('node:child_process').SpawnSyncOptions} [options]
 */
function run(command, args, options = {}) {
  console.log(`> ${command} ${args.join(' ')}`);
  const result = spawnSync(command, args, { stdio: 'inherit', ...options });
  if (result.status !== 0) fail(`${command} failed`);
}

const runtime = join(root, 'packages', 'cli');
const projectTypes = join(runtime, 'dist', 'project-types');
if (!values['skip-js'] && !existsSync(join(projectTypes, 'tsconfig.json'))) {
  fail(`${projectTypes} is missing; run pnpm install && pnpm run build:runtime in ${root}`);
}
const fromRuntime = createRequire(join(runtime, 'package.json'));

/**
 * The plugin's process: built with Cargo in the workspace the plugin's
 * Cargo.toml belongs to and copied under bin/<os>-<arch>/, with the version of
 * its package. Cargo runs in the plugin's folder, so it reads the same
 * configuration (`.cargo/config.toml`) wherever the script is started, and
 * says itself where it builds (`CARGO_TARGET_DIR` or `build.target-dir`
 * included).
 *
 * @param {string} source The plugin's folder.
 * @param {string} out The plugin directory.
 * @param {string} cargoPackage
 * @returns {{ cargoVersion: string, executable: Record<string, string> }}
 */
function buildExecutable(source, out, cargoPackage) {
  let os, arch;
  try {
    ({ os, arch } = rustPlatform({ target: values.target, cwd: source }));
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
  const binary = os === 'windows' ? `${cargoPackage}.exe` : cargoPackage;
  const cargo = process.env.CARGO ?? 'cargo';
  const manifest = join(source, 'Cargo.toml');
  const args = ['build', '--release', '--locked', '--manifest-path', manifest, '--bin', cargoPackage];
  if (values.target) args.push('--target', values.target);
  run(cargo, args, { cwd: source });
  const metadata = spawnSync(
    cargo,
    ['metadata', '--format-version', '1', '--no-deps', '--locked', '--manifest-path', manifest],
    { cwd: source, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'], maxBuffer: 64 * 1024 * 1024 },
  );
  if (metadata.status !== 0) fail(`${cargo} metadata failed`);
  /** @type {{ target_directory: string, packages: { name: string, version: string }[] }} */
  const { target_directory: targetDirectory, packages } = JSON.parse(metadata.stdout);
  const cargoVersion = packages.find(({ name }) => name === cargoPackage)?.version;
  if (cargoVersion === undefined) fail(`${manifest} is in no workspace with the package ${cargoPackage}`);
  const built = join(targetDirectory, ...(values.target ? [values.target] : []), 'release', binary);
  if (!existsSync(built)) fail(`${built} was not built`);
  mkdirSync(join(out, 'bin', `${os}-${arch}`), { recursive: true });
  copyFileSync(built, join(out, 'bin', `${os}-${arch}`, binary));
  return { cargoVersion, executable: { [`${os}-${arch}`]: `bin/${os}-${arch}/${binary}` } };
}

/**
 * The module as one ES module, and its declarations, under js/.
 *
 * @param {string} source The plugin's folder.
 * @param {string} out The plugin directory.
 * @param {string} module The module's entry, relative to `source`.
 */
async function buildModule(source, out, module) {
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

for (const directory of positionals) {
  const source = resolve(directory);
  const sourcePath = join(source, 'plugin.json');
  /** @type {import('./manifest.mjs').PluginSource} */
  const plugin = JSON.parse(readFileSync(sourcePath, 'utf8'));
  const out = join(values.out === undefined ? join(dirname(source), 'dist') : resolve(values.out), plugin.id);
  mkdirSync(out, { recursive: true });
  const manifestPath = join(out, 'celesta-plugin.json');
  const previous = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, 'utf8')) : {};

  /** @type {import('./manifest.mjs').Built} */
  let built = {};
  if (plugin.cargoPackage !== undefined) {
    const { cargoVersion, executable } = buildExecutable(source, out, plugin.cargoPackage);
    built = { cargoVersion, executable: { ...previous.executable, ...executable } };
  }
  if (!values['skip-js']) await buildModule(source, out, plugin.module);

  let manifest;
  try {
    manifest = pluginManifest(plugin, built, sourcePath);
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  const executables = manifest.executable ? ` (executables: ${Object.keys(manifest.executable).join(', ')})` : '';
  console.log(`${plugin.id}: ${out}${executables}`);
}
