import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { test } from 'vitest';

import { addDependencies, migrateSource, movedExports } from '../../../skills/celesta/scripts/migrate-packages.mjs';

const script = fileURLToPath(new URL('../../../skills/celesta/scripts/migrate-packages.mjs', import.meta.url));

/**
 * Every name each package's declarations export, values and types, from one
 * program. Reads the packages' built dist/index.d.ts, which `tsc -b test`
 * builds (test/tsconfig.json references them).
 */
function exportsOf(pkgs: string[]): Map<string, Set<string>> {
  const files = pkgs.map((pkg) => fileURLToPath(new URL(`../../${pkg}/dist/index.d.ts`, import.meta.url)));
  const program = ts.createProgram(files, { skipLibCheck: true, noEmit: true });
  const checker = program.getTypeChecker();
  return new Map(pkgs.map((pkg, i) => {
    const source = program.getSourceFile(files[i]);
    assert.ok(source, files[i]);
    const symbol = checker.getSymbolAtLocation(source);
    assert.ok(symbol, files[i]);
    return [pkg, new Set(checker.getExportsOfModule(symbol).map((s) => s.getName()))];
  }));
}

// Only the names that left @celesta.studio/react are checked: names the packages
// gained later were never in @celesta.studio/react, so there is nothing to migrate.
// Type-checking the declarations takes a few seconds on CI runners.
test('the moved names match what the packages export', { timeout: 60_000 }, () => {
  const exported = exportsOf(['react', ...Object.keys(movedExports).map((pkg) => pkg.slice('@celesta.studio/'.length))]);
  const core = exported.get('react');
  assert.ok(core);
  for (const [pkg, names] of Object.entries(movedExports)) {
    const own = exported.get(pkg.slice('@celesta.studio/'.length));
    assert.ok(own, pkg);
    for (const name of names) {
      assert.ok(own.has(name), `${pkg} does not export ${name}`);
      assert.ok(!core.has(name), `@celesta.studio/react still exports ${name}`);
    }
  }
});

test('moved names leave @celesta.studio/react for their packages, joining existing imports', () => {
  const source = [
    "import * as React from 'react';",
    "import { Character } from '@celesta.studio/character';",
    'import {',
    '  Composition,',
    '  Circle,',
    '  type CharacterViewReference,',
    '  CharacterView as View,',
    '  TextBox,',
    "} from '@celesta.studio/react';",
    "import type { LipSyncTrack, TextStyle } from \"@celesta.studio/react\";",
    "import { Center } from '@celesta.studio/react';",
    "export { Stack } from '@celesta.studio/react';",
    '',
  ].join('\n');
  const { text, packages, warnings } = migrateSource(source);
  assert.equal(text, [
    "import * as React from 'react';",
    "import { Character, type CharacterViewReference, CharacterView as View } from '@celesta.studio/character';",
    "import { Composition } from '@celesta.studio/react';",
    "import { Circle } from '@celesta.studio/shapes';",
    "import { TextBox } from '@celesta.studio/text';",
    "import type { TextStyle } from \"@celesta.studio/react\";",
    "import type { LipSyncTrack } from \"@celesta.studio/character\";",
    "import { Center } from '@celesta.studio/layout';",
    "export { Stack } from '@celesta.studio/layout';",
    '',
  ].join('\n'));
  assert.deepEqual([...packages].sort(), ['@celesta.studio/character', '@celesta.studio/layout', '@celesta.studio/shapes', '@celesta.studio/text']);
  assert.deepEqual(warnings, []);
  assert.equal(migrateSource(text).text, text, 'migrating twice changes nothing');
});

test('long lists stay multi-line, imports inside a line stay on it, and namespaces are reported', () => {
  const long = migrateSource([
    'import {',
    '  Composition, Sequence, Text, interpolate, useCurrentFrame, useVideoConfig, Easings, spring, Group, Rect,',
    '  DialogueSeries,',
    "} from '@celesta.studio/react';",
  ].join('\n')).text;
  assert.equal(long, [
    'import {',
    '  Composition,',
    '  Sequence,',
    '  Text,',
    '  interpolate,',
    '  useCurrentFrame,',
    '  useVideoConfig,',
    '  Easings,',
    '  spring,',
    '  Group,',
    '  Rect,',
    "} from '@celesta.studio/react';",
    "import { DialogueSeries } from '@celesta.studio/character';",
  ].join('\n'));

  const inline = migrateSource("const entry = `import { Composition, Line } from '@celesta.studio/react';\\n`;").text;
  assert.equal(inline, "const entry = `import { Composition } from '@celesta.studio/react'; import { Line } from '@celesta.studio/shapes';\\n`;");

  const namespaced = migrateSource("import * as C from '@celesta.studio/react';\nC.Text; C.Camera; C.useLipSync(track);\n");
  assert.equal(namespaced.text, "import * as C from '@celesta.studio/react';\nC.Text; C.Camera; C.useLipSync(track);\n");
  assert.deepEqual(namespaced.warnings, [
    'line 1: `import * as C` uses moved names; import them from their packages: C.Camera (@celesta.studio/layout), C.useLipSync (@celesta.studio/character)',
  ]);
});

