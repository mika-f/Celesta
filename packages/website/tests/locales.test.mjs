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
  assert.equal(renderToStaticMarkup(richText('Read <0>the docs</0> now.')), 'Read the docs now.');
  assert.equal(renderToStaticMarkup(richText('<0>Read <1>this</1></0>', [undefined, createElement('strong')])), 'Read <strong>this</strong>');
  assert.equal(renderToStaticMarkup(richText('<0>&lt;script&gt;</0>', ['unused'])), '&lt;script&gt;');
});

test('search aliases are localized and result counts use the correct forms', () => {
  const pages = getDocGroups('ja').flatMap(group => group.pages);
  assert.ok(pages.find(page => page.slug === 'installation').keywords.includes('セットアップ'));
  assert.ok(pages.find(page => page.slug === 'math').keywords.includes('数学'));
  for (const [locale, count, expected] of [['en', 1, '1 matching topic'], ['en', 2, '2 matching topics'], ['ja', 1, '1 件のトピック']]) {
    const key = new Intl.PluralRules(locale).select(count) === 'one' ? 'matches-one' : 'matches-other';
    assert.equal(translate(locale, key, { count }), expected);
  }
});

test('all catalogs contain every message and preserve balanced React placeholders', () => {
  const catalogs = Object.fromEntries(localeCodes.map(locale => [locale, JSON.parse(readFileSync(new URL(`../src/locales/${locale}.json`, import.meta.url)))]));
  const tokens = text => text.match(/<\/?\d+\/?>|\{\w+\}/g) ?? [];
  for (const [locale, catalog] of Object.entries(catalogs)) {
    assert.deepEqual(Object.keys(catalog).sort(), Object.keys(catalogs.en).sort(), locale);
    for (const [key, value] of Object.entries(catalog)) {
      assert.ok(value.trim(), `${locale}:${key} is empty`);
      assert.deepEqual(tokens(value).sort(), tokens(catalogs.en[key]).sort(), `${locale}:${key} placeholders`);
      const stack = [];
      for (const token of tokens(value)) {
        if (/^<\d+>$/.test(token)) stack.push(token.slice(1, -1));
        if (/^<\/\d+>$/.test(token)) assert.equal(stack.pop(), token.slice(2, -1), `${locale}:${key} nesting`);
      }
      assert.deepEqual(stack, [], `${locale}:${key} unclosed placeholder`);
    }
  }
});
