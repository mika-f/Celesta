import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { localeCodes, preferredLocale, localizedPath } from '../src/locales.ts';
import { getDocGroups } from '../src/docs-nav.ts';
import { translate } from '../src/catalog.ts';
import { richText } from '../src/i18n.ts';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import worker from '../worker.ts';

test('negotiate browser preferences, quality values, regional tags and fallback', () => {
  for (const [header, expected] of [
    ['ja-JP, en-US;q=0.8', 'ja'], ['en-US,ja;q=0.8', 'en'],
    ['en;q=0.5, ja;q=0.9', 'ja'], ['fr-FR,ja;q=0.7,en;q=0.5', 'ja'],
    ['JA-jp ; q=0.9, en;q=0.8', 'ja'], ['ja;q=0,en;q=0.4', 'en'],
    ['ja ;q=0.9, en;q=0.8', 'ja'], [' JA ; q=0.9 , EN;q=0.8 ', 'ja'],
    ['en;q=0,*;q=0.5', 'ja'], ['ja;q=0,*;q=0.5', 'en'],
    ['en-US;q=0,*;q=0.5', 'ja'], ['en;q=0.5,*;q=0.9', 'ja'],
    ['en;q=0,ja;q=0.5,*;q=1', 'ja'], ['en;q=0,ja;q=0,*;q=1', 'en'],
    ['ja;q=invalid,en', 'en'], ['ja;q=2,en', 'en'], ['fr-FR', 'en'],
    ['*', 'en'], ['', 'en'], [null, 'en'],
  ]) assert.equal(preferredLocale(header), expected, String(header));
});

test('only the root negotiates: explicit languages and assets keep their URLs', async () => {
  const calls = [];
  const env = { ASSETS: { fetch(request) { calls.push(request.url); return new Response('asset'); } } };
  const response = await worker.fetch(new Request('https://celesta.natsuneko.cat/?ref=launch', { headers: { 'Accept-Language': 'ja' } }), env);
  assert.equal(response.status, 302);
  assert.equal(response.headers.get('Location'), 'https://celesta.natsuneko.cat/ja/?ref=launch');
  assert.equal(response.headers.get('Vary'), 'Accept-Language');
  assert.equal(response.headers.get('Cache-Control'), 'no-store');
  assert.deepEqual(calls, []);
  const paths = ['/en/', '/ja/', '/ja/docs/export/', '/docs/export/', '/assets/site.js', '/unknown/'];
  for (const path of paths) {
    const result = await worker.fetch(new Request(`https://celesta.natsuneko.cat${path}`, { headers: { 'Accept-Language': 'ja' } }), env);
    assert.equal(await result.text(), 'asset');
  }
  assert.deepEqual(calls, paths.map(path => `https://celesta.natsuneko.cat${path}`));
});

test('language links replace the locale with or without a trailing slash', () => {
  for (const [path, expected] of [
    ['/en', '/ja/'], ['/en/', '/ja/'], ['/ja', '/ja/'],
    ['/en/docs/export/', '/ja/docs/export/'], ['/docs/export/', '/ja/docs/export/'],
  ]) assert.equal(localizedPath(path, 'ja'), expected);
});

test('missing rich-text wrappers preserve copy, nesting and HTML escaping', () => {
  assert.equal(renderToStaticMarkup(richText('Read <slot0>the docs</slot0> now.')), 'Read the docs now.');
  assert.equal(renderToStaticMarkup(richText('<slot0>Read <slot1>this</slot1></slot0>', [undefined, createElement('strong')])), 'Read <strong>this</strong>');
  assert.equal(renderToStaticMarkup(richText('<slot0>&lt;script&gt;</slot0>', ['unused'])), '&lt;script&gt;');
});

test('Trans preserves nested links, scalar slots and existing code children', () => {
  assert.equal(renderToStaticMarkup(richText('<slot0>Read <slot1>this</slot1></slot0>', [createElement('a', { href: '/ja/docs/' }), createElement('strong')])), '<a href="/ja/docs/">Read <strong>this</strong></a>');
  assert.equal(renderToStaticMarkup(richText('<slot0/> lines', [126])), '126 lines');
  assert.equal(renderToStaticMarkup(richText('Use <slot0/>.', [createElement('code', null, '<FreezeFrame frame={60}>')])), 'Use <code>&lt;FreezeFrame frame={60}&gt;</code>.');
});

test('nested messages interpolate values without leaking a previous language', () => {
  assert.equal(translate('ja', 'docs.metadata.title', { title: 'VOICEVOX' }), 'VOICEVOX — Celesta ドキュメント');
  assert.equal(translate('en', 'docs.metadata.title', { title: 'VOICEVOX' }), 'VOICEVOX — Celesta documentation');
  assert.equal(translate('ja', 'playground.controls.preview-at', { frame: 60 }), 'フレーム 60 のプレビュー');
});

test('search aliases are localized and result counts use the correct forms', () => {
  const pages = getDocGroups('ja').flatMap(group => group.pages);
  assert.ok(pages.find(page => page.slug === 'installation').keywords.includes('セットアップ'));
  assert.ok(pages.find(page => page.slug === 'math').keywords.includes('数学'));
  for (const [locale, count, expected] of [['en', 0, '0 matching topics'], ['en', 1, '1 matching topic'], ['en', 2, '2 matching topics'], ['ja', 1, '1 件のトピック'], ['ja', 2, '2 件のトピック']]) {
    assert.equal(translate(locale, 'docs.search.matches', { count }), expected);
  }
});

test('structured catalogs contain every message and preserve balanced Trans slots', () => {
  const flatten = (object, prefix = '') => Object.fromEntries(Object.entries(object).flatMap(([key, value]) => {
    assert.ok(!key.includes('.'), `flat key: ${prefix}${key}`);
    const path = `${prefix}${key}`;
    return typeof value === 'string' ? [[path, value]] : Object.entries(flatten(value, `${path}.`));
  }));
  const catalogs = Object.fromEntries(localeCodes.map(locale => [locale, flatten(JSON.parse(readFileSync(new URL(`../src/locales/${locale}.json`, import.meta.url))))]));
  const tokens = text => text.match(/<\/?slot\d+\/?>|\{\{\w+\}\}/g) ?? [];
  for (const [locale, catalog] of Object.entries(catalogs)) {
    for (const key of Object.keys(catalogs.en)) assert.ok(key in catalog, `${locale}:${key} is missing`);
    const pluralBases = Object.keys(catalogs.en).filter(key => key.endsWith('_other')).map(key => key.slice(0, -6));
    for (const base of pluralBases) {
      for (const category of new Intl.PluralRules(locale).resolvedOptions().pluralCategories) {
        assert.ok(`${base}_${category}` in catalog, `${locale}:${base}_${category} is missing`);
      }
    }
    for (const [key, value] of Object.entries(catalog)) {
      assert.ok(value.trim(), `${locale}:${key} is empty`);
      const source = catalogs.en[key] ?? catalogs.en[key.replace(/_(zero|one|two|few|many|other)$/, '_other')];
      assert.equal(typeof source, 'string', `${locale}:${key} has no source message`);
      assert.deepEqual(tokens(value).sort(), tokens(source).sort(), `${locale}:${key} placeholders`);
      const stack = [];
      for (const token of tokens(value)) {
        if (/^<slot\d+>$/.test(token)) stack.push(token.slice(5, -1));
        if (/^<\/slot\d+>$/.test(token)) assert.equal(stack.pop(), token.slice(6, -1), `${locale}:${key} nesting`);
      }
      assert.deepEqual(stack, [], `${locale}:${key} unclosed placeholder`);
    }
  }
});
