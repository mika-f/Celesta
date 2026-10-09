// Stage the TypeScript support directory the editor copies into a project's
// `.celesta/`: the declarations of every Celesta package the runtime serves,
// React's supporting types, Node's globals, and a base tsconfig mapping those
// imports. Entries use the bundled runtime, so projects get matching types
// without an install.
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolvePackageManifest } from '../../../scripts/resolve-package-manifest.mjs';

const source = fileURLToPath(new URL('../', import.meta.url));
const destination = join(source, 'dist', 'project-types');
const modules = join(destination, 'node_modules');
rmSync(destination, { recursive: true, force: true });

const manifest = JSON.parse(readFileSync(join(source, 'package.json'), 'utf8'));
const celesta = Object.keys(manifest.dependencies).filter((name) => name.startsWith('@celesta/'));

// Declarations and license notices only; the runtime ships the JavaScript.
function copyTypes(name, parent) {
  const packageJson = resolvePackageManifest(name, parent);
  const directory = dirname(packageJson);
  const target = join(modules, name);
  if (statSync(target, { throwIfNoEntry: false })) return;
  const metadata = JSON.parse(readFileSync(packageJson, 'utf8'));
  if (metadata.types && !statSync(join(directory, metadata.types), { throwIfNoEntry: false })?.isFile()) {
    throw new Error(`Missing declarations for ${name}: ${metadata.types}. Build the package before staging project types.`);
  }
  cpSync(directory, target, {
    recursive: true,
    dereference: true,
    filter: (path) => {
      const parts = relative(directory, path).split(sep);
      if (parts.includes('node_modules')) return false;
      const base = parts.at(-1);
      return statSync(path).isDirectory() || base.endsWith('.d.ts') || base === 'package.json' || /^licen[cs]e/i.test(base);
    },
  });
  for (const dependency of Object.keys(metadata.dependencies ?? {})) {
    copyTypes(dependency, directory);
  }
}
for (const name of celesta) {
  copyTypes(name, source);
}
copyTypes('@types/react', source);
copyTypes('@types/node', source);

writeJson(join(destination, 'tsconfig.json'), {
  compilerOptions: {
    target: 'ES2022',
    lib: ['ES2022'],
    module: 'ESNext',
    moduleResolution: 'Bundler',
    jsx: 'react-jsx',
    strict: true,
    noEmit: true,
    isolatedModules: true,
    allowImportingTsExtensions: true,
    resolveJsonModule: true,
    skipLibCheck: true,
    // Only the bundled declarations, so a project's own `@types/node` or
    // `@types/react` of another version cannot collide with them.
    typeRoots: ['./node_modules/@types'],
    types: ['node'],
    paths: {
      ...Object.fromEntries(celesta.map((name) => [name, [`./node_modules/${name}`]])),
      // Other packages' declarations import the core runtime's internals.
      '@celesta/react/internal': ['./node_modules/@celesta/react/dist/internal.d.ts'],
      react: ['./node_modules/@types/react'],
      'react/*': ['./node_modules/@types/react/*'],
    },
  },
});

// The editor refreshes a project's copy whenever this stamp differs.
const hash = createHash('sha256');
function hashTree(directory) {
  for (const name of readdirSync(directory).sort()) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) {
      hashTree(path);
    } else {
      hash.update(relative(destination, path).split(sep).join('/'));
      hash.update('\0');
      hash.update(readFileSync(path));
    }
  }
}
hashTree(destination);
writeJson(join(destination, 'version.json'), { version: manifest.version, hash: hash.digest('hex') });

function writeJson(path, value) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}
