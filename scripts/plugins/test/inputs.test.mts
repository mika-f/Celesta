import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  type Affected,
  affectedPlugins,
  covers,
  githubOutputs,
  type PluginInputs,
  repositoryPath,
  repositoryPlugins,
  SHARED_INPUTS,
  turboInputs,
} from '../inputs.mts';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

describe('covers', () => {
  it('matches a folder and the paths under it, not a sibling sharing its prefix', () => {
    expect(covers('crates/plugin', 'crates/plugin/src/lib.rs')).toBe(true);
    expect(covers('crates/plugin', 'crates/plugin')).toBe(true);
    expect(covers('crates/plugin', 'crates/plugin-host/src/lib.rs')).toBe(false);
    expect(covers('plugins/vrm', 'plugins/vrm/plugin.json')).toBe(true);
    expect(covers('plugins/vrm', 'plugins/vrm-extra/a')).toBe(false);
  });

  it('matches a file only itself', () => {
    expect(covers('LICENSE-MIT', 'LICENSE-MIT')).toBe(true);
    expect(covers('LICENSE-MIT', 'LICENSE-MIT.txt')).toBe(false);
    expect(covers('LICENSE-MIT', 'LICENSE')).toBe(false);
  });
});

describe('repositoryPath', () => {
  it('gives a path under the root relative to it, with / separators', () => {
    expect(repositoryPath(root, join(root, 'crates', 'plugin'))).toBe('crates/plugin');
  });

  it('refuses a path outside the root', () => {
    const outside = join(root, '..', 'elsewhere');
    expect(() => repositoryPath(root, outside)).toThrow(`${outside} is outside the repository ${root}`);
  });
});

describe('affectedPlugins', () => {
  const plugins: PluginInputs[] = [
    {
      id: 'a',
      directory: 'plugins/a',
      workspace: 'plugins',
      cargoPackage: 'a',
      jsPackage: null,
      inputs: ['crates/x', 'plugins/a'],
    },
    {
      id: 'b',
      directory: 'plugins/b',
      workspace: 'plugins',
      cargoPackage: 'b',
      jsPackage: null,
      inputs: ['LICENSE-GPL-2.0', 'crates/y', 'plugins/b'],
    },
  ];
  const shared = ['Cargo.lock', 'scripts/plugins'];
  const ids = (affected: Affected[]) => affected.map(({ id, because }) => [id, because]);

  it("affects a plugin whose inputs cover a changed path, because of that path", () => {
    expect(ids(affectedPlugins(plugins, ['crates/x/src/lib.rs'], shared))).toEqual([['a', 'crates/x/src/lib.rs']]);
  });

  it('affects a plugin through a deleted path', () => {
    expect(ids(affectedPlugins(plugins, ['crates/y/gone.rs'], shared))).toEqual([['b', 'crates/y/gone.rs']]);
  });

  it('affects a plugin through a license file it lists', () => {
    expect(ids(affectedPlugins(plugins, ['LICENSE-GPL-2.0'], shared))).toEqual([['b', 'LICENSE-GPL-2.0']]);
  });

  it('affects every plugin through a shared input', () => {
    expect(ids(affectedPlugins(plugins, ['docs/a.md', 'scripts/plugins/build.mts'], shared))).toEqual([
      ['a', 'scripts/plugins/build.mts'],
      ['b', 'scripts/plugins/build.mts'],
    ]);
  });

  it('affects nothing without changes', () => {
    expect(affectedPlugins(plugins, [], shared)).toEqual([]);
  });

  it('affects every plugin without a base revision', () => {
    expect(ids(affectedPlugins(plugins, null, shared))).toEqual([
      ['a', 'no base revision'],
      ['b', 'no base revision'],
    ]);
  });

  it('gives each plugin without its inputs', () => {
    const affected = affectedPlugins(plugins, null, shared);
    expect(affected[0]).toEqual({
      id: 'a',
      directory: 'plugins/a',
      workspace: 'plugins',
      cargoPackage: 'a',
      jsPackage: null,
      because: 'no base revision',
    });
    for (const entry of affected) expect('inputs' in entry).toBe(false);
  });
});