test('comments beside a name move with it', () => {
  const { text } = migrateSource([
    'import {',
    '  Composition, // root',
    '  /* drawn */ Circle,',
    '  Text, // body',
    "} from '@celesta.studio/react';",
  ].join('\n'));
  assert.equal(text, [
    "import { Composition /* root */, Text /* body */ } from '@celesta.studio/react';",
    "import { /* drawn */ Circle } from '@celesta.studio/shapes';",
  ].join('\n'));

  // `//` and `,` inside a block comment stay comment text, and a line comment
  // holding `*/` cannot close the block comment it becomes.
  const tricky = migrateSource([
    'import {',
    '  Composition, /* see https://example.com, a, b */ Circle,',
    '  Text, // ends with */ here',
    "} from '@celesta.studio/react';",
  ].join('\n')).text;
  assert.equal(tricky, [
    "import { Composition, Text /* ends with * / here */ } from '@celesta.studio/react';",
    "import { /* see https://example.com, a, b */ Circle } from '@celesta.studio/shapes';",
  ].join('\n'));
});

test('a manifest listing @celesta.studio/react gets the new packages with its range', () => {
  const manifest = '{\n  "dependencies": {\n    "@celesta.studio/react": "workspace:*",\n    "react": "^18.3.1"\n  }\n}\n';
  const { text, added } = addDependencies(manifest, new Set(['@celesta.studio/text', '@celesta.studio/shapes']));
  assert.deepEqual(added, ['@celesta.studio/shapes', '@celesta.studio/text']);
  assert.equal(text, '{\n  "dependencies": {\n    "@celesta.studio/react": "workspace:*",\n    "@celesta.studio/shapes": "workspace:*",\n    "@celesta.studio/text": "workspace:*",\n    "react": "^18.3.1"\n  }\n}\n');
  assert.deepEqual(addDependencies('{"dependencies":{"react":"^18.3.1"}}', new Set(['@celesta.studio/text'])).added, []);
});

test('the scope before @celesta.studio is renamed, in sources and manifests', () => {
  const source = [
    "import { Composition, TextReveal } from '@celesta/react';",
    'import type { Theme } from "@celesta/react";',
    "import { duckEnvelope } from '@celesta/media-utils/duck';",
    "const text = await import('@celesta/text');",
    "import { Plugin } from '@celesta-plugin/extra';",
  ].join('\n');
  const result = migrateSource(source);
  assert.equal(result.renamed, true);
  assert.deepEqual([...result.packages], ['@celesta.studio/text']);
  assert.equal(result.text, [
    "import { Composition } from '@celesta.studio/react';",
    "import { TextReveal } from '@celesta.studio/text';",
    'import type { Theme } from "@celesta.studio/react";',
    "import { duckEnvelope } from '@celesta.studio/media-utils/duck';",
    "const text = await import('@celesta.studio/text');",
    "import { Plugin } from '@celesta-plugin/extra';",
  ].join('\n'));
  assert.equal(migrateSource("import { Composition } from '@celesta.studio/react';").renamed, false);

  const manifest = '{\n  "dependencies": {\n    "@celesta/react": "workspace:*",\n    "react": "^18.3.1"\n  }\n}\n';
  const { text, added, renamed } = addDependencies(manifest, new Set(['@celesta.studio/text']));
  assert.deepEqual(renamed, ['@celesta/react']);
  assert.deepEqual(added, ['@celesta.studio/text']);
  assert.equal(text, '{\n  "dependencies": {\n    "@celesta.studio/react": "workspace:*",\n    "@celesta.studio/text": "workspace:*",\n    "react": "^18.3.1"\n  }\n}\n');
});

test('the command migrates a project, and --check reports without writing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-migrate-'));
  try {
    mkdirSync(join(dir, 'scenes'));
    mkdirSync(join(dir, 'node_modules'));
    const scene = "import { Composition, TextReveal } from '@celesta.studio/react';\n";
    writeFileSync(join(dir, 'scenes/Intro.tsx'), scene);
    writeFileSync(join(dir, 'node_modules/ignored.ts'), scene);
    writeFileSync(join(dir, 'package.json'), '{\n  "dependencies": {\n    "@celesta.studio/react": "workspace:*"\n  }\n}\n');

    const checked = spawnSync(process.execPath, [script, dir, '--check'], { encoding: 'utf8' });
    assert.equal(checked.status, 1, checked.stderr);
    assert.match(checked.stdout, /would update .*Intro\.tsx \(@celesta\.studio\/text\)/);
    assert.equal(readFileSync(join(dir, 'scenes/Intro.tsx'), 'utf8'), scene);

    const migrated = spawnSync(process.execPath, [script, dir], { encoding: 'utf8' });
    assert.equal(migrated.status, 0, migrated.stderr);
    assert.equal(readFileSync(join(dir, 'scenes/Intro.tsx'), 'utf8'),
      "import { Composition } from '@celesta.studio/react';\nimport { TextReveal } from '@celesta.studio/text';\n");
    assert.equal(readFileSync(join(dir, 'node_modules/ignored.ts'), 'utf8'), scene);
    assert.match(readFileSync(join(dir, 'package.json'), 'utf8'), /"@celesta\.studio\/text": "workspace:\*"/);

    const again = spawnSync(process.execPath, [script, dir, '--check'], { encoding: 'utf8' });
    assert.equal(again.status, 0, again.stdout);
    assert.match(again.stdout, /Nothing to migrate/);

    // A namespace import of a moved name cannot be rewritten, so it fails the check.
    writeFileSync(join(dir, 'scenes/Outro.tsx'), "import * as C from '@celesta.studio/react';\nC.Camera;\n");
    const namespaced = spawnSync(process.execPath, [script, dir, '--check'], { encoding: 'utf8' });
    assert.equal(namespaced.status, 1, namespaced.stdout);
    assert.match(namespaced.stderr, /Outro\.tsx:1: `import \* as C` uses moved names.*C\.Camera \(@celesta\.studio\/layout\)/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
