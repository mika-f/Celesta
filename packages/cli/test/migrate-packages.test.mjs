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

/** Every name each package's declarations export, values and types, from one program. */
function exportsOf(pkgs) {
  const files = pkgs.map((pkg) => fileURLToPath(new URL(`../../${pkg}/dist/index.d.ts`, import.meta.url)));
  const program = ts.createProgram(files, { skipLibCheck: true, noEmit: true });
  const checker = program.getTypeChecker();
  return new Map(pkgs.map((pkg, i) => [pkg, new Set(
    checker.getExportsOfModule(checker.getSymbolAtLocation(program.getSourceFile(files[i]))).map((s) => s.getName()),
  )]));
}

// Type-checking the declarations takes a few seconds on CI runners.
test('the moved names match what the packages export', { timeout: 60_000 }, () => {
  const exported = exportsOf(['react', ...Object.keys(movedExports).map((pkg) => pkg.slice('@celesta/'.length))]);
  const core = exported.get('react');
  const moved = new Set(Object.values(movedExports).flat());
  for (const [pkg, names] of Object.entries(movedExports)) {
    const own = exported.get(pkg.slice('@celesta/'.length));
    for (const name of names) {
      assert.ok(own.has(name), `${pkg} does not export ${name}`);
      assert.ok(!core.has(name), `@celesta/react still exports ${name}`);
    }
    // Names new with the split never were in @celesta/react, so need no move.
    const unlisted = [...own].filter((name) => !core.has(name) && !moved.has(name) && name !== 'CharacterReference');
    assert.deepEqual(unlisted, [], `${pkg} exports names the migration does not know`);
  }
});

test('moved names leave @celesta/react for their packages, joining existing imports', () => {
  const source = [
    "import * as React from 'react';",
    "import { Character } from '@celesta/character';",
    'import {',
    '  Composition,',
    '  Circle,',
    '  type CharacterViewReference,',
    '  CharacterView as View,',
    '  TextBox,',
    "} from '@celesta/react';",
    "import type { LipSyncTrack, TextStyle } from \"@celesta/react\";",
    "import { Center } from '@celesta/react';",
    "export { Stack } from '@celesta/react';",
    '',
  ].join('\n');
  const { text, packages, warnings } = migrateSource(source);
  assert.equal(text, [
    "import * as React from 'react';",
    "import { Character, type CharacterViewReference, CharacterView as View } from '@celesta/character';",
    "import { Composition } from '@celesta/react';",
    "import { Circle } from '@celesta/shapes';",
    "import { TextBox } from '@celesta/text';",
    "import type { TextStyle } from \"@celesta/react\";",
    "import type { LipSyncTrack } from \"@celesta/character\";",
    "import { Center } from '@celesta/layout';",
    "export { Stack } from '@celesta/layout';",
    '',
  ].join('\n'));
  assert.deepEqual([...packages].sort(), ['@celesta/character', '@celesta/layout', '@celesta/shapes', '@celesta/text']);
  assert.deepEqual(warnings, []);
  assert.equal(migrateSource(text).text, text, 'migrating twice changes nothing');
});

test('long lists stay multi-line, imports inside a line stay on it, and namespaces are reported', () => {
  const long = migrateSource([
    'import {',
    '  Composition, Sequence, Text, interpolate, useCurrentFrame, useVideoConfig, Easings, spring, Group, Rect,',
    '  DialogueSeries,',
    "} from '@celesta/react';",
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
    "} from '@celesta/react';",
    "import { DialogueSeries } from '@celesta/character';",
  ].join('\n'));

  const inline = migrateSource("const entry = `import { Composition, Line } from '@celesta/react';\\n`;").text;
  assert.equal(inline, "const entry = `import { Composition } from '@celesta/react'; import { Line } from '@celesta/shapes';\\n`;");

  const namespaced = migrateSource("import * as C from '@celesta/react';\nC.Text; C.Camera; C.useLipSync(track);\n");
  assert.equal(namespaced.text, "import * as C from '@celesta/react';\nC.Text; C.Camera; C.useLipSync(track);\n");
  assert.deepEqual(namespaced.warnings, [
    'line 1: `import * as C` uses moved names; import them from their packages: C.Camera (@celesta/layout), C.useLipSync (@celesta/character)',
  ]);
});

