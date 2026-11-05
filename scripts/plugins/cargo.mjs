// What Cargo says about the workspace a plugin's Cargo.toml belongs to: where
// it builds, and its packages' versions.
import { spawnSync } from 'node:child_process';

/**
 * @typedef {object} CargoMetadata
 * @property {string} targetDirectory Where the workspace builds (`CARGO_TARGET_DIR` or `build.target-dir` included).
 * @property {Map<string, string>} versions Each workspace member's version, by package name.
 */

/**
 * `cargo metadata --no-deps` for `manifest`, run with `$CARGO` (or `cargo`)
 * in `cwd`, so a `.cargo/config.toml` there applies.
 *
 * @param {string} manifest The path of a Cargo.toml.
 * @param {{ cwd?: string }} [options]
 * @returns {CargoMetadata}
 */
export function cargoMetadata(manifest, { cwd } = {}) {
  const cargo = process.env.CARGO ?? 'cargo';
  const args = ['metadata', '--format-version', '1', '--no-deps', '--locked', '--manifest-path', manifest];
  const result = spawnSync(cargo, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) throw new Error(`${cargo} metadata failed: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`${cargo} metadata failed for ${manifest}`);
  /** @type {{ target_directory: string, packages: { name: string, version: string }[] }} */
  const { target_directory: targetDirectory, packages } = JSON.parse(result.stdout);
  return { targetDirectory, versions: new Map(packages.map(({ name, version }) => [name, version])) };
}
