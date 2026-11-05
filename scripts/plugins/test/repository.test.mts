import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { cargoMetadata } from '../cargo.mjs';
import type { PluginSource } from '../manifest.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

// Every repository plugin.json that names a Cargo package, as [plugin folder, plugin.json].
const withCargoPackage = ['plugins', join('examples', 'plugins')].flatMap((folder) =>
  readdirSync(join(root, folder))
    .map((name) => join(root, folder, name))
    .filter((source) => existsSync(join(source, 'plugin.json')))
    .map((source): [string, PluginSource] => [source, JSON.parse(readFileSync(join(source, 'plugin.json'), 'utf8'))])
    .filter(([, plugin]) => plugin.cargoPackage !== undefined),
);

describe("the repository's plugins", () => {
  it('include plugins with a Cargo package', () => {
    expect(withCargoPackage.map(([source]) => source)).toContain(join(root, 'plugins', 'vrm'));
  });

  // Each `cargo metadata` call resolves a whole workspace.
  it.each(withCargoPackage.map(([source, plugin]) => [source.slice(root.length + 1), plugin] as const))(
    "%s gives its Cargo package's version",
    (source, plugin) => {
      const cargoToml = join(root, source, 'Cargo.toml');
      const { versions } = cargoMetadata(cargoToml, { cwd: join(root, source) });
      expect(versions.has(plugin.cargoPackage!), `${cargoToml} has no package named ${plugin.cargoPackage}`).toBe(
        true,
      );
      expect(plugin.version).toBe(versions.get(plugin.cargoPackage!));
    },
    60_000,
  );
});
