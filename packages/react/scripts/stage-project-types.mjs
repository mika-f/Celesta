// Stage the TypeScript support directory the editor copies into a project's
// `.celesta/`: `@celesta/react`'s declarations, the React types they build on,
// Node's globals (entries run under the bundled Node.js), and a base tsconfig
// mapping those imports. The runtime never loads these — entries always
// resolve `react` and `@celesta/react` to the bundled copies — so a project
// gets matching types without installing anything.
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../', import.meta.url));
const dist = join(source, 'dist');
const destination = join(dist, 'project-types');
const modules = join(destination, 'node_modules');
rmSync(destination, { recursive: true, force: true });

const manifest = JSON.parse(readFileSync(join(source, 'package.json'), 'utf8'));
const celesta = join(modules, '@celesta/react');
// `destination` lives inside `dist`, so walk it instead of `cpSync`-ing into itself.
function copyDeclarations(directory) {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (path === destination) continue;
    if (statSync(path).isDirectory()) {
      copyDeclarations(path);
    } else if (name.endsWith('.d.ts')) {
      const target = join(celesta, 'dist', relative(dist, path));
      mkdirSync(dirname(target), { recursive: true });
      cpSync(path, target);
    }
  }
}
copyDeclarations(dist);
writeJson(join(celesta, 'package.json'), {
  name: manifest.name,
  version: manifest.version,
  license: manifest.license,
  types: './dist/index.d.ts',
});

// Declarations and license notices only; the runtime ships the JavaScript.
function copyTypes(name, parent) {
  const require = createRequire(join(parent, 'package.json'));
  const packageJson = realpathSync(require.resolve(`${name}/package.json`));
  const directory = dirname(packageJson);
  const target = join(modules, name);
  if (statSync(target, { throwIfNoEntry: false })) return;
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
  const metadata = JSON.parse(readFileSync(packageJson, 'utf8'));
  for (const dependency of Object.keys(metadata.dependencies ?? {})) {
    copyTypes(dependency, directory);
  }
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
      '@celesta/react': ['./node_modules/@celesta/react'],
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
