// Builds the VRM plugin into a plugin directory Celesta loads: its manifest
// (celesta-plugin.json), its process, the component module entries import as
// `@celesta-plugin/vrm`, and that module's declarations. plugin.json says
// what it has. It is examples/plugins/build.mjs for this one plugin, whose
// Cargo workspace is its own.
//
//   node plugins/vrm/build.mjs [--out <dir>] [--target <triple>] [--skip-js]
//
// Run `pnpm install && pnpm run build:runtime` at the repository root first:
// the runtime supplies the declarations of `react` and `@celesta/*`, `tsc`,
// and `esbuild`. The plugin directory goes to plugins/vrm/dist/vrm; put
// plugins/vrm/dist on CELESTA_PLUGIN_PATH or pass it as the exporter's
// --plugin-dir.
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

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..', '..');
const { values } = parseArgs({
  options: {
    out: { type: 'string', default: join(here, 'dist') },
    target: { type: 'string' },
    'skip-js': { type: 'boolean', default: false },
  },
});

function fail(message) {
  console.error(`build.mjs: ${message}`);
  process.exit(1);
}

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

// The platform an executable is for, as manifests name it: `<os>-<arch>`,
// in the names of Rust's `std::env::consts`, which the host matches against.
function platform() {
  if (values.target) {
    const [triple] = values.target.split('-');
    const arch = /^i[3-6]86$/.test(triple) ? 'x86' : /^arm(?!64)/.test(triple) ? 'arm' : triple;
    const os = values.target.includes('windows')
      ? 'windows'
      : values.target.includes('apple-darwin')
        ? 'macos'
        : values.target.includes('linux')
          ? 'linux'
          : fail(`cannot tell the OS of target ${values.target}`);
    return { os, arch };
  }
  const os = { win32: 'windows', darwin: 'macos' }[process.platform] ?? process.platform;
  const arch = { x64: 'x86_64', arm64: 'aarch64', ia32: 'x86' }[process.arch] ?? process.arch;
  return { os, arch };
}

/**
 * The plugin's process: built with Cargo in the plugin's own workspace and
 * copied under bin/<os>-<arch>/. Cargo runs in the plugin's folder, so it
 * reads the same configuration (`.cargo/config.toml`) wherever the script is
 * started, and says itself where it builds (`CARGO_TARGET_DIR` or
 * `build.target-dir` included).
 */
function buildExecutable(out, cargoPackage) {
  const { os, arch } = platform();
  const binary = os === 'windows' ? `${cargoPackage}.exe` : cargoPackage;
  const cargo = process.env.CARGO ?? 'cargo';
  const manifest = join(here, 'Cargo.toml');
  const args = ['build', '--release', '--locked', '--manifest-path', manifest, '--bin', cargoPackage];
  if (values.target) args.push('--target', values.target);
  run(cargo, args, { cwd: here });
  const metadata = spawnSync(
    cargo,
    ['metadata', '--format-version', '1', '--no-deps', '--locked', '--manifest-path', manifest],
    { cwd: here, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'], maxBuffer: 64 * 1024 * 1024 },
  );
  if (metadata.status !== 0) fail(`${cargo} metadata failed`);
  const targetDirectory = JSON.parse(metadata.stdout).target_directory;
  const built = join(targetDirectory, ...(values.target ? [values.target] : []), 'release', binary);
  if (!existsSync(built)) fail(`${built} was not built`);
  mkdirSync(join(out, 'bin', `${os}-${arch}`), { recursive: true });
  copyFileSync(built, join(out, 'bin', `${os}-${arch}`, binary));
  return { [`${os}-${arch}`]: `bin/${os}-${arch}/${binary}` };
}

/** The module as one ES module, and its declarations, under js/. */
async function buildModule(out, module) {
  const entry = join(here, module);
  // Declarations, checked against the runtime's: a tsconfig extending the
  // project TypeScript support, which maps `react` and `@celesta/*`.
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

  // `react`, `@celesta/*`, and Node's modules stay imports: the CLI points
  // them at its own runtime when it bundles the module into an entry.
  await fromRuntime('esbuild').build({
    entryPoints: [entry],
    outfile: join(out, 'js', 'index.js'),
    bundle: true,
    format: 'esm',
    platform: 'node',
    external: ['react', 'react/*', '@celesta/*'],
    logLevel: 'warning',
  });
}

const plugin = JSON.parse(readFileSync(join(here, 'plugin.json'), 'utf8'));
const out = join(resolve(values.out), plugin.id);
mkdirSync(out, { recursive: true });
const manifestPath = join(out, 'celesta-plugin.json');
const previous = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, 'utf8')) : {};

const executable = { ...previous.executable, ...buildExecutable(out, plugin.cargoPackage) };
if (!values['skip-js']) await buildModule(out, plugin.module);

const manifest = {
  id: plugin.id,
  version: plugin.version,
  protocol: 1,
  executable,
  module: 'js/index.js',
  types: 'js/index.d.ts',
  components: plugin.components,
};
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`${plugin.id}: ${out} (executables: ${Object.keys(executable).join(', ')})`);
