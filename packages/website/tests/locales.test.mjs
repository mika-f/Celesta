import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { localeCodes, preferredLocale } from '../src/locales.ts';
import worker from '../worker.ts';

test('negotiate browser preferences, quality values, regional tags and fallback', () => {
  for (const [header, expected] of [
    ['ja-JP, en-US;q=0.8', 'ja'], ['en-US,ja;q=0.8', 'en'],
    ['en;q=0.5, ja;q=0.9', 'ja'], ['fr-FR,ja;q=0.7,en;q=0.5', 'ja'],
    ['JA-jp ; q=0.9, en;q=0.8', 'ja'], ['ja;q=0,en;q=0.4', 'en'],
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
  for (const path of ['/en/', '/ja/', '/ja/docs/export/', '/docs/export/', '/assets/site.js', '/unknown/']) {
    const result = await worker.fetch(new Request(`https://celesta.natsuneko.cat${path}`, { headers: { 'Accept-Language': 'ja' } }), env);
    assert.equal(await result.text(), 'asset');
  }
  assert.equal(calls.length, 6);
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
