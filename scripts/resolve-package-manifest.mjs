import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

// Some dependencies export their entry point but keep package.json private.
// Packaging still needs the installed manifest to follow their dependencies.
export function resolvePackageManifest(name, parent) {
  const require = createRequire(join(parent, 'package.json'));
  try {
    return realpathSync(require.resolve(`${name}/package.json`));
  } catch (error) {
    if (error.code !== 'ERR_PACKAGE_PATH_NOT_EXPORTED') throw error;
  }
  let directory = dirname(realpathSync(require.resolve(name)));
  while (true) {
    const manifest = join(directory, 'package.json');
    if (existsSync(manifest) && JSON.parse(readFileSync(manifest, 'utf8')).name === name) return manifest;
    const next = dirname(directory);
    if (next === directory) throw new Error(`Cannot locate the installed manifest of ${name}`);
    directory = next;
  }
}