describe('githubOutputs', () => {
  it('says nothing is affected', () => {
    expect(githubOutputs([])).toBe(
      'any=false\nplugins=[]\ndirectories=\njs=\ncargo<<CARGO_WORKSPACES\nCARGO_WORKSPACES\n',
    );
  });

  it("gives the affected plugins' folders, JavaScript filters, and Cargo packages by workspace", () => {
    const affected: Affected[] = [
      {
        id: 'vrm',
        directory: 'plugins/vrm',
        workspace: 'plugins',
        cargoPackage: 'celesta-plugin-vrm',
        jsPackage: 'celesta-plugin-vrm-js',
        because: 'x',
      },
      {
        id: 'ffmpeg',
        directory: 'plugins/ffmpeg',
        workspace: 'plugins',
        cargoPackage: 'celesta-codec-ffmpeg',
        jsPackage: null,
        because: 'x',
      },
      {
        id: 'solo',
        directory: 'plugins/solo',
        workspace: 'plugins/solo',
        cargoPackage: 'celesta-plugin-solo',
        jsPackage: null,
        because: 'x',
      },
    ];
    const byId = [...affected].sort((a, b) => a.id.localeCompare(b.id));
    // The workflow reads the cargo lines with `read -r`; a `\r` would end up in a package name.
    expect(githubOutputs(affected)).not.toContain('\r');
    expect(githubOutputs(affected)).toBe(
      [
        'any=true',
        `plugins=${JSON.stringify(byId)}`,
        'directories=plugins/ffmpeg plugins/solo plugins/vrm',
        'js=--filter celesta-plugin-vrm-js',
        'cargo<<CARGO_WORKSPACES',
        'plugins -p celesta-codec-ffmpeg -p celesta-plugin-vrm',
        'plugins/solo -p celesta-plugin-solo',
        'CARGO_WORKSPACES',
        '',
      ].join('\n'),
    );
  });
});

describe('turboInputs', () => {
  const lib = { name: 'lib', scripts: ['codegen', 'build'] };

  it("gives the repository paths of the tasks a package's scripts run", () => {
    const turbo = {
      tasks: {
        build: { dependsOn: ['^build'], outputs: ['dist/**'] },
        codegen: {
          inputs: ['$TURBO_ROOT$/Cargo.lock', '$TURBO_ROOT$/crates/project/**', 'src/**', '!src/generated/**'],
        },
        'lib#build': {
          inputs: ['$TURBO_DEFAULT$', '!$TURBO_ROOT$/crates/project/README.md', '$TURBO_ROOT$/gen/**'],
        },
        'other#build': { inputs: ['$TURBO_ROOT$/other/**'] },
        lint: { inputs: ['$TURBO_ROOT$/lint/**'] },
      },
    };
    expect(turboInputs(turbo, [lib], 'turbo.json')).toEqual(['Cargo.lock', 'crates/project', 'gen']);
  });

  it('reads the globs of a deferred-hashing entry', () => {
    const turbo = { tasks: { codegen: { inputs: [{ mode: 'jit', globs: ['$TURBO_ROOT$/schema/**'] }] } } };
    expect(turboInputs(turbo, [lib], 'turbo.json')).toEqual(['schema']);
  });

  it('gives the global dependencies, which every task hashes', () => {
    const turbo = { globalDependencies: ['tsconfig.base.json', 'config/**'], tasks: {} };
    expect(turboInputs(turbo, [{ name: 'bare', scripts: [] }], 'turbo.json')).toEqual(['config', 'tsconfig.base.json']);
  });

  it('gives nothing without a turbo.json', () => {
    expect(turboInputs(null, [lib], 'turbo.json')).toEqual([]);
  });

  it.each([
    ['$TURBO_ROOT$/crates/*/src/**', 'a glob before its last part'],
    ['$TURBO_ROOT$/crates/x/**/*.rs', 'a glob after **'],
    ['$TURBO_ROOT$/**', 'the whole repository'],
    ['$TURBO_ROOT$/*.toml', 'a glob at the root'],
    ['$TURBO_ROOT$/../outside/**', 'a path outside the repository'],
    ['$TURBO_ROOT$/crates/x/', 'a trailing /'],
    ['src/$TURBO_ROOT$/x', '$TURBO_ROOT$ not at the start'],
    ['../../crates/x/**', 'a package path outside the package'],
  ])('refuses %s (%s)', (pattern) => {
    const turbo = { tasks: { codegen: { inputs: [pattern] } } };
    expect(() => turboInputs(turbo, [lib], 'turbo.json')).toThrow(
      `turbo.json: tasks["codegen"].inputs has ${pattern}, which names no repository folder or file`,
    );
  });

  it.each(['$TURBO_ROOT$/tsconfig.json', 'config/*.json', '../shared.json'])(
    'refuses the global dependency %s, which names no repository folder or file',
    (pattern) => {
      expect(() => turboInputs({ globalDependencies: [pattern] }, [lib], 'turbo.json')).toThrow(
        `turbo.json: globalDependencies has ${pattern}, which names no repository folder or file`,
      );
    },
  );

  it("refuses a dependency on another package's task outside the packages", () => {
    const turbo = { tasks: { codegen: { dependsOn: ['^build', 'codegen', 'lib#build', 'schema#generate'] } } };
    expect(() => turboInputs(turbo, [lib], 'turbo.json')).toThrow(
      'turbo.json: tasks["codegen"].dependsOn has schema#generate, whose package is not among the plugin\'s packages',
    );
  });

  it("accepts a dependency on a task of one of the packages", () => {
    const turbo = { tasks: { codegen: { dependsOn: ['lib#build'] }, 'lib#build': { inputs: ['$TURBO_ROOT$/gen/**'] } } };
    expect(turboInputs(turbo, [lib], 'turbo.json')).toEqual(['gen']);
  });

  it('refuses an input it cannot read', () => {
    const turbo = { tasks: { codegen: { inputs: [{ mode: 'jit' }] } } };
    expect(() => turboInputs(turbo, [lib], 'turbo.json')).toThrow(
      'turbo.json: tasks["codegen"].inputs has {"mode":"jit"}, which is neither a glob nor an entry with globs',
    );
  });
});

