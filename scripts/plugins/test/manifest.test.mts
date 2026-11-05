import { describe, expect, it } from 'vitest';

import { pluginManifest, pluginVersion } from '../manifest.mjs';

const sourcePath = '/repo/plugins/vrm/plugin.json';
const component = { name: 'Vrm', label: 'VRM avatar', src: 'model', extensions: ['vrm'], aspect: 1.7777778 };

describe('pluginManifest', () => {
  it("writes the version of a plugin with a Cargo package when it is the package's", () => {
    const manifest = pluginManifest(
      {
        id: 'vrm',
        version: '0.1.0',
        cargoPackage: 'celesta-plugin-vrm',
        module: 'js/src/index.ts',
        components: [component],
      },
      { cargoVersion: '0.1.0', executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-plugin-vrm' } },
      sourcePath,
    );
    expect(manifest).toEqual({
      id: 'vrm',
      version: '0.1.0',
      protocol: 1,
      executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-plugin-vrm' },
      module: 'js/index.js',
      types: 'js/index.d.ts',
      components: [component],
    });
    expect(Object.keys(manifest)).toEqual(['id', 'version', 'protocol', 'executable', 'module', 'types', 'components']);
  });

  it("writes the version of a plugin without a Cargo package, and no executable it doesn't have", () => {
    const manifest = pluginManifest({ id: 'beep-voice', version: '0.1.0', module: 'js/index.ts' }, {}, sourcePath);
    expect(manifest).toEqual({
      id: 'beep-voice',
      version: '0.1.0',
      protocol: 1,
      module: 'js/index.js',
      types: 'js/index.d.ts',
    });
    expect(Object.keys(manifest)).toEqual(['id', 'version', 'protocol', 'module', 'types']);
  });

  it("refuses a version in plugin.json that differs from the Cargo package's", () => {
    expect(() =>
      pluginManifest(
        { id: 'vrm', version: '0.1.0', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' },
        { cargoVersion: '0.2.0' },
        sourcePath,
      ),
    ).toThrow(`${sourcePath}: version 0.1.0 differs from the Cargo package celesta-plugin-vrm's 0.2.0`);
  });

  it("throws when the Cargo package's version is missing", () => {
    expect(() =>
      pluginManifest(
        { id: 'vrm', version: '0.1.0', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' },
        {},
        sourcePath,
      ),
    ).toThrow(`${sourcePath}: Cargo gave no version for the package celesta-plugin-vrm`);
  });

  it.each([
    ['without', { id: 'beep-voice', module: 'js/index.ts' }, {}],
    ['with', { id: 'vrm', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' }, { cargoVersion: '0.1.0' }],
  ])('throws, naming plugin.json, when a plugin %s a Cargo package has no version', (_, source, built) => {
    expect(() => pluginManifest(source, built, sourcePath)).toThrow(`${sourcePath}: version is required`);
  });
});

describe('pluginVersion', () => {
  it('checks the version alone, before anything is built', () => {
    expect(pluginVersion({ version: '0.1.0', cargoPackage: 'celesta-plugin-vrm' }, '0.1.0', sourcePath)).toBe('0.1.0');
    expect(() => pluginVersion({ version: '0.1.0', cargoPackage: 'celesta-plugin-vrm' }, '0.1.1', sourcePath)).toThrow(
      /0\.1\.0 differs .* 0\.1\.1/,
    );
  });
});
