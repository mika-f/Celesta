import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { cargoMetadata } from '../cargo.mjs';

let tree: string;
let workspace: string;

function crate(folder: string, name: string, dependencies = '') {
  mkdirSync(join(tree, folder, 'src'), { recursive: true });
  writeFileSync(
    join(tree, folder, 'Cargo.toml'),
    `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\n${dependencies}`,
  );
  writeFileSync(join(tree, folder, 'src', 'lib.rs'), '');
}

beforeEach(() => {
  // macOS's temporary folder is a symlink; Cargo gives the real path.
  tree = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-cargo-')));
  // The path dependency lies outside the workspace's folder, as `crates/*`
  // lies outside `plugins/`; inside it, Cargo would make it a member.
  workspace = join(tree, 'ws');
  mkdirSync(workspace);
  writeFileSync(join(workspace, 'Cargo.toml'), '[workspace]\nresolver = "2"\nmembers = ["crates/x"]\n');
  crate(join('ws', 'crates', 'x'), 'x', 'y = { path = "../../../y" }\n');
  crate('y', 'y');
  // cargoMetadata passes --locked.
  const lock = spawnSync(process.env.CARGO ?? 'cargo', ['generate-lockfile', '--offline'], {
    cwd: workspace,
    encoding: 'utf8',
  });
  if (lock.status !== 0) throw new Error(`cargo generate-lockfile failed: ${lock.stderr}`);
});

afterEach(() => {
  rmSync(tree, { recursive: true, force: true });
});

describe('cargoMetadata', () => {
  it("gives the workspace's root and each member's and path dependency's folder", () => {
    const { workspaceRoot, directories, versions } = cargoMetadata(join(workspace, 'Cargo.toml'), { cwd: workspace });
    expect(workspaceRoot).toBe(workspace);
    // y is no member, so only its path dependency gives its folder.
    expect([...versions.keys()]).toEqual(['x']);
    expect(directories).toEqual(
      new Map([
        ['x', join(workspace, 'crates', 'x')],
        ['y', join(tree, 'y')],
      ]),
    );
  }, 60_000);
});
