import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { test } from 'vitest';
import { fileURLToPath } from 'node:url';
import { showcase } from '../site/showcase-catalog.ts';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const locales = ['en', 'ja'];
const slugs = [
  'installation', 'create-project', 'preview', 'react-compositions',
  'animation', 'motion-toolkit', 'text-camera-lines', 'layout', 'text-fonts', 'media', 'dialogue', 'data', 'export',
  'reference',
  'math', 'math-noise', 'math-shaping', 'code', 'code-typing', 'voicevox', 'aivis-speech', 'lottie',
  'examples', 'timelines', 'build-from-source', 'troubleshooting', 'plugins',
];
const groups = ['getting-started', 'guides', 'reference', 'packages', 'more'];

/** `04-(packages)` → `packages`, `02-math-noise.md` → `math-noise`. */
const bare = (name: string) => name.replace(/\.md$/, '').replace(/^\d+-/, '').replace(/^\((.*)\)$/, '$1');

/** A chapter's Markdown file: its slug, the directory it sits in, and its path. */
interface Chapter {
  slug: string;
  dir: string;
  path: string;
}

/** The chapters under `dir` in navigation order, with the directory each one sits in. */
function chapters(dir: string): Chapter[] {
  return readdirSync(dir).sort().flatMap(name => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return chapters(path);
    return name.endsWith('.md') && name !== 'index.md' ? [{ slug: bare(name), dir, path }] : [];
  });
}

const docsDir = (locale: string) => join(root, 'content', locale, 'docs');
const markdownFiles = (locale: string) => [join(docsDir(locale), 'index.md'), ...chapters(docsDir(locale)).map(chapter => chapter.path)];

test('both locales have the same 27 chapters in the same groups', () => {
  for (const locale of locales) {
    const found = chapters(docsDir(locale));
    assert.deepEqual(found.map(chapter => chapter.slug), slugs, `${locale} chapter order`);
    const groupDirs = readdirSync(docsDir(locale)).sort().filter(name => statSync(join(docsDir(locale), name)).isDirectory());
    assert.deepEqual(groupDirs.map(bare), groups, `${locale} groups`);
    for (const name of groupDirs) assert.ok(existsSync(join(docsDir(locale), name, 'category.makit.ts')), `${locale}:${name} has a category`);
    for (const chapter of found) {
      const meta = chapter.path.replace(/\.md$/, '.meta.ts');
      assert.ok(existsSync(meta), `${locale}:${chapter.slug} has metadata`);
      assert.match(readFileSync(meta, 'utf8'), new RegExp(`id: '${chapter.slug}'`), `${locale}:${chapter.slug} page id`);
    }
  }
});

test('sub-pages follow their package page', () => {
  for (const locale of locales) {
    const found = chapters(docsDir(locale));
    const at = (slug: string) => found.findIndex(chapter => chapter.slug === slug);
    const dirOf = (slug: string) => found[at(slug)].dir;
    for (const [child, parent] of [['math-noise', 'math'], ['math-shaping', 'math'], ['code-typing', 'code']]) {
      assert.equal(dirOf(child), dirOf(parent), `${locale}:${child} shares a directory with ${parent}`);
      assert.ok(at(child) > at(parent), `${locale}:${child} sorts after ${parent}`);
    }
    assert.notEqual(dirOf('math'), dirOf('code'), `${locale}: each package has its own directory`);
    assert.equal(dirOf('aivis-speech'), dirOf('voicevox'), `${locale}: aivis-speech is a top-level package page beside voicevox`);
  }
});

test('internal links stay in their locale and point at existing chapters', () => {
  let links = 0;
  for (const locale of locales) {
    for (const file of markdownFiles(locale)) {
      const markdown = readFileSync(file, 'utf8');
      for (const [, target] of markdown.matchAll(/\]\((\/[^)\s]*)\)/g)) {
        assert.ok(target.startsWith(`/${locale}/`), `${file}: ${target} stays in /${locale}/`);
        if (!/^\/[a-z]+\/docs\//.test(target)) continue;
        links++;
        const match = /^\/([a-z]+)\/docs\/(?:([a-z0-9-]+)\/)?(?:#\S+)?$/.exec(target);
        assert.ok(match, `${file}: ${target} is a chapter address`);
        assert.equal(match[1], locale, `${file}: ${target} stays in ${locale}`);
        if (match[2]) assert.ok(slugs.includes(match[2]), `${file}: ${target} names a chapter`);
      }
    }
  }
  assert.ok(links > 0, 'the chapters link to each other');
});

test('every include exists', () => {
  let includes = 0;
  for (const locale of locales) {
    for (const file of markdownFiles(locale)) {
      for (const [, quoted, bareName] of readFileSync(file, 'utf8').matchAll(/(?:^|\s)file=(?:"([^"]*)"|(\S+))/gm)) {
        const path = quoted ?? bareName;
        includes++;
        assert.ok(existsSync(resolve(root, path)), `${file}: ${path} exists`);
      }
    }
  }
  assert.ok(includes > 0, 'the documentation includes example files');
});

/** Tab groups, tabs, accordions, and toggles in a chapter, as Makit 0.9 container directives. */
const containers = (markdown: string) => Object.fromEntries(['tabs', 'tab', 'accordion', 'toggle'].map(name => [name, markdown.match(new RegExp(`^:{3,}${name}\\[`, 'gm'))?.length ?? 0]));

