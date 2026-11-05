// What Cargo says about the workspace a plugin's Cargo.toml belongs to: where
// its root is and where it builds, its packages' versions, and the folders of
// its members and their path dependencies.
import { spawnSync } from 'node:child_process';
import { dirname } from 'node:path';

/**
 * @typedef {object} CargoMetadata
 * @property {string} workspaceRoot The workspace root's folder, absolute, as Cargo gives it.
 * @property {string} targetDirectory Where the workspace builds (`CARGO_TARGET_DIR` or `build.target-dir` included).
 * @property {Map<string, string>} versions Each workspace member's version, by package name.
 * @property {Map<string, string>} directories Each member's folder and each path dependency's folder, by package name; absolute, as Cargo gives them.
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
  /**
   * @type {{
   *   workspace_root: string,
   *   target_directory: string,
   *   packages: { name: string, version: string, manifest_path: string, dependencies: { name: string, path?: string }[] }[],
   * }}
   */
  const { workspace_root: workspaceRoot, target_directory: targetDirectory, packages } = JSON.parse(result.stdout);
  /** @type {Map<string, string>} */
  const directories = new Map();
  /** @param {string} name @param {string} directory */
  const add = (name, directory) => {
    const known = directories.get(name);
    if (known !== undefined && known !== directory) {
      throw new Error(`${manifest}: package ${name} is in both ${known} and ${directory}`);
    }
    directories.set(name, directory);
  };
  for (const { name, manifest_path: manifestPath } of packages) add(name, dirname(manifestPath));
  for (const { dependencies } of packages) {
    for (const { name, path } of dependencies) if (path !== undefined) add(name, path);
  }
  return {
    workspaceRoot,
    targetDirectory,
    versions: new Map(packages.map(({ name, version }) => [name, version])),
    directories,
  };
}