test('comments beside a name move with it', () => {
  const { text } = migrateSource([
    'import {',
    '  Composition, // root',
    '  /* drawn */ Circle,',
    '  Text, // body',
    "} from '@celesta/react';",
  ].join('\n'));
  assert.equal(text, [
    "import { Composition /* root */, Text /* body */ } from '@celesta/react';",
    "import { /* drawn */ Circle } from '@celesta/shapes';",
  ].join('\n'));
});

test('a manifest listing @celesta/react gets the new packages with its range', () => {
  const manifest = '{\n  "dependencies": {\n    "@celesta/react": "workspace:*",\n    "react": "^18.3.1"\n  }\n}\n';
  const { text, added } = addDependencies(manifest, new Set(['@celesta/text', '@celesta/shapes']));
  assert.deepEqual(added, ['@celesta/shapes', '@celesta/text']);
  assert.equal(text, '{\n  "dependencies": {\n    "@celesta/react": "workspace:*",\n    "@celesta/shapes": "workspace:*",\n    "@celesta/text": "workspace:*",\n    "react": "^18.3.1"\n  }\n}\n');
  assert.deepEqual(addDependencies('{"dependencies":{"react":"^18.3.1"}}', new Set(['@celesta/text'])).added, []);
});

test('the command migrates a project, and --check reports without writing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-migrate-'));
  try {
    mkdirSync(join(dir, 'scenes'));
    mkdirSync(join(dir, 'node_modules'));
    const scene = "import { Composition, TextReveal } from '@celesta/react';\n";
    writeFileSync(join(dir, 'scenes/Intro.tsx'), scene);
    writeFileSync(join(dir, 'node_modules/ignored.ts'), scene);
    writeFileSync(join(dir, 'package.json'), '{\n  "dependencies": {\n    "@celesta/react": "workspace:*"\n  }\n}\n');

    const checked = spawnSync(process.execPath, [script, dir, '--check'], { encoding: 'utf8' });
    assert.equal(checked.status, 1, checked.stderr);
    assert.match(checked.stdout, /would update .*Intro\.tsx \(@celesta\/text\)/);
    assert.equal(readFileSync(join(dir, 'scenes/Intro.tsx'), 'utf8'), scene);

    const migrated = spawnSync(process.execPath, [script, dir], { encoding: 'utf8' });
    assert.equal(migrated.status, 0, migrated.stderr);
    assert.equal(readFileSync(join(dir, 'scenes/Intro.tsx'), 'utf8'),
      "import { Composition } from '@celesta/react';\nimport { TextReveal } from '@celesta/text';\n");
    assert.equal(readFileSync(join(dir, 'node_modules/ignored.ts'), 'utf8'), scene);
    assert.match(readFileSync(join(dir, 'package.json'), 'utf8'), /"@celesta\/text": "workspace:\*"/);

    const again = spawnSync(process.execPath, [script, dir, '--check'], { encoding: 'utf8' });
    assert.equal(again.status, 0, again.stdout);
    assert.match(again.stdout, /Nothing to migrate/);

    // A namespace import of a moved name cannot be rewritten, so it fails the check.
    writeFileSync(join(dir, 'scenes/Outro.tsx'), "import * as C from '@celesta/react';\nC.Camera;\n");
    const namespaced = spawnSync(process.execPath, [script, dir, '--check'], { encoding: 'utf8' });
    assert.equal(namespaced.status, 1, namespaced.stdout);
    assert.match(namespaced.stderr, /Outro\.tsx:1: `import \* as C` uses moved names.*C\.Camera \(@celesta\/layout\)/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
