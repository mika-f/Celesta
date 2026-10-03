import { existsSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';

// Some dependencies export their entry point but keep package.json private,
// and import-only exports have no entry require.resolve can load either.
// Packaging still needs the installed manifest to follow their dependencies.
export function resolvePackageManifest(name, parent) {
  const require = createRequire(join(parent, 'package.json'));
  try {
    return realpathSync(require.resolve(`${name}/package.json`));
  } catch (error) {
    if (error.code !== 'ERR_PACKAGE_PATH_NOT_EXPORTED') throw error;
  }
  // Search the same node_modules directories without resolving an entry point.
  for (const directory of require.resolve.paths(name) ?? []) {
    const manifest = join(directory, name, 'package.json');
    if (existsSync(manifest)) return realpathSync(manifest);
  }
  throw new Error(`Cannot locate the installed manifest of ${name}`);
}