describe('repositoryPlugins in a scratch tree', () => {
  let tree: string;

  function file(path: string, text: string) {
    mkdirSync(dirname(join(tree, path)), { recursive: true });
    writeFileSync(join(tree, path), text);
  }

  function crate(folder: string, name: string, { dependencies = '', workspace = false } = {}) {
    file(
      `${folder}/Cargo.toml`,
      `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2021"\n\n` +
        `${workspace ? '[workspace]\nresolver = "2"\n\n' : ''}[dependencies]\n${dependencies}`,
    );
    file(`${folder}/src/lib.rs`, '');
  }

  function plugin(id: string, extra: Record<string, unknown> = {}) {
    file(`plugins/${id}/plugin.json`, JSON.stringify({ id, version: '0.1.0', cargoPackage: id, ...extra }));
  }

  // cargoMetadata passes --locked.
  function lock(...folders: string[]) {
    for (const folder of folders) {
      const result = spawnSync(process.env.CARGO ?? 'cargo', ['generate-lockfile', '--offline'], {
        cwd: join(tree, folder),
        encoding: 'utf8',
      });
      if (result.status !== 0) throw new Error(`cargo generate-lockfile failed in ${folder}: ${result.stderr}`);
    }
  }

  beforeEach(() => {
    // macOS's temporary folder is a symlink; Cargo gives the real path.
    tree = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-inputs-')));
  });

  afterEach(() => {
    rmSync(tree, { recursive: true, force: true });
  });

  function excludedPluginTree() {
    file('Cargo.toml', '[workspace]\nresolver = "2"\nmembers = ["crates/x"]\nexclude = ["plugins"]\n');
    crate('crates/x', 'x');
    file('plugins/Cargo.toml', '[workspace]\nresolver = "2"\nmembers = ["a"]\n');
    crate('plugins/a', 'a');
    plugin('a');
    crate('plugins/b', 'b', { dependencies: 'x = { path = "../../crates/x" }\n', workspace: true });
    plugin('b', { licenses: ['../../NOTICE'] });
    file('NOTICE', 'notice\n');
    lock('.', 'plugins', 'plugins/b');
  }

  it('an excluded plugin reads its own Cargo.lock', () => {
    excludedPluginTree();
    expect(repositoryPlugins(tree)).toEqual([
      { id: 'a', directory: 'plugins/a', workspace: 'plugins', cargoPackage: 'a', jsPackage: null, inputs: ['plugins/a'] },
      {
        id: 'b',
        directory: 'plugins/b',
        workspace: 'plugins/b',
        cargoPackage: 'b',
        jsPackage: null,
        inputs: ['NOTICE', 'crates/x', 'plugins/b'],
      },
    ]);
  }, 60_000);

  it('a module without a js/package.json is an error', () => {
    excludedPluginTree();
    plugin('a', { module: 'js/index.ts' });
    expect(() => repositoryPlugins(tree)).toThrow(
      'plugins/a/plugin.json: its module needs a js/package.json that declares what the module uses',
    );
  }, 60_000);

  it('a local package without a folder is an error', () => {
    file('Cargo.toml', '[workspace]\nresolver = "2"\nmembers = ["crates/x"]\nexclude = ["plugins", "vendor"]\n');
    crate('crates/x', 'x');
    file('plugins/Cargo.toml', '[workspace]\nresolver = "2"\nmembers = ["a"]\n');
    crate('plugins/a', 'a', { dependencies: 'm = { path = "../../vendor/m" }\n' });
    plugin('a');
    crate('vendor/m', 'm', { dependencies: 'ghost = { path = "../ghost" }\n' });
    crate('vendor/ghost', 'ghost');
    lock('.', 'plugins');
    expect(() => repositoryPlugins(tree)).toThrow('plugins/Cargo.lock: no folder is known for the local package ghost');
  }, 60_000);
});

