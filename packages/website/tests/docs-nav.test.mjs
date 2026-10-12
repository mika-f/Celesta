import assert from 'node:assert/strict';
import { test } from 'node:test';
import { localeCodes } from '../src/site/locales.ts';
import { getDocGroups } from '../src/site/docs-nav.ts';

const slugs = groups => groups.flatMap(group => group.pages).map(page => page.slug);

test('chapters keep their addresses', () => {
  assert.deepEqual(slugs(getDocGroups('en')).sort(), [
    'aivis-speech', 'animation', 'build-from-source', 'code', 'code-typing', 'create-project', 'data', 'dialogue', 'examples', 'export',
    'installation', 'layout', 'math', 'math-noise', 'math-shaping', 'media', 'motion-toolkit', 'preview', 'react-compositions',
    'reference', 'text-camera-lines', 'text-fonts', 'timelines', 'troubleshooting', 'voicevox',
  ]);
});

test('sub-pages follow their package page, one level deep, in the same group', () => {
  for (const locale of localeCodes) {
    for (const group of getDocGroups(locale)) {
      group.pages.forEach((page, index) => {
        if (!page.parent) return;
        const parent = group.pages.findIndex(candidate => candidate.slug === page.parent);
        assert.ok(parent >= 0 && parent < index, `${locale}:${page.slug} follows ${page.parent}`);
        assert.equal(group.pages[parent].parent, undefined, `${locale}:${page.parent} is a top-level page`);
        assert.ok(group.pages.slice(parent + 1, index).every(between => between.parent === page.parent), `${locale}:${page.slug} sits with its siblings`);
      });
    }
  }
  const parents = Object.fromEntries(getDocGroups('en').flatMap(group => group.pages).filter(page => page.parent).map(page => [page.slug, page.parent]));
  assert.deepEqual(parents, { 'math-noise': 'math', 'math-shaping': 'math', 'code-typing': 'code' });
});

test('sub-pages use a short label in the navigation and keep the full title', () => {
  const page = (locale, slug) => getDocGroups(locale).flatMap(group => group.pages).find(candidate => candidate.slug === slug);
  assert.equal(page('ja', 'math-noise').navTitle, 'ノイズと fbm');
  assert.equal(page('ja', 'math-noise').title, 'math：ノイズと fbm');
  assert.equal(page('en', 'code-typing').navTitle, 'Typing, carets & tokens');
  assert.equal(page('en', 'math').navTitle, '@celesta/math');
});

test('the API reference has its own group, apart from the add-on packages', () => {
  const groups = getDocGroups('ja').map(group => [group.title, slugs([group])]);
  assert.deepEqual(groups.find(([, pages]) => pages.includes('reference')), ['リファレンス', ['reference']]);
  assert.deepEqual(groups.find(([, pages]) => pages.includes('math')), ['追加パッケージ', ['math', 'math-noise', 'math-shaping', 'code', 'code-typing', 'voicevox', 'aivis-speech']]);
});
