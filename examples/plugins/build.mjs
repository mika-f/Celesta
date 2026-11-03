// Builds the sample plugins into plugin directories Celesta loads: for each,
// its manifest (celesta-plugin.json), its process when it has one, the
// component module entries import as `@celesta-plugin/<id>`, and that
// module's declarations. Each sample's plugin.json says what it has.
//
//   node examples/plugins/build.mjs [flag] [turntable] [beep-voice]
//                                   [--out <dir>] [--target <triple>] [--skip-js]
//
// Run `pnpm install && pnpm run build:runtime` at the repository root first:
// the runtime supplies the declarations of `react` and `@celesta.studio/*`, `tsc`,
// and `esbuild`. The plugin directories go to examples/plugins/dist/<id>;
// put that directory on CELESTA_PLUGIN_PATH or pass it as the exporter's
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
const { values, positionals } = parseArgs({
  allowPositionals: true,
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

// The platform an executable is for, as manifests name it: `<os>-<arch>`.
function platform() {
  if (values.target) {
    const [arch] = values.target.split('-');
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

/** The plugin's process: built with Cargo and copied under bin/<os>-<arch>/. */
function buildExecutable(source, out, cargoPackage) {
  const { os, arch } = platform();
  const binary = os === 'windows' ? `${cargoPackage}.exe` : cargoPackage;
  const args = ['build', '--release', '-p', cargoPackage, '--bin', cargoPackage];
  if (values.target) args.push('--target', values.target);
  run(process.env.CARGO ?? 'cargo', args, { cwd: source });
  const targetDirectory = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR) : join(root, 'target');
  const built = join(targetDirectory, ...(values.target ? [values.target] : []), 'release', binary);
  if (!existsSync(built)) fail(`${built} was not built`);
  mkdirSync(join(out, 'bin', `${os}-${arch}`), { recursive: true });
  copyFileSync(built, join(out, 'bin', `${os}-${arch}`, binary));
  return { [`${os}-${arch}`]: `bin/${os}-${arch}/${binary}` };
}

/** The module as one ES module, and its declarations, under js/. */
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

const names = positionals.length ? positionals : ['flag', 'turntable', 'beep-voice'];
for (const name of names) {
  const source = join(here, name);
  const plugin = JSON.parse(readFileSync(join(source, 'plugin.json'), 'utf8'));
  const out = join(resolve(values.out), plugin.id);
  mkdirSync(out, { recursive: true });
  const manifestPath = join(out, 'celesta-plugin.json');
  const previous = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, 'utf8')) : {};

  const executable = plugin.cargoPackage
    ? { ...previous.executable, ...buildExecutable(source, out, plugin.cargoPackage) }
    : undefined;
  if (!values['skip-js']) await buildModule(source, out, plugin.module);

  const manifest = {
    id: plugin.id,
    version: plugin.version,
    protocol: 1,
    ...(executable ? { executable } : {}),
    module: 'js/index.js',
    types: 'js/index.d.ts',
    ...(plugin.components ? { components: plugin.components } : {}),
  };
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`${plugin.id}: ${out}${executable ? ` (executables: ${Object.keys(executable).join(', ')})` : ''}`);
}