// Before any plugin's Cargo package or pnpm workspace is read.
describe('repositoryPlugins in a tree without workspaces', () => {
  let tree: string;

  function file(path: string, text: string) {
    mkdirSync(dirname(join(tree, path)), { recursive: true });
    writeFileSync(join(tree, path), text);
  }

  beforeEach(() => {
    tree = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-inputs-')));
  });

  afterEach(() => {
    rmSync(tree, { recursive: true, force: true });
  });

  it('passes over dist, target, and files', () => {
    mkdirSync(join(tree, 'plugins', 'dist', 'vrm'), { recursive: true });
    mkdirSync(join(tree, 'plugins', 'target', 'debug'), { recursive: true });
    file('plugins/Cargo.toml', '[workspace]\n');
    file('plugins/README.md', 'plugins\n');
    expect(repositoryPlugins(tree)).toEqual([]);
  });

  it('refuses a folder without a plugin.json', () => {
    file('plugins/stray/Cargo.toml', '');
    expect(() => repositoryPlugins(tree)).toThrow(
      'plugins/stray: no plugin.json; every folder under plugins/ but dist and target is a plugin',
    );
  });

  it('names a plugin.json it cannot parse', () => {
    file('plugins/a/plugin.json', '{');
    expect(() => repositoryPlugins(tree)).toThrow(/^plugins\/a\/plugin\.json: /);
  });

  it('names a js/package.json it cannot parse', () => {
    file('plugins/a/plugin.json', JSON.stringify({ id: 'a', version: '0.1.0', module: 'js/index.ts' }));
    file('plugins/a/js/package.json', '{');
    expect(() => repositoryPlugins(tree)).toThrow(/^plugins\/a\/js\/package\.json: /);
  });

  it('refuses a js/package.json without a test script, which pnpm --filter would pass over', () => {
    file('plugins/a/plugin.json', JSON.stringify({ id: 'a', version: '0.1.0', module: 'js/index.ts' }));
    file('plugins/a/js/package.json', JSON.stringify({ name: 'a-js', scripts: { build: 'x' } }));
    expect(() => repositoryPlugins(tree)).toThrow(
      'plugins/a/js/package.json: no "test" script, so `pnpm --filter a-js test` would test nothing',
    );
  });
});