test('both locales have the same tabs and toggles in every chapter', () => {
  const [en, ja] = locales.map(locale => new Map(chapters(docsDir(locale)).map(chapter => [chapter.slug, containers(readFileSync(chapter.path, 'utf8'))])));
  for (const slug of slugs) assert.deepEqual(ja.get(slug), en.get(slug), `${slug}: ja matches en`);
  // The old site's OS pickers and details blocks (Task 14), plus the debug-guides contact sheet, subtitle file, looping video, and loudness main added to Export.
  const expected: Record<string, ReturnType<typeof containers>> = {
    'installation': { tabs: 1, tab: 3, accordion: 0, toggle: 0 },
    'build-from-source': { tabs: 1, tab: 3, accordion: 0, toggle: 0 },
    'create-project': { tabs: 1, tab: 2, accordion: 0, toggle: 0 },
    'export': { tabs: 9, tab: 18, accordion: 0, toggle: 0 },
    'timelines': { tabs: 0, tab: 0, accordion: 0, toggle: 3 },
    'troubleshooting': { tabs: 0, tab: 0, accordion: 1, toggle: 9 },
  };
  for (const slug of slugs) assert.deepEqual(en.get(slug), expected[slug] ?? { tabs: 0, tab: 0, accordion: 0, toggle: 0 }, slug);
});

test('platform tabs replace the flattened pickers', () => {
  // The Examples chapter's own macOS/Windows list was never a picker, so only the picker chapters are checked for one.
  const pickers = ['installation', 'build-from-source', 'create-project', 'export'];
  for (const locale of locales) {
    for (const chapter of chapters(docsDir(locale)).filter(chapter => pickers.includes(chapter.slug))) {
      assert.doesNotMatch(readFileSync(chapter.path, 'utf8'), /^- \*\*(?:macOS|Windows)[:：]\*\*/m, `${locale}:${chapter.slug}: no platform list`);
    }
    for (const file of markdownFiles(locale)) {
      const markdown = readFileSync(file, 'utf8');
      assert.doesNotMatch(markdown, /title="[^"]*(?: \((?:macOS|Windows)\)|（(?:macOS|Windows)）)"/, `${file}: no platform suffix in fence titles`);
      for (const [, attributes] of markdown.matchAll(/^::::tabs\[[^\]]+\](.*)$/gm)) assert.equal(attributes, '{default="macOS"}', `${file}: tab groups start on macOS`);
    }
  }
});

/** A `::::tabs` group: its closing fence, its `default`, and its tab labels in order. */
interface TabGroup {
  fence: string;
  default: string | undefined;
  labels: string[];
}

/** Each `::::tabs` group in a chapter: its `default` and its tab labels in order. */
function tabGroups(markdown: string): TabGroup[] {
  const groups: TabGroup[] = [];
  let group: TabGroup | undefined;
  // Windows checkouts may have CRLF line endings.
  for (const line of markdown.split(/\r?\n/)) {
    const start = /^(:{4,})tabs\[[^\]]*\](?:\{default="([^"]*)"\})?$/.exec(line);
    if (start) { group = { fence: start[1], default: start[2], labels: [] }; groups.push(group); continue; }
    if (!group) continue;
    if (line === group.fence) { group = undefined; continue; }
    // A direct child tab uses a shorter fence than its group.
    const tab = /^(:{3,})tab\[([^\]]*)\]$/.exec(line);
    if (tab && tab[1].length < group.fence.length) group.labels.push(tab[2]);
  }
  return groups;
}

test('tab labels are the old picker buttons, the same in both locales, and each default names a tab', () => {
  const [en, ja] = locales.map(locale => new Map(chapters(docsDir(locale)).map(chapter => [chapter.slug, tabGroups(readFileSync(chapter.path, 'utf8'))])));
  let groups = 0;
  for (const slug of slugs) {
    const enGroups = en.get(slug);
    const jaGroups = ja.get(slug);
    assert.ok(enGroups && jaGroups, `${slug}: both locales have the chapter`);
    assert.deepEqual(jaGroups.map(group => group.labels), enGroups.map(group => group.labels), `${slug}: ja tab labels match en`);
    for (const [locale, chapter] of [['en', enGroups], ['ja', jaGroups]] as const) {
      for (const group of chapter) {
        groups++;
        // The old pickers' buttons read exactly macOS / Windows / Linux, in that order (ruling R30).
        assert.deepEqual(group.labels, ['macOS', 'Windows', 'Linux'].slice(0, group.labels.length), `${locale}:${slug}: tab labels are the old button labels`);
        assert.ok(group.default !== undefined && group.labels.includes(group.default), `${locale}:${slug}: default "${group.default}" names a tab`);
      }
    }
  }
  assert.equal(groups, 24, 'twelve tab groups per locale');
});

test('every showcase work has a page and metadata', () => {
  const dir = join(root, 'pages', 'showcase');
  const expected = ['index.tsx', 'index.meta.ts', ...showcase.flatMap(({ slug }) => [`${slug}.tsx`, `${slug}.meta.ts`])];
  assert.deepEqual(readdirSync(dir).sort(), expected.sort(), 'pages/showcase holds exactly the gallery and one page per work');
  for (const { slug } of showcase) {
    assert.match(readFileSync(join(dir, `${slug}.tsx`), 'utf8'), new RegExp(`<ShowcaseEntry slug="${slug}" locale={locale} />`), `${slug} renders its own entry`);
    assert.match(readFileSync(join(dir, `${slug}.meta.ts`), 'utf8'), new RegExp(`showcaseMetadata\\('${slug}'\\)`), `${slug} describes itself`);
  }
});

test('catalog posters match the poster files in examples', () => {
  for (const work of showcase) assert.equal('poster' in work, existsSync(join(root, '..', '..', 'examples', work.slug, 'poster.jpg')), `${work.slug} poster flag`);
});
