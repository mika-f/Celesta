import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { afterEach, test } from 'vitest';
import { fillMarkdown, fillMarkdownOutputs } from '../scripts/postbuild.mjs';

const names = ['CELESTA_DOWNLOAD_URL', 'CELESTA_DOWNLOAD_URL_MACOS', 'CELESTA_DOWNLOAD_URL_WINDOWS'];
const saved = Object.fromEntries(names.map(name => [name, process.env[name]]));
afterEach(() => {
  for (const name of names) {
    if (saved[name] === undefined) delete process.env[name];
    else process.env[name] = saved[name];
  }
});

/** A package root with one example file, and a dist/ holding the given files. */
function fixture(files: Record<string, string>) {
  const root = mkdtempSync(join(tmpdir(), 'postbuild-'));
  mkdirSync(join(root, 'examples'));
  writeFileSync(join(root, 'examples', 'a.tsx'), 'export const a = 1;\n');
  for (const [path, contents] of Object.entries(files)) {
    mkdirSync(dirname(join(root, 'dist', path)), { recursive: true });
    writeFileSync(join(root, 'dist', path), contents);
  }
  return { root, dist: join(root, 'dist'), read: (path: string) => readFileSync(join(root, 'dist', path), 'utf8') };
}

const page = [
  '* Untouched [prose](/en/docs/) stays *as written*.',
  '',
  '> [!NOTE]',
  '> Notes keep their marker.',
  '',
  '```tsx file=examples/a.tsx',
  '```',
  '',
  '- [Download for macOS ↓](celesta:download/macos)',
  '',
].join('\n');

test('fills an include fence and rewrites a download link, leaving the rest as written', async () => {
  process.env.CELESTA_DOWNLOAD_URL_MACOS = 'https://example.com/m.dmg';
  const { root } = fixture({});
  assert.equal(await fillMarkdown(page, root), [
    '* Untouched [prose](/en/docs/) stays *as written*.',
    '',
    '> [!NOTE]',
    '> Notes keep their marker.',
    '',
    '```tsx title="a.tsx"',
    'export const a = 1;',
    '```',
    '',
    '- [Download for macOS ↓](https://example.com/m.dmg)',
    '',
  ].join('\n'));
});

test('continues a list item\'s indent inside a filled fence', async () => {
  const { root } = fixture({});
  const out = await fillMarkdown('1. Write:\n\n   ```tsx file=examples/a.tsx\n   ```\n', root);
  assert.equal(out, '1. Write:\n\n   ```tsx title="a.tsx"\n   export const a = 1;\n   ```\n');
});

test('keeps a blockquote around a filled fence, blank lines included', async () => {
  const { root } = fixture({});
  writeFileSync(join(root, 'examples', 'b.tsx'), 'a;\n\nb;\n');
  const out = await fillMarkdown('> Example:\n>\n> ```tsx file=examples/b.tsx\n> ```\n', root);
  assert.equal(out, '> Example:\n>\n> ```tsx title="b.tsx"\n> a;\n>\n> b;\n> ```\n');
});

test('rewrites link definitions and defaults to the latest release', async () => {
  for (const name of names) delete process.env[name];
  const { root } = fixture({});
  assert.equal(await fillMarkdown('[all][r]\n\n[r]: celesta:download/all "Releases"\n', root), '[all][r]\n\n[r]: https://github.com/mika-f/Celesta/releases/latest "Releases"\n');
});

test('fills the Markdown endpoints and llms-full.txt in dist', async () => {
  process.env.CELESTA_DOWNLOAD_URL_MACOS = 'https://example.com/m.dmg';
  const llms = `# Celesta\n\n## Documentation\n\n## Installation\n\nSource: https://celesta.natsuneko.cat/en/docs/installation.md\n\n${page}\n## Overview\n\nSource: https://celesta.natsuneko.cat/en/docs.md\n\nNo includes.\n`;
  const { dist, read, root } = fixture({ 'en/docs/installation.md': page, 'ja/docs.md': 'No includes.\n', 'llms-full.txt': llms, 'llms.txt': '- [Installation](celesta:download/macos)\n' });
  assert.equal(await fillMarkdownOutputs(dist, root), 2);
  for (const path of ['en/docs/installation.md', 'llms-full.txt']) {
    assert.match(read(path), /```tsx title="a\.tsx"\nexport const a = 1;\n```/, path);
    assert.match(read(path), /\(https:\/\/example\.com\/m\.dmg\)/, path);
    assert.doesNotMatch(read(path), /file=|celesta:download/, path);
  }
  assert.match(read('llms-full.txt'), /## Overview\n\nSource: https:\/\/celesta\.natsuneko\.cat\/en\/docs\.md\n\nNo includes\.\n$/);
  assert.equal(read('ja/docs.md'), 'No includes.\n');
  assert.equal(read('llms.txt'), '- [Installation](celesta:download/macos)\n', 'only *.md and llms-full.txt are filled');
});

test('fills fences and links inside tabs and toggles, keeping the directive syntax', async () => {
  process.env.CELESTA_DOWNLOAD_URL_WINDOWS = 'https://example.com/w.exe';
  const { root } = fixture({});
  const markdown = [
    '::::tabs[Platform]{default="macOS"}',
    ':::tab[macOS]',
    '',
    '```tsx file=examples/a.tsx',
    '```',
    '',
    ':::',
    ':::tab[Windows]',
    '',
    '[Download](celesta:download/windows)',
    '',
    ':::',
    '::::',
    '',
    ':::toggle[View the example]',
    '',
    '```tsx file=examples/a.tsx title="a.tsx"',
    '```',
    '',
    ':::',
    '',
  ].join('\n');
  assert.equal(await fillMarkdown(markdown, root), markdown
    .replace('```tsx file=examples/a.tsx\n```', '```tsx title="a.tsx"\nexport const a = 1;\n```')
    .replace('```tsx file=examples/a.tsx title="a.tsx"\n```', '```tsx title="a.tsx"\nexport const a = 1;\n```')
    .replace('celesta:download/windows', 'https://example.com/w.exe'));
});

test('an unreadable include fails the build', async () => {
  const { root } = fixture({});
  await assert.rejects(fillMarkdown('```tsx file=examples/missing.tsx\n```\n', root), /cannot read "examples\/missing\.tsx"/);
});