describe('repositoryPlugins in a scratch pnpm workspace', () => {
  let tree: string;

  function file(path: string, text: string) {
    mkdirSync(dirname(join(tree, path)), { recursive: true });
    writeFileSync(join(tree, path), text);
  }

  beforeEach(() => {
    // Inside the repository, where mise finds the pnpm that mise.toml pins.
    const scratch = join(root, 'scripts', 'plugins', 'node_modules', '.cache');
    mkdirSync(scratch, { recursive: true });
    tree = realpathSync(mkdtempSync(join(scratch, 'celesta-inputs-')));
    file('pnpm-workspace.yaml', 'packages:\n  - packages/*\n  - plugins/*/js\n');
    file('package.json', JSON.stringify({ name: 'root', private: true }));
    file('plugins/a/plugin.json', JSON.stringify({ id: 'a', version: '0.1.0', module: 'js/index.ts' }));
    file(
      'plugins/a/js/package.json',
      JSON.stringify({ name: 'a-js', scripts: { test: 'x' }, devDependencies: { lib: 'workspace:*' } }),
    );
    file('packages/lib/package.json', JSON.stringify({ name: 'lib', scripts: { codegen: 'x', build: 'y' } }));
    file('packages/other/package.json', JSON.stringify({ name: 'other', scripts: { build: 'y' } }));
    file(
      'turbo.json',
      JSON.stringify({
        tasks: {
          codegen: { inputs: ['$TURBO_ROOT$/Cargo.lock', '$TURBO_ROOT$/crates/project/**'] },
          'lib#build': { dependsOn: ['codegen'], inputs: ['$TURBO_DEFAULT$', '$TURBO_ROOT$/gen/**'] },
          'other#build': { inputs: ['$TURBO_ROOT$/other/**'] },
        },
      }),
    );
  });

  afterEach(() => {
    rmSync(tree, { recursive: true, force: true });
  });

  it("adds the turbo.json root inputs of the tasks the plugin's packages run", () => {
    expect(repositoryPlugins(tree)).toEqual([
      {
        id: 'a',
        directory: 'plugins/a',
        workspace: null,
        cargoPackage: null,
        jsPackage: 'a-js',
        inputs: ['Cargo.lock', 'crates/project', 'gen', 'packages/lib', 'plugins/a', 'plugins/a/js'],
      },
    ]);
  }, 60_000);

  it("refuses a package's own turbo.json, whose inputs it does not read", () => {
    file('packages/lib/turbo.json', JSON.stringify({ extends: ['//'], tasks: {} }));
    expect(() => repositoryPlugins(tree)).toThrow(
      'packages/lib/turbo.json: a package configuration, whose task inputs are not read',
    );
  }, 60_000);

  it('refuses a turbo.jsonc, which it does not read', () => {
    file('turbo.jsonc', '{}');
    expect(() => repositoryPlugins(tree)).toThrow('turbo.jsonc: not read; use turbo.json');
  }, 60_000);
});

describe("the repository's plugins", () => {
  it('are found with their inputs', () => {
    const plugins = repositoryPlugins(root);
    expect(plugins.map(({ id }) => id)).toEqual(['ffmpeg', 'vrm']);
    const [ffmpeg, vrm] = plugins;

    expect(vrm).toMatchObject({
      directory: 'plugins/vrm',
      workspace: 'plugins',
      cargoPackage: 'celesta-plugin-vrm',
      jsPackage: 'celesta-plugin-vrm-js',
    });
    expect(vrm.inputs).toEqual(
      expect.arrayContaining([
        'plugins/vrm',
        'crates/plugin',
        'crates/composition',
        'crates/gpu-renderer',
        'crates/plugin-host',
        // From turbo.json: the codegen that @celesta.studio/react runs.
        'crates/project',
        'packages/cli',
        'packages/react',
        'packages/character',
        // From turbo.json: what @celesta.studio/cli's build reads outside its folder.
        'scripts/resolve-package-manifest.mjs',
        'templates',
      ]),
    );
    for (const input of ['crates/exporter', 'crates/editor', 'plugins/ffmpeg']) expect(vrm.inputs).not.toContain(input);

    expect(ffmpeg).toMatchObject({
      directory: 'plugins/ffmpeg',
      workspace: 'plugins',
      cargoPackage: 'celesta-codec-ffmpeg',
      jsPackage: null,
    });
    expect(ffmpeg.inputs).toEqual(
      expect.arrayContaining([
        'plugins/ffmpeg',
        'crates/media',
        'crates/exporter',
        'LICENSE-GPL-2.0',
        'packaging/GPL-SOURCE-OFFER.md',
      ]),
    );
    for (const input of ['packages/cli', 'crates/editor', 'templates']) expect(ffmpeg.inputs).not.toContain(input);

    for (const { inputs } of plugins) {
      expect(inputs).toEqual([...new Set(inputs)].sort());
      for (const input of inputs) expect(input).not.toContain('\\');
    }
  }, 60_000);

  it.each(SHARED_INPUTS)('share %s, which exists', (input) => {
    expect(existsSync(join(root, input))).toBe(true);
  });
});
