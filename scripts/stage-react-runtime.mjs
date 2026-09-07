// Copy the installed, lockfile-resolved runtime closure without pnpm symlinks.
import { cpSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../packages/react/', import.meta.url));
const destination = resolve(process.argv[2]);
const copied = new Map();
mkdirSync(destination, { recursive: true });
cpSync(join(source, 'dist'), join(destination, 'dist'), { recursive: true });
cpSync(join(source, 'package.json'), join(destination, 'package.json'));

function copyPackage(name, parent) {
  const require = createRequire(join(parent, 'package.json'));
  const manifest = realpathSync(require.resolve(`${name}/package.json`));
  const directory = dirname(manifest);
  const metadata = JSON.parse(readFileSync(manifest, 'utf8'));
  if (copied.has(name)) {
    if (copied.get(name) !== metadata.version) {
      throw new Error(`Cannot flatten conflicting versions of ${name}`);
    }
    return;
  }
  copied.set(name, metadata.version);
  cpSync(directory, join(destination, 'node_modules', name), {
    recursive: true,
    dereference: true,
    filter: (path) => path === directory || !path.slice(directory.length + 1).split(/[\\/]/).includes('node_modules'),
  });
  for (const dependency of Object.keys(metadata.dependencies ?? {})) {
    copyPackage(dependency, directory);
  }
  // esbuild selects its native executable by OS and architecture.
  const dependencyRequire = createRequire(manifest);
  for (const dependency of Object.keys(metadata.optionalDependencies ?? {})) {
    try {
      dependencyRequire.resolve(`${dependency}/package.json`);
    } catch (error) {
      if (error.code === 'MODULE_NOT_FOUND') continue;
      throw error;
    }
    copyPackage(dependency, directory);
  }
}

for (const name of ['react', 'react-reconciler', 'esbuild', 'ag-psd']) {
  copyPackage(name, source);
}
// Confirm the platform-specific binary was included, even if optional deps were disabled.
if (!copied.has(`@esbuild/${process.platform}-${process.arch}`)) {
  throw new Error('Install the native esbuild optional dependency before packaging');
}
writeFileSync(join(destination, 'runtime-packages.json'), JSON.stringify(Object.fromEntries(copied), null, 2));
