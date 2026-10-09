// Copy the installed, lockfile-resolved runtime closure without pnpm symlinks.
import { cpSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolvePackageManifest } from './resolve-package-manifest.mjs';

const source = fileURLToPath(new URL('../packages/cli/', import.meta.url));
const destination = resolve(process.argv[2]);
const copied = new Map();
mkdirSync(destination, { recursive: true });
cpSync(join(source, 'dist'), join(destination, 'dist'), { recursive: true });
cpSync(join(source, 'package.json'), join(destination, 'package.json'));

function copyPackage(name, parent) {
  const manifest = resolvePackageManifest(name, parent);
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
    filter: (path) => {
      if (path === directory) return true;
      const parts = path.slice(directory.length + 1).split(/[\\/]/);
      if (parts.includes('node_modules')) return false;
      // Workspace packages contain development files that are not part of
      // the installed runtime. Registry packages already have their publish set.
      if (name.startsWith('@celesta/')) {
        const top = parts[0];
        return top === 'dist' || top === 'package.json' || /^(readme|licen[cs]e)(\.|$)/i.test(top);
      }
      return true;
    },
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

// The CLI depends on every Celesta package it serves to entries, React, and
// esbuild; each brings its own closure, such as Code's tokenizers.
const manifest = JSON.parse(readFileSync(join(source, 'package.json'), 'utf8'));
for (const name of [...Object.keys(manifest.dependencies), 'ag-psd']) {
  copyPackage(name, source);
}
// Confirm the platform-specific binary was included, even if optional deps were disabled.
if (!copied.has(`@esbuild/${process.platform}-${process.arch}`)) {
  throw new Error('Install the native esbuild optional dependency before packaging');
}
writeFileSync(join(destination, 'runtime-packages.json'), JSON.stringify(Object.fromEntries(copied), null, 2));
