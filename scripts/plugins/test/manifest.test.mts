import { describe, expect, it } from 'vitest';

import { pluginManifest } from '../manifest.mjs';

const sourcePath = '/repo/plugins/vrm/plugin.json';
const component = { name: 'Vrm', label: 'VRM avatar', src: 'model', extensions: ['vrm'], aspect: 1.7777778 };

describe('pluginManifest', () => {
  it('takes the version of a plugin with a Cargo package from that package', () => {
    const manifest = pluginManifest(
      { id: 'vrm', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts', components: [component] },
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

  it("takes the version of a plugin without one from plugin.json, and writes no executable it doesn't have", () => {
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

  it('refuses a version in plugin.json beside a Cargo package', () => {
    expect(() =>
      pluginManifest(
        { id: 'vrm', version: '0.1.0', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' },
        { cargoVersion: '0.1.0' },
        sourcePath,
      ),
    ).toThrow(`${sourcePath}: version comes from the Cargo package celesta-plugin-vrm; remove it`);
  });

  it("throws when the Cargo package's version is missing", () => {
    expect(() =>
      pluginManifest({ id: 'vrm', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' }, {}, sourcePath),
    ).toThrow(/celesta-plugin-vrm/);
  });

  it('throws, naming plugin.json, when a plugin without a Cargo package has no version', () => {
    expect(() => pluginManifest({ id: 'beep-voice', module: 'js/index.ts' }, {}, sourcePath)).toThrow(sourcePath);
  });
});
