import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { licenseFiles } from '../licenses.mts';
import type { PluginSource } from '../manifest.mts';

let scratch: string;
let repo: string;
let source: string;
let sourcePath: string;
const plugin: PluginSource = { id: 'p', version: '0.1.0', cargoPackage: 'p' };

beforeEach(() => {
  scratch = mkdtempSync(join(tmpdir(), 'celesta-licenses-'));
  repo = join(scratch, 'repo');
  source = join(repo, 'plugins', 'p');
  sourcePath = join(source, 'plugin.json');
  mkdirSync(source, { recursive: true });
  writeFileSync(join(repo, 'LICENSE-MIT'), 'MIT\n');
  writeFileSync(join(source, 'NOTICE'), 'notice\n');
  writeFileSync(sourcePath, '{}\n');
});

afterEach(() => {
  rmSync(scratch, { recursive: true, force: true });
});

describe('licenseFiles', () => {
  it("resolves each entry against the plugin's folder, in order, named by its base name", () => {
    expect(licenseFiles(source, { ...plugin, licenses: ['../../LICENSE-MIT', 'NOTICE'] }, sourcePath)).toEqual([
      { from: join(repo, 'LICENSE-MIT'), name: 'LICENSE-MIT' },
      { from: join(source, 'NOTICE'), name: 'NOTICE' },
    ]);
  });

  it('is empty without licenses', () => {
    expect(licenseFiles(source, plugin, sourcePath)).toEqual([]);
  });

  it.each([['missing'], ['.']])('refuses %s, which is not a file, naming plugin.json', (entry) => {
    expect(() => licenseFiles(source, { ...plugin, licenses: [entry] }, sourcePath)).toThrow(
      `${sourcePath}: license file ${entry} is not a file`,
    );
  });

  it('refuses two license files with one name', () => {
    writeFileSync(join(source, 'LICENSE-MIT'), 'MIT too\n');
    expect(() =>
      licenseFiles(source, { ...plugin, licenses: ['../../LICENSE-MIT', 'LICENSE-MIT'] }, sourcePath),
    ).toThrow(`${sourcePath}: two license files are named LICENSE-MIT`);
  });

  it('refuses two license files whose names differ only in case, which one folder cannot hold on macOS or Windows', () => {
    // In sibling folders, so a case-insensitive file system keeps both.
    mkdirSync(join(source, 'a'));
    mkdirSync(join(source, 'b'));
    writeFileSync(join(source, 'a', 'LICENSE'), 'a\n');
    writeFileSync(join(source, 'b', 'license'), 'b\n');
    expect(() => licenseFiles(source, { ...plugin, licenses: ['a/LICENSE', 'b/license'] }, sourcePath)).toThrow(
      `${sourcePath}: two license files are named license`,
    );
  });
});
