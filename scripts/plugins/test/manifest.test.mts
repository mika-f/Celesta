import { describe, expect, it } from 'vitest';

import { celestaRange, checkPluginSource, pluginManifest, pluginVersion } from '../manifest.mts';
import type { PluginSource } from '../manifest.mts';

const sourcePath = '/repo/plugins/vrm/plugin.json';
const celestaVersion = '0.9.0';
const celesta = '>=0.9.0, <0.10.0';
const component = { name: 'Vrm', label: 'VRM avatar', src: 'model', extensions: ['vrm'], aspect: 1.7777778 };

describe('pluginManifest', () => {
  const media = { decoders: [{ id: 'ffmpeg', name: 'FFmpeg', extensions: ['mp4'] }] };

  it('copies media and speaks protocol 2', () => {
    const manifest = pluginManifest(
      { id: 'ffmpeg', version: '0.9.0', cargoPackage: 'celesta-codec-ffmpeg', roles: ['media'], media },
      { celestaVersion, cargoVersion: '0.9.0', executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-codec-ffmpeg' } },
      sourcePath,
    );
    expect(manifest.protocol).toBe(2);
    expect(manifest.media).toEqual(media);
    expect(manifest.roles).toEqual(['media']);
  });

  it('keeps protocol 1 for content', () => {
    const content = pluginManifest({ id: 'vrm', version: '0.1.0', roles: ['content'], module: 'js/index.ts' }, { celestaVersion }, sourcePath);
    const rolesless = pluginManifest({ id: 'vrm', version: '0.1.0', module: 'js/index.ts' }, { celestaVersion }, sourcePath);
    expect(content.protocol).toBe(1);
    expect(rolesless.protocol).toBe(1);
  });

  it('rejects content and media together', () => {
    expect(() =>
      pluginManifest({ id: 'x', version: '0.1.0', module: 'js/index.ts', roles: ['content', 'media'], media }, { celestaVersion }, sourcePath),
    ).toThrow(`${sourcePath}: a plugin is either content or media`);
  });

  it('rejects the reserved id builtin', () => {
    expect(() => checkPluginSource({ id: 'builtin', version: '0.1.0', module: 'js/index.ts' }, sourcePath)).toThrow(/reserved/);
  });

  it("writes the version of a plugin with a Cargo package when it is the package's", () => {
    const manifest = pluginManifest(
      {
        id: 'vrm',
        version: '0.1.0',
        cargoPackage: 'celesta-plugin-vrm',
        module: 'js/src/index.ts',
        components: [component],
      },
      { celestaVersion, cargoVersion: '0.1.0', executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-plugin-vrm' } },
      sourcePath,
    );
    expect(manifest).toEqual({
      id: 'vrm',
      version: '0.1.0',
      protocol: 1,
      celesta,
      executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-plugin-vrm' },
      module: 'js/index.js',
      types: 'js/index.d.ts',
      components: [component],
    });
    expect(Object.keys(manifest)).toEqual(['id', 'version', 'protocol', 'celesta', 'executable', 'module', 'types', 'components']);
  });

  it("writes the version of a plugin without a Cargo package, and no executable it doesn't have", () => {
    const manifest = pluginManifest({ id: 'beep-voice', version: '0.1.0', module: 'js/index.ts' }, { celestaVersion }, sourcePath);
    expect(manifest).toEqual({
      id: 'beep-voice',
      version: '0.1.0',
      protocol: 1,
      celesta,
      module: 'js/index.js',
      types: 'js/index.d.ts',
    });
    expect(Object.keys(manifest)).toEqual(['id', 'version', 'protocol', 'celesta', 'module', 'types']);
  });

  it("refuses a version in plugin.json that differs from the Cargo package's", () => {
    expect(() =>
      pluginManifest(
        { id: 'vrm', version: '0.1.0', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' },
        { celestaVersion, cargoVersion: '0.2.0' },
        sourcePath,
      ),
    ).toThrow(`${sourcePath}: version 0.1.0 differs from the Cargo package celesta-plugin-vrm's 0.2.0`);
  });

  it("throws when the Cargo package's version is missing", () => {
    expect(() =>
      pluginManifest(
        { id: 'vrm', version: '0.1.0', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' },
        { celestaVersion },
        sourcePath,
      ),
    ).toThrow(`${sourcePath}: Cargo gave no version for the package celesta-plugin-vrm`);
  });

  it("writes a codec plugin's roles and no module or types", () => {
    const manifest = pluginManifest(
      {
        id: 'ffmpeg',
        version: '0.9.0',
        cargoPackage: 'celesta-codec-ffmpeg',
        roles: ['codec'],
        licenses: ['../../LICENSE-MIT'],
      },
      { celestaVersion, cargoVersion: '0.9.0', executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-codec-ffmpeg' } },
      sourcePath,
    );
    expect(manifest).toEqual({
      id: 'ffmpeg',
      version: '0.9.0',
      protocol: 1,
      celesta,
      roles: ['codec'],
      executable: { 'macos-aarch64': 'bin/macos-aarch64/celesta-codec-ffmpeg' },
    });
    expect(Object.keys(manifest)).toEqual(['id', 'version', 'protocol', 'celesta', 'roles', 'executable']);
  });

  it.each([
    ['without', { id: 'beep-voice', module: 'js/index.ts' }, { celestaVersion }],
    ['with', { id: 'vrm', cargoPackage: 'celesta-plugin-vrm', module: 'js/src/index.ts' }, { celestaVersion, cargoVersion: '0.1.0' }],
  ])('throws, naming plugin.json, when a plugin %s a Cargo package has no version', (_, source, built) => {
    expect(() => pluginManifest(source, built, sourcePath)).toThrow(`${sourcePath}: version is required`);
  });
});

describe('checkPluginSource', () => {
  it('refuses components without a module', () => {
    expect(() =>
      checkPluginSource({ id: 'x', version: '0.1.0', cargoPackage: 'p', components: [component] }, sourcePath),
    ).toThrow(`${sourcePath}: components need a module to import them from`);
  });

  it('accepts an empty components list without a module, as Celesta does', () => {
    expect(() => checkPluginSource({ id: 'x', version: '0.1.0', cargoPackage: 'p', components: [] }, sourcePath)).not.toThrow();
  });

  it.each([
    ['no id', { version: '0.1.0', module: 'js/index.ts' }],
    ['an empty id', { id: '', version: '0.1.0', module: 'js/index.ts' }],
    ['an id that is not a string', { id: 7, version: '0.1.0', module: 'js/index.ts' }],
  ])('refuses a plugin with %s, naming plugin.json', (_, source) => {
    expect(() => checkPluginSource(source as unknown as PluginSource, sourcePath)).toThrow(
      `${sourcePath}: id must be a non-empty string`,
    );
  });

  it.each(['vrm', 'ffmpeg', 'beep-voice', '7z', 'a.b_c-d'])('accepts the id %s, as Celesta does', (id) => {
    expect(() => checkPluginSource({ id, version: '0.1.0', module: 'js/index.ts' }, sourcePath)).not.toThrow();
  });

  it.each(['..', '.', 'a/b', 'a\\b', 'Upper', '-x', '_x', '.x', 'a b', 'é'])(
    'refuses the id %s, which Celesta refuses, naming plugin.json and the rule',
    (id) => {
      expect(() => checkPluginSource({ id, version: '0.1.0', module: 'js/index.ts' }, sourcePath)).toThrow(
        `${sourcePath}: id ${JSON.stringify(id)} may have only lowercase ASCII letters, digits, -, _, and ., starting with a letter or digit`,
      );
    },
  );

  it.each([
    ['an object', { name: 'Vrm' }],
    ['a string', 'Vrm'],
    ['null', null],
    ['a list holding null', [null]],
    ['a list holding a list', [[component]]],
    ['a list holding a string', ['Vrm']],
  ])('refuses components that are %s, naming plugin.json', (_, components) => {
    const source = { id: 'x', version: '0.1.0', module: 'js/index.ts', components };
    expect(() => checkPluginSource(source as unknown as PluginSource, sourcePath)).toThrow(
      `${sourcePath}: components must be a list of objects`,
    );
  });

  it('refuses a plugin with nothing to build', () => {
    expect(() => checkPluginSource({ id: 'x', version: '0.1.0' }, sourcePath)).toThrow(
      `${sourcePath}: it has neither a module nor a cargoPackage to build`,
    );
  });

  it('accepts every key plugin.json may have', () => {
    expect(() =>
      checkPluginSource(
        {
          id: 'x',
          version: '0.1.0',
          cargoPackage: 'p',
          module: 'js/index.ts',
          roles: ['content'],
          licenses: ['LICENSE'],
          components: [component],
        },
        sourcePath,
      ),
    ).not.toThrow();
  });

  it('refuses a key plugin.json may not have, listing the keys it may', () => {
    const source = { id: 'x', version: '0.1.0', module: 'js/index.ts', modules: 'js/index.ts', exe: 'x' };
    expect(() => checkPluginSource(source, sourcePath)).toThrow(
      `${sourcePath}: unknown keys modules, exe; plugin.json may have id, version, cargoPackage, module, roles, licenses, components, media`,
    );
  });
});

describe('pluginVersion', () => {
  it('checks the version alone, before anything is built', () => {
    expect(pluginVersion({ version: '0.1.0', cargoPackage: 'celesta-plugin-vrm' }, '0.1.0', sourcePath)).toBe('0.1.0');
    expect(() => pluginVersion({ version: '0.1.0', cargoPackage: 'celesta-plugin-vrm' }, '0.1.1', sourcePath)).toThrow(
      /0\.1\.0 differs .* 0\.1\.1/,
    );
  });

  it.each([
    ['a number', 1],
    ['an empty string', ''],
  ])('refuses a version that is %s in a plugin without a Cargo package', (_, version) => {
    expect(() => pluginVersion({ version: version as string }, undefined, sourcePath)).toThrow(
      `${sourcePath}: version must be a non-empty string`,
    );
  });
});

describe('celestaRange', () => {
  it.each([
    ['0.9.0', '>=0.9.0, <0.10.0'],
    ['0.9.3', '>=0.9.3, <0.10.0'],
    ['0.10.0-beta.1', '>=0.10.0-beta.1, <0.11.0'],
    ['1.2.3', '>=1.2.3, <2.0.0'],
    ['0.9.0+build.5', '>=0.9.0, <0.10.0'],
    ['0.0.3', '>=0.0.3, <0.0.4'],
    ['0.10.0-rc.0', '>=0.10.0-rc.0, <0.11.0'],
  ])('ranges Celesta %s as Celesta suggests: %s', (version, range) => {
    expect(celestaRange(version)).toBe(range);
  });

  it.each(['0.9', 'v0.9.0', '0.09.0', '', '0.9.0-01', '0.9.0-alpha..1', '0.9.0-', '0.9.0+', '0.9.0+a..b'])('refuses %j, which is not a semver version', (version) => {
    expect(() => celestaRange(version)).toThrow(`Celesta's version ${JSON.stringify(version)} is not a semver version`);
  });
});
