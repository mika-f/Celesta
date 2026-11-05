// What Cargo says about the workspace a plugin's Cargo.toml belongs to: where
// its root is and where it builds, its packages' versions, and the folders of
// its members and their path dependencies.
import { spawnSync } from 'node:child_process';
import { dirname } from 'node:path';

export interface CargoMetadata {
  /** The workspace root's folder, absolute, as Cargo gives it. */
  workspaceRoot: string;
  /** Where the workspace builds (`CARGO_TARGET_DIR` or `build.target-dir` included). */
  targetDirectory: string;
  /** Each workspace member's version, by package name. */
  versions: Map<string, string>;
  /** Each member's folder and each path dependency's folder, by package name; absolute, as Cargo gives them. */
  directories: Map<string, string>;
}

/**
 * `cargo metadata --no-deps` for `manifest` (the path of a Cargo.toml), run
 * with `$CARGO` (or `cargo`) in `cwd`, so a `.cargo/config.toml` there applies.
 */
export function cargoMetadata(manifest: string, { cwd }: { cwd?: string } = {}): CargoMetadata {
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
  const {
    workspace_root: workspaceRoot,
    target_directory: targetDirectory,
    packages,
  }: {
    workspace_root: string;
    target_directory: string;
    packages: { name: string; version: string; manifest_path: string; dependencies: { name: string; path?: string }[] }[];
  } = JSON.parse(result.stdout);
  const directories = new Map<string, string>();
  const add = (name: string, directory: string) => {
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
