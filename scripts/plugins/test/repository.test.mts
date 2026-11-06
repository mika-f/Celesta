import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { cargoMetadata } from '../cargo.mts';
import { checkPluginSource, type PluginSource } from '../manifest.mts';
import { FOLLOWS_CELESTA, workspaceVersion } from '../version.mts';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

// Every repository plugin.json, as [plugin folder, plugin.json].
const sources = ['plugins', join('examples', 'plugins')].flatMap((folder) =>
  readdirSync(join(root, folder))
    .map((name) => join(root, folder, name))
    .filter((source) => existsSync(join(source, 'plugin.json')))
    .map((source): [string, PluginSource] => [source, JSON.parse(readFileSync(join(source, 'plugin.json'), 'utf8'))]),
);
const withCargoPackage = sources.filter(([, plugin]) => plugin.cargoPackage !== undefined);

/**
 * The string values of `table` in `cargoToml`, by key: `key = "value"` lines
 * only, which is how the package tables give edition, rust-version and license.
 */
function tableStrings(cargoToml: string, table: string): Map<string, string> {
  const values = new Map<string, string>();
  let current = '';
  for (const line of cargoToml.split(/\r?\n/)) {
    const header = /^\s*\[\[?\s*([^\]]*?)\s*\]\]?\s*(?:#.*)?$/.exec(line);
    if (header) {
      current = header[1];
      continue;
    }
    const value = /^\s*([A-Za-z0-9_-]+)\s*=\s*"([^"]*)"/.exec(line);
    if (current === table && value) values.set(value[1], value[2]);
  }
  return values;
}

describe("the repository's plugins", () => {
  it('include plugins with a Cargo package', () => {
    expect(withCargoPackage.map(([source]) => source)).toContain(join(root, 'plugins', 'vrm'));
  });

  it.each(sources.map(([source, plugin]) => [source.slice(root.length + 1), plugin] as const))(
    '%s has only the keys plugin.json may have',
    (source, plugin) => {
      expect(() => checkPluginSource(plugin, join(source, 'plugin.json'))).not.toThrow();
    },
  );

  // The codec is no member of the root workspace, so it cannot inherit these
  // with `workspace = true`; they are written out and must not drift.
  it.each(['edition', 'rust-version', 'license'])(
    "plugins/ffmpeg/Cargo.toml gives the root [workspace.package]'s %s",
    (key) => {
      const workspace = tableStrings(readFileSync(join(root, 'Cargo.toml'), 'utf8'), 'workspace.package');
      const codec = tableStrings(readFileSync(join(root, 'plugins', 'ffmpeg', 'Cargo.toml'), 'utf8'), 'package');
      expect(workspace.get(key), `the root Cargo.toml has no [workspace.package] ${key}`).toBeDefined();
      expect(codec.get(key)).toBe(workspace.get(key));
    },
  );

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

  it.each(FOLLOWS_CELESTA)("%s gives Celesta's version", (folder) => {
    const plugin: PluginSource = JSON.parse(readFileSync(join(root, folder, 'plugin.json'), 'utf8'));
    expect(plugin.version).toBe(workspaceVersion(readFileSync(join(root, 'Cargo.toml'), 'utf8')));
  });
});
